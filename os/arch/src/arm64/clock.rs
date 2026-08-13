//! ARM64 (aarch64) clock implementation using Generic Timer
//!
//! Implements `ClockArch` for ARM64 using the ARM Generic Timer
//! (EL1 Physical Timer). The timer frequency is provided by firmware
//! via CNTFRQ_EL0.
//!
//! # Instance-based design (see 04-platform-discovery.md §3.4)
//!
//! `ArmGenericTimerDesc` carries no data (frequency is read from
//! CNTFRQ_EL0 at runtime), so `new()` is a no-op constructor. The struct
//! exists only to satisfy the instance-based trait contract.
//!
//! C: earm/arch_system.c PMU init (cycle counter for user mode)

use minix_platform::arch::aarch64::ArmGenericTimerDesc;

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
    fn new(desc: &dyn minix_platform::TimerDesc) -> Self {
        // ARM Generic Timer carries no data in the descriptor — frequency
        // is read from CNTFRQ_EL0 at runtime. We only verify the type.
        desc.as_any()
            .downcast_ref::<ArmGenericTimerDesc>()
            .expect("AArch64ClockArch::new: expected ArmGenericTimerDesc");
        Self
    }

    fn init_timer(&mut self, hz: u32) {
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

    fn read_ticks(&self) -> u64 {
        let count: u64;
        unsafe {
            core::arch::asm!("mrs {}, cntpct_el0", out(reg) count);
        }
        count
    }

    fn stop_local_timer(&mut self) {
        // Disable the EL1 Physical Timer by clearing CNTP_CTL_EL0.ENABLE.
        //
        // C: smp.c:56-61 — inline timer disable in smp_ipi_halt_handler
        unsafe {
            // CNTP_CTL_EL0: bit 0 = ENABLE, bit 1 = IMASK
            // Clear ENABLE (bit 0) and set IMASK (bit 1) to suppress IRQ
            core::arch::asm!("msr cntp_ctl_el0, {}", in(reg) 0x2u64);
        }
    }

    fn init_profile_clock(&mut self, _hz: u32) -> Result<(), ()> {
        // ARM64 statistical profiling uses the PMU (Performance Monitoring
        // Unit), not a second timer channel. The PMU is not yet integrated
        // in the arch layer — return Err to indicate the caller should
        // fall back to PROF_NMI (which is also not yet available).
        //
        // C: sprofile.c:init_profile_clock(freq) — on ARM, uses PMU
        Err(())
    }

    fn stop_profile_clock(&mut self) {
        // PMU profiling is not yet supported on ARM64.
        // C: sprofile.c:stop_profile_clock()
    }

    fn ack_profile_clock(&mut self) {
        // PMU profiling is not yet supported on ARM64.
        // C: arch_ack_profile_clock() — profile.c:123
    }
}
