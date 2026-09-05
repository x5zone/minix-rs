//! Ethernet interface policy: transmission unit, multicast cap, queue floor.
//!
//! C correspondence: `minix3/minix/net/lwip/ethif.c` (1718 lines).
//! Interface storage, driver binding, and packet flow stay in the service
//! binary. This module owns the portion that can be decided from numbers
//! alone: which transmission unit values pass, how many multicast addresses
//! one interface tracks, and how many packet buffers are always reserved.
//!
//! The Ethernet module implements the generic interface table for one
//! concrete medium. Keeping its numeric policy in one place lets the
//! generic layer and the Ethernet instance share a single source of truth
//! instead of duplicating bounds.

/// Largest Ethernet transmission unit (`ETHIF_MAX_MTU`, `ethif.c:68`).
pub const MAX_MTU: usize = 1500;

/// Default Ethernet transmission unit (`ETHIF_DEF_MTU`, `ethif.c:69`).
pub const DEFAULT_MTU: usize = 1500;

/// Largest multicast addresses per interface (`ETHIF_MCAST_MAX`,
/// `ethif.c:71`).
pub const MAX_MULTICAST_ADDRESSES: usize = 8;

/// Minimum packet buffers always reserved (`ETHIF_PBUF_MIN`, `ethif.c:119`,
/// equal to the device scatter limit so one full scatter always fits).
pub const RESERVED_BUFFERS: usize = 8;

/// Whether a transmission unit passes (up to the Ethernet maximum).
pub fn mtu_allowed(mtu: usize) -> bool {
    mtu <= MAX_MTU && mtu > 0
}

/// Whether a multicast list length fits the per-interface cap.
pub fn multicast_list_fits(count: usize) -> bool {
    count <= MAX_MULTICAST_ADDRESSES
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mtu_matches_ethernet_source() {
        assert_eq!(MAX_MTU, 1500);
        assert_eq!(DEFAULT_MTU, 1500);
        assert!(mtu_allowed(1500));
        assert!(mtu_allowed(1));
        assert!(!mtu_allowed(0));
        assert!(!mtu_allowed(1501));
    }

    #[test]
    fn test_multicast_cap_matches_source() {
        assert_eq!(MAX_MULTICAST_ADDRESSES, 8);
        assert!(multicast_list_fits(8));
        assert!(!multicast_list_fits(9));
    }

    #[test]
    fn test_reserved_buffers_match_scatter_limit() {
        assert_eq!(RESERVED_BUFFERS, 8);
    }
}
