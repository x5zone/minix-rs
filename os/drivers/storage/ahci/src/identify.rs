//! Identify data: capacity and cache policy from device words.
//!
//! C correspondence: `ata_id_check` and `atapi_id_check`
//! (`ahci.c:402-655`) plus the write-cache get/set helpers
//! (`gen_get_wcache`, `gen_set_wcache`, `ahci.c:722-775`).
//!
//! Word layout follows the ATA identify contract (little-endian words;
//! capacity in sectors); the service crate owns the register transfer,
//! this module parses the words.

/// Word holding the low half of the sector count (identify contract).
pub const WORD_SECTORS_LO: usize = 100;
/// Word holding the high half of the sector count.
pub const WORD_SECTORS_HI: usize = 101;
/// Minimum words for a usable identify block.
pub const IDENTIFY_WORDS: usize = 256;

/// Parsed identify outcome: capacity plus cache policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IdentifyData {
    /// Total sectors on the device.
    pub sectors: u64,
    /// Write cache enabled.
    pub write_cache: bool,
}

/// Parse an identify block; `None` for short or zero-capacity blocks.
///
/// C: the capacity extraction in `ata_id_check` (`ahci.c:528-...`): a
/// zero capacity means no usable device behind this port.
pub fn parse(words: &[u16], write_cache: bool) -> Option<IdentifyData> {
    if words.len() < IDENTIFY_WORDS {
        return None;
    }
    let low = words[WORD_SECTORS_LO] as u64;
    let high = words[WORD_SECTORS_HI] as u64;
    let sectors = (high << 16) | low;
    if sectors == 0 {
        return None;
    }
    Some(IdentifyData {
        sectors,
        write_cache,
    })
}

/// Bytes on the device (sectors of 512).
pub const fn bytes(sectors: u64) -> u64 {
    sectors * 512
}

#[cfg(test)]
mod tests {
    use super::*;

    fn block(sectors: u64) -> [u16; IDENTIFY_WORDS] {
        let mut words = [0u16; IDENTIFY_WORDS];
        words[WORD_SECTORS_LO] = sectors as u16;
        words[WORD_SECTORS_HI] = (sectors >> 16) as u16;
        words
    }

    #[test]
    fn test_parse_reads_capacity_and_cache() {
        let words = block(0x1234_5678);
        let data = parse(&words, true).unwrap();
        assert_eq!(data.sectors, 0x1234_5678);
        assert!(data.write_cache);
        assert_eq!(bytes(data.sectors), 0x1234_5678 * 512);
    }

    #[test]
    fn test_short_or_empty_blocks_are_refused() {
        assert_eq!(parse(&[0u16; 100], false), None);
        assert_eq!(parse(&block(0), false), None);
    }
}
