//! Mock implementations of platform traits for testing.
//!
//! These are the "mock architecture" — a fifth platform alongside
//! x86_64, arm64, and riscv64, used for user-space testing without
//! real hardware.

use minix_platform::InterruptControllerDesc;

use crate::interrupt::{InterruptController, IrqVector};
use crate::early_console::EarlyConsole;
use crate::port_io::PortIo;

/// Mock interrupt controller for testing.
///
/// Implements `InterruptController` by recording operations in logs
/// (via `log::debug!`) instead of touching real hardware.
///
/// # Instance-based design
///
/// `new(desc)` accepts any `InterruptControllerDesc` variant (the mock
/// does not care about the specific hardware parameters).
pub struct MockInterruptController;

impl InterruptController for MockInterruptController {
    fn new(_desc: &InterruptControllerDesc) -> Self {
        Self
    }

    fn init(&mut self) {
        log::debug!("mock InterruptController::init()");
    }

    fn mask(&mut self, irq: IrqVector) {
        log::debug!("mock InterruptController::mask({})", irq.get());
    }

    fn unmask(&mut self, irq: IrqVector) {
        log::debug!("mock InterruptController::unmask({})", irq.get());
    }

    fn ack(&mut self, irq: IrqVector) {
        log::debug!("mock InterruptController::ack({})", irq.get());
    }

    fn eoi(&mut self, irq: IrqVector) {
        log::debug!("mock InterruptController::eoi({})", irq.get());
    }

    fn mask_all(&mut self) {
        log::debug!("mock InterruptController::mask_all()");
    }
}

/// Mock early console for testing.
///
/// Implements `EarlyConsole` by writing to `log::debug!` instead of
/// real serial hardware.
pub struct MockEarlyConsole;

impl EarlyConsole for MockEarlyConsole {
    fn write_byte(byte: u8) {
        log::debug!("mock EarlyConsole::write_byte(0x{:02x})", byte);
    }
}

/// Mock port I/O for testing.
///
/// Implements `PortIo` by recording operations in logs
/// (via `log::debug!`) instead of touching real hardware.
/// All reads return 0.
pub struct MockPortIo;

impl MockPortIo {
    pub const fn new() -> Self {
        Self
    }
}

impl Default for MockPortIo {
    fn default() -> Self {
        Self::new()
    }
}

impl PortIo for MockPortIo {
    fn inb(&self, port: u16) -> u8 {
        log::debug!("mock PortIo::inb(0x{:04x}) → 0", port);
        0
    }

    fn outb(&self, port: u16, value: u8) {
        log::debug!("mock PortIo::outb(0x{:04x}, 0x{:02x})", port, value);
    }

    fn inw(&self, port: u16) -> u16 {
        log::debug!("mock PortIo::inw(0x{:04x}) → 0", port);
        0
    }

    fn outw(&self, port: u16, value: u16) {
        log::debug!("mock PortIo::outw(0x{:04x}, 0x{:04x})", port, value);
    }

    fn inl(&self, port: u16) -> u32 {
        log::debug!("mock PortIo::inl(0x{:04x}) → 0", port);
        0
    }

    fn outl(&self, port: u16, value: u32) {
        log::debug!("mock PortIo::outl(0x{:04x}, 0x{:08x})", port, value);
    }
}
