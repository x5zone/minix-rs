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
pub mod input;
pub mod line;
pub mod session;
pub mod termios;

/// Service initialization entry (wires the line table; transport stays out).
pub fn init() {}
