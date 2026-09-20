//! Terminal driver: consoles, serial lines, and the line discipline.
//!
//! C correspondence: `minix3/minix/drivers/tty/tty/tty.c` (1603 lines)
//! with `tty.h`, plus the console, serial, and keyboard devices behind
//! the per-line backend pointers. This crate owns the numbers and the
//! policy (line decoding, configuration, input queue, open sessions);
//! the service binary owns the transport (message pump, timers, grant
//! copies, video and serial hardware). See document `06-tty-driver.md` in
//! `notes/rewrite/fork-syscall-rewrite/16-stage-drivers/`.
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

/// Service initialization entry: the wired line table over the null
/// backend (an unplugged terminal). A real boot picks the console/serial
/// backend; the transport and birth handshake stay in the binary.
pub fn init() -> TtyService<NullBackend> {
    new_service(NullBackend)
}

