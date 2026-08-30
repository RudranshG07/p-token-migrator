//! Offline tests for the replay crate. They exercise the JSON-RPC plumbing,
//! the message decoder, and the divergence detector via a mock transport so
//! the suite never hits the network.

use anyhow::Result;
use replay::rpc::{RpcClient, Transport};
use replay::{ReplayOptions, ReplayReport};
use serde_json::{json, Value};
use solana_sdk::pubkey::Pubkey;
use std::collections::HashMap;
use std::str::FromStr;
use std::sync::Mutex;

#[derive(Default)]
struct MockTransport {
    inner: Mutex<HashMap<String, Vec<Value>>>,
}

impl MockTransport {
    fn push(&self, method: &str, response: Value) {
        self.inner
            .lock()
            .unwrap()
            .entry(method.to_string())
            .or_default()
            .push(response);
    }
}

impl Transport for MockTransport {
    fn call(&self, method: &str, _params: Value) -> Result<Value> {
        let mut guard = self.inner.lock().unwrap();
        let queue = guard
            .get_mut(method)
            .ok_or_else(|| anyhow::anyhow!("no canned response for method {method}"))?;
        if queue.is_empty() {
            anyhow::bail!("no remaining canned responses for {method}");
        }
        Ok(queue.remove(0))
    }
}

#[test]
fn rpc_decodes_signatures_for_address() {
    let mock = MockTransport::default();
    mock.push(
        "getSignaturesForAddress",
        json!([
            { "signature": "abc", "slot": 100, "err": null, "blockTime": 1234 },
            { "signature": "def", "slot": 101, "err": null }
        ]),
    );
    let rpc = RpcClient::new(mock);
    let key = Pubkey::from_str("TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA").unwrap();
    let got = rpc.get_signatures_for_address(&key, 2).unwrap();
    assert_eq!(got.len(), 2);
    assert_eq!(got[0].signature, "abc");
    assert_eq!(got[1].slot, 101);
}

#[test]
fn rpc_decodes_account_info() {
    let mock = MockTransport::default();
    let key = Pubkey::from_str("11111111111111111111111111111111").unwrap();
    mock.push(
        "getMultipleAccounts",
        json!({
            "context": { "slot": 1 },
            "value": [{
                "lamports": 1_000_000,
                "owner": "11111111111111111111111111111111",
                "executable": false,
                "rentEpoch": 0,
                "data": ["", "base64"],
            }]
        }),
    );
    let rpc = RpcClient::new(mock);
    let accounts = rpc.get_multiple_accounts(&[key]).unwrap();
    assert_eq!(accounts.len(), 1);
    let account = accounts[0].as_ref().unwrap();
    assert_eq!(account.lamports, 1_000_000);
    assert_eq!(account.owner, key);
    assert!(account.data.is_empty());
}

#[test]
fn replay_skips_alt_when_skip_alt_is_true() {
    // The default is to resolve ALTs; this test exercises the opt-in
    // skip behavior used by callers that want to focus on legacy-only txs.
    let mock = MockTransport::default();
    mock.push(
        "getTransaction",
        json!({
            "slot": 1234,
            "blockTime": 1700000000,
            "version": 0,
            "transaction": {
                "signatures": ["fakeSig"],
                "message": {
                    "accountKeys": ["11111111111111111111111111111111"],
                    "header": {
                        "numRequiredSignatures": 1,
                        "numReadonlySignedAccounts": 0,
                        "numReadonlyUnsignedAccounts": 0
                    },
                    "recentBlockhash": "11111111111111111111111111111111",
                    "instructions": [],
                    "addressTableLookups": [{ "accountKey": "11111111111111111111111111111111", "writableIndexes": [], "readonlyIndexes": [] }]
                }
            },
            "meta": {
                "err": null,
                "computeUnitsConsumed": 0,
                "logMessages": [],
                "fee": 5000
            }
        }),
    );
    let rpc = RpcClient::new(mock);
    let outcome = replay::replay_signature(
        &rpc,
        "fakeSig",
        &ReplayOptions {
            skip_alt: true,
            ..ReplayOptions::default()
        },
    )
    .unwrap();
    match &outcome.replay {
        replay::replay::ReplayResult::Skipped { reason } => {
            assert!(reason.contains("Address Lookup Table"), "got: {reason}");
        }
        other => panic!("expected Skipped, got {other:?}"),
    }
    assert_eq!(outcome.divergence, vec!["skipped:alt"]);
}

