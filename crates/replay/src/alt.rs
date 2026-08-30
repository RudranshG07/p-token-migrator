//! Address Lookup Table parsing and v0 message resolution.
//!
//! ALT account data layout (per the Solana wire format):
//!   - 56 bytes of `LookupTableMeta` (variant tag, deactivation_slot,
//!     last_extended_slot, last_extended_slot_start_index, authority option,
//!     padding)
//!   - Repeated packed 32-byte pubkeys, `addresses_len = (data.len() - 56) / 32`
//!
//! Account-list resolution for a v0 message follows the spec:
//!   1. All static `accountKeys` keep their original order and header-derived
//!      writable/signer flags.
//!   2. Then, for each `addressTableLookups` entry (in order), every index in
//!      `writableIndexes` is appended as writable, non-signer.
//!   3. Then, for each entry (same order), every `readonlyIndexes` index is
//!      appended as read-only, non-signer.

use anyhow::{anyhow, bail, Result};
use solana_sdk::pubkey::Pubkey;

pub const LOOKUP_TABLE_META_SIZE: usize = 56;

/// Decode an Address Lookup Table account's raw data into its address list.
pub fn decode_lookup_table(data: &[u8]) -> Result<Vec<Pubkey>> {
    if data.len() < LOOKUP_TABLE_META_SIZE {
        bail!(
            "lookup table data too short: {} bytes (need at least {})",
            data.len(),
            LOOKUP_TABLE_META_SIZE
        );
    }
    let rest = &data[LOOKUP_TABLE_META_SIZE..];
    if rest.len() % 32 != 0 {
        bail!(
            "lookup table address section is not a multiple of 32 bytes: {} bytes",
            rest.len()
        );
    }
    let mut out = Vec::with_capacity(rest.len() / 32);
    for chunk in rest.chunks_exact(32) {
        let arr: [u8; 32] = chunk.try_into().expect("chunk_exact(32) yields 32 bytes");
        out.push(Pubkey::new_from_array(arr));
    }
    Ok(out)
}

#[derive(Debug, Clone)]
pub struct ResolvedAccount {
    pub key: Pubkey,
    pub is_signer: bool,
    pub is_writable: bool,
    pub from_lookup: bool,
}

#[derive(Debug, Clone)]
pub struct LookupInput<'a> {
    pub addresses: &'a [Pubkey],
    pub writable_indexes: &'a [u8],
    pub readonly_indexes: &'a [u8],
}

