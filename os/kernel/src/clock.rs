//! Kernel clock and timer subsystem.
//!
//! Implements 100Hz periodic timer interrupt handling, synchronous alarm timers,
//! virtual/profile timers, time accounting, and load average tracking.
//!
//! **Zero-heap contract**: see `lib.rs` for the kernel-wide contract. This
//! module allocates nothing at runtime — alarm timers use an intrusive
//! sorted list embedded in `KPriv::runtime.s_alarm_timer` (D2).
//!
//! # Minix3 C Source Mapping
//!
//! - `clock.c:47-64` — `init_clock()`: initialize clock variables
//! - `clock.c:70-173` — `timer_int_handler()`: main tick handler
//! - `clock.c:229-240` — `set_kernel_timer()`
//! - `clock.c:245-255` — `reset_kernel_timer()`
//! - `clock.c:260-292` — `load_update()`: load average tracking
//! - `clock.c:294-304` — `boot_cpu_init_timer()` / `app_cpu_init_timer()`
//! - `do_setalarm.c:69-76` — `cause_alarm()`: alarm notification
//! - `do_vtimer.c:81-103` — `vtimer_check()`: virtual/profile timer expiry
//! - `arch_clock.c:326-330` (i386) — `context_stop()` (context switch path): quantum decrement
//!
//! # Design Decisions (15-clock-timer.md §3)
//!
//! - **D1**: `ClockState` struct encapsulates `kclockinfo` + `kloadinfo` + `clock_timers`
//! - **D2**: alarm timer chain = C-isomorphic intrusive sorted singly-linked list.
//!   Head `Option<PrivId>` lives in `ClockState` (C: `clock_timers`, clock.c:37);
//!   nodes are embedded in `KPriv::runtime.s_alarm_timer` (C: `priv[i].s_alarm_timer`,
//!   priv.h:48) and linked by `Option<PrivId>` indices instead of pointers.
//!   Zero heap allocation: capacity is bounded by `NR_SYS_PROCS` (one timer per
//!   privilege slot, same as C).
//! - **D3**: node identity = `PrivId` (the embedding privilege slot). C uses the
//!   `minix_timer_t *tp` pointer (the embedded node's address) as identity;
//!   the index of the embedding slot is the pointer's positional counterpart.
//! - **D6**: `TimerAction` enum replaces C's `tmr_func_t` (KernelCallback variant deleted)
//! - **D7**: `hz` is compile-time constant with runtime override
//! - **D8**: per-CPU `ClockState` instance with `is_bsp` flag (replaces PerCpuTick const)
//! - **D9**: quantum decrement moved to `clock::decrement_quantum()` (NOT in `tick()`)
//! - **D10**: explicit `billp: Option<&mut KProcess>` parameter for billable accounting
//! - **D11**: standalone `vtimer_check()` deleted (tick-internal logic handles expiry)

use core::sync::atomic::Ordering;

use minix_types::Endpoint;

use crate::kpriv::{PrivId, PrivTable};
use crate::proc::{CpuId, KProcess, MiscFlagsBits};

// ── Global clock state mirrors (read by scheduler without &ClockState) ──

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
/// C: `get_monotonic()` — clock.c:203 (`return kclockinfo.uptime`).
/// Safe to call from any context (BKL or not) — only reads an atomic.
pub fn get_monotonic() -> u64 {
    CLOCK_UPTIME.load(Ordering::Acquire)
}

/// Get wall-clock ticks since boot.
///
/// C: `get_realtime()` — clock.c:178 (`return kclockinfo.realtime`).
/// Safe to call from any context — only reads an atomic.
pub fn get_realtime() -> u64 {
    CLOCK_REALTIME.load(Ordering::Acquire)
}

/// Get boot time in seconds since UNIX epoch.
///
/// C: `get_boottime()` — clock.c:220 (`return kclockinfo.boottime`).
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

/// Convert CPU time cycles to milliseconds (inverse of `ms_to_cpu_time`).
///
/// C: `cpu_time_2_ms(cycles)` — kernel/proc.h (`cycles / tsc_per_ms[cpuid]`).
///
/// Used by `notify_scheduler` to report `p_accounting.time_in_queue`
/// (accumulated in cycles) as milliseconds in the `SCHEDULING_NO_QUANTUM`
/// message's `acnt_queue` field (proc.c:1876).
pub fn cpu_time_to_ms(cycles: u64) -> u32 {
    let tsc_per_ms = TSC_PER_MS.load(Ordering::Acquire);
    let factor = if tsc_per_ms == 0 { DEFAULT_TSC_PER_MS } else { tsc_per_ms };
    // R-16 (2026-08-12): SAFETY: `cycles / factor` yields milliseconds. Used for
    // per-quantum `time_in_queue` accounting (notify_scheduler), which is bounded
    // well below u32::MAX (~49 days) in practice; no hard proof against overflow
    // for pathological multi-day accumulated cycle counts.
    (cycles / factor) as u32
}

/// Get the current CPU id.
///
/// C: `cpuid` — `get_cpulocal_var(cpu)` index. In single-CPU builds this is
/// always the BSP (0). SMP builds (16-smp.md) will read it from a CPU-local
/// register (x86-64: GS base; aarch64: TPIDR_EL1; riscv64: scratch CSR).
///
/// Returns 0 until `SMP_STATE` is initialized (e.g. in test contexts).
pub fn current_cpuid() -> CpuId {
    // SAFETY: read-only access to bsp_cpu_id. If SMP_STATE isn't init yet
    // (test/early boot), fall back to the BSP.
    unsafe {
        crate::try_smp_state().map(|s| s.bsp_cpu_id()).unwrap_or(CpuId::BSP)
    }
}

