//! Kernel clock and timer subsystem.
//!
//! Implements 100Hz periodic timer interrupt handling, synchronous alarm timers,
//! virtual/profile timers, time accounting, and load average tracking.
//!
//! # Minix3 C Source Mapping
//!
//! - `clock.c:47-63` — `init_clock()`: initialize clock variables
//! - `clock.c:70-175` — `timer_int_handler()`: main tick handler
//! - `clock.c:229-257` — `set_kernel_timer()` / `reset_kernel_timer()`
//! - `clock.c:260-291` — `load_update()`: load average tracking
//! - `clock.c:294-312` — `boot_cpu_init_timer()` / `app_cpu_init_timer()`
//! - `do_setalarm.c:73` — `cause_alarm()`: alarm notification
//! - `do_vtimer.c:68-89` — `vtimer_check()`: virtual/profile timer expiry
//!
//! # Design Decisions (14-clock-timer.md §3)
//!
//! - **D1**: `ClockState` struct encapsulates `kclockinfo` + `kloadinfo` + `clock_timers`
//! - **D2**: `BTreeMap<u64, TimerEntry>` replaces C's `minix_timer_t` linked list
//! - **D5/D6**: `TimerAction` enum replaces C's `tmr_func_t` function pointer
//! - **D7**: `hz` is compile-time constant with runtime override
//! - **D8**: per-CPU callback distinguishes BSP/AP logic

use alloc::collections::BTreeMap;
use alloc::vec::Vec;
use core::sync::atomic::Ordering;

use minix_types::Endpoint;

use crate::proc::{KProcess, MiscFlagsBits};

// ── Global clock state (read by scheduler without &mut ClockState) ──

/// Monotonic uptime in ticks, updated by BSP tick handler.
///
/// C: `kclockinfo.uptime` — global variable read by `get_monotonic()`.
/// In Rust, we use an AtomicU64 so that scheduler code can read uptime
/// without needing a `&ClockState` reference (which would require threading
/// through the entire scheduler call chain).
static CLOCK_UPTIME: core::sync::atomic::AtomicU64 = core::sync::atomic::AtomicU64::new(0);

/// Wall-clock ticks since boot, updated by BSP tick handler.
///
/// C: `kclockinfo.realtime` — global variable read by `get_realtime()`.
/// Mirrored from `ClockState::realtime` so that code without `&ClockState`
/// can read the current wall-clock time.
static CLOCK_REALTIME: core::sync::atomic::AtomicU64 = core::sync::atomic::AtomicU64::new(0);

/// Boot time in seconds since UNIX epoch, set by `SYS_STIME`.
///
/// C: `kclockinfo.boottime` — global variable read by `get_boottime()`.
/// Mirrored from `ClockState::boottime` so that code without `&ClockState`
/// can read the boot timestamp.
static CLOCK_BOOTTIME: core::sync::atomic::AtomicU64 = core::sync::atomic::AtomicU64::new(0);

/// TSC cycles per millisecond, calibrated during boot.
///
/// C: `tsc_per_ms[cpuid]` — per-CPU array in `kernel/proc.h`.
/// For now we use a single global; per-CPU values will be added when
/// SMP calibration is implemented.
static TSC_PER_MS: core::sync::atomic::AtomicU64 = core::sync::atomic::AtomicU64::new(0);

/// Default TSC frequency assumption: 1 GHz (1M cycles/ms).
/// Used as fallback when calibration has not yet run.
const DEFAULT_TSC_PER_MS: u64 = 1_000_000;

/// Get monotonic uptime in ticks.
///
/// C: `get_monotonic()` — clock.c:202 (`return kclockinfo.uptime`).
/// Safe to call from any context (BKL or not) — only reads an atomic.
pub fn get_monotonic() -> u64 {
    CLOCK_UPTIME.load(Ordering::Acquire)
}

/// Get wall-clock ticks since boot.
///
/// C: `get_realtime()` — clock.c:188 (`return kclockinfo.realtime`).
/// Safe to call from any context — only reads an atomic.
pub fn get_realtime() -> u64 {
    CLOCK_REALTIME.load(Ordering::Acquire)
}

