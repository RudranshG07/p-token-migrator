use anyhow::Result;
use litesvm::LiteSVM;
use solana_sdk::{account::Account, bpf_loader_upgradeable, pubkey::Pubkey};
use std::collections::{HashMap, HashSet};

use crate::loader::{extract_elf, parse_program_account};
use crate::rpc::{RpcClient, Transport};

/// A snapshot of mainnet account state, ready to be installed into a fresh
/// [`LiteSVM`] instance.
#[derive(Debug, Default, Clone)]
pub struct Fork {
    pub accounts: HashMap<Pubkey, Account>,
}

impl Fork {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert(&mut self, key: Pubkey, account: Account) {
        self.accounts.insert(key, account);
    }

    /// Pull account state for `keys` from the cluster and add it to the fork.
    /// Accounts that are missing on chain are recorded as `None` and skipped
    /// when applied (LiteSVM treats those addresses as freshly allocated).
    pub fn extend_from_rpc<T: Transport>(
        &mut self,
        rpc: &RpcClient<T>,
        keys: &[Pubkey],
    ) -> Result<()> {
        if keys.is_empty() {
            return Ok(());
        }
        for chunk in keys.chunks(100) {
            let accounts = rpc.get_multiple_accounts(chunk)?;
            for (key, account) in chunk.iter().zip(accounts.into_iter()) {
                if let Some(acc) = account {
                    self.accounts.insert(*key, acc);
                }
            }
        }
        Ok(())
    }

    /// Walk the snapshot for upgradeable-loader Program accounts. For each
    /// one we don't already have the corresponding programdata account for,
    /// fetch it. This is what lets us deploy user programs into LiteSVM.
    pub fn resolve_programdata<T: Transport>(&mut self, rpc: &RpcClient<T>) -> Result<()> {
        let needed = self.programdata_addresses_to_fetch();
        if needed.is_empty() {
            return Ok(());
        }
        let keys: Vec<Pubkey> = needed.into_iter().collect();
        self.extend_from_rpc(rpc, &keys)
    }

    fn programdata_addresses_to_fetch(&self) -> HashSet<Pubkey> {
        let mut needed = HashSet::new();
        for (_, account) in &self.accounts {
            if !account.executable || account.owner != bpf_loader_upgradeable::ID {
                continue;
            }
            if let Some(pd) = parse_program_account(&account.data) {
                if !self.accounts.contains_key(&pd) {
                    needed.insert(pd);
                }
            }
        }
        needed
    }

    fn programdata_addresses(&self) -> HashSet<Pubkey> {
        let mut set = HashSet::new();
        for (_, account) in &self.accounts {
            if account.executable && account.owner == bpf_loader_upgradeable::ID {
                if let Some(pd) = parse_program_account(&account.data) {
                    set.insert(pd);
                }
            }
        }
        set
    }

    /// Install every account in this fork into the LiteSVM instance.
    /// Equivalent to `apply_with_overrides` with an empty override map.
    pub fn apply(&self, svm: &mut LiteSVM) -> Vec<ApplyWarning> {
        self.apply_with_overrides(svm, &HashMap::new())
    }

    /// Install every account in this fork into the LiteSVM instance, but
    /// for any pubkey present in `overrides` install the supplied ELF
    /// bytes instead of whatever the on-chain snapshot would deploy. This
    /// is the substrate for two-build diff: replay one mainnet tx with the
    /// legacy program, then again with the new program, and diff.
    ///
    ///   - Override entries: always installed via `LiteSVM::add_program`.
    ///   - Upgradeable user programs (no override): programdata's ELF is
    ///     extracted and installed via `add_program`.
    ///   - Legacy BPF programs whose data is the raw ELF: installed directly.
    ///   - Programdata accounts themselves are skipped (consumed via the
    ///     matching program install).
    ///   - Built-ins LiteSVM already bundles are skipped.
    ///   - Everything else gets `set_account`.
    pub fn apply_with_overrides(
        &self,
        svm: &mut LiteSVM,
        overrides: &HashMap<Pubkey, Vec<u8>>,
    ) -> Vec<ApplyWarning> {
        let programdata_set = self.programdata_addresses();
        let mut warnings = Vec::new();

        for (key, account) in &self.accounts {
            if is_native_loader(key) {
                continue;
            }

            // Programdata accounts are consumed via the matching program.
            if programdata_set.contains(key) {
                continue;
            }

            // Caller-supplied override takes precedence over chain state.
            if let Some(elf) = overrides.get(key) {
                svm.add_program(*key, elf);
                continue;
            }

            // Upgradeable-loader user program: install its ELF.
            if account.executable && account.owner == bpf_loader_upgradeable::ID {
                if let Some(pd_addr) = parse_program_account(&account.data) {
                    match self.accounts.get(&pd_addr) {
                        Some(pd_acc) => match extract_elf(&pd_acc.data) {
                            Some(elf) => {
                                svm.add_program(*key, elf);
                                continue;
                            }
                            None => {
                                warnings.push(ApplyWarning {
                                    pubkey: *key,
                                    reason: format!(
                                        "programdata {pd_addr} has unexpected variant"
                                    ),
                                });
                                continue;
                            }
                        },
                        None => {
                            warnings.push(ApplyWarning {
                                pubkey: *key,
                                reason: format!("programdata {pd_addr} not in snapshot"),
                            });
                            continue;
                        }
                    }
                }
            }

            // Legacy BPF: data is the raw ELF.
            if account.executable && looks_like_elf(&account.data) {
                svm.add_program(*key, &account.data);
                continue;
            }

            if let Err(e) = svm.set_account(*key, account.clone()) {
                warnings.push(ApplyWarning {
                    pubkey: *key,
                    reason: format!("{e:?}"),
                });
            }
        }

        // Overrides whose pubkey isn't in the snapshot at all — install
        // them anyway. Useful when the target program is being introduced
        // for the first time and has no on-chain account yet.
        for (key, elf) in overrides {
            if !self.accounts.contains_key(key) && !is_native_loader(key) {
                svm.add_program(*key, elf);
            }
        }

        warnings
    }
}

#[derive(Debug, Clone)]
pub struct ApplyWarning {
    pub pubkey: Pubkey,
    pub reason: String,
}

fn looks_like_elf(bytes: &[u8]) -> bool {
    bytes.len() >= 4 && bytes[..4] == [0x7f, b'E', b'L', b'F']
}

fn is_native_loader(key: &Pubkey) -> bool {
    use solana_sdk::bpf_loader;
    use solana_sdk::bpf_loader_deprecated;
    use solana_sdk::bpf_loader_upgradeable;
    use solana_sdk::system_program;
    if *key == system_program::ID
        || *key == bpf_loader::ID
        || *key == bpf_loader_deprecated::ID
        || *key == bpf_loader_upgradeable::ID
    {
        return true;
    }
    use std::str::FromStr;
    let known = [
        "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA",
        "TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb",
        "ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL",
        "ComputeBudget111111111111111111111111111111",
        "Vote111111111111111111111111111111111111111",
        "Stake11111111111111111111111111111111111111",
        "Sysvar1111111111111111111111111111111111111",
        "AddressLookupTab1e1111111111111111111111111",
    ];
    known
        .iter()
        .any(|s| Pubkey::from_str(s).map(|p| p == *key).unwrap_or(false))
}
