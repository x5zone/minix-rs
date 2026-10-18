//! The printer's parallel port behind a trait.
//!
//! C correspondence: `do_probe` (`printer.c:268-281`, BIOS parallel-port
//! lookup), `do_initialize` (`printer.c:283-303`, init pulse and select),
//! the status reads (`printer.c:345-424`, port base plus one) and the
//! Centronics byte sequence (data, assert strobe, negate strobe).
//!
//! Port traffic stays behind this trait so the request pump is host
//! testable and the absence of a parallel port is an honest probe result,
//! not a fabricated success. The kernel-backed implementation (BIOS port
//! lookup plus `sys_inb`/`sys_outb`) rides the port-IO seam shared with
//! the other drivers; until it lands, [`NullPrinter`] reports the machine
//! as having no printer, which is exactly what C's fresh-start init
//! answers (`ENODEV`, `printer.c:145-150`).

use minix_types::{EIO, ENODEV};

/// Centronics interface behavior the service needs.
pub trait PrinterHardware {
    /// Whether a parallel port exists (`do_probe`).
    fn probe(&mut self) -> bool;

    /// Initialize the interface once on first open (`do_initialize`).
    fn initialize(&mut self) -> Result<(), i32>;

    /// Read the status register (port base plus one).
    fn status(&mut self) -> Result<u8, i32>;

    /// Output one byte with the strobe sequence.
    fn output(&mut self, byte: u8) -> Result<(), i32>;

    /// Drop the select line when no output is pending
    /// (`printer.c:347-357`).
    fn deselect(&mut self) -> Result<(), i32>;

    /// Wait between online polls (`micro_delay(500000)`).
    fn delay_micros(&mut self, micros: u32);
}

/// A machine without a parallel port: the probe refuses and every port
/// operation reports an input-output error instead of pretending.
pub struct NullPrinter;

impl PrinterHardware for NullPrinter {
    fn probe(&mut self) -> bool {
        false
    }

    fn initialize(&mut self) -> Result<(), i32> {
        Err(ENODEV)
    }

    fn status(&mut self) -> Result<u8, i32> {
        Err(EIO)
    }

    fn output(&mut self, _byte: u8) -> Result<(), i32> {
        Err(EIO)
    }

    fn deselect(&mut self) -> Result<(), i32> {
        Err(EIO)
    }

    fn delay_micros(&mut self, _micros: u32) {}
}
