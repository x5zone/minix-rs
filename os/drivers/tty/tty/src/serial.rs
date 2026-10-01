//! Serial console backend: the PC16550 UART output face.
//!
//! C correspondence: the serial line device code `rs232.c` — the transmit
//! path waits for the transmitter holding register and pushes bytes
//! (`rs_write`, rs232.c:237+; the ready probe `txready`, rs232.c:121),
//! with every port access going through the kernel (`my_inb`/`sys_outb`,
//! rs232.c:110/:223-232). One 16550 register set serves one line.
//!
//! Port access model (OQ-N3 裁决, new_edge4 §6): the driver never holds
//! I/O privilege — every port read/write is a SYS_DEVIO kernel call, and
//! the kernel's `do_devio` gate (`syscall_device.rs:458-484`) enforces the
//! CHECK_IO_PORT whitelist where one is declared. Stock C declares
//! `io ALL` for the tty service (`/etc/system.conf:177`), so no whitelist
//! row is needed on this path; the kernel still mediates every access.
//!
//! Registered deviation: C `rs_init` programs the UART (baud, LCR, FIFO)
//! at line open; this backend skips initialization because the kernel's
//! own diagnostic console already owns the COM1 line settings, and a
//! second re-init would race the kernel's prints through the same chip.
//! Transmit completion is polled (`LSR.THRE`), not interrupt-driven like
//! C `rs232.c` — a wedge would stall the single-threaded driver, so the
//! wait is bounded and a stalled transmitter reports a partial write.

use crate::backend::LineBackend;
use minix_sys::syscall::{DirectKernelCallTransport, sys_inb, sys_outb};

/// First serial port's base address (standard ISA COM1).
pub const COM1_BASE: u16 = 0x3F8;

/// Line status register offset from the port base (16550 register map,
/// the `LS_*` probes of `rs232.c:121`).
const LSR_OFFSET: u16 = 5;

/// Transmitter holding register empty (16550 `LSR.THRE`; C `txready`
/// masks `LS_TRANSMITTER_READY`, rs232.c:121).
const LSR_THRE: u8 = 0x20;

/// Bounded THRE wait budget. A wedged transmitter must not hang the
/// driver's single-threaded event loop forever: past the budget the write
/// reports the bytes moved so far (C's interrupt-driven transmit never
/// spins; this bound is the polled model's honest stand-in).
const THRE_POLL_BUDGET: u32 = 100_000;

/// One UART chip seen through a port lane.
///
/// The seam keeps the backend testable without a kernel: the wire
/// implementation performs SYS_DEVIO kernel calls, tests script the chip.
pub trait UartPort {
    /// Read one byte-sized value from a port.
    fn read(&mut self, port: u16) -> Result<u8, i32>;
    /// Write one byte-sized value to a port.
    fn write(&mut self, port: u16, value: u8) -> Result<(), i32>;
}

impl<U: UartPort> UartPort for &mut U {
    fn read(&mut self, port: u16) -> Result<u8, i32> {
        (**self).read(port)
    }

    fn write(&mut self, port: u16, value: u8) -> Result<(), i32> {
        (**self).write(port, value)
    }
}

/// The wire port lane: every access is a SYS_DEVIO kernel call through
/// the direct trap transport (C `sys_inb`/`sys_outb`, syslib.h:217).
#[derive(Debug, Default, Clone, Copy)]
pub struct WireUart;

impl UartPort for WireUart {
    fn read(&mut self, port: u16) -> Result<u8, i32> {
        sys_inb(&DirectKernelCallTransport, port)
    }

    fn write(&mut self, port: u16, value: u8) -> Result<(), i32> {
        sys_outb(&DirectKernelCallTransport, port, value)
    }
}

/// Serial line backend: drains output bytes into one 16550 UART.
#[derive(Debug, Clone)]
pub struct SerialBackend<U: UartPort> {
    uart: U,
    /// Port base of the chip this backend serves.
    base: u16,
    /// Remaining THRE polls before a transmitter is declared stalled.
    poll_budget: u32,
}

impl SerialBackend<WireUart> {
    /// The COM1 output face (QEMU `-serial` carrier, OQ-N3 方案 B).
    pub fn com1() -> SerialBackend<WireUart> {
        SerialBackend::new(WireUart, COM1_BASE)
    }
}

impl<U: UartPort> SerialBackend<U> {
    /// A backend over the given port lane at the given chip base.
    pub fn new(uart: U, base: u16) -> SerialBackend<U> {
        SerialBackend {
            uart,
            base,
            poll_budget: THRE_POLL_BUDGET,
        }
    }

