//! The VirtualBox backdoor port and the system facilities the driver
//! touches: the device register write, the clock, the periodic alarm, the
//! interrupt re-enable, the physical address of a request page, and the
//! grant-to-physical mapping the call path needs.
//!
//! C correspondence: `vbox.c`'s `vbox_request` (the `sys_outl` into the
//! backdoor port), `vbox_update_time` (`time`/`stime`/`sys_setalarm`),
//! `vbox_intr` (`sys_irqenable`), and `hgcm.c`'s `store_pages`
//! (`sys_vumap`). Every one of these is a kernel or server call the C
//! driver cannot test off-target, which is why they sit behind this seam:
//! the service's logic (the drift rule, the connection state machine)
//! stays host-testable, and the mechanism lives in [`KernelBackdoor`].
//!
//! The clock verbs deliberately mirror the driver runtime transport's
//! `now_seconds`; this driver needs them on the same seam as the backdoor
//! port because the drift decision interleaves one device request with the
//! clock, and the seam is what the tests script.

use minix_sys::ipc::{DirectTrapTransport, IpcTransport};
use minix_sys::syscall::{DirectKernelCallTransport, sys_outl, sys_setalarm, sys_vumap};
use minix_types::{ENOSYS, Endpoint, VirBytes};

/// One physical span of a mapped grant (`struct vumap_phys`), re-exported
/// from the kernel-call wrapper that fills it.
pub use minix_sys::syscall::VumapPhys;

/// PCI configuration offset: the Base Address Register (`PCI_BAR`,
/// `arch/i386/include/pci.h:32`).
pub const PCI_BAR: u32 = 0x10;
/// The I/O address mask inside a BAR (`PCI_BAR_IO_MASK`,
/// `pci.h:39`).
pub const PCI_BAR_IO_MASK: u32 = 0xFFFF_FFFC;
/// PCI configuration offset: the interrupt line register (`PCI_ILR`,
/// `pci.h:52`).
pub const PCI_ILR: u32 = 0x3C;

/// The bus-controller call numbers the PCI probe uses (`com.h:103-122`,
/// `BUSC_PCI_*`). The five-step leg itself lives in `minix_sys::pci` (the
/// C's own libsys placement); this driver keeps only its identifiers, its
/// BAR mask, and the fact that it writes no command register.
pub use minix_sys::pci::busc;

/// The backdoor mechanism: what the driver cannot do without a device.
pub trait Backdoor {
    /// Writes the 32-bit backdoor command register (C
    /// `sys_outl(port, addr)`, `vbox.c:41`): the value is the physical
    /// address of the request page, and the port belongs to the backdoor
    /// itself (C keeps it in a file-static global).
    fn outl(&mut self, value: u32) -> Result<(), i32>;

    /// The physical address of one of the driver's request pages
    /// (C `alloc_contig(VMMDEV_BUF_SIZE, 0, &addr)`'s out-parameter,
    /// `vbox.c:121`).
    fn physical(&mut self, buffer: *const u8) -> Result<u64, i32>;

    /// The current wall-clock time in seconds (C `time(&otime)`,
    /// `vbox.c:68`).
    fn now_seconds(&mut self) -> Result<i64, i32>;

    /// Sets the wall clock (C `stime(&ntime)`, `vbox.c:75`; PM's
    /// `do_stime` re-anchors the boot time and gates on SUPER_USER).
    fn set_time(&mut self, seconds: i64) -> Result<(), i32>;

    /// Arms the periodic alarm in clock ticks (C `sys_setalarm(ticks, 0)`,
    /// `vbox.c:79`; relative, so the next host-time check follows after
    /// `ticks` ticks).
    fn set_alarm(&mut self, ticks: u64) -> Result<(), i32>;

    /// The system clock frequency in ticks per second (C `sys_hz()`,
    /// `vbox.c:130`; the alarm period is this times the interval).
    fn hz(&mut self) -> Result<u32, i32>;

    /// Re-enables the interrupt after an interrupt was served
    /// (C `sys_irqenable(&hook_id)`, `vbox.c:196`).
    fn irq_enable(&mut self) -> Result<(), i32>;

    /// Maps a caller's grant into physical spans (C `sys_vumap`,
    /// `hgcm.c:472-479`): `grant_size` is the vector element's `vv_size`
    /// (the grant's total length) and `offset` the read start inside it.
    /// Returns the number of spans written into `out` (C's `pcount`).
    fn map_grant(
        &mut self,
        granter: Endpoint,
        grant: i32,
        grant_size: u64,
        offset: u64,
        access: i32,
        out: &mut [VumapPhys],
    ) -> Result<usize, i32>;
}

