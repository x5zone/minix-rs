//! Pure decode and arithmetic for the DEV driver.
//!
//! C correspondence: the capability-word decode in `sef_cb_init_fresh`
//! (`amddev.c:132-146`), the map-entry value built in `init_map`
//! (`amddev.c:322-336`), the page-alignment gates in `do_add4pci`
//! (`amddev.c:395-410`), and the exclusion-bit arithmetic in `add_range`
//! (`amddev.c:434-446`).

use crate::registers::{
    DEVF_CAP_DOMS_MASK, DEVF_CAP_DOMS_SHIFT, DEVF_CAP_MAPS_MASK, DEVF_CAP_MAPS_SHIFT,
    DEVF_CAP_REV_MASK, DEVF_CAP_REV_SHIFT,
};

/// The three fields of the capability word.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DevCapability {
    /// Number of maps (`DEVF_CAP_MAPS`).
    pub maps: u32,
    /// Number of domains (`DEVF_CAP_DOMS`).
    pub domains: u32,
    /// Revision (`DEVF_CAP_REV`).
    pub revision: u32,
}

/// Decode the capability word (`amddev.c:139-143`).
pub const fn decode_capability(bits: u32) -> DevCapability {
    DevCapability {
        maps: (bits & DEVF_CAP_MAPS_MASK) >> DEVF_CAP_MAPS_SHIFT,
        domains: (bits & DEVF_CAP_DOMS_MASK) >> DEVF_CAP_DOMS_SHIFT,
        revision: (bits & DEVF_CAP_REV_MASK) >> DEVF_CAP_REV_SHIFT,
    }
}

/// The map-entry value the C build programs for map zero
/// (`init_map`, `amddev.c:322-336`: domain one, bus seven, units nine and
/// nine, both single-bit flags clear).
pub const fn init_map_value(domain: u32, bus: u32, unit1: u32, unit0: u32) -> u32 {
    (domain << 26) | (domain << 20) | (bus << 12) | (unit1 << 6) | unit0
}

/// Whether a range passes the page-alignment gates (`do_add4pci`).
pub const fn aligned(start: u64, size: u64, page: u64) -> bool {
    start.is_multiple_of(page) && size.is_multiple_of(page)
}

/// The table bit index for one page (`add_range`: `busaddr / PAGE_SIZE`).
pub const fn page_bit(busaddr: u64, page: u64) -> u64 {
    busaddr / page
}

/// The byte and mask that clear one page's exclusion bit
/// (`table[bit/8] &= ~(1 << (bit % 8))`).
pub const fn clear_mask(bit: u64) -> (usize, u8) {
    ((bit / 8) as usize, !(1u8 << (bit % 8)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_capability_decode() {
        // maps 0x12, domains 0x34, revision 0x56.
        let bits = (0x12 << 16) | (0x34 << 8) | 0x56;
        assert_eq!(
            decode_capability(bits),
            DevCapability {
                maps: 0x12,
                domains: 0x34,
                revision: 0x56
            }
        );
        assert_eq!(
            decode_capability(0),
            DevCapability {
                maps: 0,
                domains: 0,
                revision: 0
            }
        );
    }

    #[test]
    fn test_init_map_value_matches_the_c_build() {
        // dom 1, bus 7, units 9/9: the exact C expression's value.
        assert_eq!(init_map_value(1, 7, 9, 9), 0x0410_7249);
    }

    #[test]
    fn test_alignment_and_clear_arithmetic() {
        assert!(aligned(0x1000, 0x2000, 4096));
        assert!(!aligned(0x1001, 0x2000, 4096));
        assert!(!aligned(0x1000, 0x2001, 4096));
        // The first page maps to byte zero, bit zero.
        assert_eq!(clear_mask(page_bit(0, 4096)), (0, !1u8));
        // Page fifty maps to byte six, bit two.
        assert_eq!(clear_mask(page_bit(50 * 4096, 4096)), (6, !(1u8 << 2)));
    }
}
