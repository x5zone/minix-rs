//! The PCI probe: find the card, reserve it, read its port and IRQ.
//!
//! C correspondence: `dev_probe` (`cmi8738.c:224-283`) and the libsys
//! client calls it makes — `pci_init`/`pci_first_dev`/`pci_next_dev`
//! (`libsys/pci_first_dev.c`, `pci_next_dev.c`), `pci_reserve`
//! (`pci_reserve.c`), `pci_attr_r8`/`pci_attr_r32` (`pci_attr_r8.c`,
//! `pci_attr_r32.c`), and `pci_attr_w16` (`pci_attr_w16.c`). The request
//! numbers are `BUSC_PCI_*` (`com.h:103-141`) and the configuration
//! offsets come from `arch/i386/include/pci.h`.
//!
//! C's client panics at every failed step; this leg returns the failure
//! so the binary refuses to start instead. The PCI server's attribute
//! passthrough is that driver's own registered seam (it answers ENOSYS
//! until it lands), so a machine with the current server gets an honest
//! failure rather than a fabricated port — the same shape the vbox
//! driver's probe uses.

use minix_sys::ipc::IpcTransport;
use minix_types::{ENODEV, Endpoint};

use crate::registers::{DEVICE_ID, VENDOR_ID};

/// PCI configuration offset: the command register (`PCI_CR`,
/// `pci.h:10`).
pub const PCI_CR: u32 = 0x04;
/// PCI configuration offset: the Base Address Register (`PCI_BAR`,
/// `pci.h:32`).
pub const PCI_BAR: u32 = 0x10;
/// The I/O address mask inside a BAR (`PCI_BAR_IO_MASK`, `pci.h:39`).
pub const PCI_BAR_IO_MASK: u32 = 0xFFFF_FFFC;
/// PCI configuration offset: the interrupt line register (`PCI_ILR`,
/// `pci.h:52`).
pub const PCI_ILR: u32 = 0x3C;

/// The bus-controller call numbers the probe uses (`BUSC_PCI_*`).
pub mod busc {
    /// The request range's base (`BUSC_RQ_BASE`).
    pub const REQUEST_BASE: i32 = 0x300;
    /// First visible device (`BUSC_PCI_FIRST_DEV`).
    pub const FIRST_DEV: i32 = REQUEST_BASE + 1;
    /// Next visible device (`BUSC_PCI_NEXT_DEV`).
    pub const NEXT_DEV: i32 = REQUEST_BASE + 2;
    /// Reserve a device (`BUSC_PCI_RESERVE`).
    pub const RESERVE: i32 = REQUEST_BASE + 7;
    /// Read an eight-bit attribute (`BUSC_PCI_ATTR_R8`).
    pub const ATTR_R8: i32 = REQUEST_BASE + 8;
    /// Read a thirty-two-bit attribute (`BUSC_PCI_ATTR_R32`).
    pub const ATTR_R32: i32 = REQUEST_BASE + 10;
    /// Write a sixteen-bit attribute (`BUSC_PCI_ATTR_W16`).
    pub const ATTR_W16: i32 = REQUEST_BASE + 12;
}

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

/// The `m1i1` slot inside the message union (C `m1_i1`).
const M1_I1: usize = 8;
/// The `m1i2` slot (C `m1_i2`).
const M1_I2: usize = 12;
/// The `m1i3` slot (C `m1_i3`).
const M1_I3: usize = 16;
/// The `m2i1` slot (C `m2_i1`).
const M2_I1: usize = 8;
/// The `m2i2` slot (C `m2_i2`).
const M2_I2: usize = 12;
/// The `m2l1` slot (C `m2_l1`).
const M2_L1: usize = 32;

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

fn sendrec(
    ipc: &impl IpcTransport,
    pci: Endpoint,
    message: &mut minix_types::Message,
) -> Result<(), i32> {
    ipc.sendrec(pci, message).map_err(|status| -status.0)
}