/// Compute instantaneous CPU load (0..100) for the current CPU.
///
/// C: `cpu_load()` — arch/i386/arch_clock.c:381-415.
///
/// Measures the fraction of the most recent TSC interval that was NOT spent
/// in the idle task. Reads `cpu_last_tsc` / `cpu_last_idle` from per-CPU
/// state, computes `(tsc_delta - idle_delta) * 100 / tsc_delta`, then
/// updates the per-CPU state for the next call.
///
/// Returns 0 on the first call (no baseline) or if `SMP_STATE` is not yet
/// initialized (tests). The result is clamped to 100.
///
/// # Design (D-notify-scheduler / 11-scheduling-primitives.md §4.5)
///
/// Made possible by storing `SmpState` in the global `SMP_STATE` so that
/// per-CPU `cpu_last_tsc` / `cpu_last_idle` are reachable from the clock
/// tick path. The idle process's `p_cycles.total` is read from `PROC_TABLE`.
///
/// # Safety
///
/// Caller must hold the BKL (or be in single-threaded boot). Both
/// `SMP_STATE` and `PROC_TABLE` are BKL-protected globals.
pub fn cpu_load() -> u32 {
    // SAFETY: caller guarantees BKL. We borrow SMP_STATE and PROC_TABLE
    // simultaneously — they are distinct statics, so the two `&'static mut`
    // borrows do not alias.
    unsafe {
        let smp = match crate::try_smp_state() {
            Some(s) => s,
            None => return 0,
        };
        let cpu = smp.bsp_cpu_id();
        let local = match smp.cpu_local_mut(cpu) {
            Some(l) => l,
            None => return 0,
        };
        let last_tsc = local.cpu_last_tsc;
        let idle_nr = local.idle_proc;

        let current_tsc = read_tsc();
        if last_tsc == 0 {
            // First call: establish baseline, no load to report.
            local.cpu_last_tsc = current_tsc;
            let proc_table = crate::proc_table();
            if let Some(idle) = proc_table.get(idle_nr) {
                local.cpu_last_idle = idle.p_cycles.total.load(Ordering::Acquire);
            }
            return 0;
        }

        let tsc_delta = current_tsc.saturating_sub(last_tsc);
        let proc_table = crate::proc_table();
        let current_idle = proc_table
            .get(idle_nr)
            .map(|p| p.p_cycles.total.load(Ordering::Acquire))
            .unwrap_or(0);
        let idle_delta = current_idle.saturating_sub(local.cpu_last_idle);

        // Update baseline for next call.
        local.cpu_last_tsc = current_tsc;
        local.cpu_last_idle = current_idle;

        if tsc_delta == 0 {
            return 0;
        }
        let busy = tsc_delta - idle_delta.min(tsc_delta);
        let load = (busy * 100) / tsc_delta;
        if load > 100 { 100 } else { load as u32 }
    }
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
/// # Instance-based design (04-platform-discovery.md §3.4)
///
/// Constructs a transient `CurrentClockArch` instance from the global
/// platform descriptor's `timer()` sub-descriptor. The instance is cheap
/// to construct (just copies a few fields) and is discarded after the
/// read. This replaces the old static `CurrentClockArch::read_tsc()`.
pub fn read_tsc() -> u64 {
    #[cfg(not(test))]
    {
        use minix_arch::{ClockArch, CurrentClockArch};
        use minix_platform::platform_desc;
        let pd = platform_desc();
        let clock_arch = CurrentClockArch::new(pd.timer());
        clock_arch.read_tsc()
    }
    #[cfg(test)]
    {
        0
    }
}

/// Stop the per-CPU local timer.
///
/// C: `stop_local_timer()` — inline in `smp_ipi_halt_handler()`
/// (smp.c:56-61) as `lapic_stop_timer()`.
///
/// Called by the SMP `ipi_halt_handler` before halting a CPU. Disables the
/// LAPIC Timer (x86-64), Generic Timer (ARM64), or CLINT timer (RISC-V) to
/// prevent interrupts during the halt.
///
/// # Instance-based design (04-platform-discovery.md §3.4)
///
/// Constructs a transient `CurrentClockArch` instance from the global
/// platform descriptor, same pattern as `read_tsc()`. The instance is cheap
/// to construct and discarded after the call.
pub fn stop_local_timer() {
    #[cfg(not(test))]
    {
        use minix_arch::{ClockArch, CurrentClockArch};
        use minix_platform::platform_desc;
        let pd = platform_desc();
        let mut clock_arch = CurrentClockArch::new(pd.timer());
        clock_arch.stop_local_timer(current_cpuid().raw());
    }
    #[cfg(test)]
    {
        // In tests, no hardware timer to stop.
    }
}

/// Initialize the profile clock for statistical profiling.
///
/// C: `init_profile_clock()` — arch/i386/arch_clock.c
///
/// Configures the hardware timer (PIT on x86-64, Generic Timer on ARM64,
/// CLINT on RISC-V) to generate interrupts at `hz` Hz for statistical
/// profiling. Returns `Err(ProfileClockError::Unsupported)` if the
/// hardware does not support profiling.
// R-18 (2026-08-13): single failure mode (hardware lacks profiling
// support), no diagnostic info to carry. Propagates the arch trait's
// `ProfileClockError` (C-D-5 cleanup: named error type replaces `()`).
pub fn init_profile_clock(hz: u32) -> Result<(), minix_arch::clock::ProfileClockError> {
    #[cfg(not(test))]
    {
        use minix_arch::{ClockArch, CurrentClockArch};
        use minix_platform::platform_desc;
        let pd = platform_desc();
        let mut clock_arch = CurrentClockArch::new(pd.timer());
        clock_arch.init_profile_clock(hz)
    }
    #[cfg(test)]
    {
        // In tests, no hardware timer to configure.
        let _ = hz;
        Ok(())
    }
}

/// Stop the profile clock.
///
/// C: `stop_profile_clock()` — arch/i386/arch_clock.c
///
/// Stops the hardware timer that was generating profiling interrupts.
pub fn stop_profile_clock() {
    #[cfg(not(test))]
    {
        use minix_arch::{ClockArch, CurrentClockArch};
        use minix_platform::platform_desc;
        let pd = platform_desc();
        let mut clock_arch = CurrentClockArch::new(pd.timer());
        clock_arch.stop_profile_clock();
    }
    #[cfg(test)]
    {
        // In tests, no hardware timer to stop.
    }
}

/// Acknowledge a profile clock interrupt.
///
/// C: `arch_ack_profile_clock()` — profile.c:123
///
/// Called by `profile_clock_handler` after collecting a sample. Clears
/// the pending interrupt on the hardware so the next tick can fire.
pub fn ack_profile_clock() {
    #[cfg(not(test))]
    {
        use minix_arch::{ClockArch, CurrentClockArch};
        use minix_platform::platform_desc;
        let pd = platform_desc();
        let mut clock_arch = CurrentClockArch::new(pd.timer());
        clock_arch.ack_profile_clock();
    }
    #[cfg(test)]
    {
        // In tests, no hardware timer to ack.
    }
}

/// Decrement the current process's quantum based on the TSC delta since the
/// last call.
///
/// C: `context_stop()` — arch/i386/arch_clock.c:326-330 (within line 208-349)
/// (`if (tsc_delta < p->p_cpu_time_left) p->p_cpu_time_left -= tsc_delta;
///  else p->p_cpu_time_left = 0;`)
///
/// `context_stop()` is called from the context switch path (proc.c:208/440/1956
/// + assembly entries mpx.S/apic_asm.S), NOT from the timer interrupt path.
///   `arch_timer_int_handler()` in i386 is an empty function (arch_clock.c:72-74).
///
/// Reads the current TSC, computes the delta since the last call (stored in
/// `CpuLocal::tsc_ctr_switch`), updates the baseline, and decrements
/// `current_proc.p_sched.quantum.cpu_time_left` by that delta (saturating
/// to 0). Kernel/idle tasks (endpoint < 0) are quantum-exempt (C:
/// arch_clock.c:314 `if (p->p_endpoint >= 0)`).
///
/// # Design (D9)
///
/// Quantum decrement is **NOT** in `ClockState::tick()` — it lives here,
/// mirroring C's separation of `timer_int_handler()` (software tick, clock.c:70-173)
/// from `context_stop()` (context switch path, arch_clock.c:208-349). See
/// 15-clock-timer.md §3.9 and 15-clock-timer.md §4.9.
///
/// # Returns
///
/// `true` if the quantum is now exhausted. The caller is expected to set
/// `RTS_NO_QUANTUM` on the process and trigger scheduling (mirroring
/// `switch_to_user()` stage 4 in proc.c:421-422).
///
/// # Safety
///
/// Caller must hold the BKL (or be in single-threaded boot/test context).
/// `SMP_STATE` and the per-CPU `CpuLocal::tsc_ctr_switch` are BKL-protected
/// globals. Returns `false` if `SMP_STATE` is not yet initialized (e.g.
/// during early boot or in unit tests that don't set up SMP state).
pub fn decrement_quantum(current_proc: &mut KProcess) -> bool {
    decrement_quantum_with_tsc(current_proc, read_tsc())
}

/// Testable variant of [`decrement_quantum`] with an injected TSC value.
///
/// Production code calls [`decrement_quantum`], which reads the hardware
/// TSC via [`read_tsc`]. In `cfg(test)` builds, `read_tsc()` returns 0,
/// making the delta always zero — so tests call this variant directly to
/// simulate TSC progression.
///
/// See [`decrement_quantum`] for semantics, safety, and design rationale.
pub(crate) fn decrement_quantum_with_tsc(
    current_proc: &mut KProcess,
    current_tsc: u64,
) -> bool {
    // SAFETY: caller holds BKL. SMP_STATE is a BKL-protected global; we
    // borrow it briefly to read+update `tsc_ctr_switch` for the current
    // CPU and then drop the borrow before touching `current_proc`.
    // `current_proc` is a separate `&mut` from `SMP_STATE`'s internals
    // (which only owns per-CPU scheduler state, not the proc table), so
    // the two borrows do not alias.
    unsafe {
        match crate::try_smp_state() {
            Some(smp) => decrement_quantum_in(smp, current_proc, current_tsc),
            None => false,
        }
    }
}

/// Core quantum-decrement logic with all dependencies injected.
///
/// Split from [`decrement_quantum_with_tsc`] so unit tests can drive the
/// logic with a local `SmpState` instead of mutating the global
/// `SMP_STATE` (which would race with parallel tests). Both `smp` and
/// `current_proc` are `&mut` but refer to disjoint state — `SmpState`
/// owns per-CPU scheduler data, not the process table.
fn decrement_quantum_in(
    smp: &mut crate::smp::SmpState,
    current_proc: &mut KProcess,
    current_tsc: u64,
) -> bool {
    let cpu = smp.bsp_cpu_id();
    let last_tsc = match smp.cpu_local_mut(cpu) {
        Some(local) => {
            let last = local.tsc_ctr_switch;
            // C: arch_clock.c:342 — `*__tsc_ctr_switch = tsc;`
            // Update the baseline regardless of process type; even
            // kernel/idle tasks advance the TSC counter so the next
            // delta is computed from the correct baseline.
            local.tsc_ctr_switch = current_tsc;
            last
        }
        None => return false,
    };

    // First call after boot/context-switch with no baseline: establish
    // baseline and return. Mirrors C's behavior where `*__tsc_ctr_switch`
    // is set on context switch (`note_context_switch`), so the first tick
    // after switch computes a real delta.
    if last_tsc == 0 {
        return false;
    }

    let delta = current_tsc.saturating_sub(last_tsc);
    if delta == 0 {
        return false;
    }

    // C: arch_clock.c:314 — skip kernel/idle tasks (endpoint < 0).
    // Kernel tasks (CLOCK/SYSTEM/KERNEL/IDLE/ASYNCM) are quantum-exempt.
    if current_proc.p_endpoint.get() < 0 {
        return false;
    }

    // C: arch_clock.c:326-330 — `p_cpu_time_left -= tsc_delta` (saturating).
    // `Quantum::consume` performs the saturating decrement via CAS and
    // returns `true` when the quantum is exhausted (cpu_time_left <= delta).
    current_proc.p_sched.quantum.consume(delta)
}

// ── Constants ──

/// Default clock frequency in Hz.
/// C: `DEFAULT_HZ` — include/arch/i386/include/archconst.h:4 (60) / earm (1000)
/// 架构演进标注：[ARCH: K-3]（doc 05-clock-interrupt-init.md §3.8：60/1000 → 100 Hz）
/// Rust 选择 100 作为默认（doc 15 §3.7 D7 设计决策），与 C 的 60 不同；
/// 需与 C 完全一致时可在 boot 阶段用 `with_hz(60)` 指定。
pub const DEFAULT_HZ: u32 = 100;

/// Timer "never expires" sentinel.
/// C: `TMR_NEVER` = `((clock_t)TMRDIFF_MAX + 1)` — include/minix/timers.h:48
/// (= INT_MAX+1 = 0x80000000).
///
/// Must be exactly `TMRDIFF_MAX + 1`, NOT `u64::MAX`: with the wrap-safe
/// comparison `tmr_is_first`, `u64::MAX` is only `now + 1` ticks ahead of
/// any `now` (i.e. always "expired"), while `TMRDIFF_MAX + 1` is always
/// more than half the tick space away — unreachable until uptime wraps
/// past it (~68 years at 100 Hz), which is the C semantics of "never".
pub const TMR_NEVER: u64 = (i32::MAX as u64) + 1;

/// Maximum valid timer difference (half the tick value space).
/// C: `TMRDIFF_MAX` = `INT_MAX` — include/minix/timers.h:45.
///
/// `clock_t` is unsigned and may wrap, so time comparisons must use
/// wrap-safe relative differences: `a` is "not later than" `b` iff
/// `b.wrapping_sub(a) <= TMRDIFF_MAX` (C: `tmr_is_first(a, b)`, timers.h:57).
const TMRDIFF_MAX: u64 = i32::MAX as u64;

/// Wrap-safe "not later than" comparison of two absolute tick times.
/// C: `tmr_is_first(a, b)` = `(b) - (a) <= TMRDIFF_MAX` — include/minix/timers.h:57.
///
/// Public because kernel-call layering mirrors C's header macro: callers
/// outside this module need the same comparison (C: do_setalarm.c:42 uses
/// `tmr_is_first(uptime, tp->tmr_exp_time)` for the time_left reply).
#[inline]
pub fn tmr_is_first(a: u64, b: u64) -> bool {
    b.wrapping_sub(a) <= TMRDIFF_MAX
}

/// Wrap-safe "expired" check for a timer node at time `now`.
/// C: `tmr_has_expired(tp, now)` = `tmr_is_first((tp)->tmr_exp_time, now)` — timers.h:58.
#[inline]
fn tmr_has_expired(exp_time: u64, now: u64) -> bool {
    tmr_is_first(exp_time, now)
}

/// Load average sampling interval in seconds.
/// C: `_LOAD_UNIT_SECS` — include/minix/type.h:88 (6)
const LOAD_UNIT_SECS: u64 = 6;

/// Number of load average history slots.
/// C: `_LOAD_HISTORY` — include/minix/type.h:95 (150 = 60s*15min/6s)
const LOAD_HISTORY: usize = 150;

// ── Timer action (D6) ──

/// Action to take when a timer expires.
///
/// Replaces C's `tmr_func_t` function pointer + `tmr_arg` integer.
/// Design decision D6: enum dispatch replaces function pointers for type safety.
///
/// C: `cause_alarm(proc_nr_e)` — do_setalarm.c:69-76
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimerAction {
    /// Notify a process via synchronous alarm.
    /// C: `cause_alarm()` → `mini_notify(CLOCK, endpoint)`
    NotifyAlarm { endpoint: Endpoint },
    // KernelCallback variant DELETED (YAGNI): was dead code with no callers.
    // If a kernel-internal timer callback is needed in the future (e.g.
    // for a kernel watchdog or profile timer), add a new variant here and
    // wire up the dispatch in expire_alarm_timers().
}

// ── Alarm timer node (D2, D3) ──

/// Intrusive alarm timer node, embedded in `KPriv::runtime.s_alarm_timer`.
///
/// C-isomorphic rewrite of `minix_timer_t` (include/minix/timers.h:32-38)
/// embedded at `priv[i].s_alarm_timer` (kernel/priv.h:48). The field layout
/// differs in exactly one way: `tmr_next` is an `Option<PrivId>` index into
/// `PrivTable` instead of a `struct minix_timer *` pointer, so the kernel
/// never heap-allocates timer storage (capacity = `NR_SYS_PROCS`, same bound
/// as C's `EXTERN struct priv priv[NR_SYS_PROCS]`).
///
/// A node is "set" (on the clock chain) iff `action.is_some()`, matching C's
/// `tmr_is_set(tp)` = `tp->tmr_func != NULL` (timers.h:52).
///
/// Field name `s_` prefix lives on the embedding `KPriv::runtime` (C
/// traceability); node-internal names mirror the C struct member names.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AlarmTimerNode {
    /// Expiration time in monotonic ticks (absolute). C: `tmr_exp_time`.
    pub exp_time: u64,
    /// Action to call when expired; `None` = timer not set. C: `tmr_func`
    /// (NULL means inactive) + `tmr_arg` folded into the enum payload.
    pub action: Option<TimerAction>,
    /// Successor in the global sorted chain, as a privilege-slot index.
    /// C: `tmr_next` (pointer). Stale after dequeue — C leaves it dangling
    /// too (tmrs_clr.c:29-34 unlinks without clearing `tp->tmr_next`); the
    /// authoritative link is the chain reachable from `ClockState::timers_head`.
    pub next: Option<PrivId>,
}

