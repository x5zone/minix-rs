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

use crate::clock::{ClockArch, ProfileClockError};

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

/// Interval between ticks in counter units, armed by `init_timer` and
/// re-read by the per-tick re-arm. A static (not instance state) because
/// the kernel constructs transient `ClockArch` instances per call — see
/// `kernel::clock::local_tick`.
static TICK_INTERVAL: core::sync::atomic::AtomicU64 = core::sync::atomic::AtomicU64::new(0);

impl AArch64ClockArch {
    /// Re-arm the one-shot: CNTP fires once per CVAL (no auto-periodic
    /// mode), so the tick body must schedule the next deadline or the
    /// tick train stops dead — the aarch64 analogue of the x86 LAPIC ICR
    /// re-arm (`kernel::clock::local_tick` calls this after every tick).
    fn rearm_next_tick() {
        let interval = TICK_INTERVAL.load(core::sync::atomic::Ordering::Relaxed);
        let interval = if interval == 0 {
            // `local_timer_eoi` before any `init_timer` would be a wiring
            // bug (D-59: the gate opens only after the source is armed);
            // fall back to CNTFRQ at DEFAULT_HZ rather than arming a zero
            // interval (an interrupt storm).
            let freq: u64;
            // SAFETY: side-effect-free counter-frequency read.
            unsafe {
                core::arch::asm!("mrs {}, cntfrq_el0", out(reg) freq, options(nomem, nostack));
            }
            freq / crate::clock::DEFAULT_HZ as u64
        } else {
            interval
        };
        let now = Self::read_counter();
        // SAFETY: an absolute compare-value write on this CPU's own timer
        // module; architected system registers at EL1.
        unsafe {
            core::arch::asm!("msr cntp_cval_el0, {}", in(reg) now + interval, options(nostack));
        }
    }

    /// Read CNTPCT_EL0 (the system counter).
    fn read_counter() -> u64 {
        let count: u64;
        // SAFETY: side-effect-free counter read.
        unsafe {
            core::arch::asm!("mrs {}, cntpct_el0", out(reg) count, options(nomem, nostack));
        }
        count
    }
}

impl ClockArch for AArch64ClockArch {
    fn new(desc: &dyn minix_platform::TimerDesc) -> Self {
        // ARM Generic Timer carries no data in the descriptor — frequency
        // is read from CNTFRQ_EL0 at runtime. We only verify the type.
        desc.as_any()
            .downcast_ref::<ArmGenericTimerDesc>()
            .expect("AArch64ClockArch::new: expected ArmGenericTimerDesc");
        Self
    }

    fn init_timer(&mut self, hz: u32, _cpu_id: u32) {
        // ARM Generic Timer is configured by firmware (TF-A/U-Boot).
        // We need to:
        // 1. Read the counter frequency from CNTFRQ_EL0
        // 2. Read the current counter value from CNTPCT_EL0
        // 3. Calculate the absolute compare value for the desired tick rate
        // 4. Set CNTP_CVAL_EL0 and enable the timer
        //
        // CNTP_CVAL_EL0 / CNTP_CTL_EL0 are per-CPU system registers, so
        // `cpu_id` is not needed — each CPU configures its own timer.

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
        let interval = freq / hz as u64;
        let compare = current_count + interval;
        unsafe {
            // Set the compare value for the EL1 physical timer
            core::arch::asm!("msr cntp_cval_el0, {}", in(reg) compare);
            // Program the timer module but leave it gated:
            // CNTP_CTL_EL0 = 0b10 (Enable=0, IMASK=1). Opening this gate is
            // `TimerIrqGate::enable_timer_irq`'s job, done at the C
            // `boot_cpu_init_timer` position (bsp_finish_booting Step 6)
            // AFTER the handler is registered — not here. Arming + opening
            // in one step (the pre-D-59 behavior) left the timer live
            // through the rest of boot with no handler attached.
            // C parity: init_local_timer only programs the source; the
            // enable is register_local_timer_handler's `enable_irq`
            // (arch_clock.c:177-196 / interrupt.c:65).
            core::arch::asm!("msr cntp_ctl_el0, {}", in(reg) 0x2u64);
        }
        // Record the interval for the per-tick re-arm (`local_timer_eoi`
        // — CNTP is a one-shot; without the re-arm the tick train stops
        // dead after the first interrupt).
        TICK_INTERVAL.store(interval, core::sync::atomic::Ordering::Relaxed);
    }

    fn read_ticks(&self) -> u64 {
        let count: u64;
        unsafe {
            core::arch::asm!("mrs {}, cntpct_el0", out(reg) count);
        }
        count
    }

    fn stop_local_timer(&mut self, _cpu_id: u32) {
        // Disable the EL1 Physical Timer by clearing CNTP_CTL_EL0.ENABLE.
        //
        // C: smp.c:56-61 — inline timer disable in smp_ipi_halt_handler
        // CNTP_CTL_EL0 is a per-CPU system register; `cpu_id` is not needed.
        unsafe {
            // CNTP_CTL_EL0: bit 0 = ENABLE, bit 1 = IMASK
            // Clear ENABLE (bit 0) and set IMASK (bit 1) to suppress IRQ
            core::arch::asm!("msr cntp_ctl_el0, {}", in(reg) 0x2u64);
        }
    }

    fn local_timer_eoi(&mut self) {
        // Re-arm the one-shot (see `rearm_next_tick`) — called by
        // `kernel::clock::local_tick` after every tick.
        Self::rearm_next_tick();
    }

    fn init_profile_clock(&mut self, _hz: u32) -> Result<(), ProfileClockError> {
        // ARM64 statistical profiling uses the PMU (Performance Monitoring
        // Unit), not a second timer channel. The PMU is not yet integrated
        // in the arch layer — return Err to indicate the caller should
        // fall back to PROF_NMI (which is also not yet available).
        //
        // C: sprofile.c:init_profile_clock(freq) — on ARM, uses PMU
        Err(ProfileClockError::Unsupported)
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
