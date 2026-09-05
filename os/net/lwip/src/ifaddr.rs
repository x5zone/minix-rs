//! Interface address policy: version 6 flags, route flags, selection order.
//!
//! C correspondence: `minix3/minix/net/lwip/ifaddr.c` (2224 lines) with the
//! flag definitions in `minix3/minix/net/lwip/ifaddr.h:5-7`. Address storage,
//! duplicate detection, and route updates stay in the service binary. This
//! module owns the portion that can be decided from numbers alone: which
//! flag bits mean what, and which address the selection prefers.
//!
//! The address module logically belongs to the interface object and only
//! touches its address fields. That boundary matters: interface state such
//! as flags and link status stays with the interface layer, while address
//! lists, lifetimes, and selection stay here.

/// Autoconfigured address without subnet (`IFADDR_V6F_AUTOCONF`,
/// `ifaddr.h:5`).
pub const VERSION6_FLAG_AUTOCONFIGURED: u8 = 0x01;

/// Temporary privacy address (`IFADDR_V6F_TEMPORARY`, `ifaddr.h:6`).
pub const VERSION6_FLAG_TEMPORARY: u8 = 0x02;

/// Address derived from the hardware address (`IFADDR_V6F_HWBASED`,
/// `ifaddr.h:7`).
pub const VERSION6_FLAG_HARDWARE_BASED: u8 = 0x04;

/// Whether a flag set contains one flag.
pub fn has_version6_flag(set: u8, flag: u8) -> bool {
    set & flag != 0
}

/// Selection preference between two candidate source addresses, mirroring
/// the order used by the address selection walk (`ifaddr_v6_select` and
/// `ifaddr_select`, `ifaddr.c:1589-1834`, which consult the policy labels
/// from the address policy module).
///
/// Returns true when the first candidate is preferred: a smaller scope
/// distance wins first, then the smaller policy label difference. The
/// service binary classifies both candidates with the address module and
/// passes the resulting distances here, so the ordering itself can be
/// tested without an interface table.
pub fn prefer_first_candidate(
    first_scope_distance: u8,
    second_scope_distance: u8,
    first_label_distance: u8,
    second_label_distance: u8,
) -> bool {
    if first_scope_distance != second_scope_distance {
        return first_scope_distance < second_scope_distance;
    }
    first_label_distance < second_label_distance
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_flags_match_header() {
        assert_eq!(VERSION6_FLAG_AUTOCONFIGURED, 0x01);
        assert_eq!(VERSION6_FLAG_TEMPORARY, 0x02);
        assert_eq!(VERSION6_FLAG_HARDWARE_BASED, 0x04);
        assert!(has_version6_flag(0x03, VERSION6_FLAG_AUTOCONFIGURED));
        assert!(!has_version6_flag(0x02, VERSION6_FLAG_AUTOCONFIGURED));
    }

    #[test]
    fn test_selection_prefers_closer_scope_first() {
        assert!(prefer_first_candidate(1, 2, 5, 0));
        assert!(!prefer_first_candidate(2, 1, 0, 5));
    }

    #[test]
    fn test_selection_breaks_scope_ties_by_label() {
        assert!(prefer_first_candidate(1, 1, 0, 2));
        assert!(!prefer_first_candidate(1, 1, 2, 0));
        assert!(!prefer_first_candidate(1, 1, 1, 1));
    }
}
