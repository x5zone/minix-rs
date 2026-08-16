//! Clock architecture abstraction
//!
//! Defines the trait interface for hardware timer configuration.
//!
//! # Design decisions (see 05-clock-interrupt-init.md §3.1, §3.2, 15-clock-timer.md §4.5)
//!
//! - **ClockArch trait** (§3.1): Separates hardware timer configuration
//!   from software clock state. Each architecture implements its own
//!   timer source (8254 PIT / ARM Generic Timer / RISC-V mtime).
//! - **DEFAULT_HZ compile-time constant** (§3.2): Replaces C's
//!   `env_get("hz")` runtime configuration.
//!
//! # Quantum decrement (D9)
//!
//! The design (15-clock-timer.md §4.5) originally specified a `ClockArch::arch_tick()`
//! method for quantum decrement. However, `KProcess` lives in `minix-kernel`,
//! and `ClockArch` lives in `minix-arch` — adding `arch_tick(&mut KProcess)` would
//! create a circular dependency. Instead, quantum decrement is implemented as a
//! **kernel function** (`clock::decrement_quantum`) that calls `ClockArch::read_tsc()`
//! to get the TSC delta, then applies it to `current_proc`. This preserves the D9
//! invariant: quantum is NOT in `ClockState::tick()`.

/// Default clock tick frequency in Hz.
///
/// minix-rs uses 100 Hz (10ms tick) across all architectures.
///
/// # Architecture evolution ([ARCH: K-3], 32-bit → 64-bit)
///
/// | Architecture | Minix3 C (32-bit) | minix-rs (64-bit) |
/// |-------------|-------------------|-------------------|
/// | x86 | 60 Hz (`archconst.h`) | 100 Hz |
/// | ARM | 1000 Hz (`archconst.h`) | 100 Hz |
///
/// C: DEFAULT_HZ — i386/include/archconst.h:4 (60), earm/include/archconst.h:4 (1000)
///
/// minix-rs unifies to 100 Hz: ARM's 1000 Hz was for 32-bit embedded targets
/// with coarse timers; 64-bit platforms have high-resolution timers and
/// don't need 1ms ticks. 100 Hz matches typical server/desktop kernels.
///
/// **Authoritative definition**: this is the arch-level constant. The kernel
/// crate (`os/kernel/src/clock.rs`) has an independent copy to avoid a
/// cross-crate dependency; modify here first, then sync.
pub const DEFAULT_HZ: u32 = 100;

/// Number of load history slots for load average calculation.
///
/// C: _LOAD_HISTORY — include/minix/type.h:95 (150 = 60s*15min/6s)
/// 与 os/kernel/src/clock.rs 的 `LOAD_HISTORY` 保持一致（C 对齐）。
pub const LOAD_HISTORY_SIZE: usize = 150;

/// Failure configuring the statistical profiling timer.
///
/// Replaces the former `Result<(), ()>` (C-D-5): a single variant today,
/// but a named type self-documents the failure mode in the signature and
/// leaves room for variants (e.g. a busy timer channel) without breaking
/// callers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProfileClockError {
    /// The architecture has no available profiling timer: RISC-V shares
    /// the CLINT mtimecmp with scheduling, ARM64 needs the PMU (not yet
    /// integrated), and x86-64's RTC range excludes `hz < 2`.
    Unsupported,
}

