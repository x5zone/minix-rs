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
//! C: No Minix3 equivalent (Minix3 has no RISC-V port) — architectural
//! evolution `[ARCH: K-2]` (05-clock-interrupt-init.md §3.8).

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
///   Each hart's comparator lives at `mtimecmp_base + hart_id * mtimecmp_stride`.
/// - `mtimecmp_stride`: per-hart mtimecmp spacing, from `ClintDesc`.
/// - `freq`: mtime counter frequency (Hz), from `ClintDesc`.
///
/// C: No Minix3 equivalent (Minix3 has no RISC-V port).
pub struct Riscv64ClockArch {
    mtime_addr: usize,
    /// CLINT `mtimecmp` base address (hart 0). This hart's comparator is
    /// `mtimecmp_base + hart_id * mtimecmp_stride`.
    mtimecmp_base: usize,
    /// Byte distance between consecutive harts' `mtimecmp` registers.
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

    fn init_timer(&mut self, hz: u32, cpu_id: u32) {
        // Read current mtime value
        let mtime: u64;
        unsafe {
            mtime = core::ptr::read_volatile(self.mtime_addr as *const u64);
        }

        // Calculate interval between interrupts
        let interval = self.freq / hz as u64;

        // The CLINT holds one mtimecmp per hart; the caller passes the
        // current hart id so each CPU programs its own comparator instead
        // of hart 0's. C: app_cpu_init_timer() — clock.c:306.
        let mtimecmp_addr = self.mtimecmp_base + (cpu_id as usize) * self.mtimecmp_stride;
        // Set mtimecmp = mtime + interval to schedule first interrupt
        let mtimecmp = mtime + interval;
        unsafe {
            core::ptr::write_volatile(mtimecmp_addr as *mut u64, mtimecmp);
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

    fn stop_local_timer(&mut self, cpu_id: u32) {
        // Disable S-mode timer interrupt by clearing STIE bit in sie,
        // and set mtimecmp to max to prevent any pending interrupt.
        //
        // C: smp.c:56-61 — inline timer disable in smp_ipi_halt_handler
        let mtimecmp_addr = self.mtimecmp_base + (cpu_id as usize) * self.mtimecmp_stride;
        unsafe {
            // Clear STIE (bit 5) in sie CSR
            core::arch::asm!("csrc sie, {bits}", bits = in(reg) 0x20u64);
            // Set this hart's mtimecmp to u64::MAX to prevent timer fire
            core::ptr::write_volatile(mtimecmp_addr as *mut u64, u64::MAX);
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