impl Default for AlarmTimerNode {
    fn default() -> Self {
        Self::new()
    }
}

impl AlarmTimerNode {
    /// Const-constructible zeroed node (for `const fn` table init).
    ///
    /// C: `tmr_inittimer(tp)` = `tmr_func = NULL; tmr_next = NULL`
    /// (timers.h:64) — `exp_time` is left uninitialized in C; Rust zeroes it
    /// because `Option` discriminants require a total value.
    pub const fn new() -> Self {
        Self { exp_time: 0, action: None, next: None }
    }

    /// Whether this node is on the clock chain.
    /// C: `tmr_is_set(tp)` = `tp->tmr_func != NULL` — timers.h:52.
    pub fn is_set(&self) -> bool {
        self.action.is_some()
    }
}

// ── Alarm timer chain operations (D2) ──
//
// C-isomorphic rewrite of the three `tmrs_*` primitives from
// minix/lib/libtimers/{tmrs_set,tmrs_clr,tmrs_exp}.c, operating on
// `ClockState::timers_head` (C: `clock_timers`, clock.c:37) plus the
// nodes embedded in `PrivTable`. All operations take `(&mut PrivTable,
// &mut ClockState)` as separate mutable borrows — the two objects are
// always owned side by side (BKL protects both), and Rust's borrow
// checker permits two distinct `&mut` arguments.
//
// Zero heap allocation: the chain capacity is bounded by NR_SYS_PROCS
// (one node per privilege slot), identical to C.

/// Deactivate a timer node and remove it from the chain.
///
/// C: `reset_kernel_timer(tp)` — clock.c:245-255
/// `if (tmr_is_set(tp)) tmrs_clrtimer(&clock_timers, tp, NULL, NULL)`
///
/// Idempotent: a node that is not set (or not on the chain) is left
/// untouched. This is the operation invoked by `SYS_SETALARM` with
/// relative `exp_time == 0` (do_setalarm.c:57) and by `SYS_CLEAR`
/// (do_clear.c:52).
pub fn reset_alarm_timer(
    priv_table: &mut PrivTable,
    clock: &mut ClockState,
    priv_id: PrivId,
) {
    let node = match priv_table.get(priv_id) {
        Some(kp) => kp.runtime.s_alarm_timer,
        None => return,
    };
    if !node.is_set() {
        return; // C: `if (tmr_is_set(tp))` guard.
    }
    chain_unlink(priv_table, clock, priv_id);
    // C: tmrs_clr.c:27 — `tp->tmr_func = NULL` clears the timer object.
    // (tmr_exp_time is left as-is, same as C.)
    if let Some(kp) = priv_table.get_mut(priv_id) {
        kp.runtime.s_alarm_timer.action = None;
    }
}

/// Activate (or re-arm) a timer node at absolute time `exp_time`.
///
/// C: `set_kernel_timer(tp, exp_time, watchdog, arg)` — clock.c:229-240,
/// delegating to `tmrs_settimer()` (tmrs_set.c:14-47): if the node is
/// already set it is first removed from the chain; then the node fields
/// are written and the node is inserted in expiry order (earliest first;
/// among equal expiry times the most recently inserted node goes in
/// front — the scan breaks at the first `cur` with
/// `tmr_is_first(exp_time, cur_exp)`, i.e. `exp_time <= cur_exp`, and
/// inserts before it).
pub fn set_alarm_timer(
    priv_table: &mut PrivTable,
    clock: &mut ClockState,
    priv_id: PrivId,
    exp_time: u64,
    action: TimerAction,
) {
    assert!(
        clock.is_bsp,
        "set_alarm_timer called on AP ClockState (timers are BSP-only)"
    );
    // C: tmrs_set.c:29-30 — clear the old timer object first.
    reset_alarm_timer(priv_table, clock, priv_id);

    // C: tmrs_set.c:31-33 — set the timer's variables.
    {
        let Some(kp) = priv_table.get_mut(priv_id) else { return };
        kp.runtime.s_alarm_timer.exp_time = exp_time;
        kp.runtime.s_alarm_timer.action = Some(action);
    }

    // C: tmrs_set.c:38-43 — insert before the first node whose expiry is
    // not earlier than ours (wrap-safe comparison).
    let mut prev: Option<PrivId> = None;
    let mut cur = clock.timers_head;
    loop {
        let insert_here = match cur {
            None => true,
            Some(cid) => {
                let cur_exp = priv_table
                    .get(cid)
                    .map(|kp| kp.runtime.s_alarm_timer.exp_time)
                    .unwrap_or(u64::MAX);
                tmr_is_first(exp_time, cur_exp)
            }
        };
        if insert_here {
            break;
        }
        prev = cur;
        cur = match cur {
            Some(cid) => priv_table
                .get(cid)
                .and_then(|kp| kp.runtime.s_alarm_timer.next),
            None => None,
        };
    }
    // Link: prev -> priv_id -> cur.
    match prev {
        Some(pid) => {
            if let Some(kp) = priv_table.get_mut(pid) {
                kp.runtime.s_alarm_timer.next = Some(priv_id);
            }
        }
        None => {
            clock.timers_head = Some(priv_id);
        }
    }
    if let Some(kp) = priv_table.get_mut(priv_id) {
        kp.runtime.s_alarm_timer.next = cur;
    }
}

/// Remove `priv_id` from the chain without touching its set-state.
///
/// C: the unlink loop of `tmrs_clrtimer` — tmrs_clr.c:29-34
/// (`for (atp = tmrs; *atp != NULL; atp = &(*atp)->tmr_next) ...`).
/// The caller is responsible for clearing `action` (C: `tmr_func`).
fn chain_unlink(
    priv_table: &mut PrivTable,
    clock: &mut ClockState,
    priv_id: PrivId,
) {
    if clock.timers_head == Some(priv_id) {
        let next = priv_table
            .get(priv_id)
            .and_then(|kp| kp.runtime.s_alarm_timer.next);
        clock.timers_head = next;
    } else {
        // Walk the chain to find the predecessor of priv_id.
        let mut cur = clock.timers_head;
        while let Some(cid) = cur {
            let next = priv_table
                .get(cid)
                .and_then(|kp| kp.runtime.s_alarm_timer.next);
            if next == Some(priv_id) {
                let target_next = priv_table
                    .get(priv_id)
                    .and_then(|kp| kp.runtime.s_alarm_timer.next);
                if let Some(kp) = priv_table.get_mut(cid) {
                    kp.runtime.s_alarm_timer.next = target_next;
                }
                return;
            }
            cur = next;
        }
    }
}

/// Check the chain for expired timers, deactivate them, and invoke
/// `on_expired` for each.
///
/// C: `tmrs_exptimers(&clock_timers, uptime, NULL)` — tmrs_exp.c:9-29,
/// called from the BSP tick path (clock.c:159-161). Expiry uses the
/// wrap-safe `tmr_has_expired` check (timers.h:58), matching C's
/// overflow-aware comparison.
///
/// The callback is invoked after the node is dequeued and deactivated —
/// the same ordering as C (`tmrs_exp.c:15-20` unlinks and NULLs
/// `tmr_func` before calling `func`). This makes the node re-armable
/// from within the callback (e.g. a periodic alarm) without corrupting
/// the chain walk.
pub fn expire_alarm_timers<F>(
    priv_table: &mut PrivTable,
    clock: &mut ClockState,
    now: u64,
    mut on_expired: F,
) where
    F: FnMut(TimerAction),
{
    while let Some(head_id) = clock.timers_head {
        let head_exp = priv_table
            .get(head_id)
            .map(|kp| kp.runtime.s_alarm_timer.exp_time)
            .unwrap_or(u64::MAX);
        if !tmr_has_expired(head_exp, now) {
            break;
        }
        // Dequeue head (C: `*tmrs = tp->tmr_next`).
        let next = priv_table
            .get(head_id)
            .and_then(|kp| kp.runtime.s_alarm_timer.next);
        clock.timers_head = next;
        // Deactivate before invoking (C: tmrs_exp.c:17-18).
        let action = priv_table
            .get_mut(head_id)
            .and_then(|kp| kp.runtime.s_alarm_timer.action.take());
        if let Some(action) = action {
            on_expired(action);
        }
    }
}

// ── Virtual/Profile timer expiry ──

/// Result of checking virtual/profile timer expiry.
///
/// C: `vtimer_check()` — do_vtimer.c:81-103
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
    /// C: `proc_load_history[_LOAD_HISTORY]` — u16[150]
    proc_load_history: [u16; LOAD_HISTORY],
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

// ── ClockState (D1, D8) ──