/// The real backdoor: kernel calls plus this process's own endpoint.
pub struct KernelBackdoor {
    /// This process's endpoint (the grant mapper needs it: `sys_vumap`
    /// resolves the grant against the granter named in the message).
    endpoint: Endpoint,
    /// The device's I/O port, from the PCI probe (C's `port` global).
    port: u16,
}

impl KernelBackdoor {
    /// A backdoor over the given port for this process.
    pub fn new(endpoint: Endpoint, port: u16) -> Self {
        KernelBackdoor { endpoint, port }
    }

    /// The device port (the probe's answer, kept for diagnostics).
    pub fn port(&self) -> u16 {
        self.port
    }
}

impl Backdoor for KernelBackdoor {
    fn outl(&mut self, value: u32) -> Result<(), i32> {
        // C `sys_outl(port, addr)` (`syslib.h:219`); the port rides the
        // message's port field, the physical address the value field.
        sys_outl(&DirectKernelCallTransport, self.port, value)
    }

    fn physical(&mut self, buffer: *const u8) -> Result<u64, i32> {
        // C gets the physical address from `alloc_contig`; the rewrite
        // asks VM to translate the page's address in this process
        // (`vm_getphys`, `mmap.c:143-156`).
        minix_sys::vm::physical_address_via(
            &DirectTrapTransport,
            self.endpoint,
            VirBytes(buffer as u64),
        )
        .map(|physical| physical.0)
        .map_err(|error| -error.to_i32())
    }

    fn now_seconds(&mut self) -> Result<i64, i32> {
        // C `time(NULL)` → `gettimeofday` through PM.
        minix_sys::pm::gettimeofday_via(&DirectTrapTransport)
            .map(|(sec, _nsec)| sec)
            .map_err(|error| -error.to_i32())
    }

    fn set_time(&mut self, seconds: i64) -> Result<(), i32> {
        // C `stime()` (`libc/sys/stime.c:12-18`) — PM_STIME.
        minix_sys::pm::stime_via(&DirectTrapTransport, seconds).map_err(|error| -error.to_i32())
    }

    fn set_alarm(&mut self, ticks: u64) -> Result<(), i32> {
        // C `sys_setalarm(ticks, 0)` — a relative alarm (`abs_time` 0).
        sys_setalarm(&DirectKernelCallTransport, ticks, false).map(|_| ())
    }

    fn hz(&mut self) -> Result<u32, i32> {
        // C `sys_hz()` (`libsys`, SYS_GETINFO · GET_HZ).
        minix_sys::syscall::sys_get_hz(&DirectKernelCallTransport).map(|hz| hz as u32)
    }

    fn irq_enable(&mut self) -> Result<(), i32> {
        // Registered seam: C's `sys_irqenable(&hook_id)` has no Rust verb
        // yet (SYS_IRQCTL exists in the kernel and in minix-types, but no
        // libsys wrapper or policy half has landed). Reporting ENOSYS
        // keeps the driver from pretending the interrupt is unmasked; the
        // interrupt is still acknowledged by the events request, so the
        // next device event arrives as a fresh notification.
        Err(-ENOSYS)
    }

    fn map_grant(
        &mut self,
        granter: Endpoint,
        grant: i32,
        grant_size: u64,
        offset: u64,
        access: i32,
        out: &mut [VumapPhys],
    ) -> Result<usize, i32> {
        sys_vumap(
            &DirectKernelCallTransport,
            granter,
            grant,
            grant_size,
            offset,
            access,
            out,
        )
    }
}

/// What the PCI probe found (C's local `port` and `irq` in `vbox_init`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Probed {
    /// The device's I/O port (the BAR with its low bits masked off).
    pub port: u16,
    /// The interrupt line.
    pub irq: u8,
}

/// Finds the VMMDev device, reserves it, and reads its port and interrupt
/// line (C `vbox_init`, `vbox.c:88-112`).
///
/// The C sequence is `pci_first_dev`/`pci_next_dev` until the vendor and
/// device identifiers match, `pci_reserve`, then
/// `pci_attr_r32(PCI_BAR) & PCI_BAR_IO_MASK` and `pci_attr_r8(PCI_ILR)`.
/// The C client panics at every step; this leg reports the failure and the
/// driver refuses to start instead. The attribute reads are the PCI
/// server's own registered seam today (it answers ENOSYS until the
/// configuration-space passthrough lands), so a machine that runs this
/// probe gets an honest failure rather than a fabricated port.
pub fn probe_device(ipc: &impl IpcTransport, pci: Endpoint) -> Result<Probed, i32> {
    // The five-step leg: find and reserve (`pci_first_dev`/`pci_next_dev`
    // then `pci_reserve`), then the BAR and the interrupt line.
    let index = minix_sys::pci::find_and_reserve(
        ipc,
        pci,
        crate::vmmdev::PCI_VENDOR,
        crate::vmmdev::PCI_DEVICE,
    )?;
    // The port: the BAR holds the I/O address in its high bits.
    let bar = minix_sys::pci::read_attribute32(ipc, pci, index, PCI_BAR)?;
    let irq = minix_sys::pci::read_attribute8(ipc, pci, index, PCI_ILR)?;
    Ok(Probed {
        port: (bar & PCI_BAR_IO_MASK) as u16,
        irq,
    })
}

