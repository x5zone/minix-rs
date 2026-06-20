//! Clock architecture abstraction
//!
//! Defines the trait interface for hardware timer configuration and
//! architecture-independent clock state management.
//!
//! # Design decisions (see 04-clock-interrupt-init.md §3.1, §3.2)
//!
//! - **ClockArch trait** (§3.1): Separates hardware timer configuration
//!   from software clock state. Each architecture implements its own
//!   timer source (8254 PIT / ARM Generic Timer / RISC-V mtime).
//! - **ClockState** (§3.1): Architecture-independent software state
//!   (tick frequency, uptime, load average). No hardware dependencies.
//! - **DEFAULT_HZ compile-time constant** (§3.2): Replaces C's
//!   `env_get("hz")` runtime configuration.

/// Default clock tick frequency in Hz.
///
/// minix-rs uses 100 Hz (10ms tick) across all architectures.
///
/// # Architecture evolution (32-bit → 64-bit)
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
pub const DEFAULT_HZ: u32 = 100;

/// Number of load history slots for load average calculation.
///
/// C: _LOAD_HISTORY — include/minix/type.h:97
pub const LOAD_HISTORY_SIZE: usize = 16;

/// Architecture-independent clock state.
///
/// Manages tick frequency, uptime counter, realtime tracking, and load
/// average. Hardware timer configuration is delegated to `ClockArch`.
///
/// C: kclockinfo + kloadinfo + clock_timers — clock.c:33-44
pub struct ClockState {
    /// Clock tick frequency in Hz.
    /// C: kclockinfo.hz — type.h:119
    hz: u32,

    /// System uptime in ticks since boot.
    /// C: kclockinfo.uptime — type.h:107
    uptime: u64,

    /// Real time in ticks since boot (may differ from uptime due to adjtime).
    /// C: kclockinfo.realtime — type.h:109
    realtime: u64,

    /// Boot time in seconds since UNIX epoch.
    /// C: kclockinfo.boottime — type.h:105
    boottime: u64,

    /// Number of ticks to adjust realtime by (positive = speed up, negative = slow down).
    /// C: adjtime_delta — clock.c:44
    adjtime_delta: i32,

    /// Load average tracking data.
    /// C: kloadinfo (struct loadinfo) — type.h:98
    loadinfo: LoadInfo,
}

/// Load average tracking data.
///
/// Tracks the number of runnable processes over time to compute
/// 1/5/15 minute load averages.
///
/// C: struct loadinfo — include/minix/type.h:98
struct LoadInfo {
    /// History of process counts per sample slot.
    /// C: proc_load_history[_LOAD_HISTORY] — type.h:99
    proc_load_history: [u16; LOAD_HISTORY_SIZE],

    /// Last slot written in proc_load_history.
    /// C: proc_last_slot — type.h:100
    proc_last_slot: u16,

    /// Uptime at last load sample.
    /// C: last_clock — type.h:101
    last_clock: u64,
}

impl Default for LoadInfo {
    fn default() -> Self {
        Self {
            proc_load_history: [0; LOAD_HISTORY_SIZE],
            proc_last_slot: 0,
            last_clock: 0,
        }
    }
}

impl Default for ClockState {
    fn default() -> Self {
        Self::new()
    }
}

impl ClockState {
    /// Create a new clock state with default frequency.
    ///
    /// C: init_clock() — clock.c:48
    pub fn new() -> Self {
        Self {
            hz: DEFAULT_HZ,
            uptime: 0,
            realtime: 0,
            boottime: 0,
            adjtime_delta: 0,
            loadinfo: LoadInfo::default(),
        }
    }

    /// Get the clock tick frequency.
    pub fn hz(&self) -> u32 {
        self.hz
    }

    /// Get the system uptime in ticks.
    pub fn uptime(&self) -> u64 {
        self.uptime
    }

    /// Get the real time in ticks since boot.
    /// C: kclockinfo.realtime — type.h:109
    pub fn realtime(&self) -> u64 {
        self.realtime
    }

