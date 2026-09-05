//! Small rings and init block: sixteen entries, chip identity.
//!
//! C correspondence: the ring sizes (`RX_RING_SIZE` and
//! `TX_RING_SIZE`, 16 each, `lance.c:97-106`), the initialization
//! block (`lance_init_block` with mode, physical address, filter,
//! and ring addresses, `lance.c:109`), and the chip table
//! (`chip_table`, `lance.c:63-87`, matching the version read from
//! the chip registers, `lance.c:707-714`).
//!
//! Register writes stay in the service binary; this module owns the
//! pure table half: ring sizes and chip identity matching.

/// Receive ring entries (`RX_RING_SIZE`).
pub const RING_SIZE: usize = 16;

/// Known chip versions (`chip_table`, `lance.c:63-87`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChipVersion {
    /// Original LANCE 7990.
    Lance7990,
    /// 79C960.
    Am79C960,
    /// 79C961.
    Am79C961,
    /// PCnet-PCI 79C970.
    PcnetPci,
    /// PCnet-32.
    Pcnet32,
    /// 79C970A.
    Am79C970A,
    /// 79C973.
    Am79C973,
    /// 79C978.
    Am79C978,
}

/// Match a register version value to a known chip
/// (`lance.c:707-714`, masked with `0xFFF`).
pub fn identify_chip(version: u16) -> Option<ChipVersion> {
    match version & 0x0FFF {
        0x000 => Some(ChipVersion::Lance7990),
        0x003 => Some(ChipVersion::Am79C960),
        0x260 => Some(ChipVersion::Am79C961),
        0x420 => Some(ChipVersion::PcnetPci),
        0x430 => Some(ChipVersion::Pcnet32),
        0x621 => Some(ChipVersion::Am79C970A),
        0x625 => Some(ChipVersion::Am79C973),
        0x626 => Some(ChipVersion::Am79C978),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ring_holds_sixteen_entries() {
        assert_eq!(RING_SIZE, 16);
    }

    #[test]
    fn test_chip_table_matches_known_versions() {
        assert_eq!(identify_chip(0x0000), Some(ChipVersion::Lance7990));
        assert_eq!(identify_chip(0x0003), Some(ChipVersion::Am79C960));
        assert_eq!(identify_chip(0x2260), Some(ChipVersion::Am79C961));
        assert_eq!(identify_chip(0x2420), Some(ChipVersion::PcnetPci));
        assert_eq!(identify_chip(0x2430), Some(ChipVersion::Pcnet32));
        assert_eq!(identify_chip(0x2621), Some(ChipVersion::Am79C970A));
        assert_eq!(identify_chip(0x2625), Some(ChipVersion::Am79C973));
        assert_eq!(identify_chip(0x2626), Some(ChipVersion::Am79C978));
        assert_eq!(identify_chip(0x9999), None);
    }
}