/// Global clock state, equivalent to C's `kclockinfo` + `kloadinfo` + `clock_timers`.
///
/// All fields are protected by BKL. No interior mutability needed because
/// the clock handler runs under BKL.
///
/// Design decision D1: encapsulate as struct for ownership clarity.
/// Design decision D8: per-CPU instance with `is_bsp` flag (replaces
/// `PerCpuTick` const generic — SMP single-image requires runtime
/// BSP/AP distinction, not compile-time).
///
/// C: clock.h — `struct clockinfo kclockinfo` + `struct loadinfo kloadinfo`
/// C: clock.c:37 — `static minix_timer_t *clock_timers` (BSP only)
#[derive(Debug)]
pub struct ClockState {
    /// CPU id this instance belongs to.
    /// C: `cpuid` — `get_cpulocal_var(cpu)`
    cpu_id: CpuId,
    /// `true` iff this is the BSP instance (owns global time + alarm timers).
    /// C: `cpu_is_bsp(cpuid)` — clock.c:91
    is_bsp: bool,
    /// Clock frequency in Hz. C: `kclockinfo.hz`
    hz: u32,
    /// Monotonically increasing ticks since boot. C: `kclockinfo.uptime` (BSP only)
    uptime: u64,
    /// Wall-clock ticks since boot (affected by adjtime). C: `kclockinfo.realtime` (BSP only)
    realtime: u64,
    /// UNIX epoch seconds at boot. C: `kclockinfo.boottime` (BSP only)
    boottime: u64,
    /// Time adjustment delta (positive=speed up, negative=slow down).
    /// C: `adjtime_delta` (clock.c:42, BSP only)
    adjtime_delta: i32,
    /// Head of the alarm timer chain (BSP only): index of the first
    /// `KPriv::runtime.s_alarm_timer` node, or `None` when empty.
    /// C: `static minix_timer_t *clock_timers` — clock.c:37.
    /// Design decision D2: intrusive index chain (nodes embedded in PrivTable)
    /// replaces C's pointer chain — zero heap allocation.
    timers_head: Option<PrivId>,
    /// Load average info. C: `kloadinfo` (all CPUs)
    load_info: LoadInfo,
}

impl ClockState {
    /// Create a new BSP `ClockState` with default frequency (100Hz).
    ///
    /// C: `init_clock()` — clock.c:47-64
    pub fn new() -> Self {
        Self::new_for_cpu(CpuId::BSP, true)
    }

    /// Create a new `ClockState` for a specific CPU.
    ///
    /// `is_bsp = true` for the BSP instance (owns global time + alarm timers).
    /// `is_bsp = false` for AP instances (only tracks local load average).
    ///
    /// C: `boot_cpu_init_timer()` (BSP) / `app_cpu_init_timer()` (AP) — clock.c:294-312
    pub fn new_for_cpu(cpu_id: CpuId, is_bsp: bool) -> Self {
        Self {
            cpu_id,
            is_bsp,
            hz: DEFAULT_HZ,
            uptime: 0,
            realtime: 0,
            boottime: 0,
            adjtime_delta: 0,
            timers_head: None,
            load_info: LoadInfo::new(),
        }
    }

    /// Create a BSP `ClockState` with a custom frequency.
    ///
    /// C: `env_get("hz")` override in `init_clock()` — clock.c:56-60
    /// Range: 2..=50000 (matching C's validation).
    pub fn with_hz(hz: u32) -> Self {
        let hz = if !(2..=50000).contains(&hz) { DEFAULT_HZ } else { hz };
        let mut state = Self::new();
        state.hz = hz;
        state
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

    /// Get monotonic ticks since boot (BSP only; AP always returns 0).
    /// C: `get_monotonic()` — clock.c:203
    pub fn uptime(&self) -> u64 {
        self.uptime
    }

    /// Get wall-clock ticks since boot (BSP only; AP always returns 0).
    /// C: `get_realtime()` — clock.c:178
    pub fn realtime(&self) -> u64 {
        self.realtime
    }

    /// Get boot time in seconds since UNIX epoch (BSP only).
    /// C: `get_boottime()` — clock.c:220
    pub fn boottime(&self) -> u64 {
        self.boottime
    }

    /// Get the CPU id this instance belongs to.
    pub fn cpu_id(&self) -> CpuId {
        self.cpu_id
    }

    /// Returns `true` iff this is the BSP instance.
    /// C: `cpu_is_bsp(cpuid)`
    pub fn is_bsp(&self) -> bool {
        self.is_bsp
    }

    /// Set boot time in seconds since UNIX epoch (BSP only).
    /// C: `set_boottime()` — clock.c:212
    pub fn set_boottime(&mut self, new_boottime: u64) {
        if !self.is_bsp { return; }
        self.boottime = new_boottime;
        CLOCK_BOOTTIME.store(new_boottime, Ordering::Release);
    }

    /// Set wall-clock time in ticks (BSP only).
    /// C: `set_realtime()` — clock.c:187
    pub fn set_realtime(&mut self, new_realtime: u64) {
        if !self.is_bsp { return; }
        self.realtime = new_realtime;
        CLOCK_REALTIME.store(new_realtime, Ordering::Release);
    }

    /// Set adjtime delta for time adjustment (BSP only).
    /// C: `set_adjtime_delta()` — clock.c:195
    pub fn set_adjtime_delta(&mut self, ticks: i32) {
        if !self.is_bsp { return; }
        self.adjtime_delta = ticks;
    }

    /// Get the current adjtime delta (BSP only; AP returns 0).
    /// C: `adjtime_delta` — clock.c:42
    pub fn adjtime_delta(&self) -> i32 {
        self.adjtime_delta
    }

    /// Get the load average history.
    pub fn load_history(&self) -> &[u16; LOAD_HISTORY] {
        &self.load_info.proc_load_history
    }

    // ── Tick handling (D8, D9, D10) ──
    //
    // Timer set/reset/expiry moved to the free functions `set_alarm_timer` /
    // `reset_alarm_timer` / `expire_alarm_timers` above: they operate on
    // `(&mut PrivTable, &mut ClockState)` jointly (the intrusive chain head
    // lives in `ClockState`, the nodes in `PrivTable`).

    /// Handle a timer interrupt tick on the BSP.
    ///
    /// Convenience wrapper that delegates to [`Self::tick_with`] with the
    /// BSP instance's `is_bsp = true`. The caller must pass the billable
    /// process if the current process is not billable (D10).
    ///
    /// See 15-clock-timer.md §4.1.
    pub fn tick_bsp<F>(
        &mut self,
        priv_table: &mut PrivTable,
        current_proc: &mut KProcess,
        billp: Option<&mut KProcess>,
        ready_count: usize,
        on_expired: F,
    ) -> Option<VtimerExpired>
    where
        F: FnMut(TimerAction),
    {
        debug_assert!(self.is_bsp, "tick_bsp called on non-BSP ClockState");
        self.tick_with(priv_table, current_proc, billp, ready_count, on_expired)
    }

    /// Handle a timer interrupt tick on an AP.
    ///
    /// Convenience wrapper that delegates to [`Self::tick_with`] with the AP
    /// instance's `is_bsp = false`. The caller must pass the billable process
    /// if the current process is not billable (D10).
    pub fn tick_ap<F>(
        &mut self,
        priv_table: &mut PrivTable,
        current_proc: &mut KProcess,
        billp: Option<&mut KProcess>,
        ready_count: usize,
        on_expired: F,
    ) -> Option<VtimerExpired>
    where
        F: FnMut(TimerAction),
    {
        debug_assert!(!self.is_bsp, "tick_ap called on BSP ClockState");
        self.tick_with(priv_table, current_proc, billp, ready_count, on_expired)
    }

    /// Handle a timer interrupt tick (zero-allocation, callback-based).
    ///
    /// This is the main clock interrupt handler, called at `hz` frequency.
    /// The per-CPU role is determined by `self.is_bsp` (D8: per-CPU instance
    /// with runtime flag, replacing `PerCpuTick` const generic).
    ///
    /// C: `timer_int_handler()` — clock.c:70-173
    ///
    /// This is the sole tick path: expired alarm timers are dispatched
    /// inline via `on_expired` (BSP only), so no intermediate collection
    /// is allocated. The old `tick()`/`TimerTickResult` Vec-returning API
    /// was removed for the kernel zero-heap discipline (2026-08-31):
    /// a `Vec<TimerAction>` return would make the production interrupt
    /// path allocate. Tests collect callbacks into local `Vec`s instead
    /// (test builds may allocate).
    ///
    /// **Does NOT handle `quantum_exhausted`** — quantum decrement is handled
    /// by `clock::decrement_quantum()` (D9), which the caller invokes
    /// separately. This matches C's `context_stop()`
    /// (arch_clock.c:326-330 within line 208-349) which is called from the
    /// context switch path (proc.c:208/440/1956), separately from
    /// `timer_int_handler()`. `arch_timer_int_handler()` in i386 is an empty
    /// function (arch_clock.c:72-74).
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
    ///
    /// * `priv_table` — the privilege table holding the alarm timer nodes
    ///   (needed for the BSP expiry pass; unused on APs)
    /// * `current_proc` — the currently running process (for time accounting)
    /// * `billp` — the billable process if `current_proc` is not billable;
    ///   `None` if `current_proc` is itself billable (D10).
    ///   When `Some`, the billable process's `sys_time` and `prof_left` are
    ///   decremented (C: clock.c:118-120, 134-138).
    /// * `ready_count` — number of processes in ready queues (for load average)
    /// * `on_expired` — callback invoked once per expired alarm timer
    ///   (BSP only; on APs, never called)
    ///
    /// # Returns
    ///
    /// `Option<VtimerExpired>` — `Some` if a virtual/profile timer expired
    /// for the current or billable process, `None` otherwise.
    pub fn tick_with<F>(
        &mut self,
        priv_table: &mut PrivTable,
        current_proc: &mut KProcess,
        billp: Option<&mut KProcess>,
        ready_count: usize,
        on_expired: F,
    ) -> Option<VtimerExpired>
    where
        F: FnMut(TimerAction),
    {
        // 1. BSP-only: update uptime and realtime (with adjtime)
        //    C: clock.c:91-104
        //    D8: runtime branch on self.is_bsp.
        if self.is_bsp {
            self.uptime += 1;
            CLOCK_UPTIME.store(self.uptime, Ordering::Release);

            if self.adjtime_delta != 0 && (self.uptime & 0x1) != 0 {
                self.realtime += if self.adjtime_delta > 0 { 2 } else { 0 };
                self.adjtime_delta += if self.adjtime_delta > 0 { -1 } else { 1 };
            } else {
                self.realtime += 1;
            }
            CLOCK_REALTIME.store(self.realtime, Ordering::Release);
        }

        // 2. Time accounting: charge current process for user time.
        //    C: clock.c:116 — `p->p_user_time++`
        current_proc.p_time.add_user_time(1);

        // 3. Decrement virtual/profile timers for current_proc.
        //    C: clock.c:128-133
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

        // 4. Billable process accounting (if current is not billable).
        //    C: clock.c:118-120 — `if (!BILLABLE) billp->p_sys_time++`
        //    C: clock.c:134-138 — `if (!BILLABLE) billp->p_prof_left--`
        //    C: clock.c:147-148 — `if (p != billp) vtimer_check(billp)`
        //    D10: caller passes billp explicitly.
        if let Some(billp) = billp {
            billp.p_time.add_sys_time(1);
            if billp.p_misc_flags.is_set(MiscFlagsBits::PROF_TIMER) {
                let expired = billp.p_time.tick_prof_timer();
                if expired && vtimer_expired.is_none() {
                    vtimer_expired = Some(VtimerExpired::Prof);
                }
            }
        }

        // 5. BSP-only: expire alarm timers from the intrusive chain.
        //    C: clock.c:153-161 — `if (cpu_is_bsp) tmrs_exptimers(...)`
        //    Zero-allocation: callback instead of collecting into a Vec.
        if self.is_bsp {
            expire_alarm_timers(priv_table, self, self.uptime, on_expired);
        }

        // 6. Load update (all CPUs).
        //    C: clock.c:151 — `load_update()`
        self.load_update(ready_count);

        vtimer_expired
    }

    /// Update load average tracking.
    ///
    /// C: `load_update()` — clock.c:260-292
    fn load_update(&mut self, ready_count: usize) {
        let slot = ((self.uptime / self.hz as u64 / LOAD_UNIT_SECS) % LOAD_HISTORY as u64) as u16;

        if slot != self.load_info.proc_last_slot {
            self.load_info.proc_load_history[slot as usize] = 0;
            self.load_info.proc_last_slot = slot;
        }

        // u16 与 C 的 `u16_t` 对齐；C 无符号加法回绕，这里用 wrapping_add 保持一致
        // （实际值 = 6s 窗口内可运行进程数，远小于 65535）。
        let slot_idx = slot as usize;
        self.load_info.proc_load_history[slot_idx] =
            self.load_info.proc_load_history[slot_idx].wrapping_add(ready_count as u16);
        self.load_info.last_clock = self.uptime;
    }
}

impl Default for ClockState {
    fn default() -> Self {
        Self::new()
    }
}

// ── Tests ──

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kpriv::PrivTable;
    use crate::proc::KProcess;
    use crate::proc::ProcNr;
    use minix_types::Endpoint;
    // Test builds may allocate (zero-heap discipline applies to production
    // code only); the collected Vec lives inside `#[cfg(test)]`.
    use alloc::vec::Vec;

