//! The PCI probe: find the card, reserve it, read its port and IRQ.
//!
//! C correspondence: `dev_probe` (`cmi8738.c:224-283`) and the libsys client calls it
//! makes — `pci_first_dev`/`pci_next_dev`, `pci_reserve`, `pci_attr_r8`,
//! `pci_attr_r32`, and `pci_attr_w16`. The five-step leg itself lives in
//! `minix_sys::pci` (the C's own libsys placement); what stays here is
//! the three card-specific parts: the identifiers, the BAR mask, and the
//! command-register policy.

use minix_sys::ipc::IpcTransport;
use minix_types::Endpoint;

use crate::registers::{DEVICE_ID, VENDOR_ID};

pub use minix_sys::pci::{
    M1_I1, M1_I2, M1_I3, M2_I1, M2_I2, M2_L1, PCI_BAR, PCI_CR, PCI_ILR, busc,
};

/// The address mask this card's driver applies to its BAR
/// (`cmi8738.c:270-272`: `pci_attr_r32(devind, PCI_BAR + i * 4) &
/// 0xffffffe0` under the mapped variant; the port variant this build
/// models masks with `0xfffffffc`, keeping the low flag bit's
/// neighbours clear).
pub const PCI_BAR_IO_MASK: u32 = 0xFFFF_FFFC;

/// What the probe found (`dev.base[0]` and `dev.irq`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Probed {
    /// The card's I/O port (the BAR with its flag bits masked off).
    pub port: u16,
    /// The interrupt line.
    pub irq: u8,
    /// The device index, for the later attribute traffic.
    pub index: i32,
}

/// Finds the CMI8738, reserves it, enables its I/O and bus-master bits,
/// and reads its port and interrupt line (C `dev_probe`,
/// `cmi8738.c:224-283`).
pub fn probe(ipc: &impl IpcTransport, pci: Endpoint) -> Result<Probed, i32> {
    let index = minix_sys::pci::find_and_reserve(ipc, pci, VENDOR_ID, DEVICE_ID)?;
    // Enable the card's I/O space, bus-master bit, and SERR
    // (`cmi8738.c:277-278`: `temp = pci_attr_r16(devind, PCI_CR);
    // pci_attr_w16(devind, PCI_CR, temp | 0x105)`).
    let command = minix_sys::pci::read_attribute32(ipc, pci, index, PCI_CR)? as u16;
    minix_sys::pci::write_attribute16(ipc, pci, index, PCI_CR, command | 0x105);
    let bar = minix_sys::pci::read_attribute32(ipc, pci, index, PCI_BAR)?;
    let irq = minix_sys::pci::read_attribute8(ipc, pci, index, PCI_ILR)?;
    Ok(Probed {
        port: (bar & PCI_BAR_IO_MASK) as u16,
        irq,
        index,
    })
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

    #[test]
    fn test_the_probe_finds_reserves_and_reads_the_card() {
        let mut ipc = CannedTransport::new();
        // The first device is somebody else's; the second is the card.
        ipc.reply_sendrec(Ok(found(0, 0x1234, 0x1111)));
        ipc.reply_sendrec(Ok(found(3, VENDOR_ID, DEVICE_ID)));
        // Reserve succeeds (type zero).
        ipc.reply_sendrec(Ok(minix_types::Message::default()));
        // The command register and the BAR read back.
        ipc.reply_sendrec(Ok(attribute(0x0000_0005)));
        ipc.reply_sendrec(Ok(minix_types::Message::default()));
        ipc.reply_sendrec(Ok(attribute(0x0000_d241)));
        ipc.reply_sendrec(Ok(attribute(11)));

        let probed = probe(&ipc, Endpoint(5)).expect("the card is there");
        assert_eq!(
            probed,
            Probed {
                port: 0xd240,
                irq: 11,
                index: 3,
            }
        );

        // The outgoing wire: call numbers and fields in order.
        let sent = ipc.sent.borrow();
        assert_eq!(sent.len(), 7);
        assert_eq!(sent[0].1.m_type, busc::FIRST_DEV);
        assert_eq!(sent[1].1.m_type, busc::NEXT_DEV);
        assert_eq!(read_i32(&sent[1].1, M1_I1), 0, "next starts after index 0");
        assert_eq!(sent[2].1.m_type, busc::RESERVE);
        assert_eq!(read_i32(&sent[2].1, M1_I1), 3);
        assert_eq!(sent[3].1.m_type, busc::ATTR_R32);
        assert_eq!(read_i32(&sent[3].1, M2_I2), PCI_CR as i32);
        assert_eq!(sent[4].1.m_type, busc::ATTR_W16);
        assert_eq!(read_i32(&sent[4].1, M2_I1), 3);
        assert_eq!(read_i32(&sent[4].1, M2_I2), PCI_CR as i32);
        assert_eq!(read_i32(&sent[4].1, M2_L1), 0x105 | 0x0005);
        assert_eq!(sent[5].1.m_type, busc::ATTR_R32);
        assert_eq!(read_i32(&sent[5].1, M2_I2), PCI_BAR as i32);
        assert_eq!(sent[6].1.m_type, busc::ATTR_R8);
        assert_eq!(read_i32(&sent[6].1, M2_I2), PCI_ILR as i32);
    }

    #[test]
    fn test_an_empty_bus_reports_no_device() {
        let mut ipc = CannedTransport::new();
        ipc.reply_sendrec(Ok(minix_types::Message::default()));
        assert_eq!(probe(&ipc, Endpoint(5)), Err(-ENODEV));
    }

    #[test]
    fn test_a_bus_without_the_card_reports_no_device() {
        let mut ipc = CannedTransport::new();
        ipc.reply_sendrec(Ok(found(0, 0x1234, 0x1111)));
        ipc.reply_sendrec(Ok(minix_types::Message::default()));
        assert_eq!(probe(&ipc, Endpoint(5)), Err(-ENODEV));
    }

    #[test]
    fn test_a_refused_attribute_read_is_reported() {
        let mut ipc = CannedTransport::new();
        ipc.reply_sendrec(Ok(found(0, VENDOR_ID, DEVICE_ID)));
        ipc.reply_sendrec(Ok(minix_types::Message::default()));
        // The attribute passthrough is the PCI server's registered seam:
        // it answers ENOSYS today, and the probe must not invent a port.
        ipc.reply_sendrec(Ok(minix_types::Message {
            m_type: minix_types::ENOSYS,
            ..minix_types::Message::default()
        }));
        assert_eq!(probe(&ipc, Endpoint(5)), Err(minix_types::ENOSYS));
    }
}
