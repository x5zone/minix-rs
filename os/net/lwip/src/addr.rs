//! Address sizing, policy labels, scope order, and netmask helpers.
//!
//! C correspondence: the size cap (`SOCKADDR_MAX`, 256 bytes,
//! `minix3/minix/include/minix/sockdriver.h:11`, enforced by
//! `STATIC_SOCKADDR_MAX_ASSERT`, `sockdriver.h:17-19`, instantiated for the
//! three address families in `minix3/minix/net/lwip/lwip.h:37-39`), the
//! address verification and conversion in `minix3/minix/net/lwip/addr.c`
//! (699 lines), and the selection policy in
//! `minix3/minix/net/lwip/addrpol.c` (143 lines).
//!
//! Address parsing itself touches user memory, interface tables, and the
//! lightweight IP stack types, so it stays in the service binary. This module
//! owns the portion that can be decided from numbers alone: how large an
//! address may be, which policy label a 128-bit prefix earns, how scopes
//! order against each other, and whether a netmask is contiguous.

/// Largest socket address in bytes (`SOCKADDR_MAX`, 256).
///
/// The value equals the largest unsigned 8-bit value plus one
/// (`UINT8_MAX + 1`, `sockdriver.h:11`), which is why the socket driver can
/// describe any address length in a single length byte plus one extra step.
pub const SOCKADDR_MAX: usize = 256;

/// Whether an address of `length` bytes fits the cap.
pub fn address_fits(length: usize) -> bool {
    length <= SOCKADDR_MAX
}

/// One selection-table row: network prefix, prefix length, priority, label
/// (`addrpol.c:25-40`, sorted by descending prefix length so the first match
/// is also the longest match).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PolicyRow {
    /// Network prefix as a full 128-bit number in network order (significant
    /// bits at the top, remaining bits zero).
    pub prefix: u128,
    /// How many leading bits of the prefix count.
    pub prefix_len: u8,
    /// Priority of this row (kept for documentation; the current service
    /// uses labels for source selection and does not sort by priority).
    pub priority: u8,
    /// Label returned when this row matches first.
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
///
/// Order matters: longest prefix first. The loopback address (`::1`, value 1)
/// carries label 0, the mapped range carries label 4, the documentation range
/// `2001::/32` carries label 5, and the final catch-all (`::/0`) carries
/// label 1 with priority 40.
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

/// Label of one 128-bit address: first table row whose prefix matches
/// (`addrpol_get_label`, `addrpol.c:54-79`).
///
/// The original asserts an Internet Protocol version 6 address
/// (`addrpol.c:59`) and normalizes each row before comparing
/// (`addrpol.c:66`). Normalization here is the shift comparison in
/// `row_matches`. When nothing matches, the original returns the default
/// label 1 (`addrpol.c:78`); the table above always matches on the last row,
/// so the trailing return only guards a future custom table.
pub fn policy_label(address: u128) -> u8 {
    for row in POLICY_TABLE {
        if row_matches(row, address) {
            return row.label;
        }
    }
    1
}

/// Mask an address down to a prefix length (pure form of `addr_normalize`,
/// `addr.c:582-633`, without the stack address types).
pub fn normalize_prefix(address: u128, prefix_len: u8) -> u128 {
    if prefix_len == 0 {
        return 0;
    }
    if prefix_len >= 128 {
        return address;
    }
    let shift = 128 - prefix_len;
    (address >> shift) << shift
}

/// Count common leading bits of two addresses up to `max`
/// (pure form of `addr_get_common_bits`, `addr.c:640-689`).
pub fn common_bits(first: u128, second: u128, max: u32) -> u32 {
    let mut common = 0_u32;
    let limit = max.min(128);
    let mut bit = 127_i32;
    while common < limit && bit >= 0 {
        let mask = 1_u128 << (bit as u32);
        if (first & mask) != (second & mask) {
            break;
        }
        common += 1;
        bit -= 1;
    }
    common
}

/// Build an Internet Protocol version 6 mapped address from a version 4
/// address word in network order (pure form of `addr_make_v4mapped_v6`,
/// `addr.c:695-699`: `::ffff:a.b.c.d`).
pub fn v4_mapped_to_v6(version4_word: u32) -> u128 {
    (0xffff_u128 << 32) | version4_word as u128
}