/// Combine the static account keys (with header flags) and resolved ALT
/// lookups into the final ordered list the runtime treats as the tx's
/// accounts. Static keys come first, then writable ALT entries (in lookup
/// order), then readonly ALT entries.
pub fn resolve_account_list(
    static_keys: &[Pubkey],
    num_required_signatures: u8,
    num_readonly_signed_accounts: u8,
    num_readonly_unsigned_accounts: u8,
    lookups: &[LookupInput<'_>],
) -> Result<Vec<ResolvedAccount>> {
    let num_signers = num_required_signatures as usize;
    let num_writable_signers = num_signers
        .saturating_sub(num_readonly_signed_accounts as usize);
    let num_writable_unsigned = static_keys
        .len()
        .saturating_sub(num_signers)
        .saturating_sub(num_readonly_unsigned_accounts as usize);

    let mut out = Vec::with_capacity(static_keys.len());
    for (idx, key) in static_keys.iter().enumerate() {
        let is_signer = idx < num_signers;
        let is_writable = if idx < num_writable_signers {
            true
        } else if idx < num_signers {
            false
        } else if idx < num_signers + num_writable_unsigned {
            true
        } else {
            false
        };
        out.push(ResolvedAccount {
            key: *key,
            is_signer,
            is_writable,
            from_lookup: false,
        });
    }

    // Writable lookups across all tables, then readonly across all tables.
    for input in lookups {
        for &idx in input.writable_indexes {
            let key = *input
                .addresses
                .get(idx as usize)
                .ok_or_else(|| anyhow!("writable lookup index {idx} out of bounds"))?;
            out.push(ResolvedAccount {
                key,
                is_signer: false,
                is_writable: true,
                from_lookup: true,
            });
        }
    }
    for input in lookups {
        for &idx in input.readonly_indexes {
            let key = *input
                .addresses
                .get(idx as usize)
                .ok_or_else(|| anyhow!("readonly lookup index {idx} out of bounds"))?;
            out.push(ResolvedAccount {
                key,
                is_signer: false,
                is_writable: false,
                from_lookup: true,
            });
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decode_roundtrip() {
        let addr1 = Pubkey::new_unique();
        let addr2 = Pubkey::new_unique();
        let addr3 = Pubkey::new_unique();
        let mut data = vec![0u8; LOOKUP_TABLE_META_SIZE];
        data.extend_from_slice(addr1.as_ref());
        data.extend_from_slice(addr2.as_ref());
        data.extend_from_slice(addr3.as_ref());
        let decoded = decode_lookup_table(&data).unwrap();
        assert_eq!(decoded, vec![addr1, addr2, addr3]);
    }

    #[test]
    fn decode_rejects_short_data() {
        assert!(decode_lookup_table(&[0u8; 10]).is_err());
    }

    #[test]
    fn decode_rejects_misaligned_data() {
        let mut data = vec![0u8; LOOKUP_TABLE_META_SIZE + 33];
        // Fill so the test fails for size, not malformed pubkey.
        for b in &mut data[LOOKUP_TABLE_META_SIZE..] {
            *b = 1;
        }
        assert!(decode_lookup_table(&data).is_err());
    }

    #[test]
    fn resolve_static_only() {
        let payer = Pubkey::new_unique();
        let writable = Pubkey::new_unique();
        let readonly = Pubkey::new_unique();
        // 1 signer (payer), 0 readonly signed, 1 readonly unsigned (readonly).
        let resolved =
            resolve_account_list(&[payer, writable, readonly], 1, 0, 1, &[]).unwrap();
        assert_eq!(resolved.len(), 3);
        assert!(resolved[0].is_signer && resolved[0].is_writable);
        assert!(!resolved[1].is_signer && resolved[1].is_writable);
        assert!(!resolved[2].is_signer && !resolved[2].is_writable);
    }

    #[test]
    fn resolve_with_lookups_orders_writable_then_readonly() {
        let payer = Pubkey::new_unique();
        let alt1_addrs: Vec<Pubkey> = (0..4).map(|_| Pubkey::new_unique()).collect();
        let alt2_addrs: Vec<Pubkey> = (0..4).map(|_| Pubkey::new_unique()).collect();
        let resolved = resolve_account_list(
            &[payer],
            1,
            0,
            0,
            &[
                LookupInput {
                    addresses: &alt1_addrs,
                    writable_indexes: &[0, 2],
                    readonly_indexes: &[1],
                },
                LookupInput {
                    addresses: &alt2_addrs,
                    writable_indexes: &[3],
                    readonly_indexes: &[0, 2],
                },
            ],
        )
        .unwrap();

        // Expected: [payer, alt1[0], alt1[2], alt2[3], alt1[1], alt2[0], alt2[2]]
        assert_eq!(resolved.len(), 7);
        assert_eq!(resolved[0].key, payer);
        assert_eq!(resolved[1].key, alt1_addrs[0]);
        assert!(resolved[1].is_writable && !resolved[1].is_signer && resolved[1].from_lookup);
        assert_eq!(resolved[2].key, alt1_addrs[2]);
        assert!(resolved[2].is_writable);
        assert_eq!(resolved[3].key, alt2_addrs[3]);
        assert!(resolved[3].is_writable);
        assert_eq!(resolved[4].key, alt1_addrs[1]);
        assert!(!resolved[4].is_writable && !resolved[4].is_signer);
        assert_eq!(resolved[5].key, alt2_addrs[0]);
        assert!(!resolved[5].is_writable);
        assert_eq!(resolved[6].key, alt2_addrs[2]);
        assert!(!resolved[6].is_writable);
    }
}
