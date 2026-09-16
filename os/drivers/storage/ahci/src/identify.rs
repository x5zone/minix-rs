//! Identify data: support gates, capacity and cache policy from device
//! words.
//!
//! C correspondence: `ata_id_check` and `atapi_id_check`
//! (`ahci.c:402-655`) plus the write-cache get/set helpers
//! (`gen_get_wcache`, `gen_set_wcache`, `ahci.c:722-775`).
//!
//! Word layout follows the ATA identify contract (little-endian words;
//! capacity in sectors); the service crate owns the register transfer,
//! this module parses the words.

/// Word holding the general configuration (`ATA_ID_GCAP`, `ahci.h:71`).
pub const WORD_GCAP: usize = 0;
/// Word holding device capabilities (`ATA_ID_CAP`, `ahci.h:80`).
pub const WORD_CAP: usize = 49;
/// Word holding supported features (`ATA_ID_SUP1`, `ahci.h:92`).
pub const WORD_SUP1: usize = 83;
/// First of the four words holding the sector count
/// (`ATA_ID_LBA0`, `ahci.h:100`).
pub const WORD_SECTORS_LO: usize = 100;
/// Last of the four words holding the sector count
/// (`ATA_ID_LBA3`, `ahci.h:103`).
pub const WORD_SECTORS_HI: usize = 103;
/// Minimum words for a usable identify block.
pub const IDENTIFY_WORDS: usize = 256;

/// GCAP: the ATA bit is expected ZERO; a set bit means ATAPI
/// (`ATA_ID_GCAP_ATA_MASK 0x8000`, `ATA_ID_GCAP_ATA 0x0000`).
pub const GCAP_ATA_MASK: u16 = 0x8000;
/// GCAP: removable media devices are refused (`ahci.h:79`).
pub const GCAP_REMOVABLE: u16 = 0x0080;
/// GCAP: incomplete identify responses are refused (`ahci.h:80`).
pub const GCAP_INCOMPLETE: u16 = 0x0004;
/// CAP: DMA must be supported (`ATA_ID_CAP_DMA`, `ahci.h:81`).
pub const CAP_DMA: u16 = 0x0100;
/// CAP: LBA must be supported (`ATA_ID_CAP_LBA`, `ahci.h:82`).
pub const CAP_LBA: u16 = 0x0200;
/// SUP1: word-content validity bits (`ATA_ID_SUP1_VALID_MASK/VALID`,
/// `ahci.h:93-94`).
pub const SUP1_VALID_MASK: u16 = 0xC000;
pub const SUP1_VALID: u16 = 0x4000;
/// SUP1: FLUSH CACHE must be supported (`ahci.h:95`).
pub const SUP1_FLUSH: u16 = 0x1000;
/// SUP1: 48-bit addressing must be supported (`ATA_ID_SUP1_LBA48`,
/// `ahci.h:96`).
pub const SUP1_LBA48: u16 = 0x0400;

/// Parsed identify outcome: capacity plus cache policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IdentifyData {
    /// Total sectors on the device.
    pub sectors: u64,
    /// Write cache enabled.
    pub write_cache: bool,
}

/// Parse an identify block; `None` for unsupported devices or zero
/// capacity.
///
/// C: `ata_id_check` gates before it measures (`ahci.c:533-560`): the
/// device must be ATA (not ATAPI), fixed media, complete response; it must
/// support DMA, LBA, FLUSH CACHE and 48-bit addressing. Only then is the
/// capacity read — from four words (100..=103), so drives larger than two
/// terabytes read whole. Zero capacity still means no usable device.
pub fn parse(words: &[u16], write_cache: bool) -> Option<IdentifyData> {
    if words.len() < IDENTIFY_WORDS {
        return None;
    }
    let gcap = words[WORD_GCAP];
    if gcap & (GCAP_ATA_MASK | GCAP_REMOVABLE | GCAP_INCOMPLETE) != 0 {
        return None;
    }
    let cap = words[WORD_CAP];
    if cap & (CAP_LBA | CAP_DMA) != (CAP_LBA | CAP_DMA) {
        return None;
    }
    let sup1 = words[WORD_SUP1];
    if sup1 & (SUP1_VALID_MASK | SUP1_FLUSH | SUP1_LBA48)
        != (SUP1_VALID | SUP1_FLUSH | SUP1_LBA48)
    {
        return None;
    }
    let sectors = ((words[103] as u64) << 48)
        | ((words[102] as u64) << 32)
        | ((words[101] as u64) << 16)
        | (words[100] as u64);
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

    /// An identify block that passes every gate, with the given sector
    /// count spread over all four capacity words.
    fn block(sectors: u64) -> [u16; IDENTIFY_WORDS] {
        let mut words = [0u16; IDENTIFY_WORDS];
        words[WORD_GCAP] = 0x0040; // fixed, complete, non-ATAPI
        words[WORD_CAP] = CAP_LBA | CAP_DMA;
        words[WORD_SUP1] = SUP1_VALID | SUP1_FLUSH | SUP1_LBA48;
        words[WORD_SECTORS_LO] = sectors as u16;
        words[101] = (sectors >> 16) as u16;
        words[102] = (sectors >> 32) as u16;
        words[WORD_SECTORS_HI] = (sectors >> 48) as u16;
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
    fn test_capacity_reads_all_four_words() {
        // A four-terabyte drive: sectors beyond bit 31 were silently
        // truncated by the two-word parse (C reads words 100..=103,
        // ahci.c:561-565).
        let sectors: u64 = 0x0001_2345_6789_ABCD;
        let data = parse(&block(sectors), false).unwrap();
        assert_eq!(data.sectors, sectors);
        assert_eq!(bytes(data.sectors), sectors * 512);
    }

    #[test]
    fn test_gates_refuse_unsupported_devices() {
        // ATAPI device (GCAP bit 15 set).
        let mut words = block(0x1000);
        words[WORD_GCAP] |= GCAP_ATA_MASK;
        assert_eq!(parse(&words, false), None);
        // Removable media.
        let mut words = block(0x1000);
        words[WORD_GCAP] |= GCAP_REMOVABLE;
        assert_eq!(parse(&words, false), None);
        // Incomplete response.
        let mut words = block(0x1000);
        words[WORD_GCAP] |= GCAP_INCOMPLETE;
        assert_eq!(parse(&words, false), None);
        // No DMA.
        let mut words = block(0x1000);
        words[WORD_CAP] = CAP_LBA;
        assert_eq!(parse(&words, false), None);
        // No LBA48.
        let mut words = block(0x1000);
        words[WORD_SUP1] &= !SUP1_LBA48;
        assert_eq!(parse(&words, false), None);
        // SUP1 validity bits not set: the feature word is not trustworthy.
        let mut words = block(0x1000);
        words[WORD_SUP1] &= !SUP1_VALID;
        assert_eq!(parse(&words, false), None);
    }

    #[test]
    fn test_short_or_empty_blocks_are_refused() {
        assert_eq!(parse(&[0u16; 100], false), None);
        assert_eq!(parse(&block(0), false), None);
    }
}
