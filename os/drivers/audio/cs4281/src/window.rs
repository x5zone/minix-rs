//! The register-window seam: how the card's registers are reached.
//!
//! C correspondence: `io.h`'s `my_inb`/`my_inw`/`my_inl` and
//! `my_outb`/`my_outw`/`my_outl` behind the `sdr_*` macros, in the branch
//! this card compiles: `DMA_BASE_IOMAP` is defined (`cs4281.h:5`), so
//! each access is a **volatile load or store** to an address the driver
//! mapped out of the PCI BAR (`io.h:11-13`: `value = *(volatile u8_t *)
//! (port);`) rather than a port I/O call. The waits are `micro_delay`.
//!
//! The seam is infallible for the same reason the C branch is: a mapped
//! load or store either happens or faults, there is no error channel to
//! report. What *is* fallible is obtaining the mapping, and that is the
//! binary's step ([`crate::probe`]): the production window is built only
//! after the VM server placed the BAR in the driver's address space.

/// The register traffic the card's sequences need.
pub trait RegisterWindow {
    /// Read one byte (`sdr_in8`).
    fn read8(&mut self, offset: u32) -> u8;
    /// Read one 16-bit word (`sdr_in16`).
    fn read16(&mut self, offset: u32) -> u16;
    /// Read one 32-bit word (`sdr_in32`).
    fn read32(&mut self, offset: u32) -> u32;
    /// Write one byte (`sdr_out8`).
    fn write8(&mut self, offset: u32, value: u8);
    /// Write one 16-bit word (`sdr_out16`).
    fn write16(&mut self, offset: u32, value: u16);
    /// Write one 32-bit word (`sdr_out32`).
    fn write32(&mut self, offset: u32, value: u32);
    /// Wait for a hardware settling time (`micro_delay`).
    ///
    /// Infallible for the same reason as the other cards' seams: C's
    /// `micro_delay` cannot fail (it calibrates the time stamp counter
    /// once and then busy-waits, `tsc_util.c:57-72`), and the calibrated
    /// wait is a registered gap in the rewrite — the kernel reports a
    /// zero CPU frequency because the TSC is not calibrated yet, so a
    /// busy-wait here would return immediately and claim to have waited.
    /// This card's waits are codec settling times and reset pulses.
    fn delay_micros(&mut self, micros: u32);
}

/// The production window: the mapped register base.
///
/// C: the `dev.base[i]` values `dev_probe` fills from `vm_map_phys`
/// (`cs4281.c:305-318`), with `reg(n)` adding the offset (`cs4281.c:26`).
pub struct MappedWindow {
    /// The base address the VM server mapped.
    base: *mut u8,
}

impl MappedWindow {
    /// A window over an already-mapped register base.
    ///
    /// # Safety
    ///
    /// `base` must be the start of a mapping of the card's register
    /// window that stays valid for this window's lifetime — in production
    /// the address [`crate::probe::map_bar`] obtained from the VM server.
    pub unsafe fn new(base: *mut u8) -> Self {
        MappedWindow { base }
    }

    /// The base address, for diagnostics.
    pub fn base(&self) -> *mut u8 {
        self.base
    }
}

impl RegisterWindow for MappedWindow {
    fn read8(&mut self, offset: u32) -> u8 {
        // SAFETY: the mapping covers the register window, which is wider
        // than every offset in the register map; a volatile read keeps the
        // access from being optimized away or reordered, exactly as C's.
        unsafe { ::core::ptr::read_volatile(self.base.add(offset as usize)) }
    }
    fn read16(&mut self, offset: u32) -> u16 {
        // SAFETY: as `read8`; the offsets this driver reads as words are
        // even, and the window is wider than the map.
        unsafe { ::core::ptr::read_volatile(self.base.add(offset as usize) as *const u16) }
    }
    fn read32(&mut self, offset: u32) -> u32 {
        // SAFETY: as `read8`; the offsets this driver reads as long words
        // are four-byte aligned within the window.
        unsafe { ::core::ptr::read_volatile(self.base.add(offset as usize) as *const u32) }
    }
    fn write8(&mut self, offset: u32, value: u8) {
        // SAFETY: as `read8`, for the store direction.
        unsafe { ::core::ptr::write_volatile(self.base.add(offset as usize), value) }
    }
    fn write16(&mut self, offset: u32, value: u16) {
        // SAFETY: as `read16`, for the store direction.
        unsafe { ::core::ptr::write_volatile(self.base.add(offset as usize) as *mut u16, value) }
    }
    fn write32(&mut self, offset: u32, value: u32) {
        // SAFETY: as `read32`, for the store direction.
        unsafe { ::core::ptr::write_volatile(self.base.add(offset as usize) as *mut u32, value) }
    }
    fn delay_micros(&mut self, _micros: u32) {
        // See the trait method: the calibrated wait is the registered gap.
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;

    #[test]
    fn test_the_production_window_reads_and_writes_the_mapped_bytes() {
        // The production leg is exercised over a real allocation: the
        // same volatile access path a mapped BAR takes, with a host
        // buffer standing in for the card's registers.
        let mut backing = vec![0u8; 64];
        let base = backing.as_mut_ptr();
        // SAFETY: `base` points at a live 64-byte allocation that
        // outlives the window.
        let mut window = unsafe { MappedWindow::new(base) };
        window.write8(0x04, 0xab);
        window.write16(0x08, 0x1234);
        window.write32(0x10, 0xdead_beef);
        assert_eq!(window.read8(0x04), 0xab);
        assert_eq!(window.read16(0x08), 0x1234);
        assert_eq!(window.read32(0x10), 0xdead_beef);
        // The bytes really landed in the backing store (little-endian).
        assert_eq!(&backing[0x10..0x14], &[0xef, 0xbe, 0xad, 0xde]);
        assert_eq!(window.base(), base);
        // The delay is infallible, exactly as C's.
        window.delay_micros(100);
    }
}