/// Architecture abstraction for hardware timer configuration.
///
/// Each architecture implements this trait to configure its hardware
/// timer source and provide tick-reading capability.
///
/// # Instance-based design (see 04-platform-discovery.md §3.4)
///
/// `ClockArch` is **instance-based**: `new(desc)` stores parsed hardware
/// parameters (base addresses, frequencies) in instance fields. This
/// replaces the old static-trait design that hardcoded constants per arch.
/// Upper layers obtain the descriptor from `minix_platform::platform_desc()`.
///
/// # Architecture mapping
///
/// | Method | x86-64 | ARM64 | RISC-V |
/// |--------|--------|-------|--------|
/// | `new()` | store PIT freq + LAPIC base from `PitDesc` | no-op (CNTFRQ read at runtime) | store CLINT addrs from `ClintDesc` |
/// | `init_timer()` | 8254 PIT divisor / LAPIC Timer | ARM Generic Timer (CNTFRQ/CNTPCT) | CLINT mtimecmp |
/// | `read_ticks()` | TSC (rdtsc) | CNTPCT_EL0 | mtime (MMIO) |
/// | `read_tsc()` | TSC (rdtsc) | CNTPCT_EL0 | mtime (MMIO) |
///
/// C: init_clock() hardware portion + arch_init() APIC timer
pub trait ClockArch: Sized + Send + Sync {
    /// Create an instance from a timer descriptor.
    ///
    /// Stores the hardware parameters (base address, frequency) from the
    /// descriptor into instance fields. Instances are transient: each call
    /// site (timer init, read, stop, profile clock methods) constructs one
    /// from `platform_desc().timer()` and drops it after use.
    ///
    /// # Panics
    ///
    /// May panic if `desc` does not downcast to the architecture's expected
    /// concrete `TimerDesc` implementor (e.g. x86-64 expects `PitDesc`).
    /// Upper layers guarantee the correct type is passed.
    fn new(desc: &dyn minix_platform::TimerDesc) -> Self;

    /// Configure and start the hardware timer at the given frequency.
    ///
    /// Called during `init_clock_and_interrupts()`; `bsp_finish_booting()`
    /// re-calls it as an idempotent no-op safety net (re-writes the same
    /// registers, no side effects). After this call, the timer generates
    /// periodic interrupts at `hz` Hz.
    ///
    /// `cpu_id` identifies the CPU (RISC-V: hart) whose timer is configured.
    /// Only RISC-V uses it: the CLINT is a single MMIO block with one
    /// `mtimecmp` per hart at `mtimecmp_base + cpu_id * mtimecmp_stride`, so
    /// the hart id is required to reach the local comparator. x86-64 (PIT +
    /// per-CPU LAPIC) and aarch64 (CNTP_* system registers) address the
    /// current CPU's timer implicitly and ignore the argument.
    ///
    /// C: init_clock() hardware portion + arch_init() APIC timer
    fn init_timer(&mut self, hz: u32, cpu_id: u32);

    /// Read the current hardware tick count.
    ///
    /// Used for fine-grained timing and profiling.
    fn read_ticks(&self) -> u64;

    /// Read the CPU's Time Stamp Counter (cycle counter).
    ///
    /// Returns the current hardware cycle count. On x86-64 this is `rdtsc`,
    /// on aarch64 this is `CNTPCT_EL0`, on riscv64 this is `mtime`.
    ///
    /// The default implementation delegates to `read_ticks()` since all
    /// three architectures use the same hardware counter for both.
    ///
    /// C: `read_tsc_64()` — arch/i386/arch_clock.c / arch/earm/arch_clock.c
    ///
    /// # D9: quantum decrement
    ///
    /// The kernel's `clock::decrement_quantum()` calls this to compute the
    /// TSC delta since the last tick, then decrements the current process's
    /// `p_cpu_time_left`. This matches C's `arch_timer_int_handler()`
    /// (arch_clock.c:326-330: `p->p_cpu_time_left -= tsc_delta`).
    fn read_tsc(&self) -> u64 {
        self.read_ticks()
    }

    /// Stop the per-CPU local timer.
    ///
    /// Called by the SMP `ipi_halt_handler` before halting a CPU
    /// (smp.rs:708). Disables the LAPIC Timer (x86-64), Generic
    /// Timer (ARM64), or CLINT timer (RISC-V) to prevent interrupts
    /// during the halt.
    ///
    /// `cpu_id` has the same role as in `init_timer`: RISC-V needs it to
    /// locate this hart's `mtimecmp`; the other architectures ignore it.
    ///
    /// C: `stop_local_timer()` — not a named C function; inline in
    /// `smp_ipi_halt_handler()` (smp.c:56-61) as `lapic_stop_timer()`.
    fn stop_local_timer(&mut self, cpu_id: u32);

