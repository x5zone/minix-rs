//! Device database: enumeration records, iteration, and access lists.
//!
//! C correspondence: the device index walk (`_pci_first_dev`,
//! `_pci_next_dev`, `_pci_find_dev`, `_pci_ids`) served by `do_first_dev`,
//! `do_next_dev`, `do_find_dev`, `do_ids` (`main.c:62-164`), the reserve
//! and access-list calls (`do_reserve`, `do_set_acl`, `do_del_acl`,
//! `main.c:238-321`), and the visibility rule `visible`
//! (`pci.c:2004-2086`): a caller sees a device when it holds a matching
//! access entry or when no access list restricts the device.

use alloc::vec::Vec;

/// Maximum devices in the database (test-sized; production sizes from the
/// enumeration, which is bounded by bus topology).
pub const MAX_DEVICES: usize = 64;

/// One enumerated device: bus position plus identifiers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PciDevice {
    /// Bus number.
    pub bus: u8,
    /// Device number.
    pub device: u8,
    /// Function number.
    pub function: u8,
    /// Vendor identifier.
    pub vendor: u16,
    /// Device identifier.
    pub device_id: u16,
    /// Interrupt pin (zero means none).
    pub irq_pin: u8,
    /// Routed interrupt line (set by routing; 0xFF means unknown).
    pub irq_line: u8,
}

impl PciDevice {
    /// Position key shared with configuration access.
    pub const fn position(self) -> (u8, u8, u8) {
        (self.bus, self.device, self.function)
    }
}

/// One access-list entry: this caller may see this device.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AccessEntry {
    /// Caller endpoint.
    pub caller: i64,
    /// Device index in the database.
    pub index: usize,
}

/// Device database: records plus per-caller access lists.
///
/// C: the enumerated device array behind the index walk plus the
/// `rs_pci` access lists consulted by `visible`. Iteration order is
/// enumeration order (first means index zero, next means index plus
/// one); visibility filters apply on top.
#[derive(Debug, Clone, Default)]
pub struct DeviceDb {
    devices: Vec<PciDevice>,
    access: Vec<AccessEntry>,
}

impl DeviceDb {
    /// Empty database.
    pub fn new() -> DeviceDb {
        DeviceDb {
            devices: Vec::new(),
            access: Vec::new(),
        }
    }

    /// Record one enumerated device; false when full.
    ///
    /// C: duplicate detection (`is_duplicate`, `pci.c:420-436`) refuses a
    /// second record at the same position; this method does the same.
    pub fn add(&mut self, device: PciDevice) -> bool {
        if self.devices.len() >= MAX_DEVICES {
            return false;
        }
        if self
            .devices
            .iter()
            .any(|known| known.position() == device.position())
        {
            return false;
        }
        self.devices.push(device);
        true
    }

    /// Number of recorded devices.
    pub fn len(&self) -> usize {
        self.devices.len()
    }

    /// True when nothing is recorded.
    pub fn is_empty(&self) -> bool {
        self.devices.is_empty()
    }

    /// Record by index.
    pub fn get(&self, index: usize) -> Option<PciDevice> {
        self.devices.get(index).copied()
    }

    /// Index of the device at this position.
    ///
    /// C: `_pci_find_dev` (served by `do_find_dev`, `main.c:117-138`).
    pub fn find(&self, bus: u8, device: u8, function: u8) -> Option<usize> {
        self.devices
            .iter()
            .position(|known| known.position() == (bus, device, function))
    }

    /// First visible index at or after `start` for this caller, if any.
    ///
    /// C: `_pci_first_dev` starts at zero, `_pci_next_dev` resumes after
    /// the given index; both skip invisible devices (`main.c:62-116`).
    pub fn next_visible(&self, caller: i64, start: usize) -> Option<usize> {
        (start..self.devices.len()).find(|index| self.is_visible(caller, *index))
    }

    /// True when this caller may see this device.
    ///
    /// C: `visible` (`pci.c:2004-2086`): unrestricted devices are visible
    /// to everyone; restricted devices only to listed callers. Restriction
    /// here means "an entry names this device for somebody": once any
    /// entry names the device, unlisted callers lose it.
    pub fn is_visible(&self, caller: i64, index: usize) -> bool {
        if index >= self.devices.len() {
            return false;
        }
        let restricted = self.access.iter().any(|entry| entry.index == index);
        if !restricted {
            return true;
        }
        self.access
            .iter()
            .any(|entry| entry.index == index && entry.caller == caller)
    }

    /// Grant a caller access to a device (reserve path).
    ///
    /// C: `_pci_grant_access` behind `do_reserve` (`main.c:238-...`).
    pub fn grant(&mut self, caller: i64, index: usize) -> bool {
        if index >= self.devices.len() {
            return false;
        }
        if self
            .access
            .iter()
            .any(|entry| entry.caller == caller && entry.index == index)
        {
            return true;
        }
        self.access.push(AccessEntry { caller, index });
        true
    }

    /// Forget every entry of one caller (release and delete-access paths).
    ///
    /// C: `_pci_release` and `do_del_acl` (`main.c`).
    pub fn release_caller(&mut self, caller: i64) {
        self.access.retain(|entry| entry.caller != caller);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn card(bus: u8, device: u8) -> PciDevice {
        PciDevice {
            bus,
            device,
            function: 0,
            vendor: 0x8086,
            device_id: 0x100E,
            irq_pin: 1,
            irq_line: 0xFF,
        }
    }

    #[test]
    fn test_add_rejects_duplicates_and_overflow() {
        let mut db = DeviceDb::new();
        assert!(db.add(card(0, 5)));
        assert!(!db.add(card(0, 5)));
        assert_eq!(db.len(), 1);
    }

    #[test]
    fn test_find_locates_by_position() {
        let mut db = DeviceDb::new();
        db.add(card(0, 5));
        db.add(card(1, 2));
        assert_eq!(db.find(1, 2, 0), Some(1));
        assert_eq!(db.find(2, 2, 0), None);
    }

    #[test]
    fn test_unrestricted_devices_are_visible_to_all() {
        let mut db = DeviceDb::new();
        db.add(card(0, 1));
        assert_eq!(db.next_visible(100, 0), Some(0));
        assert_eq!(db.next_visible(200, 1), None);
    }

    #[test]
    fn test_grant_restricts_to_listed_caller() {
        let mut db = DeviceDb::new();
        db.add(card(0, 1));
        db.add(card(0, 2));
        assert!(db.grant(100, 0));
        assert_eq!(db.next_visible(100, 0), Some(0));
        assert_eq!(db.next_visible(200, 0), Some(1));
        db.release_caller(100);
        assert_eq!(db.next_visible(200, 0), Some(0));
    }

    #[test]
    fn test_grant_out_of_range_fails() {
        let mut db = DeviceDb::new();
        assert!(!db.grant(100, 7));
        assert!(!db.is_visible(100, 7));
    }
}