#[test]
fn diff_filters_ignored_accounts() {
    use replay::replay::{default_volatile_accounts, diff_post_states, AccountState, ReplayResult};
    use solana_sdk::sysvar;

    // Clock sysvar with different bytes between runs — should be filtered.
    let real_account = solana_sdk::pubkey::Pubkey::new_unique();
    let owner = solana_sdk::pubkey::Pubkey::new_unique();

    let mut legacy_state = std::collections::BTreeMap::new();
    legacy_state.insert(
        sysvar::clock::ID,
        AccountState {
            lamports: 1,
            data: vec![0; 40],
            owner,
        },
    );
    legacy_state.insert(
        real_account,
        AccountState {
            lamports: 100,
            data: vec![1, 2, 3],
            owner,
        },
    );

    let mut new_state = std::collections::BTreeMap::new();
    new_state.insert(
        sysvar::clock::ID,
        AccountState {
            lamports: 1,
            data: vec![99; 40], // very different bytes — but volatile
            owner,
        },
    );
    new_state.insert(
        real_account,
        AccountState {
            lamports: 100,
            data: vec![1, 2, 99],
            owner,
        },
    );

    let legacy = ReplayResult::Executed {
        success: true,
        compute_units_consumed: 1,
        logs: vec![],
        error: None,
        post_state: legacy_state,
    };
    let new = ReplayResult::Executed {
        success: true,
        compute_units_consumed: 1,
        logs: vec![],
        error: None,
        post_state: new_state,
    };

    // With default filter (sysvars excluded), only the real account surfaces.
    let diffs = diff_post_states(&legacy, &new, &default_volatile_accounts());
    assert_eq!(diffs.len(), 1, "expected only the real account diff");
    assert_eq!(diffs[0].pubkey, real_account);

    // With empty filter, the sysvar also surfaces — proving the filter is
    // what's removing it, not some other shortcut.
    let diffs_no_filter = diff_post_states(&legacy, &new, &std::collections::HashSet::new());
    assert_eq!(diffs_no_filter.len(), 2);
}

#[test]
fn account_diff_surfaces_data_lamport_and_owner_changes() {
    use replay::replay::{diff_post_states, AccountState, ReplayResult};
    use solana_sdk::pubkey::Pubkey;

    let untouched = Pubkey::new_unique();
    let lamport_only = Pubkey::new_unique();
    let data_only = Pubkey::new_unique();
    let owner_only = Pubkey::new_unique();
    let owner_a = Pubkey::new_unique();
    let owner_b = Pubkey::new_unique();
    let only_in_new = Pubkey::new_unique();

    let mut legacy_state = std::collections::BTreeMap::new();
    legacy_state.insert(
        untouched,
        AccountState {
            lamports: 1000,
            data: vec![1, 2, 3],
            owner: owner_a,
        },
    );
    legacy_state.insert(
        lamport_only,
        AccountState {
            lamports: 500,
            data: vec![9, 9],
            owner: owner_a,
        },
    );
    legacy_state.insert(
        data_only,
        AccountState {
            lamports: 100,
            data: vec![0xAA, 0xBB, 0xCC, 0xDD],
            owner: owner_a,
        },
    );
    legacy_state.insert(
        owner_only,
        AccountState {
            lamports: 200,
            data: vec![],
            owner: owner_a,
        },
    );

    let mut new_state = std::collections::BTreeMap::new();
    new_state.insert(untouched, legacy_state[&untouched].clone());
    new_state.insert(
        lamport_only,
        AccountState {
            lamports: 501,
            data: vec![9, 9],
            owner: owner_a,
        },
    );
    new_state.insert(
        data_only,
        AccountState {
            lamports: 100,
            data: vec![0xAA, 0xBB, 0xFF, 0xDD],
            owner: owner_a,
        },
    );
    new_state.insert(
        owner_only,
        AccountState {
            lamports: 200,
            data: vec![],
            owner: owner_b,
        },
    );
    new_state.insert(
        only_in_new,
        AccountState {
            lamports: 1,
            data: vec![1],
            owner: owner_a,
        },
    );

    let legacy = ReplayResult::Executed {
        success: true,
        compute_units_consumed: 1000,
        logs: vec![],
        error: None,
        post_state: legacy_state,
    };
    let new = ReplayResult::Executed {
        success: true,
        compute_units_consumed: 1000,
        logs: vec![],
        error: None,
        post_state: new_state,
    };

    let diffs = diff_post_states(&legacy, &new, &std::collections::HashSet::new());
    // 4 diffs: lamport_only, data_only, owner_only, only_in_new.
    // untouched is bytewise identical → not in diffs.
    assert_eq!(diffs.len(), 4, "got diffs: {diffs:#?}");

    let pubkeys: std::collections::HashSet<_> = diffs.iter().map(|d| d.pubkey).collect();
    assert!(pubkeys.contains(&lamport_only));
    assert!(pubkeys.contains(&data_only));
    assert!(pubkeys.contains(&owner_only));
    assert!(pubkeys.contains(&only_in_new));
    assert!(!pubkeys.contains(&untouched));

    let data_diff = diffs.iter().find(|d| d.pubkey == data_only).unwrap();
    assert_eq!(data_diff.first_diff_byte, Some(2), "differing byte at offset 2");
    assert!(data_diff.kind.contains("data"));

    let lamport_diff = diffs.iter().find(|d| d.pubkey == lamport_only).unwrap();
    assert!(lamport_diff.kind.contains("lamports"));
    assert_eq!(lamport_diff.first_diff_byte, None);

    let owner_diff = diffs.iter().find(|d| d.pubkey == owner_only).unwrap();
    assert_eq!(owner_diff.kind, "owner");

    let only_new_diff = diffs.iter().find(|d| d.pubkey == only_in_new).unwrap();
    assert_eq!(only_new_diff.kind, "missing-in-legacy");
}