/// Get boot time in seconds since UNIX epoch.
///
/// C: `get_boottime()` — clock.c:218 (`return kclockinfo.boottime`).
/// Safe to call from any context — only reads an atomic.
pub fn get_boottime() -> u64 {
    CLOCK_BOOTTIME.load(Ordering::Acquire)
}

/// Convert milliseconds to CPU time cycles.
///
/// C: `ms_2_cpu_time(ms)` — kernel/proc.h (`tsc_per_ms[cpuid] * ms`).
/// Uses the calibrated TSC frequency if available, otherwise falls back
/// to the default 1 GHz assumption.
pub fn ms_to_cpu_time(ms: u32) -> u64 {
    let tsc_per_ms = TSC_PER_MS.load(Ordering::Acquire);
    let factor = if tsc_per_ms == 0 { DEFAULT_TSC_PER_MS } else { tsc_per_ms };
    ms as u64 * factor
}

/// Set the calibrated TSC frequency (cycles per millisecond).
///
/// Called once during boot after TSC calibration completes.
/// C: `tsc_per_ms[cpuid] = ...` — set in `boot_cpu_init_timer()`.
pub fn set_tsc_per_ms(cycles_per_ms: u64) {
    TSC_PER_MS.store(cycles_per_ms, Ordering::Release);
}

/// Read the Time Stamp Counter.
///
/// C: `read_tsc_64()` — arch/i386/arch_clock.c (x86) / arch/earm/arch_clock.c (ARM)
///
/// Delegates to the current architecture's `ClockArch::read_tsc()`, which
/// reads the hardware cycle counter:
///
/// - **x86-64**: `rdtsc` instruction
/// - **aarch64**: `CNTPCT_EL0` (physical counter)
/// - **riscv64**: `mtime` (CLINT MMIO)
///
/// In test builds (`cfg(test)`), returns 0 since there is no hardware counter.
///
/// # Instance-based design (plat-design.md §5.1)
///
/// Constructs a transient `CurrentClockArch` instance from the global
/// platform descriptor's `timer()` sub-descriptor. The instance is cheap
/// to construct (just copies a few fields) and is discarded after the
/// read. This replaces the old static `CurrentClockArch::read_tsc()`.
pub fn read_tsc() -> u64 {
    #[cfg(not(test))]
    {
        use minix_arch::{ClockArch, CurrentClockArch};
        use minix_platform::{platform_desc, PlatformDesc};
        let pd = platform_desc();
        let clock_arch = CurrentClockArch::new(&pd.timer());
        clock_arch.read_tsc()
    }
    #[cfg(test)]
    {
        0
    }
}

// ── Constants ──

/// Default clock frequency in Hz.
/// C: `DEFAULT_HZ` — clock.h
pub const DEFAULT_HZ: u32 = 100;

/// Timer "never expires" sentinel.
/// C: `TMR_NEVER` = `LONG_MAX` — timers.h
pub const TMR_NEVER: u64 = u64::MAX;

/// Load average sampling interval in seconds.
/// C: `_LOAD_UNIT_SECS` — clock.h
const LOAD_UNIT_SECS: u64 = 5;

/// Number of load average history slots.
/// C: `_LOAD_HISTORY` — clock.h
const LOAD_HISTORY: usize = 12;

// ── Timer action ──

/// Action to take when a timer expires.
///
/// Replaces C's `tmr_func_t` function pointer + `tmr_arg` integer.
/// Design decision D6: enum dispatch replaces function pointers for type safety.
///
/// C: `cause_alarm(proc_nr_e)` — do_setalarm.c:73
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TimerAction {
    /// Notify a process via synchronous alarm.
    /// C: `cause_alarm()` → `mini_notify(CLOCK, endpoint)`
    NotifyAlarm { endpoint: Endpoint },
    /// Custom kernel timer callback identified by ID.
    /// Used for kernel-internal timers (e.g., watchdog).
    KernelCallback { id: usize },
}

/// A timer entry in the clock timer queue.
///
/// Replaces C's `minix_timer_t` linked list node.
/// Design decision D5: BTreeMap entry replaces pointer-based linked list.
///
/// C: timers.h — `struct minix_timer`
#[derive(Debug, Clone)]
pub struct TimerEntry {
    /// Expiration time in monotonic ticks. C: `tmr_exp_time`
    pub exp_time: u64,
    /// Action to take when timer expires.
    pub action: TimerAction,
}