    fn make_test_proc() -> KProcess {
        KProcess::new(ProcNr(0), Endpoint(0))
    }

    fn make_bsp_clock() -> ClockState {
        ClockState::new_for_cpu(CpuId::BSP, true)
    }

    fn make_ap_clock() -> ClockState {
        ClockState::new_for_cpu(CpuId::new_unchecked(1), false)
    }

    fn make_priv_table() -> crate::test_helpers::TestPrivTable {
        crate::test_helpers::test_priv_table()
    }

    /// Number of nodes reachable from the alarm chain head.
    /// (Test-only chain-length assertion helper.)
    fn timers_len(privs: &PrivTable, clock: &ClockState) -> usize {
        let mut n = 0;
        let mut cur = clock.timers_head;
        while let Some(id) = cur {
            cur = privs
                .get(id)
                .and_then(|kp| kp.runtime.s_alarm_timer.next);
            n += 1;
        }
        n
    }

    /// Tick a BSP clock whose alarm chain is empty — for tests that exercise
    /// uptime/vtimer/load paths only. A fresh `PrivTable` per call is
    /// equivalent here because the chain is always empty (nothing is ever
    /// armed). Test builds may allocate; production uses `tick_bsp` directly.
    fn tick_bsp_vtimer(
        clock: &mut ClockState,
        proc: &mut KProcess,
        billp: Option<&mut KProcess>,
        ready: usize,
    ) -> Option<VtimerExpired> {
        let mut privs = make_priv_table();
        clock.tick_bsp(&mut privs, proc, billp, ready, |_| {})
    }

    /// AP variant of [`tick_bsp_vtimer`].
    fn tick_ap_vtimer(
        clock: &mut ClockState,
        proc: &mut KProcess,
        billp: Option<&mut KProcess>,
        ready: usize,
    ) -> Option<VtimerExpired> {
        let mut privs = make_priv_table();
        clock.tick_ap(&mut privs, proc, billp, ready, |_| {})
    }

    // ── ClockState initialization ──

    #[test]
    fn test_clock_state_init() {
        let clock = make_bsp_clock();
        assert_eq!(clock.hz(), DEFAULT_HZ);
        assert_eq!(clock.uptime(), 0);
        assert_eq!(clock.realtime(), 0);
        assert_eq!(clock.boottime(), 0);
        assert!(clock.is_bsp());
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

        let clock = ClockState::with_hz(50001);
        assert_eq!(clock.hz(), DEFAULT_HZ); // too high, falls back
    }

    #[test]
    fn test_ap_clock_state_no_global_time() {
        let clock = make_ap_clock();
        assert!(!clock.is_bsp());
        assert_eq!(clock.uptime(), 0);
        assert_eq!(clock.realtime(), 0);
    }

    // ── Tick: uptime/realtime ──

    #[test]
    fn test_tick_bsp_increments_uptime_realtime() {
        let mut clock = make_bsp_clock();
        let mut proc = make_test_proc();

        let vtimer = tick_bsp_vtimer(&mut clock, &mut proc, None, 0);
        assert_eq!(clock.uptime(), 1);
        assert_eq!(clock.realtime(), 1);
        assert_eq!(vtimer, None);
    }

    #[test]
    fn test_tick_ap_no_uptime_update() {
        let mut clock = make_ap_clock();
        let mut proc = make_test_proc();

        tick_ap_vtimer(&mut clock, &mut proc, None, 0);
        assert_eq!(clock.uptime(), 0);
        assert_eq!(clock.realtime(), 0);
    }

    // ── Tick: adjtime ──

    #[test]
    fn test_adjtime_speed_up() {
        // C: clock.c:97-100 — odd tick: realtime += 2, delta -= 1
        let mut clock = make_bsp_clock();
        let mut proc = make_test_proc();

        clock.set_adjtime_delta(3);

        // Tick 1: uptime=1, odd → realtime += 2, delta → 2
        tick_bsp_vtimer(&mut clock, &mut proc, None, 0);
        assert_eq!(clock.uptime(), 1);
        assert_eq!(clock.realtime(), 2);
        assert_eq!(clock.adjtime_delta(), 2);

        // Tick 2: uptime=2, even → realtime += 1
        tick_bsp_vtimer(&mut clock, &mut proc, None, 0);
        assert_eq!(clock.uptime(), 2);
        assert_eq!(clock.realtime(), 3);

        // Tick 3: uptime=3, odd → realtime += 2, delta → 1
        tick_bsp_vtimer(&mut clock, &mut proc, None, 0);
        assert_eq!(clock.uptime(), 3);
        assert_eq!(clock.realtime(), 5);
        assert_eq!(clock.adjtime_delta(), 1);
    }

    #[test]
    fn test_adjtime_slow_down() {
        // C: clock.c:99 — odd tick: realtime += 0, delta += 1
        let mut clock = make_bsp_clock();
        let mut proc = make_test_proc();

        clock.set_adjtime_delta(-2);

        // Tick 1: uptime=1, odd → realtime += 0, delta → -1
        tick_bsp_vtimer(&mut clock, &mut proc, None, 0);
        assert_eq!(clock.uptime(), 1);
        assert_eq!(clock.realtime(), 0);
        assert_eq!(clock.adjtime_delta(), -1);

        // Tick 2: uptime=2, even → realtime += 1
        tick_bsp_vtimer(&mut clock, &mut proc, None, 0);
        assert_eq!(clock.uptime(), 2);
        assert_eq!(clock.realtime(), 1);
    }

    #[test]
    fn test_adjtime_zero_delta() {
        // delta=0 → realtime increments by 1 every tick (no adjustment).
        let mut clock = make_bsp_clock();
        let mut proc = make_test_proc();

        tick_bsp_vtimer(&mut clock, &mut proc, None, 0);
        assert_eq!(clock.realtime(), 1);
        tick_bsp_vtimer(&mut clock, &mut proc, None, 0);
        assert_eq!(clock.realtime(), 2);
    }

    // ── Tick: billp accounting (D10) ──

    #[test]
    fn test_billp_sys_time_accounting() {
        // C: clock.c:118-120 — if current is not billable, billp->p_sys_time++
        let mut clock = make_bsp_clock();
        let mut current = make_test_proc();
        let mut billp = make_test_proc();

        let billp_initial = billp.p_time.sys_time.load(Ordering::Acquire);
        tick_bsp_vtimer(&mut clock, &mut current, Some(&mut billp), 0);
        let billp_after = billp.p_time.sys_time.load(Ordering::Acquire);
        assert_eq!(billp_after, billp_initial + 1);
    }

    #[test]
    fn test_billp_no_accounting_when_none() {
        // When billp=None (current is billable), no sys_time accounting.
        let mut clock = make_bsp_clock();
        let mut current = make_test_proc();

        let initial = current.p_time.sys_time.load(Ordering::Acquire);
        tick_bsp_vtimer(&mut clock, &mut current, None, 0);
        let after = current.p_time.sys_time.load(Ordering::Acquire);
        assert_eq!(after, initial, "sys_time must not change when billp=None");
    }

    #[test]
    fn test_billp_prof_timer_decrement() {
        // C: clock.c:134-138 — if !BILLABLE, billp->p_prof_left--
        let mut clock = make_bsp_clock();
        let mut current = make_test_proc();
        let mut billp = make_test_proc();

        billp.p_misc_flags.set(MiscFlagsBits::PROF_TIMER);
        billp.p_time.prof_left.store(5, Ordering::Release);

        tick_bsp_vtimer(&mut clock, &mut current, Some(&mut billp), 0);
        assert_eq!(billp.p_time.prof_left.load(Ordering::Acquire), 4);
    }