#[test]
fn build_diff_report_classifies_matched_diverged_skipped() {
    use replay::replay::{BuildDiffOutcome, ChainOutcome, ReplayResult};
    use solana_sdk::pubkey::Pubkey;

    let program = Pubkey::new_unique();
    let outcomes = vec![
        // Match: legacy and new produced identical executed results.
        BuildDiffOutcome {
            signature: "match".into(),
            slot: 1,
            target_program: program,
            mainnet: ChainOutcome {
                success: true,
                error: None,
                compute_units_consumed: Some(1000),
                log_count: 1,
            },
            legacy: ReplayResult::Executed {
                success: true,
                compute_units_consumed: 1000,
                logs: vec!["ok".into()],
                error: None,
                post_state: std::collections::BTreeMap::new(),
            },
            new: ReplayResult::Executed {
                success: true,
                compute_units_consumed: 1000,
                logs: vec!["ok".into()],
                error: None,
                post_state: std::collections::BTreeMap::new(),
            },
            legacy_vs_new: vec![],
            account_diffs: vec![],
            legacy_apply_warnings: vec![],
            new_apply_warnings: vec![],
        },
        // Diverge: legacy succeeded, new failed.
        BuildDiffOutcome {
            signature: "diff".into(),
            slot: 2,
            target_program: program,
            mainnet: ChainOutcome {
                success: true,
                error: None,
                compute_units_consumed: Some(1000),
                log_count: 1,
            },
            legacy: ReplayResult::Executed {
                success: true,
                compute_units_consumed: 1000,
                logs: vec![],
                error: None,
                post_state: std::collections::BTreeMap::new(),
            },
            new: ReplayResult::Executed {
                success: false,
                compute_units_consumed: 500,
                logs: vec![],
                error: Some("err".into()),
                post_state: std::collections::BTreeMap::new(),
            },
            legacy_vs_new: vec![
                "success:legacy=true new=false".into(),
                "cu:legacy=1000 new=500".into(),
            ],
            account_diffs: vec![],
            legacy_apply_warnings: vec![],
            new_apply_warnings: vec![],
        },
        // Both skipped due to ALT.
        BuildDiffOutcome {
            signature: "skip".into(),
            slot: 3,
            target_program: program,
            mainnet: ChainOutcome {
                success: true,
                error: None,
                compute_units_consumed: None,
                log_count: 0,
            },
            legacy: ReplayResult::Skipped { reason: "alt".into() },
            new: ReplayResult::Skipped { reason: "alt".into() },
            legacy_vs_new: vec!["skipped:alt".into()],
            account_diffs: vec![],
            legacy_apply_warnings: vec![],
            new_apply_warnings: vec![],
        },
    ];

    let report = replay::BuildDiffReport::from_outcomes(
        program.to_string(),
        "legacy.so",
        "new.so",
        "https://rpc",
        outcomes,
    );
    assert_eq!(report.total, 3);
    assert_eq!(report.matched, 1);
    assert_eq!(report.diverged, 1);
    assert_eq!(report.skipped, 1);
    assert_eq!(report.legacy_so, "legacy.so");
    assert_eq!(report.new_so, "new.so");
}

#[test]
fn report_summary_counts_match_diverge_skip() {
    use replay::replay::{ChainOutcome, ReplayOutcome, ReplayResult};
    let outcomes = vec![
        ReplayOutcome {
            signature: "match".into(),
            slot: 1,
            mainnet: ChainOutcome {
                success: true,
                error: None,
                compute_units_consumed: Some(1000),
                log_count: 1,
            },
            replay: ReplayResult::Executed {
                success: true,
                compute_units_consumed: 1000,
                logs: vec!["ok".into()],
                error: None,
                post_state: std::collections::BTreeMap::new(),
            },
            divergence: vec![],
        },
        ReplayOutcome {
            signature: "diff".into(),
            slot: 2,
            mainnet: ChainOutcome {
                success: true,
                error: None,
                compute_units_consumed: Some(1000),
                log_count: 1,
            },
            replay: ReplayResult::Executed {
                success: false,
                compute_units_consumed: 500,
                logs: vec![],
                error: Some("err".into()),
                post_state: std::collections::BTreeMap::new(),
            },
            divergence: vec!["success:mainnet=true replay=false".into()],
        },
        ReplayOutcome {
            signature: "skip".into(),
            slot: 3,
            mainnet: ChainOutcome {
                success: true,
                error: None,
                compute_units_consumed: None,
                log_count: 0,
            },
            replay: ReplayResult::Skipped { reason: "alt".into() },
            divergence: vec!["skipped:alt".into()],
        },
    ];
    let report = ReplayReport::from_outcomes("Prog111", "https://rpc", outcomes);
    assert_eq!(report.total, 3);
    assert_eq!(report.matched, 1);
    assert_eq!(report.diverged, 1);
    assert_eq!(report.skipped, 1);
}