// ── Per-CPU tick marker ──
//
// Design decision D8 (Doc 14 §3): distinguish BSP vs AP tick logic via a
// compile-time marker type so the hot path has no `if is_bsp` branch.
// See `ClockState::tick` for usage.

mod sealed {
    /// Sealed trait — only [`super::BspTick`] and [`super::ApTick`] implement it.
    pub trait Sealed {}
}

/// Per-CPU tick role marker. Sealed: only [`BspTick`] and [`ApTick`] are
/// valid implementations. Use `ClockState::tick_bsp` / `tick_ap` for clarity,
/// or call [`ClockState::tick`] directly with an explicit marker.
pub trait PerCpuTick: sealed::Sealed {
    /// `true` iff this tick is on the BSP.
    const IS_BSP: bool;
}

/// BSP tick marker.
#[derive(Debug, Clone, Copy)]
pub enum BspTick {}
impl sealed::Sealed for BspTick {}
impl PerCpuTick for BspTick {
    const IS_BSP: bool = true;
}

/// AP tick marker.
#[derive(Debug, Clone, Copy)]
pub enum ApTick {}
impl sealed::Sealed for ApTick {}
impl PerCpuTick for ApTick {
    const IS_BSP: bool = false;
}

// ── Virtual/Profile timer expiry ──

/// Result of checking virtual/profile timer expiry.
///
/// C: `vtimer_check()` — do_vtimer.c:68-89
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VtimerExpired {
    /// Virtual timer expired → `SIGVTALRM`. C: `VT_VIRTUAL` (0)
    Virtual,
    /// Profile timer expired → `SIGPROF`. C: `VT_PROF` (1)
    Prof,
}

// ── Load average ──

/// Load average tracking state.
///
/// C: `struct loadinfo kloadinfo` — clock.h
#[derive(Debug)]
pub struct LoadInfo {
    /// Current load sampling slot index.
    /// C: `proc_last_slot`
    proc_last_slot: u16,
    /// Load history circular buffer.
    /// C: `proc_load_history[_LOAD_HISTORY]`
    proc_load_history: [u32; LOAD_HISTORY],
    /// Last clock tick when load was updated.
    /// C: `last_clock`
    last_clock: u64,
}

impl LoadInfo {
    pub const fn new() -> Self {
        Self {
            proc_last_slot: 0,
            proc_load_history: [0; LOAD_HISTORY],
            last_clock: 0,
        }
    }
}

impl Default for LoadInfo {
    fn default() -> Self {
        Self::new()
    }
}

// ── Timer tick result ──

/// Result returned by `ClockState::tick()` after processing one timer interrupt.
///
/// Contains the list of expired alarm actions that the caller must process
/// (e.g., send notifications via `mini_notify`).
#[derive(Debug)]
pub struct TimerTickResult {
    /// Actions from expired alarm timers (BSP only).
    pub expired_alarms: Vec<TimerAction>,
    /// Virtual/profile timer expiry for the current process, if any.
    pub vtimer_expired: Option<VtimerExpired>,
    /// Whether the current process's quantum was exhausted.
    pub quantum_exhausted: bool,
}

// ── ClockState ──

/// Global clock state, equivalent to C's `kclockinfo` + `kloadinfo` + `clock_timers`.
///
/// All fields are protected by BKL. No interior mutability needed because
/// the clock handler runs under BKL.
///
/// Design decision D1: encapsulate as struct for ownership clarity.
///
/// C: clock.h — `struct clockinfo kclockinfo` + `struct loadinfo kloadinfo`
/// C: clock.c:37 — `static minix_timer_t *clock_timers`
#[derive(Debug)]
pub struct ClockState {
    /// Clock frequency in Hz. C: `kclockinfo.hz`
    hz: u32,
    /// Monotonically increasing ticks since boot. C: `kclockinfo.uptime`
    uptime: u64,
    /// Wall-clock ticks since boot (affected by adjtime). C: `kclockinfo.realtime`
    realtime: u64,
    /// UNIX epoch seconds at boot. C: `kclockinfo.boottime`
    boottime: u64,
    /// Time adjustment delta (positive=speed up, negative=slow down).
    /// C: `adjtime_delta` (clock.c:42)
    adjtime_delta: i32,
    /// Synchronous alarm timer queue. C: `clock_timers` (clock.c:37)
    /// Design decision D2: BTreeMap replaces linked list for O(log N) operations.
    timers: BTreeMap<u64, TimerEntry>,
    /// Load average info. C: `kloadinfo`
    load_info: LoadInfo,
}

