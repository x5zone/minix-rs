//! The port-I/O seam: how the card's registers are reached.
//!
//! C correspondence: `pci_helper.c`'s `pci_inb`/`pci_inw`/`pci_inl` and
//! `pci_outb`/`pci_outw`/`pci_outl` (the register traffic), which are
//! libsys calls (`sys_inb`/`sys_inl`/... ) mediated by the kernel through
//! the process's I/O privilege.
//!
//! One difference from the C helpers is deliberate: on a failed read C's
//! `pci_inb` prints a warning and returns an **uninitialized** local
//! (`pci_helper.c:19-24`: `u32_t value; if (sys_inb(port, &value) != OK)
//! printf(...); return value;`). The rewrite reports the failure to the
//! caller, which turns it into an error instead of continuing with a
//! value that was never written.
//!
//! Unlike the DMA allocator, this seam has a **real** production
//! implementation: `minix-sys` carries the `sys_in*`/`sys_out*` wrappers,
//! so [`KernelPorts`] talks to the actual card through the kernel.

use minix_sys::syscall::{
    DirectKernelCallTransport, sys_inb, sys_inl, sys_inw, sys_outb, sys_outl, sys_outw,
};

/// The register traffic the card's sequences need.
pub trait PortIo {
    /// Write one byte (`pci_outb`): the memory page selector is
    /// byte-wide.
    fn out8(&mut self, port: u16, value: u8) -> Result<(), i32>;
    /// Write one 16-bit word (`pci_outw`): the chip-select control and
    /// the sample counts are words.
    fn out16(&mut self, port: u16, value: u16) -> Result<(), i32>;
    /// Write one 32-bit word (`pci_outl`).
    fn out32(&mut self, port: u16, value: u32) -> Result<(), i32>;
    /// Read one byte (`pci_inb`).
    fn in8(&mut self, port: u16) -> Result<u8, i32>;
    /// Read one 16-bit word (`pci_inw`).
    fn in16(&mut self, port: u16) -> Result<u16, i32>;
    /// Read one 32-bit word (`pci_inl`).
    fn in32(&mut self, port: u16) -> Result<u32, i32>;
}

/// The production seam: the kernel-mediated port I/O calls.
///
/// The base port comes from the PCI probe's BAR (C's `dev.base`, the
/// `pci_attr_r32(devind, PCI_BAR) & 0xfffffffe` path at
/// `es1370.c:200`), which the binary resolves; the offsets are added per
/// access.
pub struct KernelPorts {
    /// The card's base port (`dev.base`).
    pub base: u16,
}

impl PortIo for KernelPorts {
    fn out8(&mut self, port: u16, value: u8) -> Result<(), i32> {
        sys_outb(&DirectKernelCallTransport, self.base + port, value)
    }
    fn out16(&mut self, port: u16, value: u16) -> Result<(), i32> {
        sys_outw(&DirectKernelCallTransport, self.base + port, value)
    }
    fn out32(&mut self, port: u16, value: u32) -> Result<(), i32> {
        sys_outl(&DirectKernelCallTransport, self.base + port, value)
    }
    fn in8(&mut self, port: u16) -> Result<u8, i32> {
        sys_inb(&DirectKernelCallTransport, self.base + port)
    }
    fn in16(&mut self, port: u16) -> Result<u16, i32> {
        sys_inw(&DirectKernelCallTransport, self.base + port)
    }
    fn in32(&mut self, port: u16) -> Result<u32, i32> {
        sys_inl(&DirectKernelCallTransport, self.base + port)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registers::{CHIP_SEL_CTRL, INTERRUPT_STATUS, MEM_PAGE};

    #[test]
    fn test_the_production_seam_reaches_the_kernel_calls() {
        // The seam is real: in a host build the kernel call cannot
        // complete, so the failure is the transport's, not a stand-in
        // (a stand-in would answer ENOSYS).
        let mut ports = KernelPorts { base: 0x0300 };
        assert_eq!(ports.out8(MEM_PAGE, 1), Err(-minix_types::EIO));
        assert_eq!(ports.in16(INTERRUPT_STATUS), Err(-minix_types::EIO));
        assert_eq!(ports.in32(CHIP_SEL_CTRL), Err(-minix_types::EIO));
    }
}
