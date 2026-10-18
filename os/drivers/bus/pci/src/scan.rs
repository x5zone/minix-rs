//! Bus enumeration: configuration-space reads into the device database.
//!
//! C correspondence: `probe_bus` (`pci.c:1519-1620`) walks bus, device,
//! and function; an absent function zero ends the device (higher
//! functions only exist when zero does); each present function records
//! the vendor and device identifiers, the subsystem identifiers, the
//! three class bytes, and the interrupt line and pin.
//!
//! Two C legs are not modeled and are named here so nobody reads this as
//! the whole enumeration: following a bridge's secondary bus (the caller
//! supplies the bus numbers to walk) and the BAR sizing probing
//! (`pci.c:1400-1510`, the base-address registers keep no model in
//! `PciDevice`). Bridge header types are recorded as ordinary devices.

use crate::config::{AccessWidth, ConfigAddress, ConfigSpace};
use crate::database::{DeviceDb, PciDevice};

/// Register offsets (`minix3/minix/include/arch/i386/include/pci.h:7-52`).
pub const PCI_VID: u8 = 0x00;
pub const PCI_DID: u8 = 0x02;
pub const PCI_PIFR: u8 = 0x09;
pub const PCI_SCR: u8 = 0x0A;
pub const PCI_BCR: u8 = 0x0B;
pub const PCI_SUBVID: u8 = 0x2C;
pub const PCI_SUBDID: u8 = 0x2E;
pub const PCI_ILR: u8 = 0x3C;
pub const PCI_IPR: u8 = 0x3D;

/// Devices per bus and functions per device.
pub const DEVICES_PER_BUS: u8 = 32;
pub const FUNCTIONS_PER_DEVICE: u8 = 8;

/// The identifier answering from an absent function (`NO_VID`/`NO_DID`).
const ABSENT: u16 = 0xFFFF;

fn read8<C: ConfigSpace>(
    space: &mut C,
    bus: u8,
    device: u8,
    function: u8,
    offset: u8,
) -> u32 {
    space
        .read(ConfigAddress {
            bus,
            device,
            function,
            offset,
            width: AccessWidth::Bits8,
        })
        .unwrap_or(0xFF)
}

fn read16<C: ConfigSpace>(
    space: &mut C,
    bus: u8,
    device: u8,
    function: u8,
    offset: u8,
) -> u16 {
    space
        .read(ConfigAddress {
            bus,
            device,
            function,
            offset,
            width: AccessWidth::Bits16,
        })
        .map(|value| value as u16)
        .unwrap_or(ABSENT)
}

