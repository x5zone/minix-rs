//! Printer driver: status reading, retry budget, and the char pump.
//!
//! C correspondence: `minix3/minix/drivers/printer/printer/printer.c`
//! (424 lines). The driver registers `printer_tab`
//! (`printer.c:99-103`, open, write, cancel, and interrupt
//! callbacks) and serves it through `chardriver_task`
//! (`printer.c:109-116`). This crate owns the status half, the hardware
//! seam, and the request pump; the service binary names the label and
//! builds the transport. See document `24-misc-drivers.md` in
//! `rewrite-notes/16-stage-drivers/`.
//!
//! Single-threaded event loop: one message at a time, no shared mutable
//! state across threads.

#![no_std]

#[cfg(test)]
extern crate std;

pub mod hardware;
pub mod service;
pub mod status;

use hardware::NullPrinter;
use service::PrinterService;

/// Build the driver's service for the binary.
///
/// The parallel-port backend rides the port-IO seam (BIOS port lookup
/// plus `sys_inb`/`sys_outb`); without it the honest seam reports the
/// machine as having no printer, and the fresh-start probe refuses to
/// start the driver — the same `ENODEV` C answers (`printer.c:145-150`).
pub fn init() -> PrinterService<NullPrinter> {
    PrinterService::new(NullPrinter)
}
