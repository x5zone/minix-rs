//! PCI client library: the driver-side queries against the PCI server.
//!
//! C: `minix3/minix/lib/libsys/pci_first_dev.c`, `pci_next_dev.c`,
//! `pci_reserve.c`, `pci_attr_r8.c`, `pci_attr_r32.c`, `pci_attr_w16.c`,
//! and `pci_get_bar.c` — the libsys calls every PCI device driver makes
//! before it touches its card. The request numbers are `BUSC_PCI_*`
//! (`com.h:103-141`) and the configuration offsets come from
//! `arch/i386/include/pci.h`.
//!
//! The C client panics at every failed step; this leg returns the failure
//! so a driver refuses to start instead of aborting the process. The one
//! exception is [`write_attribute16`], whose C counterpart returns void —
//! there is no failure channel to translate.
//!
//! # Why one leg and not six
//!
//! Six drivers carried the same five-step probe (the VMMDev guest, and the
//! five sound cards that reach their registers through a PCI BAR), each
//! with its own copy of the request numbers, the lane offsets, and the
//! message shapes. They differ in exactly three places — the vendor and
//! device identifiers, the mask applied to the BAR, and whether the
//! command register is written as a read-modify-write or as a constant —
//! so those stay with the drivers and everything else lives here. The C
//! side has the same split: one libsys client, one call site per driver.

use crate::ipc::IpcTransport;
use minix_types::{ENODEV, Endpoint, PhysBytes, VirBytes};

/// PCI configuration offset: the command register (`PCI_CR`,
/// `arch/i386/include/pci.h:10`).
pub const PCI_CR: u32 = 0x04;
/// PCI configuration offset: the first Base Address Register (`PCI_BAR`,
/// `pci.h:32`).
pub const PCI_BAR: u32 = 0x10;
/// PCI configuration offset: the interrupt line register (`PCI_ILR`,
/// `pci.h:52`).
pub const PCI_ILR: u32 = 0x3C;

/// The bus-controller call numbers the probe uses (`BUSC_PCI_*`,
/// `com.h:103-141`).
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
    /// Fetch one Base Address Register (`BUSC_PCI_GET_BAR`).
    pub const GET_BAR: i32 = REQUEST_BASE + 19;
}

/// One Base Address Register (`pci_get_bar`'s three outputs).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Bar {
    /// The register's base address.
    pub base: u32,
    /// How much address space it claims.
    pub size: u64,
    /// Whether the register describes an I/O port window (`ioflag`).
    pub io: bool,
}

/// The `m1i1` slot inside the message union (C `m1_i1`): the device index
/// of a find reply and the value lane of an attribute reply.
pub const M1_I1: usize = 8;
/// The `m1i2` slot (C `m1_i2`): the vendor identifier.
pub const M1_I2: usize = 12;
/// The `m1i3` slot (C `m1_i3`): the device identifier.
pub const M1_I3: usize = 16;
/// The `m2i1` slot (C `m2_i1`): the device index of an attribute request
/// and the base address of a BAR reply.
pub const M2_I1: usize = 8;
/// The `m2i2` slot (C `m2_i2`): the configuration offset.
pub const M2_I2: usize = 12;
/// The `m2l1` slot (C `m2_l1`): the attribute read's value and the write's
/// value.
pub const M2_L1: usize = 32;
/// The BAR reply's size lane (`mess_pci_lsys_busc_get_bar.size`, a
/// `size_t` at offset eight of the payload).
const BAR_SIZE: usize = 16;
/// The BAR reply's flags lane (`.flags` at offset sixteen).
const BAR_FLAGS: usize = 24;

fn read_i32(message: &minix_types::Message, offset: usize) -> i32 {
    // SAFETY: byte-level read of the union payload at a documented offset.
    let raw = unsafe { message.m_u.raw };
    i32::from_ne_bytes(raw[offset..offset + 4].try_into().unwrap())
}

