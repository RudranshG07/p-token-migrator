use anyhow::{anyhow, Context, Result};
use litesvm::LiteSVM;
use solana_sdk::{
    instruction::{AccountMeta, Instruction},
    pubkey::Pubkey,
    signature::{Keypair, Signer},
    transaction::Transaction,
};
use std::collections::HashMap;
use std::str::FromStr;

use crate::alt::{decode_lookup_table, resolve_account_list, LookupInput, ResolvedAccount};
use crate::fork::{ApplyWarning, Fork};
use crate::rpc::{RpcClient, Transport, TxMessage, TxResponse};

#[derive(Debug, Clone)]
pub struct ReplayOptions {
    /// When true, transactions that reference an Address Lookup Table are
    /// skipped instead of resolved. Off by default — ALT resolution is on.
    pub skip_alt: bool,
    /// Maximum compute units to allow during replay. None = LiteSVM default.
    pub compute_unit_limit: Option<u32>,
    /// Accounts whose post-state should be excluded from `account_diffs`.
    /// Defaults to the full set of sysvars (Clock, RecentBlockhashes, Rent,
    /// SlotHashes, SlotHistory, EpochSchedule, StakeHistory, Fees, Rewards) —
    /// these are volatile by design and produce false positives in the
    /// safety check. Clear or extend as needed.
    pub diff_ignored_accounts: std::collections::HashSet<Pubkey>,
}

impl Default for ReplayOptions {
    fn default() -> Self {
        Self {
            skip_alt: false,
            compute_unit_limit: None,
            diff_ignored_accounts: default_volatile_accounts(),
        }
    }
}

/// The standard set of accounts whose post-state changes between any two
/// runs and would generate diff noise if not filtered. Pre-populated into
/// `ReplayOptions::diff_ignored_accounts` by default.
pub fn default_volatile_accounts() -> std::collections::HashSet<Pubkey> {
    use solana_sdk::sysvar;
    let mut set = std::collections::HashSet::new();
    set.insert(sysvar::clock::ID);
    set.insert(sysvar::epoch_schedule::ID);
    set.insert(sysvar::fees::ID);
    set.insert(sysvar::recent_blockhashes::ID);
    set.insert(sysvar::rent::ID);
    set.insert(sysvar::rewards::ID);
    set.insert(sysvar::slot_hashes::ID);
    set.insert(sysvar::slot_history::ID);
    set.insert(sysvar::stake_history::ID);
    set.insert(sysvar::instructions::ID);
    set
}

