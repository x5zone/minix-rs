//! Routing table policy: prefix assumption, lookup order, miss handling.
//!
//! C correspondence: `minix3/minix/net/lwip/rttree.c` (744 lines) with the
//! route management in `minix3/minix/net/lwip/route.c` (1654 lines) and the
//! gateway hooks in the lightweight stack glue. Table storage, message
//! traffic, and stack overrides stay in the service binary. This module owns
//! the portion that can be decided from numbers alone: which prefix lengths
//! are valid, how bit positions map to bytes, and which lookup outcome
//! applies.
//!
//! The tree assumes every mask can be expressed as a prefix length: some
//! leading number of set bits followed by all clear bits. Entries live at
//! the node matching their bit count, and that node may still have children
//! that refine the prefix. There are no pure leaf-or-internal nodes, only
//! data nodes (with an entry, zero to two children) and link nodes (without
//! an entry, exactly two children).

/// Largest version 4 prefix length (32 address bits).
pub const VERSION4_BITS: u8 = 32;

/// Largest version 6 prefix length (128 address bits).
pub const VERSION6_BITS: u8 = 128;

/// Byte index holding a bit position (`RTTREE_BITS_TO_BYTE`, `rttree.c:38`).
pub fn bit_to_byte(bit: u32) -> usize {
    (bit >> 3) as usize
}

/// Shift selecting a bit inside its byte (`RTTREE_BITS_TO_SHIFT`,
/// `rttree.c:39`: bit 0 is the most significant bit of byte 0).
pub fn bit_to_shift(bit: u32) -> u32 {
    7 - (bit & 7)
}

/// Bytes needed to hold a bit count (`RTTREE_BITS_TO_BYTES`, `rttree.c:40`).
pub fn bits_to_bytes(bits: u32) -> usize {
    ((bits + 7) >> 3) as usize
}

/// Whether a version 4 prefix length is valid.
pub fn version4_prefix_allowed(prefix: u8) -> bool {
    prefix <= VERSION4_BITS
}

/// Whether a version 6 prefix length is valid.
pub fn version6_prefix_allowed(prefix: u8) -> bool {
    prefix <= VERSION6_BITS
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bit_mapping_matches_tree_macros() {
        assert_eq!(bit_to_byte(0), 0);
        assert_eq!(bit_to_byte(8), 1);
        assert_eq!(bit_to_shift(0), 7);
        assert_eq!(bit_to_shift(7), 0);
        assert_eq!(bit_to_shift(8), 7);
        assert_eq!(bits_to_bytes(0), 0);
        assert_eq!(bits_to_bytes(1), 1);
        assert_eq!(bits_to_bytes(8), 1);
        assert_eq!(bits_to_bytes(9), 2);
    }

    #[test]
    fn test_prefix_bounds_match_address_sizes() {
        assert_eq!(VERSION4_BITS, 32);
        assert_eq!(VERSION6_BITS, 128);
        assert!(version4_prefix_allowed(32));
        assert!(!version4_prefix_allowed(33));
        assert!(version6_prefix_allowed(128));
    }
}