// Multicast scope values (`ip6_addr.h:218-227`). Larger means wider.
pub const SCOPE_RESERVED: u8 = 0;
pub const SCOPE_INTERFACE_LOCAL: u8 = 1;
pub const SCOPE_LINK_LOCAL: u8 = 2;
pub const SCOPE_ADMIN_LOCAL: u8 = 4;
pub const SCOPE_SITE_LOCAL: u8 = 5;
pub const SCOPE_ORGANIZATION_LOCAL: u8 = 8;
pub const SCOPE_GLOBAL: u8 = 14;
pub const SCOPE_RESERVED_TOP: u8 = 15;

/// Decide the scope order value for an already classified address
/// (`addrpol_get_scope`, `addrpol.c:90-143`).
///
/// The service binary classifies the address with the stack helpers
/// (`ip6_addr_isglobal`, `ip6_addr_islinklocal`, and so on); this function
/// owns the ordering table so it can be tested without the stack. Order
/// follows the original: version 4 is always global (`addrpol.c:98-99`),
/// then global, then link-local and loopback (`addrpol.c:112-113`), then
/// unique-local (`addrpol.c:123-124`, a deliberate deviation from Request For
/// Comments 6724 Section 3.1 documented at `addrpol.c:115-122`: unique-local
/// sorts below global so a deprecated global destination does not select a
/// preferred unique-local source and break return traffic), then multicast
/// (returns the embedded scope), then site-local, then the source-or-destination
/// fallback (`addrpol.c:139-142`: sources sort above global, destinations
/// sort as global).
#[allow(clippy::too_many_arguments)]
pub fn address_scope(
    is_version4: bool,
    is_global: bool,
    is_link_local: bool,
    is_loopback: bool,
    is_unique_local: bool,
    is_multicast: bool,
    multicast_scope: u8,
    is_site_local: bool,
    is_source: bool,
) -> u8 {
    if is_version4 {
        return SCOPE_GLOBAL;
    }
    if is_global {
        return SCOPE_GLOBAL;
    }
    if is_link_local || is_loopback {
        return SCOPE_LINK_LOCAL;
    }
    if is_unique_local {
        return SCOPE_ORGANIZATION_LOCAL;
    }
    if is_multicast {
        return multicast_scope;
    }
    if is_site_local {
        return SCOPE_SITE_LOCAL;
    }
    if is_source {
        return SCOPE_RESERVED_TOP;
    }
    SCOPE_GLOBAL
}

/// Prefix length of a version 4 netmask word in network order, or `None` when
/// the mask is not contiguous (`addr_get_netmask`, `addr.c:409-444`: find the
/// first zero bit, then require every later bit to be zero).
pub fn prefix_from_netmask_v4(mask: u32) -> Option<u8> {
    let mut prefix = 0_u8;
    let mut seen_zero = false;
    for bit in 0..32 {
        let set = (mask >> (31 - bit)) & 1 == 1;
        if !set {
            seen_zero = true;
        } else if seen_zero {
            return None;
        } else {
            prefix += 1;
        }
    }
    Some(prefix)
}

/// Prefix length of a 16-byte version 6 netmask, or `None` when not
/// contiguous (`addr_get_netmask`, `addr.c:446-492`).
pub fn prefix_from_netmask_v6(mask: &[u8; 16]) -> Option<u8> {
    let mut prefix = 0_u8;
    let mut seen_zero = false;
    for &byte in mask.iter() {
        for bit in 0..8 {
            let set = (byte >> (7 - bit)) & 1 == 1;
            if !set {
                seen_zero = true;
            } else if seen_zero {
                return None;
            } else {
                prefix += 1;
            }
        }
    }
    Some(prefix)
}

/// Build a version 4 netmask word from a prefix length
/// (`addr_make_netmask`, `addr.c:503-518`).
pub fn netmask_v4_from_prefix(prefix: u8) -> Option<u32> {
    if prefix > 32 {
        return None;
    }
    if prefix == 0 {
        return Some(0);
    }
    Some(u32::MAX << (32 - prefix))
}