    /// Called on each clock tick interrupt.
    ///
    /// Updates uptime, realtime, and load average tracking.
    /// C: timer_int_handler() — clock.c:70
    pub fn tick(&mut self) {
        self.uptime += 1;

        // Update realtime with adjtime_delta adjustment.
        // C: clock.c:92-103
        if self.adjtime_delta != 0 && self.uptime & 0x1 != 0 {
            self.realtime += if self.adjtime_delta > 0 { 2 } else { 0 };
            self.adjtime_delta += if self.adjtime_delta > 0 { -1 } else { 1 };
        } else {
            self.realtime += 1;
        }

        // Load average update and timer queue expiry are runtime tick-handler
        // concerns, not initialization. See the clock/timer subsystem doc.
        // C: load_update() — clock.c:260-291
        // C: tmrs_exptimers(&clock_timers) — clock.c:160-161
    }
}

/// Architecture abstraction for hardware timer configuration.
///
/// Each architecture implements this trait to configure its hardware
/// timer source and provide tick-reading capability.
///
/// # Instance-based design (see `plat-design.md` §5.1)
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
/// | `new()` | store PIT freq + LAPIC base from `TimerDesc::Pit` | no-op (CNTFRQ read at runtime) | store CLINT addrs from `TimerDesc::Clint` |
/// | `init_timer()` | 8254 PIT divisor / LAPIC Timer | ARM Generic Timer (CNTFRQ/CNTPCT) | CLINT mtimecmp |
/// | `read_ticks()` | TSC (rdtsc) | CNTPCT_EL0 | mtime (MMIO) |
/// | `read_tsc()` | TSC (rdtsc) | CNTPCT_EL0 | mtime (MMIO) |
///
/// C: init_clock() hardware portion + arch_init() APIC timer
pub trait ClockArch: Sized + Send + Sync {
    /// Create an instance from a timer descriptor.
    ///
    /// Stores the hardware parameters (base address, frequency) from the
    /// descriptor into instance fields. Called once during
    /// `init_clock_and_interrupts()` after `PlatformContext` is initialized.
    ///
    /// # Panics
    ///
    /// May panic if `desc` does not match the architecture's expected
    /// `TimerDesc` variant (e.g. x86-64 receives `TimerDesc::Clint`).
    /// Upper layers guarantee the correct variant is passed.
    fn new(desc: &minix_platform::TimerDesc) -> Self;