/// Enumerate every present function on the given buses.
///
/// Returns the number of devices recorded. Duplicate positions are
/// refused by [`DeviceDb::add`] without stopping the walk; a full
/// database stops it (C panics on overflow, `pci.c:1527-1528`).
pub fn scan<C: ConfigSpace>(space: &mut C, db: &mut DeviceDb, buses: &[u8]) -> usize {
    let before = db.len();
    'bus: for &bus in buses {
        for device in 0..DEVICES_PER_BUS {
            for function in 0..FUNCTIONS_PER_DEVICE {
                let vendor = read16(space, bus, device, function, PCI_VID);
                let device_id = read16(space, bus, device, function, PCI_DID);
                if vendor == ABSENT && device_id == ABSENT {
                    // Nothing at function zero means nothing on this
                    // device; otherwise an empty function slot is just
                    // the end of a multifunction device.
                    if function == 0 {
                        break;
                    }
                    continue;
                }
                let recorded = PciDevice {
                    bus,
                    device,
                    function,
                    vendor,
                    device_id,
                    sub_vendor: read16(space, bus, device, function, PCI_SUBVID),
                    sub_device: read16(space, bus, device, function, PCI_SUBDID),
                    base_class: read8(space, bus, device, function, PCI_BCR) as u8,
                    sub_class: read8(space, bus, device, function, PCI_SCR) as u8,
                    interface: read8(space, bus, device, function, PCI_PIFR) as u8,
                    irq_pin: read8(space, bus, device, function, PCI_IPR) as u8,
                    irq_line: read8(space, bus, device, function, PCI_ILR) as u8,
                    in_use: false,
                    owner: 0,
                };
                if !db.add(recorded) && db.len() >= crate::database::MAX_DEVICES {
                    break 'bus;
                }
            }
        }
    }
    db.len() - before
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::MemConfigSpace;

    fn write16(space: &mut MemConfigSpace, bus: u8, device: u8, function: u8, offset: u8, value: u16) {
        space
            .write(
                ConfigAddress {
                    bus,
                    device,
                    function,
                    offset,
                    width: AccessWidth::Bits16,
                },
                value as u32,
            )
            .unwrap();
    }

    fn write8(space: &mut MemConfigSpace, bus: u8, device: u8, function: u8, offset: u8, value: u8) {
        space
            .write(
                ConfigAddress {
                    bus,
                    device,
                    function,
                    offset,
                    width: AccessWidth::Bits8,
                },
                value as u32,
            )
            .unwrap();
    }

    fn present(space: &mut MemConfigSpace, bus: u8, device: u8, function: u8, vid: u16, did: u16) {
        write16(space, bus, device, function, PCI_VID, vid);
        write16(space, bus, device, function, PCI_DID, did);
    }

    #[test]
    fn test_scan_records_present_functions_with_identity() {
        let mut space = MemConfigSpace::new();
        present(&mut space, 0, 0, 0, 0x8086, 0x1234);
        write8(&mut space, 0, 0, 0, PCI_BCR, 0x02);
        write8(&mut space, 0, 0, 0, PCI_SCR, 0x00);
        write8(&mut space, 0, 0, 0, PCI_PIFR, 0x00);
        write16(&mut space, 0, 0, 0, PCI_SUBVID, 0x1AF4);
        write16(&mut space, 0, 0, 0, PCI_SUBDID, 0x0001);
        write8(&mut space, 0, 0, 0, PCI_ILR, 11);
        write8(&mut space, 0, 0, 0, PCI_IPR, 1);
        // A second device, function 0 only.
        present(&mut space, 0, 1, 0, 0x10EC, 0x8139);

        let mut db = DeviceDb::new();
        assert_eq!(scan(&mut space, &mut db, &[0]), 2);
        let first = db.get(0).unwrap();
        assert_eq!(first.bus, 0);
        assert_eq!(first.device, 0);
        assert_eq!(first.function, 0);
        assert_eq!(first.vendor, 0x8086);
        assert_eq!(first.device_id, 0x1234);
        assert_eq!(first.sub_vendor, 0x1AF4);
        assert_eq!(first.sub_device, 0x0001);
        assert_eq!(first.base_class, 0x02);
        assert_eq!(first.irq_line, 11);
        assert_eq!(first.irq_pin, 1);
        assert!(!first.in_use);
    }

    #[test]
    fn test_multifunction_walk_and_absent_zero_break() {
        let mut space = MemConfigSpace::new();
        // Device 0: function 0 present, function 1 present.
        present(&mut space, 0, 0, 0, 0x8086, 0x100E);
        present(&mut space, 0, 0, 1, 0x8086, 0x100F);
        // Device 2: function 0 absent but function 1 present — the walk
        // must stop at function zero (C `pci.c:1558-1564`).
        present(&mut space, 0, 2, 1, 0xDEAD, 0xBEEF);

        let mut db = DeviceDb::new();
        assert_eq!(scan(&mut space, &mut db, &[0]), 2);
        assert_eq!(db.len(), 2);
        assert_eq!(db.get(1).unwrap().function, 1);
    }

    #[test]
    fn test_scan_walks_every_supplied_bus() {
        let mut space = MemConfigSpace::new();
        present(&mut space, 0, 0, 0, 0x8086, 0x0001);
        present(&mut space, 1, 0, 0, 0x8086, 0x0002);

        let mut db = DeviceDb::new();
        assert_eq!(scan(&mut space, &mut db, &[0, 1]), 2);
        assert_eq!(db.get(0).unwrap().bus, 0);
        assert_eq!(db.get(1).unwrap().bus, 1);
    }
}
