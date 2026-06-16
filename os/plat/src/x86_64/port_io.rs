//! x86-64 I/O port implementation via inline assembly.
//!
//! Implements `PortIo` using x86 `in/out` instructions.
//! These are privileged instructions available in ring 0 (kernel mode).

use core::arch::asm;
use crate::port_io::PortIo;

/// x86-64 I/O port access via inline assembly.
///
/// Uses `in al, dx` / `out dx, al` and wider variants.
/// All methods are `#[inline]` to minimize call overhead.
pub struct X86_64PortIo;

impl X86_64PortIo {
    pub const fn new() -> Self {
        Self
    }
}

impl Default for X86_64PortIo {
    fn default() -> Self {
        Self::new()
    }
}

impl PortIo for X86_64PortIo {
    #[inline]
    fn inb(&self, port: u16) -> u8 {
        let val: u8;
        // SAFETY: `in al, dx` is a privileged x86 instruction that reads
        // one byte from the I/O port specified in DX. Safe in kernel mode
        // (ring 0). The port value comes from the caller (validated by
        // dispatch_devio's CHECK_IO_PORT permission check).
        unsafe {
            asm!("in al, dx", out("al") val, in("dx") port, options(nostack, preserves_flags));
        }
        val
    }

    #[inline]
    fn outb(&self, port: u16, value: u8) {
        // SAFETY: `out dx, al` is a privileged x86 instruction that writes
        // one byte to the I/O port specified in DX. Safe in kernel mode.
        unsafe {
            asm!("out dx, al", in("dx") port, in("al") value, options(nostack, preserves_flags));
        }
    }

    #[inline]
    fn inw(&self, port: u16) -> u16 {
        let val: u16;
        // SAFETY: `in ax, dx` reads a 16-bit word from the I/O port.
        unsafe {
            asm!("in ax, dx", out("ax") val, in("dx") port, options(nostack, preserves_flags));
        }
        val
    }

    #[inline]
    fn outw(&self, port: u16, value: u16) {
        // SAFETY: `out dx, ax` writes a 16-bit word to the I/O port.
        unsafe {
            asm!("out dx, ax", in("dx") port, in("ax") value, options(nostack, preserves_flags));
        }
    }

    #[inline]
    fn inl(&self, port: u16) -> u32 {
        let val: u32;
        // SAFETY: `in eax, dx` reads a 32-bit long from the I/O port.
        unsafe {
            asm!("in eax, dx", out("eax") val, in("dx") port, options(nostack, preserves_flags));
        }
        val
    }

    #[inline]
    fn outl(&self, port: u16, value: u32) {
        // SAFETY: `out dx, eax` writes a 32-bit long to the I/O port.
        unsafe {
            asm!("out dx, eax", in("dx") port, in("eax") value, options(nostack, preserves_flags));
        }
    }
}
