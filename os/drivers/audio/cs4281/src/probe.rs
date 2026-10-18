//! The PCI probe: find the card, reserve it, fetch its BAR, map it.
//!
//! C correspondence: `dev_probe` (`cs4281.c:317-355`) and the libsys
//! client calls it makes — `pci_first_dev`/`pci_next_dev`,
//! `pci_reserve`, `pci_get_bar`, `pci_attr_r8`, and `pci_attr_w16` —
//! plus the mapping step `vm_map_phys(SELF, base, size)`
//! (`cs4281.c:305-318`). The five-step leg itself lives in
//! `minix_sys::pci` (the C's own libsys placement); what stays here is
//! the card's own policy: the identifiers, the command-register constant,
//! and the BAR fetch-and-map pair.
//!
//! This card's register window is memory-mapped, so its probe is the only
//! one in the family that maps the BAR: `dev_probe` walks the six BARs,
//! keeps the memory ones, and stores the mapped addresses; only the first
//! is ever used (`reg(n)` reads `base[0]`, `cs4281.c:26`), which is what
//! [`map_bar`] returns.

use minix_sys::ipc::IpcTransport;
use minix_types::Endpoint;

use crate::registers::{DEVICE_ID, IO_ACCESS, PCI_MASTER, SERR_EN, VENDOR_ID};

pub use minix_sys::pci::{
    Bar, M1_I1, M1_I2, M1_I3, M2_I1, M2_I2, M2_L1, PCI_BAR, PCI_CR, PCI_ILR, busc,
};

/// What the probe found (`dev.irq` and the device index).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Probed {
    /// The interrupt line.
    pub irq: u8,
    /// The device index, for the later attribute and BAR traffic.
    pub index: i32,
}

/// Fetches one Base Address Register (`pci_get_bar`, `pci_get_bar.c`).
pub fn get_bar(
    ipc: &impl IpcTransport,
    pci: Endpoint,
    index: i32,
    register: u32,
) -> Result<Bar, i32> {
    minix_sys::pci::get_bar(ipc, pci, index, register)
}

/// Maps a physical span for this process (`vm_map_phys(SELF, base, size)`,
/// `cs4281.c:313-316`) and returns the address the VM server placed.
pub fn map_bar(ipc: &impl IpcTransport, physical: u32, size: u64) -> Result<*mut u8, i32> {
    minix_sys::pci::map_bar(ipc, physical, size)
}

/// Finds the CS4281, reserves it, enables its I/O, bus-master, and SERR
/// bits, and reads its interrupt line (C `dev_probe`, `cs4281.c:317-355`,
/// including the command-register write at `cs4281.c:350-351`).
pub fn probe(ipc: &impl IpcTransport, pci: Endpoint) -> Result<Probed, i32> {
    let index = minix_sys::pci::find_and_reserve(ipc, pci, VENDOR_ID, DEVICE_ID)?;
    // Enable the I/O space, bus-master bit, and SERR driver
    // (`cs4281.c:350-351`).
    minix_sys::pci::write_attribute16(ipc, pci, index, PCI_CR, SERR_EN | PCI_MASTER | IO_ACCESS);
    let irq = minix_sys::pci::read_attribute8(ipc, pci, index, PCI_ILR)?;
    Ok(Probed { irq, index })
}

#[cfg(test)]
mod tests {
    use super::*;
    use minix_sys::ipc::CannedTransport;
    use minix_types::ENODEV;

    fn read_i32(message: &minix_types::Message, offset: usize) -> i32 {
        // SAFETY: byte-level read of the union payload at a documented offset.
        let raw = unsafe { message.m_u.raw };
        i32::from_ne_bytes(raw[offset..offset + 4].try_into().unwrap())
    }

    fn write_i32(message: &mut minix_types::Message, offset: usize, value: i32) {
        // SAFETY: byte-level write of the union payload at a documented offset.
        unsafe {
            message.m_u.raw[offset..offset + 4].copy_from_slice(&value.to_ne_bytes());
        }
    }

