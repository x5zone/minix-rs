//! The port-I/O seam: how the card's registers are reached.
//!
//! C correspondence: `io.h`'s `my_inb`/`my_inw`/`my_inl` and
//! `my_outb`/`my_outw`/`my_outl` behind the `sdr_*` macros (the register
//! traffic) plus `micro_delay` (the mixer's settling waits, `als4000.c:79`
//! and friends). Without `DMA_BASE_IOMAP` the C helpers are libsys calls
//! (`sys_inb`/`sys_outl`/...) mediated by the kernel through the
//! process's I/O privilege; the mapped variant (`io.h:11-13`: a volatile
//! pointer write to the mapped BAR) is the other branch of the same file.
//!
//! One difference from the C helpers is deliberate: on a failed read C's
//! `my_inb` prints a warning and returns an **uninitialized** local
//! (`io.h:10-19`: `u32_t value; if ((r = sys_inb(port, &value)) != OK)
//! printf(...); return (u8_t)value;`). The rewrite reports the failure to
//! the caller, which turns it into an error instead of continuing with a
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
    /// Write one byte (`sdr_out8`): the mixer pair, the Sound Blaster
    /// command port, and the global control index are byte-wide.
    fn out8(&mut self, port: u16, value: u8) -> Result<(), i32>;
    /// Write one 16-bit word (`sdr_out16`).
    fn out16(&mut self, port: u16, value: u16) -> Result<(), i32>;
    /// Write one 32-bit word (`sdr_out32`): the global control data
    /// register is long-word wide.
    fn out32(&mut self, port: u16, value: u32) -> Result<(), i32>;
    /// Read one byte (`sdr_in8`).
    fn in8(&mut self, port: u16) -> Result<u8, i32>;
    /// Read one 16-bit word (`sdr_in16`).
    fn in16(&mut self, port: u16) -> Result<u16, i32>;
    /// Read one 32-bit word (`sdr_in32`).
    fn in32(&mut self, port: u16) -> Result<u32, i32>;
    /// Wait for a hardware settling time (`micro_delay`).
    ///
    /// Infallible for the same reason as the third card's seam: C's
    /// `micro_delay` cannot fail (it calibrates the time stamp counter
    /// once and then busy-waits, `tsc_util.c:57-72`), and the calibrated
    /// wait is a registered gap in the rewrite — the kernel reports a
    /// zero CPU frequency because the TSC is not calibrated yet, so a
    /// busy-wait here would return immediately and claim to have waited.
    /// The mixer's two hundred-microsecond pair and the reset pulses are
    /// settling waits between register writes, not timing any observable
    /// result depends on.
    fn delay_micros(&mut self, micros: u32);
}

/// The production seam: the kernel-mediated port I/O calls.
///
/// The base port comes from the PCI probe's BAR (C's `dev.base[0]`, the
/// `pci_attr_r32(devind, PCI_BAR + i * 4) & 0xffffffe0` path at
/// `als4000.c:307-308`), which the binary resolves; the offsets are
/// added per access.
pub struct KernelPorts {
    /// The card's base port (`dev.base[0]`).
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
    fn delay_micros(&mut self, _micros: u32) {
        // See the trait method: the calibrated wait is the registered gap.
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registers::{REG_GCR_DATA, REG_INTR_STS, REG_SB_CMD};

    #[test]
    fn test_the_production_seam_reaches_the_kernel_calls() {
        // The seam is real: in a host build the kernel call cannot
        // complete, so the failure is the transport's, not a stand-in
        // (a stand-in would answer ENOSYS).
        let mut ports = KernelPorts { base: 0x0400 };
        assert_eq!(ports.out8(REG_SB_CMD, 1), Err(-minix_types::EIO));
        assert_eq!(ports.in8(REG_INTR_STS), Err(-minix_types::EIO));
        assert_eq!(ports.in32(REG_GCR_DATA), Err(-minix_types::EIO));
        // The delay is infallible, exactly as C's.
        ports.delay_micros(100);
    }
}