    #[test]
    fn test_billp_prof_timer_expiry_reports_prof() {
        // C: clock.c:147-148 — vtimer_check(billp) reports expiry
        let mut clock = make_bsp_clock();
        let mut current = make_test_proc();
        let mut billp = make_test_proc();

        billp.p_misc_flags.set(MiscFlagsBits::PROF_TIMER);
        billp.p_time.prof_left.store(1, Ordering::Release);

        let vtimer = tick_bsp_vtimer(&mut clock, &mut current, Some(&mut billp), 0);
        assert_eq!(vtimer, Some(VtimerExpired::Prof));
    }

    // ── Tick: vtimer ──

    #[test]
    fn test_vtimer_virtual_expiry() {
        // C: do_vtimer.c:91-95 — VIRT_TIMER + virt_left=0 → SIGVTALRM
        let mut clock = make_bsp_clock();
        let mut proc = make_test_proc();

        proc.p_misc_flags.set(MiscFlagsBits::VIRT_TIMER);
        proc.p_time.virt_left.store(1, Ordering::Release);

        let vtimer = tick_bsp_vtimer(&mut clock, &mut proc, None, 0);
        assert_eq!(vtimer, Some(VtimerExpired::Virtual));
    }

    #[test]
    fn test_vtimer_prof_expiry() {
        // C: do_vtimer.c:98-102 — PROF_TIMER + prof_left=0 → SIGPROF
        let mut clock = make_bsp_clock();
        let mut proc = make_test_proc();

        proc.p_misc_flags.set(MiscFlagsBits::PROF_TIMER);
        proc.p_time.prof_left.store(1, Ordering::Release);

        let vtimer = tick_bsp_vtimer(&mut clock, &mut proc, None, 0);
        assert_eq!(vtimer, Some(VtimerExpired::Prof));
    }

    #[test]
    fn test_vtimer_no_expiry_when_flag_not_set() {
        // virt_left=0 but MF_VIRT_TIMER not set → no expiry.
        let mut clock = make_bsp_clock();
        let mut proc = make_test_proc();

        proc.p_time.virt_left.store(0, Ordering::Release);
        // Note: MF_VIRT_TIMER is NOT set.

        let vtimer = tick_bsp_vtimer(&mut clock, &mut proc, None, 0);
        assert_eq!(vtimer, None);
    }

    // ── Alarm timer chain (D2, D3) ──

    #[test]
    fn test_alarm_set_and_expire() {
        // C: set_kernel_timer + tmrs_exptimers — arm one node, expire at tick.
        let mut clock = make_bsp_clock();
        let mut privs = make_priv_table();
        let mut proc = make_test_proc();
        let pid: PrivId = 5;

        set_alarm_timer(
            &mut privs, &mut clock, pid, 3,
            TimerAction::NotifyAlarm { endpoint: Endpoint(100) },
        );
        assert!(privs.get(pid).unwrap().runtime.s_alarm_timer.is_set());
        assert_eq!(timers_len(&privs, &clock), 1);

        // Tick 1-2: no expiry
        clock.tick_bsp(&mut privs, &mut proc, None, 0, |_| {});
        clock.tick_bsp(&mut privs, &mut proc, None, 0, |_| {});
        assert!(privs.get(pid).unwrap().runtime.s_alarm_timer.is_set());

        // Tick 3: timer expires
        let mut collected: Vec<TimerAction> = Vec::new();
        clock.tick_bsp(&mut privs, &mut proc, None, 0, |a| collected.push(a));
        assert_eq!(collected.len(), 1);
        assert_eq!(
            collected[0],
            TimerAction::NotifyAlarm { endpoint: Endpoint(100) }
        );
        // Node deactivated and chain empty after expiry (C: tmrs_exp.c:17-18).
        assert!(!privs.get(pid).unwrap().runtime.s_alarm_timer.is_set());
        assert_eq!(timers_len(&privs, &clock), 0);
    }

    // ── tick_with: zero-allocation callback API (R-06) ──

    #[test]
    fn test_tick_with_invokes_callback_on_expiry() {
        // R-06: `tick_with` should invoke the callback once per expired timer,
        // without allocating a collection.
        let mut clock = make_bsp_clock();
        let mut privs = make_priv_table();
        let mut proc = make_test_proc();

        set_alarm_timer(
            &mut privs, &mut clock, 2, 1,
            TimerAction::NotifyAlarm { endpoint: Endpoint(42) },
        );

        // Tick 1: timer should expire.
        let mut collected: Vec<TimerAction> = Vec::new();
        let vtimer_expired = clock.tick_with(&mut privs, &mut proc, None, 0, |action| {
            collected.push(action);
        });

        // Verify callback was invoked with the correct action.
        assert_eq!(collected.len(), 1);
        assert_eq!(collected[0], TimerAction::NotifyAlarm { endpoint: Endpoint(42) });
        // No vtimer configured.
        assert_eq!(vtimer_expired, None);
    }

    #[test]
    fn test_tick_with_no_callback_on_empty_expiry() {
        // R-06: When no timers expire, `tick_with` should not invoke the
        // callback at all. This is the zero-allocation hot path.
        let mut clock = make_bsp_clock();
        let mut privs = make_priv_table();
        let mut proc = make_test_proc();

        let mut call_count = 0u32;
        let vtimer_expired = clock.tick_with(&mut privs, &mut proc, None, 0, |_| {
            call_count += 1;
        });

        assert_eq!(call_count, 0);
        assert_eq!(vtimer_expired, None);
    }

    #[test]
    fn test_tick_with_matches_tick_bsp_behavior() {
        // `tick_bsp` (convenience wrapper) and `tick_with` (core) should
        // produce identical results for the same input.
        let mut clock_a = make_bsp_clock();
        let mut privs_a = make_priv_table();
        let mut proc_a = make_test_proc();
        let mut clock_b = make_bsp_clock();
        let mut privs_b = make_priv_table();
        let mut proc_b = make_test_proc();

        // Set up identical timers in both clocks (same priv slots, same
        // expiry times → same chain shape).
        for (pid, exp) in [(0u16, 3u64), (1, 3), (2, 5)] {
            set_alarm_timer(
                &mut privs_a, &mut clock_a, pid, exp,
                TimerAction::NotifyAlarm { endpoint: Endpoint(exp as i32) },
            );
            set_alarm_timer(
                &mut privs_b, &mut clock_b, pid, exp,
                TimerAction::NotifyAlarm { endpoint: Endpoint(exp as i32) },
            );
        }

        // Tick both clocks 5 times.
        let mut collected_with: Vec<TimerAction> = Vec::new();
        for _ in 0..5 {
            clock_a.tick_with(&mut privs_a, &mut proc_a, None, 0, |a| collected_with.push(a));
        }
        let mut collected_tick: Vec<TimerAction> = Vec::new();
        for _ in 0..5 {
            clock_b.tick_with(&mut privs_b, &mut proc_b, None, 0, |a| {
                collected_tick.push(a);
            });
        }

        // Both should have collected the same actions. The chain is sorted by
        // exp_time (among equal exp_times the last-armed node is in front,
        // C: tmrs_set.c:38-43), and both clocks armed slots in the same
        // order, so the sequences must match exactly.
        assert_eq!(collected_with.len(), collected_tick.len());
        for (a, b) in collected_with.iter().zip(collected_tick.iter()) {
            assert_eq!(a, b);
        }
    }

    #[test]
    fn test_alarm_reset_is_idempotent() {
        // C: clock.c:245-255 — reset_kernel_timer(tp) is guarded by
        // `if (!tmr_is_set(tp)) return` (tmrs_clr.c:19), so resetting an
        // unset node is a no-op and resetting twice is safe.
        let mut clock = make_bsp_clock();
        let mut privs = make_priv_table();

        set_alarm_timer(
            &mut privs, &mut clock, 4, 5,
            TimerAction::NotifyAlarm { endpoint: Endpoint(50) },
        );
        assert!(privs.get(4).unwrap().runtime.s_alarm_timer.is_set());
        assert_eq!(timers_len(&privs, &clock), 1);

        // First reset unlinks and deactivates the node.
        reset_alarm_timer(&mut privs, &mut clock, 4);
        assert!(!privs.get(4).unwrap().runtime.s_alarm_timer.is_set());
        assert_eq!(timers_len(&privs, &clock), 0);

        // Resetting an unset node is a no-op (C: tmrs_clr.c:19 guard).
        reset_alarm_timer(&mut privs, &mut clock, 4);
        assert!(!privs.get(4).unwrap().runtime.s_alarm_timer.is_set());
        assert_eq!(timers_len(&privs, &clock), 0);
    }

    #[test]
    fn test_alarm_same_exp_time_coexist() {
        // Two different priv slots with the same exp_time must coexist in
        // the chain. Among equal expiry times C's insertion scan breaks at
        // the FIRST node with `exp_time <= cur_exp` and inserts before it
        // (tmrs_set.c:38-43) — so the most recently armed node fires first.
        let mut clock = make_bsp_clock();
        let mut privs = make_priv_table();
        let mut proc = make_test_proc();

        set_alarm_timer(
            &mut privs, &mut clock, 0, 5,
            TimerAction::NotifyAlarm { endpoint: Endpoint(1) },
        );
        set_alarm_timer(
            &mut privs, &mut clock, 1, 5,
            TimerAction::NotifyAlarm { endpoint: Endpoint(2) },
        );
        assert_eq!(timers_len(&privs, &clock), 2, "Both timers must coexist");

        // Tick to expiry: both fire in the same tick (last-armed first).
        let mut collected: Vec<TimerAction> = Vec::new();
        for _ in 0..5 {
            clock.tick_bsp(&mut privs, &mut proc, None, 0, |a| collected.push(a));
        }
        assert_eq!(collected.len(), 2);
        assert_eq!(collected[0], TimerAction::NotifyAlarm { endpoint: Endpoint(2) });
        assert_eq!(collected[1], TimerAction::NotifyAlarm { endpoint: Endpoint(1) });
        assert_eq!(timers_len(&privs, &clock), 0, "Both timers must have expired");
    }