/// Build a 16-byte version 6 netmask from a prefix length
/// (`addr_make_netmask`, `addr.c:503-518`).
pub fn netmask_v6_from_prefix(prefix: u8) -> Option<[u8; 16]> {
    if prefix > 128 {
        return None;
    }
    let mut out = [0_u8; 16];
    let full = (prefix / 8) as usize;
    let rest = prefix % 8;
    for (index, slot) in out.iter_mut().enumerate() {
        if index < full {
            *slot = 0xff;
        } else if index == full && rest != 0 {
            *slot = 0xff << (8 - rest);
        } else {
            *slot = 0;
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_max_size_matches_socket_driver_header() {
        assert_eq!(SOCKADDR_MAX, 256);
        assert!(address_fits(256));
        assert!(!address_fits(257));
        assert!(address_fits(0));
    }

    #[test]
    fn test_table_holds_nine_rows_in_prefix_order() {
        assert_eq!(POLICY_TABLE.len(), 9);
        assert_eq!(POLICY_TABLE[0].prefix_len, 128);
        assert_eq!(POLICY_TABLE[8].prefix_len, 0);
        assert_eq!(POLICY_TABLE[8].label, 1);
    }

    #[test]
    fn test_loopback_earns_label_zero() {
        assert_eq!(policy_label(1), 0);
    }

    #[test]
    fn test_mapped_version4_earns_label_four() {
        let mapped = v4_mapped_to_v6(0x0a00_0001);
        assert_eq!(policy_label(mapped), 4);
    }

    #[test]
    fn test_documentation_prefix_earns_label_five() {
        assert_eq!(policy_label(0x2001_0000_0000_0000_0000_0000_0000_0001), 5);
    }

    #[test]
    fn test_normalize_keeps_only_prefix_bits() {
        assert_eq!(normalize_prefix(u128::MAX, 0), 0);
        assert_eq!(normalize_prefix(0xabcd, 128), 0xabcd);
        assert_eq!(normalize_prefix(0xff, 4), 0);
        assert_eq!(
            normalize_prefix(u128::MAX, 4),
            0xf000_0000_0000_0000_0000_0000_0000_0000_u128
        );
    }

    #[test]
    fn test_common_bits_counts_leading_match() {
        assert_eq!(common_bits(0, 0, 128), 128);
        assert_eq!(common_bits(0, 1, 128), 127);
        assert_eq!(common_bits(0, u128::MAX, 10), 0);
    }

    #[test]
    fn test_scope_orders_version4_as_global() {
        assert_eq!(
            address_scope(true, false, false, false, false, false, 0, false, false),
            SCOPE_GLOBAL
        );
    }

    #[test]
    fn test_scope_prefers_link_local_over_unique_local() {
        assert_eq!(
            address_scope(false, false, true, false, true, false, 0, false, false),
            SCOPE_LINK_LOCAL
        );
    }

    #[test]
    fn test_scope_deviates_for_unique_local_addresses() {
        assert_eq!(
            address_scope(false, false, false, false, true, false, 0, false, false),
            SCOPE_ORGANIZATION_LOCAL
        );
    }

    #[test]
    fn test_scope_fallback_distinguishes_source_and_destination() {
        assert_eq!(
            address_scope(false, false, false, false, false, false, 0, false, true),
            SCOPE_RESERVED_TOP
        );
        assert_eq!(
            address_scope(false, false, false, false, false, false, 0, false, false),
            SCOPE_GLOBAL
        );
    }

    #[test]
    fn test_netmask_v4_round_trips_when_contiguous() {
        assert_eq!(prefix_from_netmask_v4(0xffffff00), Some(24));
        assert_eq!(prefix_from_netmask_v4(0), Some(0));
        assert_eq!(prefix_from_netmask_v4(0xffffffff), Some(32));
        assert_eq!(prefix_from_netmask_v4(0xff00ff00), None);
        assert_eq!(netmask_v4_from_prefix(24), Some(0xffffff00));
        assert_eq!(netmask_v4_from_prefix(33), None);
    }

    #[test]
    fn test_netmask_v6_round_trips_when_contiguous() {
        let full = netmask_v6_from_prefix(64).expect("prefix 64 fits");
        assert_eq!(prefix_from_netmask_v6(&full), Some(64));
        let mut broken = [0xff_u8; 16];
        broken[0] = 0xfe;
        broken[1] = 0xff;
        assert_eq!(prefix_from_netmask_v6(&broken), None);
        assert_eq!(netmask_v6_from_prefix(129), None);
    }
}