    /// Initialize and start the statistical profiling timer.
    ///
    /// Called by `SYS_SPROF` with `action=PROF_START` and
    /// `intr_type=PROF_RTC` (misc.rs:1854-1857). Configures a separate
    /// timer (RTC on x86-64, or a second generic timer channel) to
    /// generate periodic interrupts at `hz` Hz for statistical
    /// profiling.
    ///
    /// Returns `Err(ProfileClockError::Unsupported)` if the architecture
    /// does not support profiling timers (e.g., RISC-V without a spare
    /// CLINT channel).
    ///
    /// C: `init_profile_clock(freq)` — sprofile.c:init_profile_clock
    fn init_profile_clock(&mut self, hz: u32) -> Result<(), ProfileClockError>;

    /// Stop the statistical profiling timer.
    ///
    /// Called by `SYS_SPROF` with `action=PROF_STOP` (misc.rs:1831-1836).
    /// Disables the profiling timer interrupt and returns the hardware
    /// to normal operation.
    ///
    /// C: `stop_profile_clock()` — sprofile.c:stop_profile_clock
    fn stop_profile_clock(&mut self);

    /// Acknowledge a statistical profiling timer interrupt.
    ///
    /// Called by `profile_clock_handler` after collecting a sample, to
    /// clear the pending interrupt on the hardware so the next tick can
    /// fire. On x86-64 this reads RTC Register C (which clears the IRQ).
    ///
    /// C: `arch_ack_profile_clock()` — profile.c:123
    fn ack_profile_clock(&mut self);
}

// ── Mock ClockArch (for tests) ──

/// Mock clock implementation — all operations are no-ops.
///
/// Used when `feature = "mock"` is enabled (test mode). Timer operations
/// do nothing; `read_ticks` returns 0.
#[cfg(feature = "mock")]
pub struct MockClockArch;

#[cfg(feature = "mock")]
impl ClockArch for MockClockArch {
    fn new(_desc: &dyn minix_platform::TimerDesc) -> Self {
        Self
    }
    fn init_timer(&mut self, _hz: u32, _cpu_id: u32) {}
    fn read_ticks(&self) -> u64 { 0 }
    fn stop_local_timer(&mut self, _cpu_id: u32) {}
    fn init_profile_clock(&mut self, _hz: u32) -> Result<(), ProfileClockError> { Ok(()) }
    fn stop_profile_clock(&mut self) {}
    fn ack_profile_clock(&mut self) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_hz_value() {
        assert_eq!(DEFAULT_HZ, 100);
    }

    #[test]
    fn test_load_history_size() {
        assert_eq!(LOAD_HISTORY_SIZE, 150);
    }

    /// Verify that `read_tsc()` default implementation delegates to `read_ticks()`.
    #[test]
    fn test_read_tsc_default_delegates_to_read_ticks() {
        use core::any::Any;
        use minix_platform::TimerDesc;

        // Local test TimerDesc implementor — avoids depending on any
        // arch-specific submodule (which may be cfg-gated out).
        #[derive(Debug)]
        struct TestTimerDesc;
        impl TimerDesc for TestTimerDesc {
            fn frequency(&self) -> u64 { 0 }
            fn as_any(&self) -> &dyn Any { self }
        }

        struct TestClock {
            ticks: u64,
        }
        impl ClockArch for TestClock {
            fn new(_desc: &dyn TimerDesc) -> Self {
                Self { ticks: 42 }
            }
            fn init_timer(&mut self, _hz: u32, _cpu_id: u32) {}
            fn read_ticks(&self) -> u64 { self.ticks }
            fn stop_local_timer(&mut self, _cpu_id: u32) {}
            fn init_profile_clock(&mut self, _hz: u32) -> Result<(), ProfileClockError> { Ok(()) }
            fn stop_profile_clock(&mut self) {}
            fn ack_profile_clock(&mut self) {}
        }
        let desc = TestTimerDesc;
        let clock = TestClock::new(&desc);
        assert_eq!(clock.read_tsc(), 42);
    }
}
