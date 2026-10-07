//! Terminal driver: consoles, serial lines, and the line discipline.
//!
//! C correspondence: `minix3/minix/drivers/tty/tty/tty.c` (1603 lines)
//! with `tty.h`, plus the console, serial, and keyboard devices behind
//! the per-line backend pointers. This crate owns the numbers and the
//! policy (line decoding, configuration, input queue, open sessions);
//! the service binary owns the transport (message pump, timers, grant
//! copies, video and serial hardware). See document `06-tty-driver.md` in
//! `rewrite-notes/16-stage-drivers/`.
//!
//! Single-threaded event loop: one message at a time, no shared mutable
//! state across threads.

#![no_std]

extern crate alloc;

pub mod backend;
pub mod char_face;
pub mod input;
pub mod keyboard;
pub mod line;
pub mod serial;
pub mod session;
pub mod service;
pub mod termios;

use backend::{LineBackend, NullBackend};
use char_face::TtyDriver;
use service::TtyService;

/// Line-table slot count: four consoles followed by the serial lines
/// (C: `NR_CONSOLES` consoles plus `NR_RS_LINES` serials, `tty.h`). The
/// configured console line is minor zero.
const LINE_SLOTS: usize = 8;
const CONSOLE_LINE: u32 = 0;

/// Build the service over a device backend: the line table (one session
/// per slot) plus the chardriver face the message pump drives.
pub fn new_service<B: LineBackend>(backend: B) -> TtyService<B> {
    TtyService::new(TtyDriver::new(LINE_SLOTS, CONSOLE_LINE, backend))
}

/// Service initialization entry (x86_64): the wired line table over the
/// COM1 serial output face (OQ-N3 裁决方案 B — /dev/console drains to the
/// UART through kernel-mediated SYS_DEVIO; the video-text backend stays
/// registered until a display-visible acceptance harness exists).
#[cfg(target_arch = "x86_64")]
pub fn init() -> TtyService<serial::SerialBackend<serial::WireUart>> {
    new_service(serial::SerialBackend::com1())
}

/// Service initialization entry (aarch64): the wired line table over the
/// QEMU-`virt` PL011 console UART (NK4-C 续-132 — the 16550/COM1 lane has
/// no aarch64 equivalent; the PL011 page is mapped into this driver via
/// `VM_MAP_PHYS` at startup and driven with volatile register accesses,
/// the C ARM-board serial model). A failed device map fail-fasts the
/// driver: without the UART there is no console to serve.
#[cfg(target_arch = "aarch64")]
pub fn init() -> TtyService<serial::Pl011Backend<serial::WirePl011>> {
    let backend = serial::Pl011Backend::virt_console()
        .expect("tty: cannot map PL011 console UART");
    new_service(backend)
}

/// Service initialization entry (riscv64): the wired line table over the
/// QEMU-`virt` NS16550A console UART (NK4-C 续-132 — register model is
/// the 16550 family the x86 arm already serves; the access lane is the
/// mapped MMIO page from `VM_MAP_PHYS`, base 0 so `port` operands carry
/// the register offset). A failed device map fail-fasts the driver.
#[cfg(target_arch = "riscv64")]
pub fn init() -> TtyService<serial::SerialBackend<serial::WireMmioUart>> {
    let uart = serial::WireMmioUart::map().expect("tty: cannot map NS16550A console UART");
    new_service(serial::SerialBackend::new(uart, 0))
}

/// Service initialization over the null backend (an unplugged terminal),
/// kept for tests and for a boot that carries no UART.
pub fn init_null() -> TtyService<NullBackend> {
    new_service(NullBackend)
}

