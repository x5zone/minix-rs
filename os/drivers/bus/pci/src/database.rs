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
///
/// C: `struct pcidev` (`pci.c:62-77`) carries the same identity fields the
/// access rules match against: vendor and device identifiers, subsystem
/// identifiers, and the three class bytes.
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
    /// Subsystem vendor identifier (`pd_sub_vid`).
    pub sub_vendor: u16,
    /// Subsystem device identifier (`pd_sub_did`).
    pub sub_device: u16,
    /// Base class (`pd_baseclass`).
    pub base_class: u8,
    /// Subclass (`pd_subclass`).
    pub sub_class: u8,
    /// Programming interface (`pd_infclass`).
    pub interface: u8,
    /// Interrupt pin (zero means none).
    pub irq_pin: u8,
    /// Routed interrupt line (set by routing; 0xFF means unknown).
    pub irq_line: u8,
}

/// Wildcard meaning "any subsystem vendor" (`NO_SUB_VID`, rs.h:79).
pub const NO_SUB_VENDOR: u16 = 0xFFFF;
/// Wildcard meaning "any subsystem device" (`NO_SUB_DID`, rs.h:80).
pub const NO_SUB_DEVICE: u16 = 0xFFFF;

impl PciDevice {
    /// Position key shared with configuration access.
    pub const fn position(self) -> (u8, u8, u8) {
        (self.bus, self.device, self.function)
    }

    /// The twenty-four-bit class code the access rules match against.
    ///
    /// C: `(pd_baseclass << 16) | (pd_subclass << 8) | pd_infclass`
    /// (`pci.c:2069-2071`).
    pub const fn class_id(&self) -> u32 {
        ((self.base_class as u32) << 16) | ((self.sub_class as u32) << 8) | self.interface as u32
    }
}

/// One device-identifier pattern inside an access list.
///
/// C: `struct rs_pci_id` (`rs.h:73-78`): vendor and device must match
/// exactly; the subsystem pair may be the `NO_SUB_*` wildcards.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AclDevice {
    /// Vendor identifier (exact).
    pub vendor: u16,
    /// Device identifier (exact).
    pub device_id: u16,
    /// Subsystem vendor, or [`NO_SUB_VENDOR`] for any.
    pub sub_vendor: u16,
    /// Subsystem device, or [`NO_SUB_DEVICE`] for any.
    pub sub_device: u16,
}

/// One class pattern inside an access list: matches when the pattern equals
/// the device class code masked by [`AclClass::mask`].
///
/// C: `struct rs_pci_class` (`rs.h:82-85`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AclClass {
    /// Pattern value compared against the masked class code.
    pub class: u32,
    /// Mask applied to the device class code first.
    pub mask: u32,
}

/// One caller's access list: the device and class patterns it may see.
///
/// C: `struct rs_pci` (`rs.h`): RS installs one list per driver endpoint
/// (`do_set_acl`, RS-only); the list holds device-identifier entries and
/// class entries.
#[derive(Debug, Clone, Default)]
pub struct PciAcl {
    /// Device patterns, any of which grants visibility.
    pub devices: Vec<AclDevice>,
    /// Class patterns, any of which grants visibility.
    pub classes: Vec<AclClass>,
}

impl PciAcl {
    /// True when this list matches the device: a device-identifier entry
    /// first, then a class entry (`visible`, `pci.c:2047-2083`).
    pub fn matches(&self, device: &PciDevice) -> bool {
        for entry in &self.devices {
            if entry.vendor == device.vendor
                && entry.device_id == device.device_id
                && (entry.sub_vendor == NO_SUB_VENDOR
                    || entry.sub_vendor == device.sub_vendor)
                && (entry.sub_device == NO_SUB_DEVICE
                    || entry.sub_device == device.sub_device)
            {
                return true;
            }
        }
        for entry in &self.classes {
            if entry.class == (device.class_id() & entry.mask) {
                return true;
            }
        }
        false
    }
}

/// Device database: records plus per-caller access lists.
///
/// C: the enumerated device array behind the index walk plus the `pci_acl`
/// table of per-driver `rs_pci` lists. Iteration order is enumeration order
/// (first means index zero, next means index plus one); visibility filters
/// apply on top: a caller without a list sees everything, a caller with a
/// list sees only pattern matches.
#[derive(Debug, Clone, Default)]
pub struct DeviceDb {
    devices: Vec<PciDevice>,
    acls: Vec<(i64, PciAcl)>,
}