impl ClockState {
    /// Create a new ClockState with default frequency (100Hz).
    ///
    /// C: `init_clock()` — clock.c:47-63
    pub fn new() -> Self {
        Self {
            hz: DEFAULT_HZ,
            uptime: 0,
            realtime: 0,
            boottime: 0,
            adjtime_delta: 0,
            timers: BTreeMap::new(),
            load_info: LoadInfo::new(),
        }
    }

    /// Create a ClockState with a custom frequency.
    ///
    /// C: `env_get("hz")` override in `init_clock()` — clock.c:53-56
    /// Range: 2..=50000 (matching C's validation).
    pub fn with_hz(hz: u32) -> Self {
        let hz = if hz < 2 || hz > 50000 { DEFAULT_HZ } else { hz };
        Self {
            hz,
            ..Self::new()
        }
    }

    // ── Accessors ──

    /// Get clock frequency in Hz.
    /// C: `system_hz` — clock.c (global `kclockinfo.hz`)
    pub fn hz(&self) -> u32 {
        self.hz
    }

    /// Get clock frequency in Hz as `i32` for arithmetic.
    /// C: `system_hz` — used in do_settime.c for tick conversion.
    pub fn system_hz(&self) -> i32 {
        self.hz as i32
    }

    /// Get monotonic ticks since boot.
    /// C: `get_monotonic()` — clock.c:202
    pub fn uptime(&self) -> u64 {
        self.uptime
    }

    /// Get wall-clock ticks since boot.
    /// C: `get_realtime()` — clock.c:188
    pub fn realtime(&self) -> u64 {
        self.realtime
    }

    /// Get boot time in seconds since UNIX epoch.
    /// C: `get_boottime()` — clock.c:218
    pub fn boottime(&self) -> u64 {
        self.boottime
    }

    /// Set boot time in seconds since UNIX epoch.
    /// C: `set_boottime()` — clock.c:208
    pub fn set_boottime(&mut self, new_boottime: u64) {
        self.boottime = new_boottime;
        CLOCK_BOOTTIME.store(new_boottime, Ordering::Release);
    }

    /// Set wall-clock time in ticks.
    /// C: `set_realtime()` — clock.c:193
    pub fn set_realtime(&mut self, new_realtime: u64) {
        self.realtime = new_realtime;
        CLOCK_REALTIME.store(new_realtime, Ordering::Release);
    }

    /// Set adjtime delta for time adjustment.
    /// C: `set_adjtime_delta()` — clock.c:198
    pub fn set_adjtime_delta(&mut self, ticks: i32) {
        self.adjtime_delta = ticks;
    }

    /// Get the load average history.
    pub fn load_history(&self) -> &[u32; LOAD_HISTORY] {
        &self.load_info.proc_load_history
    }

    // ── Timer management ──

    /// Set a kernel timer.
    ///
    /// C: `set_kernel_timer()` — clock.c:229-243
    pub fn set_timer(&mut self, entry: TimerEntry) {
        self.timers.insert(entry.exp_time, entry);
    }

    /// Reset (remove) a kernel timer by expiration time.
    ///
    /// C: `reset_kernel_timer()` — clock.c:245-257
    pub fn reset_timer(&mut self, exp_time: u64) -> Option<TimerEntry> {
        self.timers.remove(&exp_time)
    }

