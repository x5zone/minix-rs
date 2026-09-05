//! Interface object policy: operation count, hardware list, loopback bounds.
//!
//! C correspondence: `minix3/minix/net/lwip/ifdev.c` (1064 lines) with the
//! object layout in `minix3/minix/net/lwip/ifdev.h` (155 lines), plus the
//! loopback instance in `minix3/minix/net/lwip/loopif.c` (420 lines).
//! Interface storage, packet flow, and neighbor state stay in the service
//! binary. This module owns the portion that can be decided from numbers
//! alone: how many operations the table holds, how many hardware addresses
//! the list keeps, which entry is active, and which loopback bounds apply.
//!
//! The hardware list exists to shield interface modules from list
//! management. The interface layer keeps up to three addresses and notifies
//! the module through a single set-address call whenever the active entry
//! changes, so modules never walk the list themselves.

/// Operations in the interface table (`struct ifdev_ops`, `ifdev.h:17-35`,
/// from initialization through destruction).
pub const OPERATION_COUNT: usize = 16;

/// Hardware addresses kept per interface (`IFDEV_NUM_HWADDRS`, `ifdev.h:14`,
/// at least 2 so changing addresses remains possible).
pub const HARDWARE_LIST_LENGTH: usize = 3;

/// List entry holds a valid address (`IFHWAF_VALID`, `ifdev.h:54`).
pub const HARDWARE_FLAG_VALID: u8 = 0x01;

/// List entry holds the factory address (`IFHWAF_FACTORY`, `ifdev.h:55`).
pub const HARDWARE_FLAG_FACTORY: u8 = 0x02;

/// Loopback input burst limit (`LOOPIF_LIMIT`, `loopif.c:17`: at most this
/// many packets are drained per poll so one busy loopback cannot starve the
/// main loop).
pub const LOOPBACK_BURST_LIMIT: usize = 65536;

/// Largest loopback transmission unit (`LOOPIF_MAX_MTU`, `loopif.c:23`, the
/// largest 16-bit value minus the 4-byte loopback tag).
pub const LOOPBACK_MAX_MTU: usize = 65531;

/// Number of loopback devices (`NR_LOOPIF`, `loopif.c:26`).
pub const LOOPBACK_DEVICE_COUNT: usize = 2;

/// Whether a hardware list index names a valid slot.
pub fn hardware_index_valid(index: usize) -> bool {
    index < HARDWARE_LIST_LENGTH
}

/// Whether a list entry counts as usable (the valid bit is set).
pub fn hardware_entry_usable(flags: u8) -> bool {
    flags & HARDWARE_FLAG_VALID != 0
}

/// Whether a loopback transmission unit passes (`loopif_set_mtu`,
/// `loopif.c:341-344`: any value up to the maximum passes).
pub fn loopback_mtu_allowed(mtu: usize) -> bool {
    mtu <= LOOPBACK_MAX_MTU
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_table_and_list_sizes_match_header() {
        assert_eq!(OPERATION_COUNT, 16);
        assert_eq!(HARDWARE_LIST_LENGTH, 3);
        assert_eq!(HARDWARE_FLAG_VALID, 0x01);
        assert_eq!(HARDWARE_FLAG_FACTORY, 0x02);
        assert!(hardware_index_valid(0));
        assert!(hardware_index_valid(2));
        assert!(!hardware_index_valid(3));
    }

    #[test]
    fn test_entry_usable_checks_valid_bit() {
        assert!(hardware_entry_usable(0x01));
        assert!(hardware_entry_usable(0x03));
        assert!(!hardware_entry_usable(0x00));
        assert!(!hardware_entry_usable(0x02));
    }

    #[test]
    fn test_loopback_bounds_match_source() {
        assert_eq!(LOOPBACK_BURST_LIMIT, 65536);
        assert_eq!(LOOPBACK_MAX_MTU, 65531);
        assert_eq!(LOOPBACK_DEVICE_COUNT, 2);
        assert!(loopback_mtu_allowed(1500));
        assert!(loopback_mtu_allowed(65531));
        assert!(!loopback_mtu_allowed(65532));
    }
}