#[derive(Debug, Clone)]
pub struct ReplayOutcome {
    pub signature: String,
    pub slot: u64,
    pub mainnet: ChainOutcome,
    pub replay: ReplayResult,
    pub divergence: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct ChainOutcome {
    pub success: bool,
    pub error: Option<String>,
    pub compute_units_consumed: Option<u64>,
    pub log_count: usize,
}

#[derive(Debug, Clone)]
pub enum ReplayResult {
    Executed {
        success: bool,
        compute_units_consumed: u64,
        logs: Vec<String>,
        error: Option<String>,
        /// Snapshot of every writable account at the end of the execution.
        /// Used for two-build state diffing — the actual "is my migration
        /// safe" signal (bytes written to user accounts).
        post_state: std::collections::BTreeMap<Pubkey, AccountState>,
    },
    Skipped {
        reason: String,
    },
    Failed {
        reason: String,
    },
}

#[derive(Debug, Clone)]
pub struct AccountState {
    pub lamports: u64,
    pub data: Vec<u8>,
    pub owner: Pubkey,
}

/// Side-by-side outcome of running one signature against two builds of the
/// same target program. This is the core "is my migration safe" primitive.
#[derive(Debug, Clone)]
pub struct BuildDiffOutcome {
    pub signature: String,
    pub slot: u64,
    pub target_program: Pubkey,
    pub mainnet: ChainOutcome,
    pub legacy: ReplayResult,
    pub new: ReplayResult,
    /// Diff of execution metadata between the legacy and new builds —
    /// success, CU, error code, log count.
    pub legacy_vs_new: Vec<String>,
    /// Per-account diff of post-tx state — the **safety-critical** field.
    /// Each entry describes how some writable account ended up different
    /// between the legacy run and the new run. Empty == bytewise safe.
    pub account_diffs: Vec<AccountDiff>,
    pub legacy_apply_warnings: Vec<String>,
    pub new_apply_warnings: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct AccountDiff {
    pub pubkey: Pubkey,
    pub lamports_legacy: Option<u64>,
    pub lamports_new: Option<u64>,
    pub data_len_legacy: Option<usize>,
    pub data_len_new: Option<usize>,
    pub owner_legacy: Option<Pubkey>,
    pub owner_new: Option<Pubkey>,
    /// Byte offset of the first differing byte in the data, if both sides
    /// had data and it differed. None means either the lengths match and
    /// every byte is equal, or one side is absent.
    pub first_diff_byte: Option<usize>,
    /// Concise human-readable summary classifying the divergence.
    pub kind: String,
}

// ---------------------------------------------------------------------------
// Single-build replay (existing public API)
// ---------------------------------------------------------------------------

pub fn replay_signature<T: Transport>(
    rpc: &RpcClient<T>,
    signature: &str,
    options: &ReplayOptions,
) -> Result<ReplayOutcome> {
    let tx = rpc
        .get_transaction(signature)?
        .ok_or_else(|| anyhow!("transaction not found: {signature}"))?;
    replay_tx_response(rpc, tx, signature, options)
}

pub fn replay_tx_response<T: Transport>(
    rpc: &RpcClient<T>,
    tx: TxResponse,
    signature: &str,
    options: &ReplayOptions,
) -> Result<ReplayOutcome> {
    let mainnet = extract_chain_outcome(&tx);
    let slot = tx.slot;

    let prepared = prepare_replay_state(rpc, tx, options)?;
    match prepared {
        PrepareOutcome::Skipped(reason) => Ok(ReplayOutcome {
            signature: signature.to_string(),
            slot,
            mainnet,
            replay: ReplayResult::Skipped { reason },
            divergence: vec!["skipped:alt".to_string()],
        }),
        PrepareOutcome::Ready(state) => {
            let (result, warnings) = execute_replay(&state, &HashMap::new());
            let mut divergence = compute_divergence(&mainnet, &result);
            for warning in &warnings {
                divergence.push(format!(
                    "apply-warning:{}={}",
                    warning.pubkey, warning.reason
                ));
            }
            Ok(ReplayOutcome {
                signature: signature.to_string(),
                slot,
                mainnet,
                replay: result,
                divergence,
            })
        }
    }
}

// ---------------------------------------------------------------------------
// Two-build diff
// ---------------------------------------------------------------------------

pub fn compare_builds<T: Transport>(
    rpc: &RpcClient<T>,
    signature: &str,
    target_program: &Pubkey,
    legacy_elf: &[u8],
    new_elf: &[u8],
    options: &ReplayOptions,
) -> Result<BuildDiffOutcome> {
    let tx = rpc
        .get_transaction(signature)?
        .ok_or_else(|| anyhow!("transaction not found: {signature}"))?;
    let mainnet = extract_chain_outcome(&tx);
    let slot = tx.slot;

    let prepared = prepare_replay_state(rpc, tx, options)?;
    let state = match prepared {
        PrepareOutcome::Skipped(reason) => {
            return Ok(BuildDiffOutcome {
                signature: signature.to_string(),
                slot,
                target_program: *target_program,
                mainnet,
                legacy: ReplayResult::Skipped { reason: reason.clone() },
                new: ReplayResult::Skipped { reason },
                legacy_vs_new: vec!["skipped:alt".to_string()],
                account_diffs: vec![],
                legacy_apply_warnings: vec![],
                new_apply_warnings: vec![],
            });
        }
        PrepareOutcome::Ready(state) => state,
    };

    let mut legacy_overrides = HashMap::new();
    legacy_overrides.insert(*target_program, legacy_elf.to_vec());

    let mut new_overrides = HashMap::new();
    new_overrides.insert(*target_program, new_elf.to_vec());

    let (legacy_result, legacy_warnings) = execute_replay(&state, &legacy_overrides);
    let (new_result, new_warnings) = execute_replay(&state, &new_overrides);

    let legacy_vs_new = diff_results(&legacy_result, &new_result);
    let account_diffs =
        diff_post_states(&legacy_result, &new_result, &options.diff_ignored_accounts);

    Ok(BuildDiffOutcome {
        signature: signature.to_string(),
        slot,
        target_program: *target_program,
        mainnet,
        legacy: legacy_result,
        new: new_result,
        legacy_vs_new,
        account_diffs,
        legacy_apply_warnings: legacy_warnings
            .iter()
            .map(|w| format!("{}={}", w.pubkey, w.reason))
            .collect(),
        new_apply_warnings: new_warnings
            .iter()
            .map(|w| format!("{}={}", w.pubkey, w.reason))
            .collect(),
    })
}

/// Compare the post-tx account state captured by two `Executed` results.
/// Returns the set of writable accounts that ended up different — the
/// safety-critical signal for "is my migration safe."
///
/// `ignored` is the set of accounts to skip — typically the sysvars
/// (`default_volatile_accounts`) so per-run clock/blockhash drift doesn't
/// generate false positives.
pub fn diff_post_states(
    legacy: &ReplayResult,
    new: &ReplayResult,
    ignored: &std::collections::HashSet<Pubkey>,
) -> Vec<AccountDiff> {
    let legacy_state = match legacy {
        ReplayResult::Executed { post_state, .. } => post_state,
        _ => return Vec::new(),
    };
    let new_state = match new {
        ReplayResult::Executed { post_state, .. } => post_state,
        _ => return Vec::new(),
    };

    let mut keys: std::collections::BTreeSet<&Pubkey> = std::collections::BTreeSet::new();
    keys.extend(legacy_state.keys());
    keys.extend(new_state.keys());

    let mut diffs = Vec::new();
    for key in keys {
        if ignored.contains(key) {
            continue;
        }
        let l = legacy_state.get(key);
        let n = new_state.get(key);
        match (l, n) {
            (Some(la), Some(na)) => {
                let same_lamports = la.lamports == na.lamports;
                let same_owner = la.owner == na.owner;
                let same_data = la.data == na.data;
                if same_lamports && same_owner && same_data {
                    continue;
                }
                let first_diff_byte = if !same_data {
                    la.data
                        .iter()
                        .zip(na.data.iter())
                        .position(|(x, y)| x != y)
                        .or_else(|| {
                            if la.data.len() != na.data.len() {
                                Some(la.data.len().min(na.data.len()))
                            } else {
                                None
                            }
                        })
                } else {
                    None
                };
                let kind = classify_account_kind(same_data, same_owner, same_lamports, la, na);
                diffs.push(AccountDiff {
                    pubkey: *key,
                    lamports_legacy: Some(la.lamports),
                    lamports_new: Some(na.lamports),
                    data_len_legacy: Some(la.data.len()),
                    data_len_new: Some(na.data.len()),
                    owner_legacy: Some(la.owner),
                    owner_new: Some(na.owner),
                    first_diff_byte,
                    kind,
                });
            }
            (Some(la), None) => {
                diffs.push(AccountDiff {
                    pubkey: *key,
                    lamports_legacy: Some(la.lamports),
                    lamports_new: None,
                    data_len_legacy: Some(la.data.len()),
                    data_len_new: None,
                    owner_legacy: Some(la.owner),
                    owner_new: None,
                    first_diff_byte: None,
                    kind: "missing-in-new".to_string(),
                });
            }
            (None, Some(na)) => {
                diffs.push(AccountDiff {
                    pubkey: *key,
                    lamports_legacy: None,
                    lamports_new: Some(na.lamports),
                    data_len_legacy: None,
                    data_len_new: Some(na.data.len()),
                    owner_legacy: None,
                    owner_new: Some(na.owner),
                    first_diff_byte: None,
                    kind: "missing-in-legacy".to_string(),
                });
            }
            (None, None) => {}
        }
    }
    diffs
}

fn classify_account_kind(
    same_data: bool,
    same_owner: bool,
    same_lamports: bool,
    la: &AccountState,
    na: &AccountState,
) -> String {
    let mut parts = Vec::new();
    if !same_data {
        if la.data.len() != na.data.len() {
            parts.push(format!(
                "data-len({}≠{})",
                la.data.len(),
                na.data.len()
            ));
        } else {
            parts.push("data".to_string());
        }
    }
    if !same_owner {
        parts.push("owner".to_string());
    }
    if !same_lamports {
        parts.push(format!("lamports({}≠{})", la.lamports, na.lamports));
    }
    parts.join("+")
}

// ---------------------------------------------------------------------------
// Shared prepare + execute primitives
// ---------------------------------------------------------------------------

struct PreparedState {
    fork: Fork,
    resolved: Vec<ResolvedAccount>,
    message: TxMessage,
    payer_bytes: [u8; 64],
}

impl PreparedState {
    fn payer(&self) -> Keypair {
        Keypair::from_bytes(&self.payer_bytes).expect("valid keypair bytes")
    }
}

enum PrepareOutcome {
    Skipped(String),
    Ready(PreparedState),
}

fn prepare_replay_state<T: Transport>(
    rpc: &RpcClient<T>,
    tx: TxResponse,
    options: &ReplayOptions,
) -> Result<PrepareOutcome> {
    let message = tx.transaction.message;
    let has_alt = !message.address_table_lookups.is_empty();

    if options.skip_alt && has_alt {
        return Ok(PrepareOutcome::Skipped(
            "transaction uses an Address Lookup Table and skip_alt=true".to_string(),
        ));
    }

    let static_keys: Vec<Pubkey> = message
        .account_keys
        .iter()
        .map(|k| Pubkey::from_str(k).map_err(|e| anyhow!("decode key {k}: {e}")))
        .collect::<Result<_>>()?;

    // Resolve ALTs.
    let mut lookup_addresses: Vec<Vec<Pubkey>> = Vec::with_capacity(message.address_table_lookups.len());
    if has_alt {
        let alt_keys: Vec<Pubkey> = message
            .address_table_lookups
            .iter()
            .map(|l| Pubkey::from_str(&l.account_key).map_err(|e| anyhow!("decode ALT key: {e}")))
            .collect::<Result<_>>()?;
        let alt_accounts = rpc.get_multiple_accounts(&alt_keys)?;
        for (lookup, account) in message.address_table_lookups.iter().zip(alt_accounts.into_iter()) {
            let account = account
                .ok_or_else(|| anyhow!("ALT {} not found on chain", lookup.account_key))?;
            let addresses = decode_lookup_table(&account.data)
                .with_context(|| format!("decoding ALT {}", lookup.account_key))?;
            lookup_addresses.push(addresses);
        }
    }

    let lookup_inputs: Vec<LookupInput> = message
        .address_table_lookups
        .iter()
        .zip(lookup_addresses.iter())
        .map(|(lookup, addresses)| LookupInput {
            addresses,
            writable_indexes: &lookup.writable_indexes,
            readonly_indexes: &lookup.readonly_indexes,
        })
        .collect();

    let resolved = resolve_account_list(
        &static_keys,
        message.header.num_required_signatures,
        message.header.num_readonly_signed_accounts,
        message.header.num_readonly_unsigned_accounts,
        &lookup_inputs,
    )?;

    let all_keys: Vec<Pubkey> = resolved.iter().map(|r| r.key).collect();
    let mut fork = Fork::new();
    fork.extend_from_rpc(rpc, &all_keys)?;
    fork.resolve_programdata(rpc)?;

    // One synthetic payer reused across executions so PDA seeds that depend
    // on the payer derive the same way for legacy and new builds.
    let payer = Keypair::new();
    let payer_bytes: [u8; 64] = payer
        .to_bytes()
        .as_slice()
        .try_into()
        .map_err(|_| anyhow!("Keypair::to_bytes returned wrong length"))?;

    Ok(PrepareOutcome::Ready(PreparedState {
        fork,
        resolved,
        message,
        payer_bytes,
    }))
}

fn execute_replay(
    state: &PreparedState,
    overrides: &HashMap<Pubkey, Vec<u8>>,
) -> (ReplayResult, Vec<ApplyWarning>) {
    let mut svm = LiteSVM::new();
    let warnings = state.fork.apply_with_overrides(&mut svm, overrides);

    let payer = state.payer();
    if let Err(e) = svm.airdrop(&payer.pubkey(), 10_000_000_000) {
        return (
            ReplayResult::Failed {
                reason: format!("airdrop synthetic payer: {e:?}"),
            },
            warnings,
        );
    }

    let instructions = match decode_instructions(&state.message, &state.resolved, &payer.pubkey()) {
        Ok(ix) => ix,
        Err(e) => {
            return (
                ReplayResult::Failed {
                    reason: format!("rebuild instructions: {e}"),
                },
                warnings,
            )
        }
    };

    let blockhash = svm.latest_blockhash();
    let new_tx = Transaction::new_signed_with_payer(
        &instructions,
        Some(&payer.pubkey()),
        &[&payer],
        blockhash,
    );

    let send_result = svm.send_transaction(new_tx);

    // Snapshot every writable account's post-tx state. This is the
    // load-bearing signal for two-build diff: identical metadata means
    // nothing if the bytes written to user accounts differ.
    let mut post_state = std::collections::BTreeMap::new();
    for resolved in state.resolved.iter().filter(|r| r.is_writable) {
        if let Some(account) = svm.get_account(&resolved.key) {
            post_state.insert(
                resolved.key,
                AccountState {
                    lamports: account.lamports,
                    data: account.data,
                    owner: account.owner,
                },
            );
        }
    }

    let result = match send_result {
        Ok(meta) => ReplayResult::Executed {
            success: true,
            compute_units_consumed: meta.compute_units_consumed,
            logs: meta.logs,
            error: None,
            post_state,
        },
        Err(failed) => ReplayResult::Executed {
            success: false,
            compute_units_consumed: failed.meta.compute_units_consumed,
            logs: failed.meta.logs,
            error: Some(format!("{:?}", failed.err)),
            post_state,
        },
    };

    (result, warnings)
}

fn extract_chain_outcome(tx: &TxResponse) -> ChainOutcome {
    match &tx.meta {
        Some(meta) => ChainOutcome {
            success: meta.err.is_none(),
            error: meta.err.as_ref().map(|e| e.to_string()),
            compute_units_consumed: meta.compute_units_consumed,
            log_count: meta.log_messages.as_ref().map(|l| l.len()).unwrap_or(0),
        },
        None => ChainOutcome {
            success: false,
            error: Some("no transaction meta".to_string()),
            compute_units_consumed: None,
            log_count: 0,
        },
    }
}

fn decode_instructions(
    message: &TxMessage,
    resolved: &[ResolvedAccount],
    new_payer: &Pubkey,
) -> Result<Vec<Instruction>> {
    let mut out = Vec::with_capacity(message.instructions.len());
    for ix in &message.instructions {
        let program_id_index = ix.program_id_index as usize;
        let program_id = resolved
            .get(program_id_index)
            .ok_or_else(|| anyhow!("program_id_index {program_id_index} out of bounds"))?
            .key;

        let accounts = ix
            .accounts
            .iter()
            .map(|&i| {
                let idx = i as usize;
                let acc = resolved
                    .get(idx)
                    .ok_or_else(|| anyhow!("account index {idx} out of bounds"))?;
                let key = if idx == 0 { *new_payer } else { acc.key };
                Ok(AccountMeta {
                    pubkey: key,
                    is_signer: acc.is_signer,
                    is_writable: acc.is_writable,
                })
            })
            .collect::<Result<Vec<_>>>()?;

        let data = bs58::decode(&ix.data)
            .into_vec()
            .with_context(|| format!("decode instruction data bs58 for {}", program_id))?;

        out.push(Instruction { program_id, accounts, data });
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// Divergence detectors
// ---------------------------------------------------------------------------

fn compute_divergence(mainnet: &ChainOutcome, replay: &ReplayResult) -> Vec<String> {
    let mut diffs = Vec::new();
    match replay {
        ReplayResult::Skipped { .. } => {}
        ReplayResult::Failed { reason } => {
            diffs.push(format!("replay-failed:{reason}"));
        }
        ReplayResult::Executed {
            success,
            compute_units_consumed,
            error,
            ..
        } => {
            if *success != mainnet.success {
                diffs.push(format!(
                    "success:mainnet={} replay={}",
                    mainnet.success, success
                ));
            }
            if let (Some(m_cu), Some(r_cu)) =
                (mainnet.compute_units_consumed, Some(*compute_units_consumed))
            {
                if m_cu != r_cu {
                    diffs.push(format!("cu:mainnet={m_cu} replay={r_cu}"));
                }
            }
            if let (Some(m_err), Some(r_err)) = (&mainnet.error, error) {
                if m_err != r_err {
                    diffs.push(format!("error:mainnet={m_err} replay={r_err}"));
                }
            } else if mainnet.error.is_some() != error.is_some() {
                diffs.push(format!(
                    "error-presence:mainnet={} replay={}",
                    mainnet.error.is_some(),
                    error.is_some()
                ));
            }
        }
    }
    diffs
}

fn diff_results(legacy: &ReplayResult, new: &ReplayResult) -> Vec<String> {
    let mut diffs = Vec::new();
    match (legacy, new) {
        (ReplayResult::Skipped { .. }, ReplayResult::Skipped { .. }) => {}
        (
            ReplayResult::Executed {
                success: ls,
                compute_units_consumed: lcu,
                error: le,
                logs: ll,
                ..
            },
            ReplayResult::Executed {
                success: ns,
                compute_units_consumed: ncu,
                error: ne,
                logs: nl,
                ..
            },
        ) => {
            if ls != ns {
                diffs.push(format!("success:legacy={ls} new={ns}"));
            }
            if lcu != ncu {
                diffs.push(format!("cu:legacy={lcu} new={ncu}"));
            }
            match (le, ne) {
                (Some(l), Some(n)) if l != n => {
                    diffs.push(format!("error:legacy={l} new={n}"));
                }
                (Some(_), None) | (None, Some(_)) => {
                    diffs.push(format!(
                        "error-presence:legacy={} new={}",
                        le.is_some(),
                        ne.is_some()
                    ));
                }
                _ => {}
            }
            if ll.len() != nl.len() {
                diffs.push(format!("log-count:legacy={} new={}", ll.len(), nl.len()));
            }
        }
        (other_l, other_n) => {
            diffs.push(format!(
                "kind:legacy={} new={}",
                result_kind(other_l),
                result_kind(other_n)
            ));
        }
    }
    diffs
}

fn result_kind(r: &ReplayResult) -> &'static str {
    match r {
        ReplayResult::Executed { .. } => "executed",
        ReplayResult::Skipped { .. } => "skipped",
        ReplayResult::Failed { .. } => "failed",
    }
}
