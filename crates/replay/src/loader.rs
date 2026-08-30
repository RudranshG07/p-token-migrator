//! BPF Loader Upgradeable account decoding.
//!
//! An upgradeable Solana program has two on-chain accounts:
//!   - The *program* account, `executable = true`, owner = BPF Loader
//!     Upgradeable. Its data deserializes to `UpgradeableLoaderState::Program`
//!     which is a 4-byte variant tag followed by the 32-byte
//!     `programdata_address`.
//!   - The *programdata* account, `executable = false`, owner = BPF Loader
//!     Upgradeable. Its data is `UpgradeableLoaderState::ProgramData` — a
//!     fixed 45-byte metadata header followed by the raw ELF bytes.
//!
//! 45 bytes is the value of `UpgradeableLoaderState::size_of_programdata_metadata()`
//! in solana-program. It is fixed regardless of whether the upgrade
//! authority is `Some` or `None`.

const PROGRAM_VARIANT_TAG: u32 = 2;
const PROGRAMDATA_VARIANT_TAG: u32 = 3;
const PROGRAM_ACCOUNT_SIZE: usize = 36; // tag(4) + Pubkey(32)
pub const PROGRAMDATA_METADATA_SIZE: usize = 45;

use solana_sdk::pubkey::Pubkey;

/// If `data` is the body of an upgradeable-loader *Program* account, return
/// the address of the matching programdata account.
pub fn parse_program_account(data: &[u8]) -> Option<Pubkey> {
    if data.len() < PROGRAM_ACCOUNT_SIZE {
        return None;
    }
    let tag = u32::from_le_bytes(data[0..4].try_into().ok()?);
    if tag != PROGRAM_VARIANT_TAG {
        return None;
    }
    let bytes: [u8; 32] = data[4..36].try_into().ok()?;
    Some(Pubkey::new_from_array(bytes))
}

/// If `data` is the body of an upgradeable-loader *ProgramData* account,
/// return the raw ELF bytes that follow the 45-byte metadata header.
pub fn extract_elf(programdata: &[u8]) -> Option<&[u8]> {
    if programdata.len() < PROGRAMDATA_METADATA_SIZE {
        return None;
    }
    let tag = u32::from_le_bytes(programdata[0..4].try_into().ok()?);
    if tag != PROGRAMDATA_VARIANT_TAG {
        return None;
    }
    Some(&programdata[PROGRAMDATA_METADATA_SIZE..])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_program_variant() {
        let pd_address = Pubkey::new_unique();
        let mut data = vec![0u8; PROGRAM_ACCOUNT_SIZE];
        data[0..4].copy_from_slice(&PROGRAM_VARIANT_TAG.to_le_bytes());
        data[4..36].copy_from_slice(pd_address.as_ref());
        assert_eq!(parse_program_account(&data), Some(pd_address));
    }

    #[test]
    fn rejects_wrong_variant() {
        let mut data = vec![0u8; PROGRAM_ACCOUNT_SIZE];
        data[0..4].copy_from_slice(&5u32.to_le_bytes());
        assert_eq!(parse_program_account(&data), None);
    }

    #[test]
    fn rejects_short_data() {
        assert_eq!(parse_program_account(&[1, 2, 3]), None);
    }

    #[test]
    fn extracts_elf_after_metadata() {
        let elf_bytes = [0x7f, b'E', b'L', b'F', 0x01, 0x02, 0x03];
        let mut data = vec![0u8; PROGRAMDATA_METADATA_SIZE];
        data[0..4].copy_from_slice(&PROGRAMDATA_VARIANT_TAG.to_le_bytes());
        data.extend_from_slice(&elf_bytes);
        assert_eq!(extract_elf(&data), Some(&elf_bytes[..]));
    }

    #[test]
    fn extracts_elf_rejects_wrong_variant() {
        let mut data = vec![0u8; PROGRAMDATA_METADATA_SIZE + 4];
        data[0..4].copy_from_slice(&1u32.to_le_bytes());
        assert_eq!(extract_elf(&data), None);
    }
}