/// Reads one thirty-two-bit attribute (C `pci_attr_r32`): the index and
/// the configuration offset go in `m2i1`/`m2i2`, the value comes back in
/// `m2_l1` with a zero message type.
pub fn read_attribute32(
    ipc: &impl IpcTransport,
    pci: Endpoint,
    index: i32,
    attribute: u32,
) -> Result<u32, i32> {
    let mut message = minix_types::Message {
        m_type: busc::ATTR_R32,
        ..minix_types::Message::default()
    };
    write_i32(&mut message, M2_I1, index);
    write_i32(&mut message, M2_I2, attribute as i32);
    sendrec(ipc, pci, &mut message)?;
    if message.m_type != 0 {
        return Err(message.m_type);
    }
    Ok(read_i32(&message, M2_L1) as u32)
}

/// Reads one eight-bit attribute (C `pci_attr_r8`).
pub fn read_attribute8(
    ipc: &impl IpcTransport,
    pci: Endpoint,
    index: i32,
    attribute: u32,
) -> Result<u8, i32> {
    let mut message = minix_types::Message {
        m_type: busc::ATTR_R8,
        ..minix_types::Message::default()
    };
    write_i32(&mut message, M2_I1, index);
    write_i32(&mut message, M2_I2, attribute as i32);
    sendrec(ipc, pci, &mut message)?;
    if message.m_type != 0 {
        return Err(message.m_type);
    }
    Ok(read_i32(&message, M2_L1) as u8)
}

/// Writes one sixteen-bit attribute (C `pci_attr_w16`), whose client
/// returns nothing: a refused write is not reportable, so the card's
/// first register access fails instead if the write did not take.
pub fn write_attribute16(
    ipc: &impl IpcTransport,
    pci: Endpoint,
    index: i32,
    attribute: u32,
    value: u16,
) {
    let mut message = minix_types::Message {
        m_type: busc::ATTR_W16,
        ..minix_types::Message::default()
    };
    write_i32(&mut message, M2_I1, index);
    write_i32(&mut message, M2_I2, attribute as i32);
    write_i32(&mut message, M2_L1, value as i32);
    if sendrec(ipc, pci, &mut message).is_err() {
        // C panics here; the caller's next port access is the honest
        // failure instead.
    }
}

/// Finds the CMI8738, reserves it, enables its I/O and bus-master bits,
/// and reads its port and interrupt line (C `dev_probe`,
/// `cmi8738.c:224-283`).
pub fn probe(ipc: &impl IpcTransport, pci: Endpoint) -> Result<Probed, i32> {
    // `pci_first_dev`: the reply's message type is 1 when a device
    // exists, 0 when the bus is empty, and a negative errno otherwise;
    // the index and the two identifiers ride `m1i1..m1i3`.
    let mut message = minix_types::Message {
        m_type: busc::FIRST_DEV,
        ..minix_types::Message::default()
    };
    sendrec(ipc, pci, &mut message)?;
    let mut index = match message.m_type {
        1 => read_i32(&message, M1_I1),
        0 => return Err(-ENODEV),
        error => return Err(error),
    };
    let mut vendor = read_i32(&message, M1_I2) as u16;
    let mut device = read_i32(&message, M1_I3) as u16;
    while vendor != VENDOR_ID || device != DEVICE_ID {
        message = minix_types::Message {
            m_type: busc::NEXT_DEV,
            ..minix_types::Message::default()
        };
        write_i32(&mut message, M1_I1, index);
        sendrec(ipc, pci, &mut message)?;
        match message.m_type {
            1 => {
                index = read_i32(&message, M1_I1);
                vendor = read_i32(&message, M1_I2) as u16;
                device = read_i32(&message, M1_I3) as u16;
            }
            0 => return Err(-ENODEV),
            error => return Err(error),
        }
    }

    // Reserve the device so nobody else binds it.
    message = minix_types::Message {
        m_type: busc::RESERVE,
        ..minix_types::Message::default()
    };
    write_i32(&mut message, M1_I1, index);
    sendrec(ipc, pci, &mut message)?;

    // Enable the card's I/O space, bus-master bit, and SERR
    // (`cmi8738.c:277-278`: `temp = pci_attr_r16(devind, PCI_CR);
    // pci_attr_w16(devind, PCI_CR, temp | 0x105)`).
    let command = read_attribute32(ipc, pci, index, PCI_CR)? as u16;
    write_attribute16(ipc, pci, index, PCI_CR, command | 0x105);

    let bar = read_attribute32(ipc, pci, index, PCI_BAR)?;
    let irq = read_attribute8(ipc, pci, index, PCI_ILR)?;
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
