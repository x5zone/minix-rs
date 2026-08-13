//! RISC-V 64-bit clock implementation using CLINT mtime
//!
//! Implements `ClockArch` for RISC-V 64-bit using the CLINT (Core Local
//! Interruptor) mtime register. The mtime register is a memory-mapped
//! counter that increments at a fixed frequency.
//!
//! # Instance-based design (see 04-platform-discovery.md §3.4)
//!
//! Hardware parameters (CLINT mtime/mtimecmp addresses, frequency) are
//! stored in instance fields, populated by `new(desc)` via `Any` downcast
//! to `ClintDesc`. This replaces the previous hardcoded
//! `CLINT_MTIME` / `CLINT_MTIMECMP` / `MTIME_FREQ` constants.
//!
//! C: No Minix3 equivalent (Minix3 has no RISC-V port).

use minix_platform::arch::riscv64::ClintDesc;

use crate::clock::{ClockArch, ProfileClockError};

/// RISC-V 64-bit clock using CLINT mtime.
///
/// The CLINT provides:
/// - mtime: 64-bit memory-mapped counter (read-only)
/// - mtimecmp: 64-bit compare register per hart (read-write)
///
/// When mtime >= mtimecmp, a timer interrupt is generated.
/// The handler must update mtimecmp to schedule the next interrupt.
///
/// # Fields
///
/// - `mtime_addr`: CLINT mtime register MMIO address, from `ClintDesc`.
/// - `mtimecmp_base`: CLINT mtimecmp base address (hart 0), from `ClintDesc`.
/// - `mtimecmp_stride`: per-hart mtimecmp spacing (SMP-ready), from `ClintDesc`.
/// - `freq`: mtime counter frequency (Hz), from `ClintDesc`.
///
/// C: No Minix3 equivalent (Minix3 has no RISC-V port).
pub struct Riscv64ClockArch {
    mtime_addr: usize,
    mtimecmp_base: usize,
    #[allow(dead_code)]
    mtimecmp_stride: usize,
    freq: u64,
}

impl ClockArch for Riscv64ClockArch {
    fn new(desc: &dyn minix_platform::TimerDesc) -> Self {
        let clint = desc.as_any()
            .downcast_ref::<ClintDesc>()
            .expect("Riscv64ClockArch::new: expected ClintDesc");
        Self {
            mtime_addr: clint.mtime_addr,
            mtimecmp_base: clint.mtimecmp_base,
            mtimecmp_stride: clint.mtimecmp_stride,
            freq: clint.freq,
        }
    }

    fn init_timer(&mut self, hz: u32) {
        // Read current mtime value
        let mtime: u64;
        unsafe {
            mtime = core::ptr::read_volatile(self.mtime_addr as *const u64);
        }

        // Calculate interval between interrupts
        let interval = self.freq / hz as u64;

        // Set mtimecmp = mtime + interval to schedule first interrupt
        let mtimecmp = mtime + interval;
        unsafe {
            core::ptr::write_volatile(self.mtimecmp_base as *mut u64, mtimecmp);
        }

        // Enable S-mode timer interrupt (STIE bit in sie)
        unsafe {
            core::arch::asm!("csrs sie, {bits}", bits = in(reg) 0x20u64);
        }
    }

    fn read_ticks(&self) -> u64 {
        unsafe {
            core::ptr::read_volatile(self.mtime_addr as *const u64)
        }
    }

    fn stop_local_timer(&mut self) {
        // Disable S-mode timer interrupt by clearing STIE bit in sie,
        // and set mtimecmp to max to prevent any pending interrupt.
        //
        // C: smp.c:56-61 — inline timer disable in smp_ipi_halt_handler
        unsafe {
            // Clear STIE (bit 5) in sie CSR
            core::arch::asm!("csrc sie, {bits}", bits = in(reg) 0x20u64);
            // Set mtimecmp to u64::MAX to prevent timer fire
            core::ptr::write_volatile(self.mtimecmp_base as *mut u64, u64::MAX);
        }
    }

    fn init_profile_clock(&mut self, _hz: u32) -> Result<(), ProfileClockError> {
        // RISC-V does not have a separate profiling timer. The CLINT
        // mtimecmp is already used for scheduling. A second mtimecmp
        // (if available for S-mode) could be used, but this is not
        // standardized in the privilege spec.
        //
        // Return Err to indicate profiling is not available on RISC-V.
        Err(ProfileClockError::Unsupported)
    }

    fn stop_profile_clock(&mut self) {
        // No profiling timer to stop on RISC-V.
    }

    fn ack_profile_clock(&mut self) {
        // No profiling timer to ack on RISC-V.
        // C: arch_ack_profile_clock() — profile.c:123
    }
}
