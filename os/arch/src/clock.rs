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

/// Architecture-independent clock state.
///
/// Manages tick frequency, uptime counter, and load average tracking.
/// Hardware timer configuration is delegated to `ClockArch`.
///
/// C: kclockinfo + kloadinfo + clock_timers — clock.c:33-40
pub struct ClockState {
    /// Clock tick frequency in Hz.
    hz: u32,

    /// System uptime in ticks since boot.
    uptime: u64,

    /// Real-time offset for time adjustment (in ticks).
    realtime_offset: i64,
}

impl ClockState {
    /// Create a new clock state with default frequency.
    ///
    /// C: init_clock() — clock.c:48
    pub fn new() -> Self {
        Self {
            hz: DEFAULT_HZ,
            uptime: 0,
            realtime_offset: 0,
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

    /// Called on each clock tick interrupt.
    ///
    /// Updates uptime and load average tracking.
    /// C: timer_int_handler() — clock.c:76
    pub fn tick(&mut self) {
        self.uptime += 1;
        // TODO: update load average (kloadinfo)
        // TODO: check timer queue (clock_timers)
    }
}

/// Architecture abstraction for hardware timer configuration.
///
/// Each architecture implements this trait to configure its hardware
/// timer source and provide tick-reading capability.
///
/// # Architecture mapping
///
/// | Method | x86-64 | ARM64 | RISC-V |
/// |--------|--------|-------|--------|
/// | `init_timer()` | 8254 PIT divisor / LAPIC Timer | ARM Generic Timer (CNTFRQ/CNTPCT) | CLINT mtimecmp |
/// | `read_ticks()` | TSC (rdtsc) | CNTPCT_EL0 | mtime (MMIO) |
///
/// C: init_clock() hardware portion + arch_init() APIC timer
pub trait ClockArch {
    /// Configure and start the hardware timer at the given frequency.
    ///
    /// Called once during `init_clock_and_interrupts()`. After this call, the timer
    /// generates periodic interrupts at `hz` Hz.
    ///
    /// C: init_clock() hardware portion + arch_init() APIC timer
    fn init_timer(hz: u32);

    /// Read the current hardware tick count.
    ///
    /// Used for fine-grained timing and profiling.
    fn read_ticks() -> u64;
}
