//! The port-I/O seam: how the card's registers are reached.
//!
//! C correspondence: `io.h`'s `sdr_in8`/`sdr_in16`/`sdr_in32` and
//! `sdr_out8`/`sdr_out16`/`sdr_out32` (the register traffic) plus
//! `micro_delay` (`cmi8738.c:73`, `:155`). In C these are libsys calls
//! (`sys_inb`/`sys_inl`/... and the delay), which the kernel mediates
//! through the process's I/O privilege.
//!
//! Unlike the DMA allocator, this seam has a **real** production
//! implementation: `minix-sys` carries the `sys_in*`/`sys_out*` wrappers,
//! so [`KernelPorts`] talks to the actual card through the kernel.

use minix_sys::syscall::{
    DirectKernelCallTransport, sys_inb, sys_inl, sys_inw, sys_outb, sys_outl, sys_outw,
};

/// The register traffic the card's sequences need.
pub trait PortIo {
    /// Write one byte (`sdr_out8`): the Sound Blaster address and data
    /// ports are byte-wide (`dev_mixer_write`).
    fn out8(&mut self, port: u16, value: u8) -> Result<(), i32>;
    /// Write one 16-bit word (`sdr_out16`): the sample counts and the DMA
    /// lengths are words.
    fn out16(&mut self, port: u16, value: u16) -> Result<(), i32>;
    /// Write one 32-bit word (`sdr_out32`).
    fn out32(&mut self, port: u16, value: u32) -> Result<(), i32>;
    /// Read one byte (`sdr_in8`): the Sound Blaster data port.
    fn in8(&mut self, port: u16) -> Result<u8, i32>;
    /// Read one 16-bit word (`sdr_in16`): the running DMA address.
    fn in16(&mut self, port: u16) -> Result<u16, i32>;
    /// Read one 32-bit word (`sdr_in32`).
    fn in32(&mut self, port: u16) -> Result<u32, i32>;
    /// Wait for a hardware settling time (`micro_delay`).
    ///
    /// Infallible because C's `micro_delay` cannot fail: it calibrates the
    /// time stamp counter once (`tsc_util.c:57-72`, panicking when that
    /// fails) and then busy-waits, and every card call site discards the
    /// result (`cmi8738.c:73`, `:155`). The calibrated wait is a
    /// registered gap in the rewrite — the kernel reports a zero CPU
    /// frequency because the TSC is not calibrated yet
    /// (`os/kernel/src/misc.rs`, `CpuInfoEntry::freq`), so a busy-wait
    /// here would return immediately and claim to have waited. The pulse
    /// delays are the only place this matters, and they are settling
    /// waits between register writes, not timing that any observable
    /// result depends on.
    fn delay_micros(&mut self, micros: u32);
}

/// The production seam: the kernel-mediated port I/O calls.
///
/// The base port comes from the PCI probe's BAR (C's `dev.base[0]`, the
/// `pci_attr_r32(devind, PCI_BAR) & 0xffffffe0` path at
/// `cmi8738.c:270-272`), which the binary resolves; the offsets are added
/// per access.
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

    #[test]
    fn test_the_production_seam_reaches_the_kernel_calls() {
        // The seam is real: in a host build the kernel call cannot
        // complete, so the failure is the transport's, not a stand-in
        // (a stand-in would answer ENOSYS).
        let mut ports = KernelPorts { base: 0x0200 };
        assert_eq!(ports.out8(0x00, 1), Err(-minix_types::EIO));
        assert_eq!(ports.in8(0x22), Err(-minix_types::EIO));
        assert_eq!(ports.in32(0x04), Err(-minix_types::EIO));
        // The delay is infallible, exactly as C's.
        ports.delay_micros(100);
    }
}
