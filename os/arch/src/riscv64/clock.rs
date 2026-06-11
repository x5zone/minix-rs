//! RISC-V 64-bit clock implementation using CLINT mtime
//!
//! Implements `ClockArch` for RISC-V 64-bit using the CLINT (Core Local
//! Interruptor) mtime register. The mtime register is a memory-mapped
//! counter that increments at a fixed frequency.
//!
//! C: No Minix3 equivalent (Minix3 has no RISC-V port).

use crate::clock::ClockArch;

/// CLINT mtime register address for QEMU virt machine.
/// TODO: Should be discovered from device tree.
const CLINT_MTIME: usize = 0x200_BFF8;

/// CLINT mtimecmp register address for QEMU virt machine (hart 0).
/// NOTE: On multi-hart systems, each hart has its own mtimecmp.
/// Current implementation supports hart 0 only.
const CLINT_MTIMECMP: usize = 0x200_4000;

/// CLINT mtime frequency for QEMU virt machine (10 MHz).
/// TODO: Should be discovered from device tree.
const MTIME_FREQ: u64 = 10_000_000;

/// RISC-V 64-bit clock using CLINT mtime.
///
/// The CLINT provides:
/// - mtime: 64-bit memory-mapped counter (read-only)
/// - mtimecmp: 64-bit compare register per hart (read-write)
///
/// When mtime >= mtimecmp, a timer interrupt is generated.
/// The handler must update mtimecmp to schedule the next interrupt.
///
/// C: No Minix3 equivalent (Minix3 has no RISC-V port).
pub struct Riscv64ClockArch;

impl ClockArch for Riscv64ClockArch {
    fn init_timer(hz: u32) {
        // Read current mtime value
        let mtime: u64;
        unsafe {
            mtime = core::ptr::read_volatile(CLINT_MTIME as *const u64);
        }

        // Calculate interval between interrupts
        let interval = MTIME_FREQ / hz as u64;

        // Set mtimecmp = mtime + interval to schedule first interrupt
        let mtimecmp = mtime + interval;
        unsafe {
            core::ptr::write_volatile(CLINT_MTIMECMP as *mut u64, mtimecmp);
        }

        // Enable S-mode timer interrupt (STIE bit in sie)
        unsafe {
            core::arch::asm!("csrs sie, {bits}", bits = in(reg) 0x20u64);
        }
    }

    fn read_ticks() -> u64 {
        unsafe {
            core::ptr::read_volatile(CLINT_MTIME as *const u64)
        }
    }
}