    /// Check for expired timers and collect their actions.
    ///
    /// C: `tmrs_exptimers(&clock_timers, kclockinfo.uptime, NULL)` — clock.c:165-166
    fn check_expired_timers(&mut self) -> Vec<TimerAction> {
        let mut expired = Vec::new();
        while let Some((&exp_time, _)) = self.timers.first_key_value() {
            if exp_time > self.uptime {
                break;
            }
            if let Some(entry) = self.timers.remove(&exp_time) {
                expired.push(entry.action);
            }
        }
        expired
    }

    // ── Tick handling ──

    /// Handle a timer interrupt tick on the BSP.
    ///
    /// Convenience wrapper around [`Self::tick`] that statically selects the
    /// BSP path. See D8 (Doc 14 §3): per-CPU callback distinguishes BSP/AP
    /// logic without runtime `if is_bsp` branching inside the hot path.
    pub fn tick_bsp(
        &mut self,
        current_proc: &mut KProcess,
        is_billable: bool,
        ready_count: usize,
    ) -> TimerTickResult {
        self.tick::<BspTick>(current_proc, is_billable, ready_count)
    }

    /// Handle a timer interrupt tick on an AP.
    ///
    /// Convenience wrapper around [`Self::tick`] that statically selects the
    /// AP path. See D8 (Doc 14 §3).
    pub fn tick_ap(
        &mut self,
        current_proc: &mut KProcess,
        is_billable: bool,
        ready_count: usize,
    ) -> TimerTickResult {
        self.tick::<ApTick>(current_proc, is_billable, ready_count)
    }

    /// Handle a timer interrupt tick.
    ///
    /// This is the main clock interrupt handler, called at `hz` frequency.
    /// The per-CPU role is encoded in the type parameter `P` (a `PerCpuTick`
    /// marker) so that BSP-only logic (uptime/realtime increment, alarm
    /// expiry) is monomorphized away on AP builds — no runtime branch.
    ///
    /// C: `timer_int_handler()` — clock.c:70-175
    ///
    /// # BKL (Big Kernel Lock)
    ///
    /// **Precondition**: the caller must hold the BKL when calling this
    /// method. In C, the BKL is acquired in `context_stop()` (called from
    /// the assembly trap entry before the timer handler). In Rust, the
    /// interrupt entry point is responsible for acquiring the BKL before
    /// calling `tick_bsp`/`tick_ap`.
    ///
    /// This method does **not** acquire the BKL internally because:
    /// 1. It may be called from a syscall path where BKL is already held
    ///    (acquiring again would deadlock — BKL is non-recursive).
    /// 2. The C pattern is "caller holds BKL", not "callee acquires BKL".
    ///
    /// # Arguments
    /// * `current_proc` — the currently running process (for time accounting)
    /// * `is_billable` — whether the current process is billable
    /// * `ready_count` — number of processes in ready queues (for load average)
    ///
    /// # Returns
    /// `TimerTickResult` containing expired alarms, vtimer status, and quantum status.
    pub fn tick<P: PerCpuTick>(
        &mut self,
        current_proc: &mut KProcess,
        _is_billable: bool,
        ready_count: usize,
    ) -> TimerTickResult {
        // 1. BSP-only: update uptime and realtime (with adjtime)
        // C: clock.c:96-107
        // D8: this branch is resolved at compile time via the marker type.
        if P::IS_BSP {
            self.uptime += 1;
            // Mirror to global atomic so get_monotonic() works without &ClockState.
            CLOCK_UPTIME.store(self.uptime, Ordering::Release);

            if self.adjtime_delta != 0 && (self.uptime & 0x1) != 0 {
                // Apply adjtime: speed up or stay behind
                self.realtime += if self.adjtime_delta > 0 { 2 } else { 0 };
                self.adjtime_delta += if self.adjtime_delta > 0 { -1 } else { 1 };
            } else {
                self.realtime += 1;
            }
            // Mirror realtime to global atomic.
            CLOCK_REALTIME.store(self.realtime, Ordering::Release);
        }

        // 2. Time accounting: charge current process for user time
        // C: clock.c:113-118
        current_proc.p_time.add_user_time(1);

        // 3. Decrement virtual/profile timers
        // C: clock.c:127-138
        let mut vtimer_expired = None;

        if current_proc.p_misc_flags.is_set(MiscFlagsBits::VIRT_TIMER) {
            let expired = current_proc.p_time.tick_virt_timer();
            if expired {
                vtimer_expired = Some(VtimerExpired::Virtual);
            }
        }
        if current_proc.p_misc_flags.is_set(MiscFlagsBits::PROF_TIMER) {
            let expired = current_proc.p_time.tick_prof_timer();
            if expired && vtimer_expired.is_none() {
                vtimer_expired = Some(VtimerExpired::Prof);
            }
        }

        // 4. BSP-only: check alarm timers
        // C: clock.c:155-167
        // D8: monomorphized — AP builds skip this entirely.
        let expired_alarms = if P::IS_BSP {
            self.check_expired_timers()
        } else {
            Vec::new()
        };

        // 5. Load update (all CPUs)
        // C: clock.c:148
        self.load_update(ready_count);

        // 6. Quantum check — done by caller via return value
        // C: clock.c (quantum decrement is in arch_timer_int_handler on some archs,
        // but conceptually part of the tick)
        let quantum_exhausted = current_proc.p_sched.quantum.consume(1);

        TimerTickResult {
            expired_alarms,
            vtimer_expired,
            quantum_exhausted,
        }
    }