    /// Wait until the transmitter can take another byte.
    fn transmitter_ready(&mut self) -> bool {
        let mut polls = 0;
        while polls < self.poll_budget {
            match self.uart.read(self.base + LSR_OFFSET) {
                Ok(lsr) if lsr & LSR_THRE != 0 => return true,
                Ok(_) => polls += 1,
                // A failed ready probe cannot distinguish a gated port
                // from a dead chip; treat it as a stall either way.
                Err(_) => return false,
            }
        }
        false
    }
}

impl<U: UartPort> LineBackend for SerialBackend<U> {
    fn write_bytes(&mut self, bytes: &[u8]) -> usize {
        for (moved, byte) in bytes.iter().enumerate() {
            if !self.transmitter_ready() {
                return moved;
            }
            if self.uart.write(self.base, *byte).is_err() {
                return moved;
            }
        }
        bytes.len()
    }
}

// ── aarch64 PL011 MMIO arm (NK4-C 续-132) ─────────────────────────────────
//
// C correspondence: the ARM board serial output face. C Minix3's i386
// `rs232.c` talks a 16550 over port I/O; the ARM boards' UART is the
// PrimeCell PL011, memory-mapped — C drivers reach it through
// `vm_map_phys` device mappings (the same channel this arm uses:
// `SYS_VMCTL`/`VM_MAP_PHYS` maps the UART page into the caller, C
// `vm_map_phys`, memory.c). QEMU `virt` places the PL011 at
// `0x0900_0000` (one page); the register subset the transmit path needs:
//
// - `DR`  @ +0x00 — data register: a write's low 8 bits transmits.
// - `FR`  @ +0x18 — flag register: `TXFF` (bit 5) set means the transmit
//   FIFO cannot take another byte; ready = `FR & TXFF == 0`.
//
// Port-access model: unlike the 16550 lane (SYS_DEVIO kernel mediation,
// OQ-N3), there is no port lane on aarch64 — the page is mapped once at
// driver init via `VM_MAP_PHYS` (VM's `map_perm_check` admits TTY by
// endpoint, C mmap.c parity) and driven with volatile accesses. The same
// registered deviation as the 16550 arm applies: no re-initialization
// (the kernel's own diagnostic console owns the line settings), and the
// ready wait is a bounded poll — a wedged transmitter reports a partial
// write rather than hanging the single-threaded driver.

use minix_sys::ipc::DirectTrapTransport as TrapTransport;
use minix_types::{Endpoint, PhysBytes, VirBytes};

/// QEMU `virt` PL011 base (board constant, the C `uart_base` analogue).
pub const PL011_BASE: u64 = 0x0900_0000;

/// Data register offset (PL011 primecell register map, DDI0183).
const PL011_DR: u64 = 0x00;
/// Flag register offset (PL011 primecell register map, DDI0183).
const PL011_FR: u64 = 0x18;
/// Flag-register bit: transmit FIFO full (ready when clear).
const PL011_TXFF: u32 = 0x20;

/// One PL011 register set seen through a mapped page.
///
/// The seam mirrors [`UartPort`]: the wire implementation drives the
/// mapped MMIO page with volatile accesses; tests script the register
/// bank without a kernel.
pub trait Pl011Regs {
    /// Read the flag register (`FR`, 32-bit).
    fn flags(&mut self) -> Result<u32, i32>;
    /// Push one byte into the data register (`DR`, 32-bit write, low 8
    /// bits carry the payload).
    fn push(&mut self, byte: u8) -> Result<(), i32>;
}

impl<P: Pl011Regs> Pl011Regs for &mut P {
    fn flags(&mut self) -> Result<u32, i32> {
        (**self).flags()
    }
    fn push(&mut self, byte: u8) -> Result<(), i32> {
        (**self).push(byte)
    }
}

/// The wire register bank: the UART page mapped into this driver at init
/// via `VM_MAP_PHYS` (C `vm_map_phys`), then driven directly.
pub struct WirePl011 {
    /// Virtual address of the mapped PL011 page (64-bit on every
    /// consumer of this arm).
    base: u64,
}

impl WirePl011 {
    /// Map the QEMU-`virt` PL011 page for the caller itself. The mapping
    /// is kept for the driver's lifetime (C never unmaps the console
    /// UART either); a failed map leaves the driver without its device —
    /// surfaced as an error the construction site fail-fasts on.
    pub fn map() -> Result<WirePl011, i32> {
        let va = minix_sys::vm::map_physical_via(
            &TrapTransport,
            Endpoint::SELF,
            PhysBytes(PL011_BASE),
            VirBytes(4096),
        )
        .map_err(|e| e.to_i32())?;
        Ok(WirePl011 { base: va.0 })
    }
}

