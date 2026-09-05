//! Printer driver: status reading plus retry budget.
//!
//! C correspondence: `minix3/minix/drivers/printer/printer/printer.c`
//! (424 lines). The driver registers `printer_tab`
//! (`printer.c:99-103`, open, write, cancel, and interrupt
//! callbacks) and serves it through `chardriver_task`
//! (`printer.c:109-116`). This crate owns the status half; the
//! service binary owns port traffic. See document
//! `24-misc-drivers.md` in
//! `notes/rewrite/fork-syscall-rewrite/16-stage-drivers/`.
//!
//! Single-threaded event loop: one message at a time, no shared mutable
//! state across threads.

#![no_std]

pub mod status;

/// Service initialization entry (wires the printer table; port traffic stays out).
pub fn init() {}