    /// Configure and start the hardware timer at the given frequency.
    ///
    /// Called once during `init_clock_and_interrupts()`. After this call, the timer
    /// generates periodic interrupts at `hz` Hz.
    ///
    /// C: init_clock() hardware portion + arch_init() APIC timer
    fn init_timer(&mut self, hz: u32);

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
    fn read_tsc(&self) -> u64 {
        self.read_ticks()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_clock_state_new() {
        let cs = ClockState::new();
        assert_eq!(cs.hz(), DEFAULT_HZ);
        assert_eq!(cs.uptime(), 0);
        assert_eq!(cs.realtime(), 0);
    }

    #[test]
    fn test_clock_state_default() {
        let cs = ClockState::default();
        assert_eq!(cs.hz(), DEFAULT_HZ);
        assert_eq!(cs.uptime(), 0);
        assert_eq!(cs.realtime(), 0);
    }

    #[test]
    fn test_clock_state_tick_increment_uptime() {
        let mut cs = ClockState::new();
        for _ in 0..100 {
            cs.tick();
        }
        assert_eq!(cs.uptime(), 100);
    }

    #[test]
    fn test_clock_state_tick_realtime_no_adjtime() {
        let mut cs = ClockState::new();
        for _ in 0..10 {
            cs.tick();
        }
        // Without adjtime_delta, realtime == uptime
        assert_eq!(cs.realtime(), cs.uptime());
    }

    #[test]
    fn test_clock_state_tick_realtime_with_positive_adjtime() {
        let mut cs = ClockState::new();
        cs.adjtime_delta = 10;

        // Positive adjtime: every odd tick adds 2 to realtime (not 1),
        // and adjtime_delta decreases by 1 toward 0.
        // After 20 ticks: adjtime_delta goes 10→0
        // Odd ticks: realtime += 2 (extra +1 each), even ticks: realtime += 1
        // Total: 20 + 10 = 30
        for _ in 0..20 {
            cs.tick();
        }
        assert_eq!(cs.realtime(), 30);
        assert_eq!(cs.adjtime_delta, 0);
    }

    #[test]
    fn test_clock_state_tick_realtime_with_negative_adjtime() {
        let mut cs = ClockState::new();
        cs.adjtime_delta = -10;

        // Negative adjtime: every odd tick, realtime += 0 (not 1),
        // and adjtime_delta increases by 1 toward 0.
        // After 20 ticks: adjtime_delta goes -10→0
        // Odd ticks: realtime += 0 (skip), even ticks: realtime += 1
        // Total: 10
        for _ in 0..20 {
            cs.tick();
        }
        assert_eq!(cs.realtime(), 10);
        assert_eq!(cs.adjtime_delta, 0);
    }

    #[test]
    fn test_clock_state_tick_realtime_adjtime_stops_when_zero() {
        let mut cs = ClockState::new();
        cs.adjtime_delta = 2;

        // Tick 1 (odd): adjtime_delta > 0 → realtime += 2, adjtime_delta = 1
        // Tick 2 (even): adjtime_delta != 0 but uptime & 0x1 == 0 → realtime += 1
        // Tick 3 (odd): adjtime_delta > 0 → realtime += 2, adjtime_delta = 0
        // Tick 4 (even): adjtime_delta == 0 → realtime += 1
        // Tick 5 (odd): adjtime_delta == 0 → realtime += 1
        cs.tick(); // tick 1
        assert_eq!(cs.uptime, 1);
        assert_eq!(cs.realtime, 2);
        assert_eq!(cs.adjtime_delta, 1);

        cs.tick(); // tick 2
        assert_eq!(cs.uptime, 2);
        assert_eq!(cs.realtime, 3);
        assert_eq!(cs.adjtime_delta, 1);

        cs.tick(); // tick 3
        assert_eq!(cs.uptime, 3);
        assert_eq!(cs.realtime, 5);
        assert_eq!(cs.adjtime_delta, 0);

        cs.tick(); // tick 4
        assert_eq!(cs.uptime, 4);
        assert_eq!(cs.realtime, 6);
        assert_eq!(cs.adjtime_delta, 0);

        cs.tick(); // tick 5
        assert_eq!(cs.uptime, 5);
        assert_eq!(cs.realtime, 7);
        assert_eq!(cs.adjtime_delta, 0);
    }

    #[test]
    fn test_clock_state_large_uptime_no_overflow() {
        let mut cs = ClockState::new();
        // Simulate ~1M ticks (~2.8 hours at 100 Hz)
        for _ in 0..1_000_000 {
            cs.tick();
        }
        assert_eq!(cs.uptime(), 1_000_000);
        assert_eq!(cs.realtime(), 1_000_000);
    }

    #[test]
    fn test_load_info_default() {
        let li = LoadInfo::default();
        assert_eq!(li.proc_load_history.len(), LOAD_HISTORY_SIZE);
        assert_eq!(li.proc_last_slot, 0);
        assert_eq!(li.last_clock, 0);
        for &val in li.proc_load_history.iter() {
            assert_eq!(val, 0);
        }
    }

    #[test]
    fn test_default_hz_value() {
        assert_eq!(DEFAULT_HZ, 100);
    }

    #[test]
    fn test_load_history_size() {
        assert_eq!(LOAD_HISTORY_SIZE, 16);
    }

    /// Verify that `read_tsc()` default implementation delegates to `read_ticks()`.
    #[test]
    fn test_read_tsc_default_delegates_to_read_ticks() {
        struct TestClock {
            ticks: u64,
        }
        impl ClockArch for TestClock {
            fn new(_desc: &minix_platform::TimerDesc) -> Self {
                Self { ticks: 42 }
            }
            fn init_timer(&mut self, _hz: u32) {}
            fn read_ticks(&self) -> u64 { self.ticks }
        }
        let desc = minix_platform::TimerDesc::ArmGenericTimer;
        let clock = TestClock::new(&desc);
        assert_eq!(clock.read_tsc(), 42);
    }
}