fn read_u64(message: &minix_types::Message, offset: usize) -> u64 {
    // SAFETY: byte-level read of the union payload at a documented offset.
    let raw = unsafe { message.m_u.raw };
    u64::from_ne_bytes(raw[offset..offset + 8].try_into().unwrap())
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

/// Finds the first device with these identifiers and reserves it
/// (`pci_first_dev`/`pci_next_dev` then `pci_reserve`,
/// `pci_first_dev.c` and friends).
///
/// The reply's message type is one when a device exists, zero when the bus
/// is empty, and a negative errno otherwise; the index and the two
/// identifiers ride `m1i1..m1i3`. The reserve reply's type is zero on
/// success.
pub fn find_and_reserve(
    ipc: &impl IpcTransport,
    pci: Endpoint,
    vendor: u16,
    device: u16,
) -> Result<i32, i32> {
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
    let mut found_vendor = read_i32(&message, M1_I2) as u16;
    let mut found_device = read_i32(&message, M1_I3) as u16;
    while found_vendor != vendor || found_device != device {
        message = minix_types::Message {
            m_type: busc::NEXT_DEV,
            ..minix_types::Message::default()
        };
        write_i32(&mut message, M1_I1, index);
        sendrec(ipc, pci, &mut message)?;
        match message.m_type {
            1 => {
                index = read_i32(&message, M1_I1);
                found_vendor = read_i32(&message, M1_I2) as u16;
                found_device = read_i32(&message, M1_I3) as u16;
            }
            0 => return Err(-ENODEV),
            error => return Err(error),
        }
    }

    let mut message = minix_types::Message {
        m_type: busc::RESERVE,
        ..minix_types::Message::default()
    };
    write_i32(&mut message, M1_I1, index);
    sendrec(ipc, pci, &mut message)?;
    if message.m_type != 0 {
        return Err(message.m_type);
    }
    Ok(index)
}

/// Reads one eight-bit attribute (`pci_attr_r8`): the index and the
/// configuration offset go in `m2i1`/`m2i2`, the value comes back in
/// `m2l1` with a zero message type.
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

/// Reads one thirty-two-bit attribute (`pci_attr_r32`).
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

/// Writes one sixteen-bit attribute (`pci_attr_w16`), whose C client
/// returns void: a refused write is not reportable, so the driver's first
/// register access fails instead if the write did not take.
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
        // C panics here; the caller's next register access is the honest
        // failure instead.
    }
}

/// Fetches one Base Address Register (`pci_get_bar`, `pci_get_bar.c`).
pub fn get_bar(
    ipc: &impl IpcTransport,
    pci: Endpoint,
    index: i32,
    register: u32,
) -> Result<Bar, i32> {
    let mut message = minix_types::Message {
        m_type: busc::GET_BAR,
        ..minix_types::Message::default()
    };
    write_i32(&mut message, M2_I1, index);
    write_i32(&mut message, M2_I2, register as i32);
    sendrec(ipc, pci, &mut message)?;
    if message.m_type != 0 {
        return Err(message.m_type);
    }
    Ok(Bar {
        base: read_i32(&message, M2_I1) as u32,
        size: read_u64(&message, BAR_SIZE),
        io: read_i32(&message, BAR_FLAGS) != 0,
    })
}

