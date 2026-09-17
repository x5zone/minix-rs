//! Bus-query wiring: the PCI driver's query-protocol dispatch face.
//!
//! C correspondence: `do_first_dev`/`do_next_dev`/`do_find_dev`/
//! `do_reserve` (`main.c:62-236`, `pci.c:2320-2362`) — the BUSC_PCI_*
//! query family served over the driver's message loop. Visibility and
//! reserve policy live in [`crate::database`]; this face routes decoded
//! queries to them and reports what the service replies.
//!
//! Grant transport (the kernel privilege additions in
//! `_pci_grant_access`: I/O ranges, memory ranges, IRQ) stays with the
//! service; the face answers the occupancy/permission outcomes.

use minix_types::OK;

use crate::database::DeviceDb;

/// What the service should reply to one query.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QueryReply {
    /// No reply content beyond success (reserve/release/set-acl).
    Status(i32),
    /// A device index answer (first/next/find).
    Index(Option<usize>),
}

/// The PCI driver's query face over its device database.
pub struct PciFace {
    pub db: DeviceDb,
}

impl PciFace {
    /// A face over an empty database.
    pub fn new() -> Self {
        PciFace { db: DeviceDb::new() }
    }
}

impl Default for PciFace {
    fn default() -> Self {
        Self::new()
    }
}

impl PciFace {
    /// First visible device for this caller (BUSC_PCI_FIRST_DEV).
    pub fn first_dev(&self, caller: i64) -> QueryReply {
        QueryReply::Index(self.db.next_visible(caller, 0))
    }

    /// Next visible device strictly after `previous` (BUSC_PCI_NEXT_DEV).
    pub fn next_dev(&self, caller: i64, previous: usize) -> QueryReply {
        QueryReply::Index(self.db.next_visible(caller, previous + 1))
    }

    /// Find a device by position (BUSC_PCI_FIND_DEV), gated by
    /// visibility — an invisible device "does not exist" for the caller.
    pub fn find_dev(&self, caller: i64, bus: u8, device: u8, function: u8) -> QueryReply {
        let found = self
            .db
            .find(bus, device, function)
            .filter(|index| self.db.is_visible(caller, *index));
        QueryReply::Index(found)
    }

    /// Reserve a device (BUSC_PCI_RESERVE): the occupancy policy with
    /// its invalid/permitted/busy outcomes.
    pub fn reserve(&mut self, caller: i64, index: usize) -> QueryReply {
        QueryReply::Status(match self.db.reserve(caller, index) {
            Ok(()) => OK,
            Err(code) => code,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::{AclDevice, NO_SUB_DEVICE, NO_SUB_VENDOR, PciAcl, PciDevice};
    use minix_types::{EBUSY, EINVAL};

    fn card(bus: u8, device: u8) -> PciDevice {
        PciDevice {
            bus,
            device,
            function: 0,
            vendor: 0x8086,
            device_id: 0x100E,
            sub_vendor: 0x1234,
            sub_device: 0x5678,
            base_class: 0x02,
            sub_class: 0x00,
            interface: 0x00,
            irq_pin: 1,
            irq_line: 0xFF,
            in_use: false,
            owner: 0,
        }
    }

    #[test]
    fn test_first_next_walk_visible_devices_only() {
        let mut face = PciFace::new();
        face.db.add(card(0, 1));
        face.db.add(card(0, 2));
        assert_eq!(face.first_dev(100), QueryReply::Index(Some(0)));
        assert_eq!(face.next_dev(100, 0), QueryReply::Index(Some(1)));
        assert_eq!(face.next_dev(100, 1), QueryReply::Index(None));
    }

    #[test]
    fn test_find_dev_hides_invisible_devices() {
        let mut face = PciFace::new();
        face.db.add(card(0, 1));
        // Blind caller: the device exists but is invisible.
        let mut acl = PciAcl::default();
        acl.devices.push(AclDevice {
            vendor: 0x10EC,
            device_id: 0x8168,
            sub_vendor: NO_SUB_VENDOR,
            sub_device: NO_SUB_DEVICE,
        });
        face.db.set_acl(300, acl);
        assert_eq!(face.find_dev(300, 0, 1, 0), QueryReply::Index(None));
        assert_eq!(face.find_dev(100, 0, 1, 0), QueryReply::Index(Some(0)));
    }

    #[test]
    fn test_reserve_reports_occupancy_outcomes() {
        let mut face = PciFace::new();
        face.db.add(card(0, 1));
        assert_eq!(face.reserve(100, 0), QueryReply::Status(OK));
        assert_eq!(face.reserve(200, 0), QueryReply::Status(EBUSY));
        assert_eq!(face.reserve(300, 9), QueryReply::Status(EINVAL));
    }
}