impl Pl011Regs for WirePl011 {
    fn flags(&mut self) -> Result<u32, i32> {
        // SAFETY: `base` names the 4 KiB PL011 page mapped at
        // construction; `+0x18` is inside that page. MMIO must not be
        // reordered or elided — volatile both ways.
        Ok(unsafe { core::ptr::read_volatile((self.base + PL011_FR) as *const u32) })
    }
    fn push(&mut self, byte: u8) -> Result<(), i32> {
        // SAFETY: same page; DR accepts a byte in its low 8 bits.
        unsafe { core::ptr::write_volatile((self.base + PL011_DR) as *mut u32, byte as u32) }
        Ok(())
    }
}

/// aarch64 serial line backend: drains output bytes into one PL011.
///
/// Structurally the PL011 twin of [`SerialBackend`]: the same bounded
/// ready-poll and the same partial-write contract on a stall; only the
/// register model differs (`FR.TXFF` polarity — ready when the bit is
/// *clear* — versus the 16550's `LSR.THRE` set-when-ready).
#[derive(Debug, Clone)]
pub struct Pl011Backend<P: Pl011Regs> {
    uart: P,
    poll_budget: u32,
}

impl Pl011Backend<WirePl011> {
    /// The QEMU-`virt` console UART output face. Fails (and the caller
    /// fail-fasts) when the device page cannot be mapped.
    pub fn virt_console() -> Result<Pl011Backend<WirePl011>, i32> {
        Ok(Pl011Backend {
            uart: WirePl011::map()?,
            poll_budget: THRE_POLL_BUDGET,
        })
    }
}

impl<P: Pl011Regs> Pl011Backend<P> {
    /// A backend over a scripted register bank (the test seam the
    /// `SerialBackend::new` analogue).
    pub fn new(uart: P) -> Pl011Backend<P> {
        Pl011Backend {
            uart,
            poll_budget: THRE_POLL_BUDGET,
        }
    }

    /// Wait until the transmit FIFO can take another byte.
    fn transmitter_ready(&mut self) -> bool {
        let mut polls = 0;
        while polls < self.poll_budget {
            match self.uart.flags() {
                Ok(fr) if fr & PL011_TXFF == 0 => return true,
                Ok(_) => polls += 1,
                Err(_) => return false,
            }
        }
        false
    }
}

