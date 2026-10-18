//! The PCI probe: find the card, reserve it, fetch its BAR, map it.
//!
//! C correspondence: `dev_probe` (`cs4281.c:317-355`) and the libsys
//! client calls it makes — `pci_init`/`pci_first_dev`/`pci_next_dev`
//! (`libsys/pci_first_dev.c`, `pci_next_dev.c`), `pci_reserve`
//! (`pci_reserve.c`), `pci_get_bar` (`pci_get_bar.c`), `pci_attr_r8`
//! (`pci_attr_r8.c`), and `pci_attr_w16` (`pci_attr_w16.c`) — plus the
//! mapping step `vm_map_phys(SELF, base, size)` (`cs4281.c:313-316`).
//! The request numbers are `BUSC_PCI_*` (`com.h:103-141`) and the
//! configuration offsets come from `arch/i386/include/pci.h`.
//!
//! This card's register window is memory-mapped, so its probe is the only
//! one in the family that maps the BAR: `dev_probe` walks the six BARs,
//! keeps the memory ones, and stores the mapped addresses; only the first
//! is ever used (`reg(n)` reads `base[0]`, `cs4281.c:26`), which is what
//! [`map_bar`] returns.

use minix_sys::ipc::IpcTransport;
use minix_types::{ENODEV, Endpoint, PhysBytes, VirBytes};

use crate::registers::{DEVICE_ID, VENDOR_ID};

/// PCI configuration offset: the command register (`PCI_CR`,
/// `pci.h:10`).
pub const PCI_CR: u32 = 0x04;
/// PCI configuration offset: the first Base Address Register (`PCI_BAR`,
/// `pci.h:32`).
pub const PCI_BAR: u32 = 0x10;
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
    /// Fetch one Base Address Register (`BUSC_PCI_GET_BAR`).
    pub const GET_BAR: i32 = REQUEST_BASE + 19;
}

/// What the probe found (`dev.irq` and the device index).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Probed {
    /// The interrupt line.
    pub irq: u8,
    /// The device index, for the later attribute and BAR traffic.
    pub index: i32,
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

/// Reads one thirty-two-bit attribute (C `pci_attr_r32`).
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
        // C panics here; the caller's next window access is the honest
        // failure instead.
    }
}

/// Fetches one Base Address Register (C `pci_get_bar`, `pci_get_bar.c`):
/// the device index and the configuration offset go in `m2i1`/`m2i2`, the
/// reply carries the base, the size, and the I/O flag.
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
/// `cs4281.c:313-316`) and returns the address the VM server placed.
pub fn map_bar(ipc: &impl IpcTransport, physical: u32, size: u64) -> Result<*mut u8, i32> {
    let placed = minix_sys::vm::map_physical_via(
        ipc,
        Endpoint::SELF,
        PhysBytes(physical as u64),
        VirBytes(size),
    )
    .map_err(|error| -error.to_i32())?;
    Ok(placed.0 as *mut u8)
}

/// Finds the CS4281, reserves it, enables its I/O, bus-master, and SERR
/// bits, and reads its interrupt line (C `dev_probe`, `cs4281.c:317-355`,
/// including the command-register write at `cs4281.c:350-351`).
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

    // Enable the I/O space, bus-master bit, and SERR driver
    // (`cs4281.c:350-351`).
    let command = read_attribute32(ipc, pci, index, PCI_CR)? as u16;
    write_attribute16(ipc, pci, index, PCI_CR, command | 0x105);

    let irq = read_attribute8(ipc, pci, index, PCI_ILR)?;
    Ok(Probed { irq, index })
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
    fn test_the_probe_finds_and_reserves_the_card() {
        let mut ipc = CannedTransport::new();
        ipc.reply_sendrec(Ok(found(0, 0x1234, 0x1111)));
        ipc.reply_sendrec(Ok(found(2, VENDOR_ID, DEVICE_ID)));
        ipc.reply_sendrec(Ok(minix_types::Message::default()));
        ipc.reply_sendrec(Ok(attribute(0x0000_0005)));
        ipc.reply_sendrec(Ok(minix_types::Message::default()));
        ipc.reply_sendrec(Ok(attribute(9)));

        let probed = probe(&ipc, Endpoint(5)).expect("the card is there");
        assert_eq!(probed, Probed { irq: 9, index: 2 });

        let sent = ipc.sent.borrow();
        assert_eq!(sent.len(), 6);
        assert_eq!(sent[0].1.m_type, busc::FIRST_DEV);
        assert_eq!(sent[1].1.m_type, busc::NEXT_DEV);
        assert_eq!(sent[2].1.m_type, busc::RESERVE);
        assert_eq!(sent[3].1.m_type, busc::ATTR_R32);
        assert_eq!(sent[4].1.m_type, busc::ATTR_W16);
        assert_eq!(read_i32(&sent[4].1, M2_L1), 0x105 | 0x0005);
        assert_eq!(sent[5].1.m_type, busc::ATTR_R8);
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
