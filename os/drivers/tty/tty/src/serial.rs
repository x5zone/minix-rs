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
}