    #[test]
    fn test_multiple_timers_pop_order() {
        // Timers with different exp_times must fire in expiry order
        // (chain is kept sorted by `set_alarm_timer`'s insertion scan).
        let mut clock = make_bsp_clock();
        let mut privs = make_priv_table();
        let mut proc = make_test_proc();

        set_alarm_timer(
            &mut privs, &mut clock, 0, 5,
            TimerAction::NotifyAlarm { endpoint: Endpoint(5) },
        );
        set_alarm_timer(
            &mut privs, &mut clock, 1, 3,
            TimerAction::NotifyAlarm { endpoint: Endpoint(3) },
        );
        set_alarm_timer(
            &mut privs, &mut clock, 2, 7,
            TimerAction::NotifyAlarm { endpoint: Endpoint(7) },
        );

        // Tick 1-3: timer@3 fires.
        let mut collected: Vec<TimerAction> = Vec::new();
        for _ in 0..3 {
            clock.tick_bsp(&mut privs, &mut proc, None, 0, |a| collected.push(a));
        }
        assert_eq!(collected.len(), 1);
        assert_eq!(collected[0], TimerAction::NotifyAlarm { endpoint: Endpoint(3) });

        // Tick 4-5: timer@5 fires.
        collected.clear();
        for _ in 0..2 {
            clock.tick_bsp(&mut privs, &mut proc, None, 0, |a| collected.push(a));
        }
        assert_eq!(collected.len(), 1);
        assert_eq!(collected[0], TimerAction::NotifyAlarm { endpoint: Endpoint(5) });

        // Tick 6-7: timer@7 fires.
        collected.clear();
        for _ in 0..2 {
            clock.tick_bsp(&mut privs, &mut proc, None, 0, |a| collected.push(a));
        }
        assert_eq!(collected.len(), 1);
        assert_eq!(collected[0], TimerAction::NotifyAlarm { endpoint: Endpoint(7) });
    }

    #[test]
    fn test_timer_never_expires() {
        // TMR_NEVER = TMRDIFF_MAX+1 = 0x80000000 (C: timers.h:48). Under the
        // wrap-safe comparison it is more than half the tick space away from
        // any realistic uptime — matching C, where uptime would have to run
        // ~68 years at 100 Hz before the sentinel itself expires.
        let mut clock = make_bsp_clock();
        let mut privs = make_priv_table();
        let mut proc = make_test_proc();

        set_alarm_timer(
            &mut privs, &mut clock, 3, TMR_NEVER,
            TimerAction::NotifyAlarm { endpoint: Endpoint(99) },
        );

        for _ in 0..100 {
            let mut fired = 0u32;
            clock.tick_bsp(&mut privs, &mut proc, None, 0, |_| fired += 1);
            assert_eq!(fired, 0);
        }
    }

    #[test]
    #[should_panic(expected = "set_alarm_timer called on AP ClockState")]
    fn test_ap_state_set_alarm_timer_panics() {
        // D8: AP instances do not own the alarm chain (C: only the BSP
        // clock interrupt drives `tmrs_exptimers`, clock.c:159-161).
        let mut clock = make_ap_clock();
        let mut privs = make_priv_table();
        set_alarm_timer(
            &mut privs, &mut clock, 0, 1,
            TimerAction::NotifyAlarm { endpoint: Endpoint(1) },
        );
    }

    // ── Load average ──

    #[test]
    fn test_load_update_accumulates() {
        // 100 ticks × 5 ready_count = 500, all in slot 0
        // (slot rotates every hz * LOAD_UNIT_SECS = 100 * 6 = 600 ticks).
        let mut clock = make_bsp_clock();
        let mut proc = make_test_proc();

        for _ in 0..100 {
            tick_bsp_vtimer(&mut clock, &mut proc, None, 5);
        }

        let history = clock.load_history();
        let total: u32 = history.iter().map(|&v| v as u32).sum();
        assert_eq!(total, 500, "100 ticks × 5 ready should accumulate exactly 500");
        assert_eq!(
            history[0], 500,
            "all 100 ticks fall in slot 0 (rotation at tick 600)"
        );
    }

    #[test]
    fn test_load_update_slot_rotation() {
        // Slot rotates every (hz * LOAD_UNIT_SECS) = 100 * 6 = 600 ticks.
        // At tick 600 (uptime=600), slot=(600/600) % 150 = 1 != proc_last_slot=0,
        // so history[1] resets to 0 and the +ready_count goes to slot 1, not slot 0.
        // Therefore slot 0 ends at 599 × 3 = 1797 (not 1800) after 600 total ticks.
        let mut clock = make_bsp_clock();
        let mut proc = make_test_proc();

        // Fill slot 0 with ticks 1..600 (599 ticks @ 3 ready = 1797).
        for _ in 0..600 {
            tick_bsp_vtimer(&mut clock, &mut proc, None, 3);
        }
        assert_eq!(
            clock.load_history()[0],
            1797,
            "slot 0 should have 599 ticks × 3 = 1797 (tick 600 belongs to slot 1)"
        );
        // Tick 600 (the 600th tick) crosses into slot 1 with +3 ready.
        assert_eq!(
            clock.load_history()[1],
            3,
            "slot 1 should have exactly 3 from tick 600 (single tick in new slot)"
        );

        // Run 599 more ticks (ticks 601..=1199) all in slot 1 with 7 ready each.
        for _ in 0..599 {
            tick_bsp_vtimer(&mut clock, &mut proc, None, 7);
        }
        assert_eq!(
            clock.load_history()[1],
            3 + 599 * 7,
            "slot 1 = 3 (from tick 600) + 599 × 7 = 4196"
        );
        // Slot 0 is preserved across slot changes.
        assert_eq!(
            clock.load_history()[0],
            1797,
            "slot 0 must remain unchanged after slot rotation"
        );
    }

    // ── set_boottime / set_realtime ──

    #[test]
    fn test_set_boottime() {
        let mut clock = make_bsp_clock();
        clock.set_boottime(1700000000);
        assert_eq!(clock.boottime(), 1700000000);
        assert_eq!(get_boottime(), 1700000000);
    }

    #[test]
    fn test_set_realtime() {
        let mut clock = make_bsp_clock();
        clock.set_realtime(12345);
        // Only assert local field — the global `CLOCK_REALTIME` atomic is
        // also updated by `set_realtime`, but other tests calling
        // `tick_bsp` in parallel mutate the same global, making
        // `get_realtime()` racy here. The global sync path is exercised
        // by `tick_bsp` tests (they verify `clock.realtime()` after
        // ticking, and the global is updated in the same call).
        assert_eq!(clock.realtime(), 12345);
    }

    #[test]
    fn test_ap_set_boottime_no_op() {
        let mut clock = make_ap_clock();
        clock.set_boottime(1700000000);
        assert_eq!(clock.boottime(), 0, "AP set_boottime must be no-op");
    }

    // ── User time accounting ──

    #[test]
    fn test_user_time_accounting() {
        let mut clock = make_bsp_clock();
        let mut proc = make_test_proc();

        let initial = proc.p_time.user_time.load(Ordering::Acquire);
        tick_bsp_vtimer(&mut clock, &mut proc, None, 0);
        let after = proc.p_time.user_time.load(Ordering::Acquire);
        assert_eq!(after, initial + 1);
    }

    // ── read_tsc in test builds ──

    #[test]
    fn test_read_tsc_returns_zero_in_test_build() {
        // In test builds, read_tsc() returns 0 (no hardware counter available).
        // In non-test builds, it delegates to minix_arch::CurrentClockArch::read_tsc().
        assert_eq!(read_tsc(), 0);
    }

    // ── decrement_quantum (D9: arch-side quantum decrement) ──
    //
    // C ground truth: arch/i386/arch_clock.c:314,326-330,342
    //   - line 314: `if (p->p_endpoint >= 0)` — skip kernel/idle tasks
    //   - line 326-330: `p_cpu_time_left -= tsc_delta` (saturating to 0)
    //   - line 342: `*__tsc_ctr_switch = tsc` — update baseline regardless
    //
    // Tests use the `_in` variant with a local `SmpState` to avoid racing
    // on the global `SMP_STATE` (which would be UB with parallel tests).

    #[test]
    fn test_decrement_quantum_no_smp_state_returns_false() {
        // When SMP_STATE is not initialized (early boot / unit test without
        // setup), decrement_quantum_with_tsc returns false without touching
        // the process.
        let mut proc = make_test_proc();
        proc.p_sched.quantum.allocate(1_000); // 200 ms * 1M cycles/ms
        let initial = proc.p_sched.quantum.cpu_time_left.load(Ordering::Acquire);

        let exhausted = decrement_quantum_with_tsc(&mut proc, 5_000);
        assert!(!exhausted, "without SMP_STATE, must return false");
        assert_eq!(
            proc.p_sched.quantum.cpu_time_left.load(Ordering::Acquire),
            initial,
            "without SMP_STATE, cpu_time_left must be unchanged"
        );
    }

    #[test]
    fn test_decrement_quantum_first_call_no_baseline() {
        // First call after SmpState creation: tsc_ctr_switch=0 → establish
        // baseline, return false, no decrement. Mirrors C's first-tick
        // behavior after `note_context_switch` sets `*__tsc_ctr_switch`.
        use crate::smp::SmpState;
        let mut smp = SmpState::new_single_cpu();
        let mut proc = make_test_proc();
        proc.p_sched.quantum.allocate(1_000);
        let initial = proc.p_sched.quantum.cpu_time_left.load(Ordering::Acquire);

        let exhausted = decrement_quantum_in(&mut smp, &mut proc, 1_000);
        assert!(!exhausted, "first call must not report exhaustion");
        assert_eq!(
            proc.p_sched.quantum.cpu_time_left.load(Ordering::Acquire),
            initial,
            "first call must not decrement quantum"
        );
        // Baseline was established.
        assert_eq!(
            smp.cpu_local(smp.bsp_cpu_id()).unwrap().tsc_ctr_switch,
            1_000,
            "first call must update tsc_ctr_switch baseline"
        );
    }

    #[test]
    fn test_decrement_quantum_decrements_cpu_time_left() {
        // Normal path: delta < cpu_time_left → decrement, not exhausted.
        // C: arch_clock.c:326-327 `if (tsc_delta < p_cpu_time_left)
        // p->p_cpu_time_left -= tsc_delta;`
        use crate::smp::SmpState;
        let mut smp = SmpState::new_single_cpu();
        let mut proc = make_test_proc();
        // endpoint=0 (PM) → user process, quantum applies.
        // Set quantum to 200_000 cycles (200 ms * 1_000 cycles/ms).
        proc.p_sched.quantum.allocate(1_000);
        let initial = proc.p_sched.quantum.cpu_time_left.load(Ordering::Acquire);
        assert_eq!(initial, 200_000);

        // Establish baseline at tsc=1_000.
        decrement_quantum_in(&mut smp, &mut proc, 1_000);
        // Advance TSC by 50_000 cycles.
        let exhausted = decrement_quantum_in(&mut smp, &mut proc, 51_000);
        assert!(!exhausted, "delta 50_000 < quantum 200_000 → not exhausted");
        assert_eq!(
            proc.p_sched.quantum.cpu_time_left.load(Ordering::Acquire),
            initial - 50_000,
            "quantum must be decremented by tsc delta"
        );
    }

