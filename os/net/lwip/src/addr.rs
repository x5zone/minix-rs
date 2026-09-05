//! Address sizes and selection policy: max size plus label table.
//!
//! C correspondence: the size cap (`SOCKADDR_MAX`, 256 bytes,
//! `sockdriver.h:11`, enforced by `STATIC_SOCKADDR_MAX_ASSERT`,
//! `sockdriver.h:17-19`), and the selection table
//! (`addrpol_table` with nine entries, `addrpol.c:25-40`, longest
//! prefix wins, first hit in descending prefix order), with the
//! label lookup (`addrpol_get_label`, `addrpol.c:54-79`, defaulting
//! to label one when nothing matches, `addrpol.c:78`).
//!
//! Address parsing stays in the service binary; this module owns the
//! pure sizing half: how big an address may be and which label a
//! prefix earns.

/// Largest socket address in bytes (`SOCKADDR_MAX`).
pub const SOCKADDR_MAX: usize = 256;

/// One selection-table row: network prefix, prefix length,
/// priority, label (`addrpol.c:25-40`, descending prefix length,
/// first hit wins).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PolicyRow {
    /// Network prefix as a full 128-bit number (significant bits on
    /// top, the rest zero).
    pub prefix: u128,
    /// Bits of the prefix that count.
    pub prefix_len: u8,
    /// Priority of this row.
    pub priority: u8,
    /// Label of this row.
    pub label: u8,
}

/// Whether an address falls inside one row.
fn row_matches(row: PolicyRow, address: u128) -> bool {
    if row.prefix_len == 0 {
        return true;
    }
    let shift = 128 - row.prefix_len;
    (address >> shift) == (row.prefix >> shift)
}

/// The nine default rows (`addrpol_table`, `addrpol.c:25-40`).
pub const POLICY_TABLE: [PolicyRow; 9] = [
    PolicyRow { prefix: 1, prefix_len: 128, priority: 50, label: 0 },
    PolicyRow { prefix: 0xffff << 32, prefix_len: 96, priority: 35, label: 4 },
    PolicyRow { prefix: 0, prefix_len: 96, priority: 1, label: 3 },
    PolicyRow { prefix: 0x2001_0000 << 96, prefix_len: 32, priority: 5, label: 5 },
    PolicyRow { prefix: 0x2002 << 112, prefix_len: 16, priority: 30, label: 2 },
    PolicyRow { prefix: 0x3ffe << 112, prefix_len: 16, priority: 1, label: 12 },
    PolicyRow { prefix: 0xfec0 << 112, prefix_len: 10, priority: 1, label: 11 },
    PolicyRow { prefix: 0xfc00 << 112, prefix_len: 7, priority: 3, label: 13 },
    PolicyRow { prefix: 0, prefix_len: 0, priority: 40, label: 1 },
];

/// Whether an address of `length` bytes fits the cap.
pub fn address_fits(length: usize) -> bool {
    length <= SOCKADDR_MAX
}

/// Label of one address: first table row whose prefix matches
/// (`addrpol_get_label`, `addrpol.c:54-79`).
pub fn policy_label(address: u128) -> u8 {
    for row in POLICY_TABLE {
        if row_matches(row, address) {
            return row.label;
        }
    }
    1
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_max_size_matches_header() {
        assert_eq!(SOCKADDR_MAX, 256);
        assert!(address_fits(256));
        assert!(!address_fits(257));
        assert!(address_fits(0));
    }

    #[test]
    fn test_table_holds_nine_rows() {
        assert_eq!(POLICY_TABLE.len(), 9);
    }

    #[test]
    fn test_loopback_earns_label_zero() {
        assert_eq!(policy_label(1), 0);
    }

    #[test]
    fn test_mapped_v4_earns_label_four() {
        let mapped = (0xffff_u128 << 32) | 0x0a00_0001;
        assert_eq!(policy_label(mapped), 4);
    }

    #[test]
    fn test_documented_prefix_earns_label_five() {
        assert_eq!(policy_label(0x2001_0000_0000_0000_0000_0000_0000_0001), 5);
    }
}