/// Maps a physical span for this process (`vm_map_phys(SELF, base, size)`,
/// `vm_map_phys.c`) and returns the address the VM server placed.
///
/// This is the step a driver whose register window is memory-mapped needs
/// after [`get_bar`] reported a memory window.
pub fn map_bar(ipc: &impl IpcTransport, physical: u32, size: u64) -> Result<*mut u8, i32> {
    let placed = crate::vm::map_physical_via(
        ipc,
        Endpoint::SELF,
        PhysBytes(physical as u64),
        VirBytes(size),
    )
    .map_err(|error| -error.to_i32())?;
    Ok(placed.0 as *mut u8)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ipc::CannedTransport;

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
            message.m_u.raw[BAR_SIZE..BAR_SIZE + 8].copy_from_slice(&size.to_ne_bytes());
        }
        write_i32(&mut message, BAR_FLAGS, flags as i32);
        message
    }

    #[test]
    fn test_the_find_walks_until_the_identifiers_match_then_reserves() {
        let mut ipc = CannedTransport::new();
        ipc.reply_sendrec(Ok(found(0, 0x1234, 0x1111)));
        ipc.reply_sendrec(Ok(found(3, 0x80ee, 0xcafe)));
        ipc.reply_sendrec(Ok(minix_types::Message::default()));
        let index = find_and_reserve(&ipc, Endpoint(5), 0x80ee, 0xcafe).expect("found");
        assert_eq!(index, 3);
        let sent = ipc.sent.borrow();
        assert_eq!(sent.len(), 3);
        assert_eq!(sent[0].1.m_type, busc::FIRST_DEV);
        assert_eq!(sent[1].1.m_type, busc::NEXT_DEV);
        assert_eq!(read_i32(&sent[1].1, M1_I1), 0, "next starts after index 0");
        assert_eq!(sent[2].1.m_type, busc::RESERVE);
        assert_eq!(read_i32(&sent[2].1, M1_I1), 3);
    }

    #[test]
    fn test_an_empty_bus_reports_no_device() {
        let mut ipc = CannedTransport::new();
        ipc.reply_sendrec(Ok(minix_types::Message::default()));
        assert_eq!(find_and_reserve(&ipc, Endpoint(5), 1, 2), Err(-ENODEV));
    }

    #[test]
    fn test_a_bus_without_the_card_reports_no_device() {
        let mut ipc = CannedTransport::new();
        ipc.reply_sendrec(Ok(found(0, 0x1234, 0x1111)));
        ipc.reply_sendrec(Ok(minix_types::Message::default()));
        assert_eq!(
            find_and_reserve(&ipc, Endpoint(5), 0x80ee, 0xcafe),
            Err(-ENODEV)
        );
    }

    #[test]
    fn test_a_refused_reserve_is_reported() {
        let mut ipc = CannedTransport::new();
        ipc.reply_sendrec(Ok(found(0, 0x80ee, 0xcafe)));
        ipc.reply_sendrec(Ok(minix_types::Message {
            m_type: minix_types::EBUSY,
            ..minix_types::Message::default()
        }));
        assert_eq!(
            find_and_reserve(&ipc, Endpoint(5), 0x80ee, 0xcafe),
            Err(minix_types::EBUSY)
        );
    }

    #[test]
    fn test_the_attribute_reads_use_the_two_slot_request() {
        let mut ipc = CannedTransport::new();
        ipc.reply_sendrec(Ok(attribute(0x0d)));
        assert_eq!(read_attribute8(&ipc, Endpoint(5), 2, PCI_ILR), Ok(0x0d));
        let sent = ipc.sent.borrow();
        assert_eq!(sent[0].1.m_type, busc::ATTR_R8);
        assert_eq!(read_i32(&sent[0].1, M2_I1), 2);
        assert_eq!(read_i32(&sent[0].1, M2_I2), PCI_ILR as i32);

        let mut ipc = CannedTransport::new();
        ipc.reply_sendrec(Ok(attribute(0xf000_0000)));
        assert_eq!(
            read_attribute32(&ipc, Endpoint(5), 2, PCI_BAR),
            Ok(0xf000_0000)
        );
        let sent = ipc.sent.borrow();
        assert_eq!(sent[0].1.m_type, busc::ATTR_R32);
    }

    #[test]
    fn test_a_refused_attribute_read_is_reported() {
        let mut ipc = CannedTransport::new();
        // The attribute passthrough is the PCI server's own registered
        // seam: it answers ENOSYS today, and no caller may invent a value.
        ipc.reply_sendrec(Ok(minix_types::Message {
            m_type: minix_types::ENOSYS,
            ..minix_types::Message::default()
        }));
        assert_eq!(
            read_attribute32(&ipc, Endpoint(5), 0, PCI_BAR),
            Err(minix_types::ENOSYS)
        );
    }

    #[test]
    fn test_the_attribute_write_carries_the_value_in_the_long_slot() {
        let mut ipc = CannedTransport::new();
        ipc.reply_sendrec(Ok(minix_types::Message::default()));
        write_attribute16(&ipc, Endpoint(5), 3, PCI_CR, 0x0105);
        let sent = ipc.sent.borrow();
        assert_eq!(sent[0].1.m_type, busc::ATTR_W16);
        assert_eq!(read_i32(&sent[0].1, M2_I1), 3);
        assert_eq!(read_i32(&sent[0].1, M2_I2), PCI_CR as i32);
        assert_eq!(read_i32(&sent[0].1, M2_L1), 0x0105);
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
}