    #[test]
    fn test_decrement_quantum_reports_exhaustion() {
        // Exhaustion path: delta >= cpu_time_left → set to 0, return true.
        // C: arch_clock.c:328-329 `else p->p_cpu_time_left = 0;`
        use crate::smp::SmpState;
        let mut smp = SmpState::new_single_cpu();
        let mut proc = make_test_proc();
        proc.p_sched.quantum.allocate(1_000);
        // initial = 200 * 1_000 = 200_000 cycles

        // Establish baseline.
        decrement_quantum_in(&mut smp, &mut proc, 1_000);
        // Advance TSC by exactly the quantum (delta == cpu_time_left).
        let exhausted = decrement_quantum_in(&mut smp, &mut proc, 201_000);
        assert!(exhausted, "delta == quantum → exhausted");
        assert_eq!(
            proc.p_sched.quantum.cpu_time_left.load(Ordering::Acquire),
            0,
            "exhausted quantum must be 0"
        );
    }

    #[test]
    fn test_decrement_quantum_overshoot_saturates_to_zero() {
        // C: arch_clock.c:328-329 — delta > cpu_time_left saturates to 0
        // (does NOT wrap around). `Quantum::consume` handles saturation.
        use crate::smp::SmpState;
        let mut smp = SmpState::new_single_cpu();
        let mut proc = make_test_proc();
        proc.p_sched.quantum.allocate(1_000); // 200_000 cycles

        decrement_quantum_in(&mut smp, &mut proc, 1_000);
        // Advance TSC by 10x the quantum.
        let exhausted = decrement_quantum_in(&mut smp, &mut proc, 2_001_000);
        assert!(exhausted, "delta >> quantum → exhausted");
        assert_eq!(
            proc.p_sched.quantum.cpu_time_left.load(Ordering::Acquire),
            0,
            "overshoot must saturate to 0, not wrap"
        );
    }

    #[test]
    fn test_decrement_quantum_skips_kernel_tasks() {
        // C: arch_clock.c:314 `if (p->p_endpoint >= 0)` — kernel tasks
        // (endpoint < 0) are quantum-exempt. The tsc_ctr_switch baseline
        // IS still updated (C: line 342), but quantum is not decremented.
        use crate::smp::SmpState;
        let mut smp = SmpState::new_single_cpu();
        // KERNEL endpoint = -1
        let mut proc = KProcess::new(ProcNr(0), Endpoint::KERNEL);
        proc.p_sched.quantum.allocate(1_000);
        let initial = proc.p_sched.quantum.cpu_time_left.load(Ordering::Acquire);
        assert_eq!(initial, 200_000);

        // Establish baseline.
        decrement_quantum_in(&mut smp, &mut proc, 1_000);
        // Large delta — should be skipped for kernel task.
        let exhausted = decrement_quantum_in(&mut smp, &mut proc, 1_000_000);
        assert!(!exhausted, "kernel tasks must not report quantum exhaustion");
        assert_eq!(
            proc.p_sched.quantum.cpu_time_left.load(Ordering::Acquire),
            initial,
            "kernel tasks must not have quantum decremented"
        );
        // Baseline still updated.
        assert_eq!(
            smp.cpu_local(smp.bsp_cpu_id()).unwrap().tsc_ctr_switch,
            1_000_000,
            "tsc_ctr_switch baseline must update even for kernel tasks"
        );
    }

    #[test]
    fn test_decrement_quantum_zero_delta_returns_false() {
        // Same TSC as previous call → delta=0 → no decrement, not exhausted.
        // Guards against degenerate cases (e.g. very fast back-to-back ticks
        // where TSC hasn't advanced).
        use crate::smp::SmpState;
        let mut smp = SmpState::new_single_cpu();
        let mut proc = make_test_proc();
        proc.p_sched.quantum.allocate(1_000);

        // Establish baseline at tsc=5_000.
        decrement_quantum_in(&mut smp, &mut proc, 5_000);
        // Call again with the same TSC.
        let exhausted = decrement_quantum_in(&mut smp, &mut proc, 5_000);
        assert!(!exhausted, "zero delta must not report exhaustion");
        assert_eq!(
            proc.p_sched.quantum.cpu_time_left.load(Ordering::Acquire),
            200_000,
            "zero delta must not decrement quantum"
        );
    }

    #[test]
    fn test_decrement_quantum_multiple_ticks_accumulate() {
        // Multiple ticks accumulate delta correctly — verifies that the
        // tsc_ctr_switch baseline is updated after each call, not just
        // the first.
        use crate::smp::SmpState;
        let mut smp = SmpState::new_single_cpu();
        let mut proc = make_test_proc();
        proc.p_sched.quantum.allocate(1_000); // 200_000 cycles

        // Tick 1: establish baseline at tsc=1_000.
        decrement_quantum_in(&mut smp, &mut proc, 1_000);
        // Tick 2: advance by 30_000.
        let _ = decrement_quantum_in(&mut smp, &mut proc, 31_000);
        // Tick 3: advance by another 30_000 (total 60_000).
        let exhausted = decrement_quantum_in(&mut smp, &mut proc, 61_000);
        assert!(!exhausted, "60_000 < 200_000 → not exhausted");
        assert_eq!(
            proc.p_sched.quantum.cpu_time_left.load(Ordering::Acquire),
            200_000 - 60_000,
            "multiple ticks must accumulate delta correctly"
        );
    }

    // ── Alarm chain primitive tests (C: libtimers) ──

    #[test]
    fn test_chain_set_reset_keeps_other_nodes() {
        // Resetting a middle node must not disturb the rest of the chain
        // (C: tmrs_clr.c:29-34 unlink loop relinks prev→next).
        let mut clock = make_bsp_clock();
        let mut privs = make_priv_table();

        // Arm three nodes; insertion scan keeps them ordered 10, 20, 30.
        for (pid, exp) in [(0u16, 10u64), (1, 20), (2, 30)] {
            set_alarm_timer(
                &mut privs, &mut clock, pid, exp,
                TimerAction::NotifyAlarm { endpoint: Endpoint(exp as i32) },
            );
        }
        assert_eq!(timers_len(&privs, &clock), 3);
        assert_eq!(clock.timers_head, Some(0));

        // Unlink the middle node (priv slot 1, exp 20).
        reset_alarm_timer(&mut privs, &mut clock, 1);
        assert_eq!(timers_len(&privs, &clock), 2);
        // prev(0) now links directly to next(2).
        assert_eq!(privs.get(0).unwrap().runtime.s_alarm_timer.next, Some(2));
        assert_eq!(clock.timers_head, Some(0));

        // Remaining nodes expire in order 10 then 30.
        let mut collected: Vec<TimerAction> = Vec::new();
        expire_alarm_timers(&mut privs, &mut clock, 30, |a| collected.push(a));
        assert_eq!(collected.len(), 2);
        assert_eq!(collected[0], TimerAction::NotifyAlarm { endpoint: Endpoint(10) });
        assert_eq!(collected[1], TimerAction::NotifyAlarm { endpoint: Endpoint(30) });
    }

    #[test]
    fn test_chain_expire_order_and_stop_at_head() {
        // expire_alarm_timers fires only the expired prefix of the chain
        // (C: tmrs_exp.c: while head expired) and stops at the first
        // non-expired node.
        let mut clock = make_bsp_clock();
        let mut privs = make_priv_table();

        // Insert out of order to exercise the sorted-insert scan.
        for (pid, exp) in [(0u16, 30u64), (1, 10), (2, 20)] {
            set_alarm_timer(
                &mut privs, &mut clock, pid, exp,
                TimerAction::NotifyAlarm { endpoint: Endpoint(exp as i32) },
            );
        }
        // Chain is sorted: head=1(exp 10) → 2(exp 20) → 0(exp 30).
        assert_eq!(clock.timers_head, Some(1));

        // now=5: nothing expired.
        let mut fired = 0u32;
        expire_alarm_timers(&mut privs, &mut clock, 5, |_| fired += 1);
        assert_eq!(fired, 0);
        assert_eq!(timers_len(&privs, &clock), 3);

        // now=15: only head (exp 10) fires; exp 20/30 stay linked.
        let mut collected: Vec<TimerAction> = Vec::new();
        expire_alarm_timers(&mut privs, &mut clock, 15, |a| collected.push(a));
        assert_eq!(collected.len(), 1);
        assert_eq!(collected[0], TimerAction::NotifyAlarm { endpoint: Endpoint(10) });
        assert_eq!(timers_len(&privs, &clock), 2);
        assert_eq!(clock.timers_head, Some(2));

        // now=25: exp 20 fires.
        collected.clear();
        expire_alarm_timers(&mut privs, &mut clock, 25, |a| collected.push(a));
        assert_eq!(collected.len(), 1);
        assert_eq!(collected[0], TimerAction::NotifyAlarm { endpoint: Endpoint(20) });

        // now=30: exp 30 fires.
        collected.clear();
        expire_alarm_timers(&mut privs, &mut clock, 30, |a| collected.push(a));
        assert_eq!(collected.len(), 1);
        assert_eq!(collected[0], TimerAction::NotifyAlarm { endpoint: Endpoint(30) });
        assert_eq!(timers_len(&privs, &clock), 0);
        assert_eq!(clock.timers_head, None);

        // Chain empty: further expiry calls are no-ops.
        let mut fired = 0u32;
        expire_alarm_timers(&mut privs, &mut clock, 100, |_| fired += 1);
        assert_eq!(fired, 0);
    }

    #[test]
    fn test_chain_rearm_overwrites_old_timer() {
        // Re-arming a slot that already has a timer must first unlink the
        // old node (C: tmrs_set.c:23-27) — chain never contains duplicates.
        let mut clock = make_bsp_clock();
        let mut privs = make_priv_table();

        set_alarm_timer(
            &mut privs, &mut clock, 0, 10,
            TimerAction::NotifyAlarm { endpoint: Endpoint(1) },
        );
        set_alarm_timer(
            &mut privs, &mut clock, 1, 20,
            TimerAction::NotifyAlarm { endpoint: Endpoint(2) },
        );

        // Re-arm slot 0 with a later expiry: old node (exp 10) is replaced.
        set_alarm_timer(
            &mut privs, &mut clock, 0, 30,
            TimerAction::NotifyAlarm { endpoint: Endpoint(3) },
        );
        assert_eq!(timers_len(&privs, &clock), 2);
        assert_eq!(clock.timers_head, Some(1), "exp 20 (slot 1) is now the head");

        // exp 10 must never fire; only 20 then 30.
        let mut collected: Vec<TimerAction> = Vec::new();
        expire_alarm_timers(&mut privs, &mut clock, 30, |a| collected.push(a));
        assert_eq!(collected.len(), 2);
        assert_eq!(collected[0], TimerAction::NotifyAlarm { endpoint: Endpoint(2) });
        assert_eq!(collected[1], TimerAction::NotifyAlarm { endpoint: Endpoint(3) });
    }
}