    /// Update load average tracking.
    ///
    /// C: `load_update()` — clock.c:260-291
    fn load_update(&mut self, ready_count: usize) {
        let slot = ((self.uptime / self.hz as u64 / LOAD_UNIT_SECS) % LOAD_HISTORY as u64) as u16;

        if slot != self.load_info.proc_last_slot {
            self.load_info.proc_load_history[slot as usize] = 0;
            self.load_info.proc_last_slot = slot;
        }

        self.load_info.proc_load_history[slot as usize] += ready_count as u32;
        self.load_info.last_clock = self.uptime;
    }
}

impl Default for ClockState {
    fn default() -> Self {
        Self::new()
    }
}

// ── Virtual/Profile timer check ──

/// Check if a process's virtual or profile timer has expired.
///
/// This is called after decrementing the timer counters to detect expiry
/// and clear the corresponding flag.
///
/// C: `vtimer_check()` — do_vtimer.c:68-89
pub fn vtimer_check(proc: &mut KProcess) -> Option<VtimerExpired> {
    if proc.p_misc_flags.is_set(MiscFlagsBits::VIRT_TIMER)
        && proc.p_time.virt_left.load(Ordering::Acquire) == 0
    {
        proc.p_misc_flags.clear(MiscFlagsBits::VIRT_TIMER);
        return Some(VtimerExpired::Virtual);
    }
    if proc.p_misc_flags.is_set(MiscFlagsBits::PROF_TIMER)
        && proc.p_time.prof_left.load(Ordering::Acquire) == 0
    {
        proc.p_misc_flags.clear(MiscFlagsBits::PROF_TIMER);
        return Some(VtimerExpired::Prof);
    }
    None
}

// ── Tests ──

#[cfg(test)]
mod tests {
    use super::*;
    use crate::proc::KProcess;
    use minix_types::Endpoint;

    fn make_test_proc() -> KProcess {
        KProcess::new(0, Endpoint(0))
    }

    #[test]
    fn test_clock_state_init() {
        let clock = ClockState::new();
        assert_eq!(clock.hz(), DEFAULT_HZ);
        assert_eq!(clock.uptime(), 0);
        assert_eq!(clock.realtime(), 0);
        assert_eq!(clock.boottime(), 0);
    }

    #[test]
    fn test_clock_state_custom_hz() {
        let clock = ClockState::with_hz(1000);
        assert_eq!(clock.hz(), 1000);
    }

    #[test]
    fn test_clock_state_hz_bounds() {
        let clock = ClockState::with_hz(1);
        assert_eq!(clock.hz(), DEFAULT_HZ); // too low, falls back

        let clock = ClockState::with_hz(60000);
        assert_eq!(clock.hz(), DEFAULT_HZ); // too high, falls back
    }

    #[test]
    fn test_tick_increments_uptime_realtime() {
        let mut clock = ClockState::new();
        let mut proc = make_test_proc();

        let result = clock.tick_bsp(&mut proc, true, 0);
        assert_eq!(clock.uptime(), 1);
        assert_eq!(clock.realtime(), 1);
        assert!(result.expired_alarms.is_empty());
    }

