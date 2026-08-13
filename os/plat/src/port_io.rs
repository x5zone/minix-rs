//! Port I/O abstraction for x86-style I/O ports.
//!
//! Defines the `PortIo` trait for architecture-specific I/O port access.
//! On x86-64, this maps to `inb/outb/inw/outw/inl/outl` instructions.
//! On ARM64/RISC-V, I/O ports don't exist — the trait is implemented
//! as no-ops and `SYS_DEVIO` returns `BadCall` at the dispatch level.
//!
//! # Design decisions (20-syscall-device.md §3 D2)
//!
//! - **PortIo trait**: Abstracts hardware I/O port access so that the
//!   kernel's device I/O system calls (`SYS_DEVIO`, `SYS_VDEVIO`) do
//!   not directly depend on architecture-specific inline assembly.
//! - **Board-level placement**: Port I/O is a hardware mechanism, not
//!   a CPU ISA feature — it belongs in `minix-plat` alongside
//!   `InterruptController` and `EarlyConsole`.

/// Architecture-specific port I/O operations.
///
/// Each supported architecture provides its own implementation.
/// On architectures without I/O ports (aarch64, riscv64), the methods
/// are no-ops that return 0, and `SYS_DEVIO` returns `BadCall` at
/// the dispatch level (Doc 19 §3 D6).
///
/// C: `inb(port)`, `outb(port, val)`, etc. — inline assembly in
/// Minix3 kernel's `ibm.h` / `protect.c`.
pub trait PortIo {
    /// Read a byte from an I/O port. C: `inb(port)`
    fn inb(&self, port: u16) -> u8;

    /// Write a byte to an I/O port. C: `outb(port, value)`
    fn outb(&self, port: u16, value: u8);

    /// Read a word (16-bit) from an I/O port. C: `inw(port)`
    fn inw(&self, port: u16) -> u16;

    /// Write a word (16-bit) to an I/O port. C: `outw(port, value)`
    fn outw(&self, port: u16, value: u16);

    /// Read a long (32-bit) from an I/O port. C: `inl(port)`
    fn inl(&self, port: u16) -> u32;

    /// Write a long (32-bit) to an I/O port. C: `outl(port, value)`
    fn outl(&self, port: u16, value: u32);

    /// Read a block of bytes from an I/O port. C: `phys_insb(port, buf, count)`
    ///
    /// Default implementation loops `inb`; x86_64 can override with
    /// `rep insb` for performance.
    fn insb(&self, port: u16, buf: &mut [u8]) {
        for byte in buf.iter_mut() {
            *byte = self.inb(port);
        }
    }

    /// Write a block of bytes to an I/O port. C: `phys_outsb(port, buf, count)`
    ///
    /// Default implementation loops `outb`; x86_64 can override with
    /// `rep outsb` for performance.
    fn outsb(&self, port: u16, buf: &[u8]) {
        for byte in buf {
            self.outb(port, *byte);
        }
    }

    /// Read a block of words from an I/O port. C: `phys_insw(port, buf, count)`
    fn insw(&self, port: u16, buf: &mut [u16]) {
        for word in buf.iter_mut() {
            *word = self.inw(port);
        }
    }

    /// Write a block of words to an I/O port. C: `phys_outsw(port, buf, count)`
    fn outsw(&self, port: u16, buf: &[u16]) {
        for word in buf {
            self.outw(port, *word);
        }
    }
}