/// The lane constants the tests pin the outgoing wire with, re-exported
/// from the shared leg.
#[cfg(test)]
use minix_sys::pci::{M1_I1, M1_I2, M1_I3, M2_I1, M2_I2, M2_L1};

#[cfg(test)]
mod tests {
    use super::*;
    use minix_sys::ipc::CannedTransport;
    use minix_types::{ENODEV, Message};

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

    /// A reply message whose type is the bus answer and whose `m1i1..m1i3`
    /// slots carry the device triple.
    fn found(index: i32, vendor: u16, device: u16) -> Message {
        let mut message = Message {
            m_type: 1,
            ..Message::default()
        };
        write_i32(&mut message, M1_I1, index);
        write_i32(&mut message, M1_I2, vendor as i32);
        write_i32(&mut message, M1_I3, device as i32);
        message
    }

    /// A reply whose type is the status and whose `m2l1` slot is the
    /// attribute value.
    fn attribute(value: u32) -> Message {
        let mut message = Message::default();
        write_i32(&mut message, M2_L1, value as i32);
        message
    }

    #[test]
    fn test_probe_walks_to_the_vmmdev_device_and_reads_port_and_irq() {
        let mut ipc = CannedTransport::new();
        // Two devices; the first is not the VMMDev.
        ipc.reply_sendrec(Ok(found(0, 0x1234, 0x1111)));
        ipc.reply_sendrec(Ok(found(1, 0x80ee, 0xcafe)));
        // Reserve succeeds (type zero).
        ipc.reply_sendrec(Ok(Message::default()));
        // BAR = 0xd240 with I/O flag bits set; ILR = 11.
        ipc.reply_sendrec(Ok(attribute(0x0000_d241)));
        ipc.reply_sendrec(Ok(attribute(11)));

        let probed = probe_device(&ipc, Endpoint(5)).expect("device found");
        assert_eq!(
            probed,
            Probed {
                port: 0xd240,
                irq: 11
            }
        );

        // The outgoing wire: call numbers and fields in order.
        let sent = ipc.sent.borrow();
        assert_eq!(sent.len(), 5);
        assert_eq!(sent[0].0, Endpoint(5));
        assert_eq!(sent[0].1.m_type, busc::FIRST_DEV);
        assert_eq!(sent[1].1.m_type, busc::NEXT_DEV);
        assert_eq!(read_i32(&sent[1].1, M1_I1), 0, "next starts after index 0");
        assert_eq!(sent[2].1.m_type, busc::RESERVE);
        assert_eq!(read_i32(&sent[2].1, M1_I1), 1);
        assert_eq!(sent[3].1.m_type, busc::ATTR_R32);
        assert_eq!(read_i32(&sent[3].1, M2_I1), 1);
        assert_eq!(read_i32(&sent[3].1, M2_I2), PCI_BAR as i32);
        assert_eq!(sent[4].1.m_type, busc::ATTR_R8);
        assert_eq!(read_i32(&sent[4].1, M2_I2), PCI_ILR as i32);
    }

    #[test]
    fn test_probe_reports_an_empty_bus_and_a_refused_attribute_read() {
        // No device at all: C panics "backdoor device not found"; the leg
        // reports ENODEV and the driver refuses to start.
        let mut empty = CannedTransport::new();
        empty.reply_sendrec(Ok(Message::default()));
        assert_eq!(probe_device(&empty, Endpoint(5)), Err(-ENODEV));

        // The device is there but the attribute read is refused — the
        // PCI server's current answer (its configuration-space
        // passthrough is a registered seam), or a permission failure.
        let mut refused = CannedTransport::new();
        refused.reply_sendrec(Ok(found(0, 0x80ee, 0xcafe)));
        refused.reply_sendrec(Ok(Message::default()));
        let denied = Message {
            m_type: -ENOSYS,
            ..Message::default()
        };
        refused.reply_sendrec(Ok(denied));
        assert_eq!(probe_device(&refused, Endpoint(5)), Err(-ENOSYS));
    }

    #[test]
    fn test_probe_reports_a_dead_bus() {
        // A transport failure (no PCI server) surfaces as the trap status.
        let ipc = CannedTransport::new();
        assert_eq!(probe_device(&ipc, Endpoint(5)), Err(-minix_types::EIO));
    }
}