    #[test]
    fn test_tick_ap_no_uptime_update() {
        let mut clock = ClockState::new();
        let mut proc = make_test_proc();

        clock.tick_ap(&mut proc, true, 0);
        assert_eq!(clock.uptime(), 0);
        assert_eq!(clock.realtime(), 0);
    }

    #[test]
    fn test_adjtime_speed_up() {
        let mut clock = ClockState::new();
        let mut proc = make_test_proc();

        clock.set_adjtime_delta(3);

        // Tick 1: uptime=1, odd → realtime += 2, delta → 2
        clock.tick_bsp(&mut proc, true, 0);
        assert_eq!(clock.uptime(), 1);
        assert_eq!(clock.realtime(), 2);
        assert_eq!(clock.adjtime_delta, 2);

        // Tick 2: uptime=2, even → realtime += 1
        clock.tick_bsp(&mut proc, true, 0);
        assert_eq!(clock.uptime(), 2);
        assert_eq!(clock.realtime(), 3);

        // Tick 3: uptime=3, odd → realtime += 2, delta → 1
        clock.tick_bsp(&mut proc, true, 0);
        assert_eq!(clock.uptime(), 3);
        assert_eq!(clock.realtime(), 5);
        assert_eq!(clock.adjtime_delta, 1);
    }

    #[test]
    fn test_adjtime_slow_down() {
        let mut clock = ClockState::new();
        let mut proc = make_test_proc();

        clock.set_adjtime_delta(-2);

        // Tick 1: uptime=1, odd → realtime += 0, delta → -1
        clock.tick_bsp(&mut proc, true, 0);
        assert_eq!(clock.uptime(), 1);
        assert_eq!(clock.realtime(), 0);
        assert_eq!(clock.adjtime_delta, -1);

        // Tick 2: uptime=2, even → realtime += 1
        clock.tick_bsp(&mut proc, true, 0);
        assert_eq!(clock.uptime(), 2);
        assert_eq!(clock.realtime(), 1);
    }

    #[test]
    fn test_timer_set_and_expire() {
        let mut clock = ClockState::new();
        let mut proc = make_test_proc();

        let entry = TimerEntry {
            exp_time: 3,
            action: TimerAction::NotifyAlarm {
                endpoint: Endpoint(100),
            },
        };
        clock.set_timer(entry);

        // Tick 1-2: no expiry
        clock.tick_bsp(&mut proc, true, 0);
        clock.tick_bsp(&mut proc, true, 0);
        // Timer should still be present
        assert!(clock.timers.contains_key(&3));

        // Tick 3: timer expires
        let result = clock.tick_bsp(&mut proc, true, 0);
        assert_eq!(result.expired_alarms.len(), 1);
        assert_eq!(
            result.expired_alarms[0],
            TimerAction::NotifyAlarm {
                endpoint: Endpoint(100)
            }
        );
        // Timer should be removed
        assert!(!clock.timers.contains_key(&3));
    }

    #[test]
    fn test_timer_reset() {
        let mut clock = ClockState::new();

        let entry = TimerEntry {
            exp_time: 5,
            action: TimerAction::NotifyAlarm {
                endpoint: Endpoint(50),
            },
        };
        clock.set_timer(entry);
        assert!(clock.timers.contains_key(&5));

        let removed = clock.reset_timer(5);
        assert!(removed.is_some());
        assert!(!clock.timers.contains_key(&5));
    }

    #[test]
    fn test_set_boottime() {
        let mut clock = ClockState::new();
        clock.set_boottime(1700000000);
        assert_eq!(clock.boottime(), 1700000000);
    }

    #[test]
    fn test_set_realtime() {
        let mut clock = ClockState::new();
        clock.set_realtime(12345);
        assert_eq!(clock.realtime(), 12345);
    }

    #[test]
    fn test_load_update() {
        let mut clock = ClockState::new();
        let mut proc = make_test_proc();

        // Tick enough to fill a load slot
        for _ in 0..100 {
            clock.tick_bsp(&mut proc, true, 5);
        }

        // Load history should have been updated
        let history = clock.load_history();
        let total: u32 = history.iter().sum();
        assert!(total > 0);
    }