impl DeviceDb {
    /// Empty database.
    pub fn new() -> DeviceDb {
        DeviceDb {
            devices: Vec::new(),
            acls: Vec::new(),
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

    /// Install (or replace) one caller's access list.
    ///
    /// C: `do_set_acl` (`main.c:238-281`) — RS alone installs lists, one
    /// per driver; setting twice for the same endpoint replaces the older
    /// list. The RS-source check lives in the service dispatch.
    pub fn set_acl(&mut self, caller: i64, acl: PciAcl) {
        if let Some(slot) = self.acls.iter_mut().find(|(who, _)| *who == caller) {
            slot.1 = acl;
        } else {
            self.acls.push((caller, acl));
        }
    }

    /// Drop one caller's access list; the caller sees everything again.
    ///
    /// C: `do_del_acl` (`main.c:283-321`).
    pub fn del_acl(&mut self, caller: i64) {
        self.acls.retain(|(who, _)| *who != caller);
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
    /// C: `visible` (`pci.c:2039-2086`): the rule is per *caller* — a
    /// caller with no access list sees every device (procfs relies on it),
    /// while a listed caller sees only what its device-identifier or class
    /// patterns match.
    pub fn is_visible(&self, caller: i64, index: usize) -> bool {
        let Some(device) = self.devices.get(index) else {
            return false;
        };
        match self.acls.iter().find(|(who, _)| *who == caller) {
            None => true,
            Some((_, acl)) => acl.matches(device),
        }
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
            sub_vendor: 0x1234,
            sub_device: 0x5678,
            base_class: 0x02,
            sub_class: 0x00,
            interface: 0x00,
            irq_pin: 1,
            irq_line: 0xFF,
        }
    }

    fn pattern(vendor: u16, device_id: u16) -> AclDevice {
        AclDevice {
            vendor,
            device_id,
            sub_vendor: NO_SUB_VENDOR,
            sub_device: NO_SUB_DEVICE,
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
    fn test_caller_without_acl_sees_everything() {
        // C visible(): `!aclp → TRUE` — procfs relies on seeing all
        // devices without a list (pci.c:2044-2047).
        let mut db = DeviceDb::new();
        db.add(card(0, 1));
        assert_eq!(db.next_visible(100, 0), Some(0));
        assert_eq!(db.next_visible(200, 1), None);
    }

    #[test]
    fn test_acl_restricts_listed_caller_to_matches() {
        let mut db = DeviceDb::new();
        db.add(card(0, 1)); // 0x8086:0x100E, class 02_00_00
        db.add(card(0, 2)); // same identifiers: patterns hit both
        let mut acl = PciAcl::default();
        acl.devices.push(pattern(0x8086, 0x100E));
        db.set_acl(100, acl);
        assert_eq!(db.next_visible(100, 0), Some(0));
        // An unlisted caller still sees everything: the list binds only
        // its owner, unlike the old per-device restriction model.
        assert_eq!(db.next_visible(200, 0), Some(0));
        // A pattern that matches nothing hides the whole bus from its
        // owner.
        let mut acl = PciAcl::default();
        acl.devices.push(pattern(0x10EC, 0x8168));
        db.set_acl(300, acl);
        assert_eq!(db.next_visible(300, 0), None);
        db.del_acl(300);
        assert_eq!(db.next_visible(300, 0), Some(0));
    }

    #[test]
    fn test_acl_wildcards_and_class_masks() {
        let mut db = DeviceDb::new();
        db.add(card(0, 1));
        // Subsystem wildcards pass; an exact wrong subsystem fails.
        let mut acl = PciAcl::default();
        acl.devices.push(AclDevice {
            vendor: 0x8086,
            device_id: 0x100E,
            sub_vendor: NO_SUB_VENDOR,
            sub_device: 0x5678,
        });
        db.set_acl(100, acl);
        assert_eq!(db.is_visible(100, 0), true);
        let mut acl = PciAcl::default();
        acl.devices.push(AclDevice {
            vendor: 0x8086,
            device_id: 0x100E,
            sub_vendor: 0x9999,
            sub_device: NO_SUB_DEVICE,
        });
        db.set_acl(100, acl);
        assert!(!db.is_visible(100, 0));
        // Class mask: the class code is (base<<16)|(sub<<8)|interface, so
        // network base class 02 is 0x0002_0000 under mask 0x00FF_0000.
        db.del_acl(100);
        let mut acl = PciAcl::default();
        acl.classes.push(AclClass {
            class: 0x0002_0000,
            mask: 0x00FF_0000,
        });
        db.set_acl(100, acl);
        assert!(db.is_visible(100, 0));
    }

    #[test]
    fn test_set_acl_replaces_previous_list() {
        // C keeps one list per driver: a second do_set_acl for the same
        // endpoint replaces the first.
        let mut db = DeviceDb::new();
        db.add(card(0, 1));
        let mut deny_all = PciAcl::default();
        deny_all.devices.push(pattern(0x10EC, 0x8168));
        db.set_acl(100, deny_all);
        assert!(!db.is_visible(100, 0));
        let mut allow_all = PciAcl::default();
        allow_all.devices.push(pattern(0x8086, 0x100E));
        db.set_acl(100, allow_all);
        assert!(db.is_visible(100, 0));
    }

    #[test]
    fn test_out_of_range_index_is_invisible() {
        let mut db = DeviceDb::new();
        assert!(!db.is_visible(100, 7));
    }
}
