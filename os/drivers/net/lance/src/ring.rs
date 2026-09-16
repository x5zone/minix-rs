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
///
/// The C search skips table entry zero, so `Lance7990` is never a search
/// hit — it is the fallback identity the C probe reports when the version
/// gate fails (`lance_probe`, `lance.c:704`). This module models that as
/// `None` and lets the caller choose the fallback name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChipVersion {
    /// Original LANCE 7990 (the C fallback identity, not a search hit).
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

/// Identify a chip from the thirty-two-bit version in CSR88/CSR89.
///
/// C: `lance_probe` (`lance.c:707-722`) — a two-field algorithm. The low
/// twelve bits must read exactly `0x003`, otherwise the version is not
/// trusted (the probe falls back to the ancient-LANCE identity). The
/// identifier is then the next sixteen bits, `(version >> 12) & 0xffff`,
/// matched against `chip_table` entries one through seven; identifier
/// zero and anything unknown land on the terminator entry "PCnet
/// (unknown)".
pub fn identify_chip(version: u32) -> Option<ChipVersion> {
    if version & 0xFFF != 0x003 {
        return None;
    }
    match (version >> 12) & 0xFFFF {
        0x0003 => Some(ChipVersion::Am79C960),
        0x2260 => Some(ChipVersion::Am79C961),
        0x2420 => Some(ChipVersion::PcnetPci),
        0x2430 => Some(ChipVersion::Pcnet32),
        0x2621 => Some(ChipVersion::Am79C970A),
        0x2625 => Some(ChipVersion::Am79C973),
        0x2626 => Some(ChipVersion::Am79C978),
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
        // Full register values: low twelve bits 0x003, identifier in bits
        // 12..=27. Written as (id << 12) | 0x003 — e.g. 79C960 reads as
        // 0x0000_3003.
        assert_eq!(
            identify_chip((0x0003 << 12) | 0x003),
            Some(ChipVersion::Am79C960)
        );
        assert_eq!(
            identify_chip((0x2260 << 12) | 0x003),
            Some(ChipVersion::Am79C961)
        );
        assert_eq!(
            identify_chip((0x2420 << 12) | 0x003),
            Some(ChipVersion::PcnetPci)
        );
        assert_eq!(
            identify_chip((0x2430 << 12) | 0x003),
            Some(ChipVersion::Pcnet32)
        );
        assert_eq!(
            identify_chip((0x2621 << 12) | 0x003),
            Some(ChipVersion::Am79C970A)
        );
        assert_eq!(
            identify_chip((0x2625 << 12) | 0x003),
            Some(ChipVersion::Am79C973)
        );
        assert_eq!(
            identify_chip((0x2626 << 12) | 0x003),
            Some(ChipVersion::Am79C978)
        );
    }

    #[test]
    fn test_gate_and_unknown_identifiers_are_refused() {
        // Gate fails: low twelve bits are not 0x003 (lance.c:711-714).
        assert_eq!(identify_chip(0x0000_0001), None);
        assert_eq!(identify_chip(0x0000_0000), None);
        // Gate passes but the identifier is zero or unknown: the search
        // walks off into the terminator entry "PCnet (unknown)"
        // (lance.c:715-720).
        assert_eq!(identify_chip(0x0000_0003), None);
        assert_eq!(identify_chip(0x5000_0003), None);
    }
}