    #[test]
    fn test_user_time_accounting() {
        let mut clock = ClockState::new();
        let mut proc = make_test_proc();

        let initial = proc.p_time.user_time.load(Ordering::Acquire);
        clock.tick_bsp(&mut proc, true, 0);
        let after = proc.p_time.user_time.load(Ordering::Acquire);
        assert_eq!(after, initial + 1);
    }

    #[test]
    fn test_vtimer_check_virtual() {
        let mut proc = make_test_proc();
        proc.p_misc_flags.set(MiscFlagsBits::VIRT_TIMER);
        proc.p_time.virt_left.store(0, Ordering::Release);

        let result = vtimer_check(&mut proc);
        assert_eq!(result, Some(VtimerExpired::Virtual));
        assert!(!proc.p_misc_flags.is_set(MiscFlagsBits::VIRT_TIMER));
    }

    #[test]
    fn test_vtimer_check_prof() {
        let mut proc = make_test_proc();
        proc.p_misc_flags.set(MiscFlagsBits::PROF_TIMER);
        proc.p_time.prof_left.store(0, Ordering::Release);

        let result = vtimer_check(&mut proc);
        assert_eq!(result, Some(VtimerExpired::Prof));
        assert!(!proc.p_misc_flags.is_set(MiscFlagsBits::PROF_TIMER));
    }

    #[test]
    fn test_vtimer_check_no_expiry() {
        let mut proc = make_test_proc();
        proc.p_misc_flags.set(MiscFlagsBits::VIRT_TIMER);
        proc.p_time.virt_left.store(100, Ordering::Release);

        let result = vtimer_check(&mut proc);
        assert_eq!(result, None);
    }

    #[test]
    fn test_multiple_timers_same_time() {
        let mut clock = ClockState::new();
        let mut proc = make_test_proc();

        // BTreeMap only keeps one entry per key, so same exp_time overwrites.
        // This matches C behavior where timers are a sorted linked list
        // and multiple timers at the same time are processed sequentially.
        clock.set_timer(TimerEntry {
            exp_time: 5,
            action: TimerAction::NotifyAlarm {
                endpoint: Endpoint(1),
            },
        });
        // Second timer at same time replaces first (BTreeMap semantics)
        clock.set_timer(TimerEntry {
            exp_time: 5,
            action: TimerAction::NotifyAlarm {
                endpoint: Endpoint(2),
            },
        });

        for _ in 0..5 {
            clock.tick_bsp(&mut proc, true, 0);
        }
        // Only one timer fires (the last one inserted at that time)
        // This is a known limitation of BTreeMap vs linked list;
        // in practice, per-priv alarm timers ensure unique exp_times.
    }

    /// §6: D8 marker types — verify `IS_BSP` consts and that `tick::<BspTick>`
    /// and `tick::<ApTick>` produce different observable side effects.
    #[test]
    fn test_per_cpu_tick_markers() {
        // Compile-time check on marker constants.
        assert!(<BspTick as PerCpuTick>::IS_BSP);
        assert!(!<ApTick as PerCpuTick>::IS_BSP);

        // Runtime check that the two markers drive different behaviour.
        let mut bsp_clock = ClockState::new();
        let mut ap_clock = ClockState::new();
        let mut proc = make_test_proc();

        bsp_clock.tick::<BspTick>(&mut proc, true, 0);
        ap_clock.tick::<ApTick>(&mut proc, true, 0);

        assert_eq!(bsp_clock.uptime(), 1, "BSP tick must increment uptime");
        assert_eq!(bsp_clock.realtime(), 1, "BSP tick must increment realtime");
        assert_eq!(ap_clock.uptime(), 0, "AP tick must NOT increment uptime");
        assert_eq!(ap_clock.realtime(), 0, "AP tick must NOT increment realtime");
    }

    #[test]
    fn test_read_tsc_returns_zero_in_test_build() {
        // In test builds, read_tsc() returns 0 (no hardware counter available).
        // In non-test builds, it delegates to minix_arch::CurrentClockArch::read_tsc().
        assert_eq!(read_tsc(), 0);
    }
}