    /// A reply carrying the found device.
    fn found(index: i32, vendor: u16, device: u16) -> minix_types::Message {
        let mut message = minix_types::Message {
            m_type: 1,
            ..minix_types::Message::default()
        };
        write_i32(&mut message, M1_I1, index);
        write_i32(&mut message, M1_I2, vendor as i32);
        write_i32(&mut message, M1_I3, device as i32);
        message
    }

    /// A reply carrying one attribute value.
    fn attribute(value: u32) -> minix_types::Message {
        let mut message = minix_types::Message::default();
        write_i32(&mut message, M2_L1, value as i32);
        message
    }

    /// A reply carrying one BAR.
    fn bar(base: u32, size: u64, flags: u32) -> minix_types::Message {
        let mut message = minix_types::Message::default();
        write_i32(&mut message, M2_I1, base as i32);
        // SAFETY: byte-level write of the union payload's size lane.
        unsafe {
            message.m_u.raw[16..24].copy_from_slice(&size.to_ne_bytes());
        }
        write_i32(&mut message, 24, flags as i32);
        message
    }

    #[test]
    fn test_the_probe_finds_and_reserves_the_card() {
        let mut ipc = CannedTransport::new();
        ipc.reply_sendrec(Ok(found(0, 0x1234, 0x1111)));
        ipc.reply_sendrec(Ok(found(2, VENDOR_ID, DEVICE_ID)));
        ipc.reply_sendrec(Ok(minix_types::Message::default()));
        ipc.reply_sendrec(Ok(minix_types::Message::default()));
        ipc.reply_sendrec(Ok(attribute(9)));

        let probed = probe(&ipc, Endpoint(5)).expect("the card is there");
        assert_eq!(probed, Probed { irq: 9, index: 2 });

        let sent = ipc.sent.borrow();
        assert_eq!(sent.len(), 5);
        assert_eq!(sent[0].1.m_type, busc::FIRST_DEV);
        assert_eq!(sent[1].1.m_type, busc::NEXT_DEV);
        assert_eq!(sent[2].1.m_type, busc::RESERVE);
        assert_eq!(sent[3].1.m_type, busc::ATTR_W16);
        assert_eq!(
            read_i32(&sent[3].1, M2_L1),
            (SERR_EN | PCI_MASTER | IO_ACCESS) as i32
        );
        assert_eq!(sent[4].1.m_type, busc::ATTR_R8);
    }

    #[test]
    fn test_the_bar_fetch_reads_the_three_outputs() {
        let mut ipc = CannedTransport::new();
        ipc.reply_sendrec(Ok(bar(0xf000_0000, 0x1000, 0)));
        let fetched = get_bar(&ipc, Endpoint(5), 2, PCI_BAR).expect("the BAR answers");
        assert_eq!(
            fetched,
            Bar {
                base: 0xf000_0000,
                size: 0x1000,
                io: false
            }
        );
        let sent = ipc.sent.borrow();
        assert_eq!(sent[0].1.m_type, busc::GET_BAR);
        assert_eq!(read_i32(&sent[0].1, M2_I1), 2);
        assert_eq!(read_i32(&sent[0].1, M2_I2), PCI_BAR as i32);
    }

    #[test]
    fn test_an_io_window_is_reported_as_such() {
        let mut ipc = CannedTransport::new();
        ipc.reply_sendrec(Ok(bar(0x0000_d240, 0x100, 1)));
        let fetched = get_bar(&ipc, Endpoint(5), 0, PCI_BAR).expect("the BAR answers");
        assert!(fetched.io, "an I/O window is not a register mapping");
    }

    #[test]
    fn test_a_refused_bar_fetch_is_reported() {
        let mut ipc = CannedTransport::new();
        ipc.reply_sendrec(Ok(minix_types::Message {
            m_type: minix_types::ENOSYS,
            ..minix_types::Message::default()
        }));
        assert_eq!(
            get_bar(&ipc, Endpoint(5), 0, PCI_BAR),
            Err(minix_types::ENOSYS)
        );
    }

    #[test]
    fn test_a_bus_without_the_card_reports_no_device() {
        let mut ipc = CannedTransport::new();
        ipc.reply_sendrec(Ok(minix_types::Message::default()));
        assert_eq!(probe(&ipc, Endpoint(5)), Err(-ENODEV));
    }
}