impl<P: Pl011Regs> LineBackend for Pl011Backend<P> {
    fn write_bytes(&mut self, bytes: &[u8]) -> usize {
        for (moved, byte) in bytes.iter().enumerate() {
            if !self.transmitter_ready() {
                return moved;
            }
            if self.uart.push(*byte).is_err() {
                return moved;
            }
        }
        bytes.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Scripted chip: transmitter ready per the script, every accepted
    /// byte recorded. `Err` on the status lane models a gated port.
    #[derive(Debug, Default)]
    struct FakeUart {
        /// THRE value handed out per LSR read.
        ready: bool,
        /// Bytes pushed into the transmitter holding register.
        tx: alloc::vec::Vec<u8>,
        read_err: Option<i32>,
        write_err: Option<i32>,
    }

    impl UartPort for FakeUart {
        fn read(&mut self, _port: u16) -> Result<u8, i32> {
            match self.read_err {
                Some(e) => Err(e),
                None => Ok(if self.ready { LSR_THRE } else { 0 }),
            }
        }

        fn write(&mut self, _port: u16, value: u8) -> Result<(), i32> {
            match self.write_err {
                Some(e) => Err(e),
                None => {
                    self.tx.push(value);
                    Ok(())
                }
            }
        }
    }

    #[test]
    fn test_drains_all_bytes_when_ready() {
        let mut uart = FakeUart {
            ready: true,
            ..FakeUart::default()
        };
        let mut backend = SerialBackend::new(&mut uart, COM1_BASE);
        let moved = backend.write_bytes(b"ok\n");
        assert_eq!(moved, 3);
        assert_eq!(uart.tx, b"ok\n".to_vec());
    }

    #[test]
    fn test_stalled_transmitter_reports_partial() {
        let mut uart = FakeUart {
            ready: false, // LSR never asserts THRE: stall on the first byte
            ..FakeUart::default()
        };
        let mut backend = SerialBackend::new(&mut uart, COM1_BASE);
        assert_eq!(backend.write_bytes(b"stall"), 0);
        assert!(uart.tx.is_empty());
    }

    #[test]
    fn test_failed_probe_stops_the_drain() {
        let mut uart = FakeUart {
            ready: true,
            read_err: Some(-minix_types::EPERM),
            ..FakeUart::default()
        };
        let mut backend = SerialBackend::new(&mut uart, COM1_BASE);
        assert_eq!(backend.write_bytes(b"abc"), 0);
    }

    #[test]
    fn test_failed_write_reports_bytes_moved_so_far() {
        let mut uart = FakeUart {
            ready: true,
            write_err: Some(-minix_types::EIO),
            ..FakeUart::default()
        };
        let mut backend = SerialBackend::new(&mut uart, COM1_BASE);
        assert_eq!(backend.write_bytes(b"abc"), 0);
    }

    #[test]
    fn test_partial_stall_after_ready_bytes() {
        // Ready for the first byte only: the first LSR read asserts THRE,
        // later reads never do, so exactly one byte drains.
        #[derive(Default)]
        struct OneShot {
            lsr_reads: core::cell::Cell<u32>,
            tx: core::cell::RefCell<alloc::vec::Vec<u8>>,
        }
        impl UartPort for OneShot {
            fn read(&mut self, _port: u16) -> Result<u8, i32> {
                let n = self.lsr_reads.get();
                self.lsr_reads.set(n + 1);
                Ok(if n == 0 { LSR_THRE } else { 0 })
            }
            fn write(&mut self, _port: u16, value: u8) -> Result<(), i32> {
                self.tx.borrow_mut().push(value);
                Ok(())
            }
        }
        let uart = OneShot::default();
        let mut backend = SerialBackend::new(uart, COM1_BASE);
        // Borrow Split: the backend owns the chip; record after the drain.
        let moved = backend.write_bytes(b"abcd");
        assert_eq!(moved, 1);
        // SAFETY(test): SerialBackend<OneShot> 字段访问需先取回所有权。
        let SerialBackend { uart, .. } = backend;
        assert_eq!(uart.tx.into_inner(), b"a".to_vec());
    }

    // ── PL011 arm tests (续-132) ────────────────────────────────────────

    /// Scripted PL011 bank: `flags` per script, every pushed byte
    /// recorded. Mirrors `FakeUart` for the ARM register model.
    #[derive(Default)]
    struct FakePl011 {
        txff: bool,
        tx: alloc::vec::Vec<u8>,
        flags_err: Option<i32>,
        push_err: Option<i32>,
    }

    impl Pl011Regs for FakePl011 {
        fn flags(&mut self) -> Result<u32, i32> {
            match self.flags_err {
                Some(e) => Err(e),
                None => Ok(if self.txff { PL011_TXFF } else { 0 }),
            }
        }
        fn push(&mut self, byte: u8) -> Result<(), i32> {
            match self.push_err {
                Some(e) => Err(e),
                None => {
                    self.tx.push(byte);
                    Ok(())
                }
            }
        }
    }

    #[test]
    fn test_pl011_drains_all_bytes_when_not_full() {
        let mut uart = FakePl011 {
            txff: false,
            ..FakePl011::default()
        };
        let mut backend = Pl011Backend::new(&mut uart);
        assert_eq!(backend.write_bytes(b"ok\n"), 3);
        assert_eq!(uart.tx, b"ok\n".to_vec());
    }

    #[test]
    fn test_pl011_full_fifo_reports_zero_moved() {
        let mut uart = FakePl011 {
            txff: true, // FR.TXFF never clears: stall on the first byte
            ..FakePl011::default()
        };
        let mut backend = Pl011Backend::new(&mut uart);
        assert_eq!(backend.write_bytes(b"stall"), 0);
        assert!(uart.tx.is_empty());
    }

    #[test]
    fn test_pl011_failed_flag_probe_stops_the_drain() {
        let mut uart = FakePl011 {
            flags_err: Some(-minix_types::EPERM),
            ..FakePl011::default()
        };
        let mut backend = Pl011Backend::new(&mut uart);
        assert_eq!(backend.write_bytes(b"abc"), 0);
    }

    #[test]
    fn test_pl011_failed_push_reports_bytes_moved_so_far() {
        let mut uart = FakePl011 {
            push_err: Some(-minix_types::EIO),
            ..FakePl011::default()
        };
        let mut backend = Pl011Backend::new(&mut uart);
        assert_eq!(backend.write_bytes(b"abc"), 0);
    }
}
