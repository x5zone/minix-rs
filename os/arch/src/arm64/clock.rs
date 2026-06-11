//! ARM64 (aarch64) clock implementation using Generic Timer
//!
//! Implements `ClockArch` for ARM64 using the ARM Generic Timer
//! (EL1 Physical Timer). The timer frequency is provided by firmware
//! via CNTFRQ_EL0.
//!
//! C: earm/arch_system.c PMU init (cycle counter for user mode)

use crate::clock::ClockArch;

/// ARM64 clock using Generic Timer.
///
/// The ARM Generic Timer consists of:
/// - CNTFRQ_EL0: Counter frequency (set by firmware)
/// - CNTPCT_EL0: Physical counter value (read-only)
/// - CNTP_CVAL_EL0: Timer compare value (triggers interrupt when counter >= this)
/// - CNTP_CTL_EL0: Timer control (enable, IMASK, ISTATUS)
///
/// C: earm/arch_system.c PMU init (cycle counter for user mode)
pub struct AArch64ClockArch;

impl ClockArch for AArch64ClockArch {
    fn init_timer(hz: u32) {
        // ARM Generic Timer is configured by firmware (TF-A/U-Boot).
        // We need to:
        // 1. Read the counter frequency from CNTFRQ_EL0
        // 2. Read the current counter value from CNTPCT_EL0
        // 3. Calculate the absolute compare value for the desired tick rate
        // 4. Set CNTP_CVAL_EL0 and enable the timer

        let freq: u64;
        unsafe {
            core::arch::asm!("mrs {}, cntfrq_el0", out(reg) freq);
        }

        // Read current counter value for absolute compare calculation
        let current_count: u64;
        unsafe {
            core::arch::asm!("mrs {}, cntpct_el0", out(reg) current_count);
        }

        // Set compare value: current_count + (freq / hz) ticks per interrupt
        // CNTP_CVAL_EL0 is an absolute value, not a relative interval.
        let compare = current_count + freq / hz as u64;
        unsafe {
            // Set the compare value for the EL1 physical timer
            core::arch::asm!("msr cntp_cval_el0, {}", in(reg) compare);
            // Enable the timer (bit 0 = ENABLE)
            core::arch::asm!("msr cntp_ctl_el0, {}", in(reg) 1u64);
        }
    }

    fn read_ticks() -> u64 {
        let count: u64;
        unsafe {
            core::arch::asm!("mrs {}, cntpct_el0", out(reg) count);
        }
        count
    }
}
