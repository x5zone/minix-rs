//! Clock system calls: times, setalarm, stime, settime, vtimer.
//!
//! # Minix3 C Source Mapping
//!
//! - `do_times.c` — SYS_TIMES
//! - `do_setalarm.c` — SYS_SETALARM
//! - `do_stime.c` — SYS_STIME
//! - `do_settime.c` — SYS_SETTIME
//! - `do_vtimer.c` — SYS_VTIMER
//!
//! # Design Decisions (21-syscall-clock.md §3)
//!
//! - **D3**: `VtimerType` enum for VT_VIRTUAL/VT_PROF.
//! - **D5**: ClockState + PrivTable + ProcessTable passed as parameters via
//!   `kernel_call_dispatch`, avoiding global state.

use core::sync::atomic::Ordering;

use minix_types::{
    Endpoint, Message,
    MessKrnLsysSysTimes, MessLsysKrnSysSetalarm,
    MessageM1, MessageM2,
};

use crate::clock::{self, ClockState, TimerAction, TMR_NEVER};
use crate::kpriv::{KPriv, PrivTable};
use crate::proc::{KProcess, MiscFlagsBits, ProcNr};
use crate::proc_table::ProcessTable;
use crate::syscall::{KcallResult, Syscall};

// ── Minix3 error codes ──
// Centralized in `crate::errno` to prevent value drift (FIX-01: R-02/R-09/R-18).
use crate::errno::*;

// ── Virtual timer types ──

/// Virtual timer type. C: `VT_VIRTUAL` / `VT_PROF` — com.h:420-421.
///
/// Values strictly align with C: `VT_VIRTUAL = 1`, `VT_PROF = 2`.
/// `#[repr(i32)]` preserves the ABI for IPC message compatibility.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum VtimerType {
    /// Virtual timer (counts user-mode time). C: `VT_VIRTUAL = 1`
    Virtual = 1,
    /// Profile timer (counts user + system time). C: `VT_PROF = 2`
    Prof = 2,
}

impl TryFrom<i32> for VtimerType {
    type Error = ();

    fn try_from(value: i32) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(Self::Virtual),
            2 => Ok(Self::Prof),
            _ => Err(()),
        }
    }
}

// ── Clock ID ──

/// Clock ID for SETTIME. C: `CLOCK_REALTIME` — time.h
pub const CLOCK_REALTIME: i32 = 0;

// ── SELF ──

/// SELF endpoint sentinel. C: `SELF` — endpoint.h:56, `_ENDPOINT_SLOT_TOP - 3`.
/// Derived from the minix-types authority (single source) — the former local
/// `-2` is the SYSTEM task endpoint (com.h:50), so authority-valued wire
/// fields (callers send `minix_types::Endpoint::SELF.0`, e.g.
/// ipc-server boundary.rs:108 / is acquire.rs:353) never matched and the
/// do_times.c:33-34 SELF replacement silently never fired
/// (E-MIBGRANT constant class; see `syscall_copy.rs::SELF`, `grant.rs::ANY`).
const SELF: i32 = minix_types::Endpoint::SELF.0;

// ── Helpers ──

#[allow(dead_code)] // message accessor; unused for now
fn msg_m1(msg: &Message) -> MessageM1 {
    // SAFETY: `m_type` has been validated by the caller to select the M1
    // format. All union variants share the same size and `#[repr(C)]`
    // layout, so reading a different variant is sound.
    unsafe { msg.m_u.m_m1 }
}

fn msg_m2(msg: &Message) -> MessageM2 {
    // SAFETY: `m_type` has been validated by the caller to select the M2
    // format. All union variants share the same size and `#[repr(C)]`
    // layout, so reading a different variant is sound.
    unsafe { msg.m_u.m_m2 }
}

// ── Dispatch functions ──

/// Dispatch SYS_TIMES.
///
/// C: `do_times()` — do_times.c:22-46
///
/// Retrieve accounting information for a process.
///
/// # Implementation
///
/// Full implementation matching C `do_times`:
/// 1. SELF replacement: `endpt == SELF` → use `caller.p_endpoint`.
/// 2. Endpoint validation via `ProcessTable::endpoint_to_nr()`.
/// 3. If endpoint is valid and not NONE: read `p_user_time` + `p_sys_time`.
/// 4. Always read `get_monotonic()`, `get_realtime()`, `get_boottime()`.
/// 5. Pack into `MessKrnLsysSysTimes` reply overlay.
pub fn dispatch_times(
    caller_nr: ProcNr,
    msg: &mut Message,
    proc_table: &ProcessTable,
) -> KcallResult {
    // C: do_times.c:33-34 — extract endpoint (SELF replacement inline)
    msg.debug_check_m_type_any(&[Syscall::Times as i32]);
    // SAFETY: `m_type` verified above (debug) / guaranteed by dispatch (release).
    let req = unsafe { &msg.m_u.m_lsys_krn_sys_times };
    let endpt = req.endpt;

    // C: do_times.c:33-34 — SELF replacement
    let target_endpoint = if endpt == SELF {
        proc_table
            .get(caller_nr)
            .map(|p| p.p_endpoint)
            .expect("dispatch_times: caller slot must exist")
    } else {
        Endpoint(endpt)
    };

    // C: do_times.c:35-38 — if valid endpoint, read user/sys time
    let (user_time, sys_time) = if target_endpoint != Endpoint::NONE {
        if let Some(proc_nr) = proc_table.endpoint_to_nr(target_endpoint) {
            // C: do_times.c:36-38 — rp = proc_addr(proc_nr)
            if let Some(rp) = proc_table.get(proc_nr) {
                (
                    rp.p_time.user_time.load(Ordering::Relaxed),
                    rp.p_time.sys_time.load(Ordering::Relaxed),
                )
            } else {
                (0, 0)
            }
        } else {
            (0, 0)
        }
    } else {
        // C: do_times.c:35 — if e_proc_nr == NONE, skip user/sys time
        (0, 0)
    };

    // C: do_times.c:40-42 — always fill these fields.
    // I-15 单源直读 ClockState（生产持 BKL；hosted 测试单线程）。
    let clock = unsafe { crate::clock_state_boot_unchecked() };
    let reply = MessKrnLsysSysTimes {
        boot_ticks: clock.uptime(),
        real_ticks: clock.realtime(),
        user_time,
        system_time: sys_time,
        boot_time: clock.boottime(),
        _padding: [0u8; 16],
    };

    // Write reply into message
    msg.debug_check_m_type_any(&[Syscall::Times as i32]);
    // SAFETY: `m_type` verified above (debug) / guaranteed by dispatch (release).
    msg.m_u.m_krn_lsys_sys_times = reply;

    KcallResult::Ok(OK)
}

/// Dispatch SYS_SETALARM.
///
/// C: `do_setalarm()` — do_setalarm.c:22-64 (cause_alarm: 69-76)
///
/// Set or cancel a synchronous alarm timer for a system process.
/// The alarm fires via `mini_notify(CLOCK, endpoint)`.
///
/// # Implementation
///
/// Full implementation matching C `do_setalarm`:
/// 1. SYS_PROC permission check via PrivTable.
/// 2. Get `s_alarm_timer` from caller's KPriv.
/// 3. Calculate time_left on previous alarm.
/// 4. Return time_left and current uptime.
/// 5. Set or reset timer in ClockState.
pub fn dispatch_setalarm(
    caller_nr: ProcNr,
    msg: &mut Message,
    priv_table: &mut PrivTable,
    clock_state: &mut ClockState,
    proc_table: &ProcessTable,
) -> KcallResult {
    // C: do_setalarm.c:31-32 — extract parameters
    msg.debug_check_m_type_any(&[Syscall::Setalarm as i32]);
    // SAFETY: `m_type` verified above (debug) / guaranteed by dispatch (release).
    let req = unsafe { &msg.m_u.m_lsys_krn_sys_setalarm };
    let exp_time = req.exp_time;
    let use_abs_time = req.abs_time != 0;

    // C: do_setalarm.c:33 — SYS_PROC permission check
    let sa_caller_sys = proc_table
        .get(caller_nr)
        .is_some_and(|c| caller_has_sys_proc_with_table(c, priv_table));
    // NK4-C 1.10y 取证探针（task1-close 裁决删除）：每次 setalarm 的
    // caller/priv_id/生效 flags——boot 首臂 OK 而 5s 平衡轮 re-arm EPERM
    // 的「中途剥旗」定位（s17c：SET_SYS 落 0x12 后 re-arm 仍死）。
    #[cfg(not(feature = "mock"))]
    {
        use core::sync::atomic::{AtomicUsize, Ordering as AtomicOrd};
        use minix_plat::{CurrentEarlyConsole as Console, EarlyConsole as _};
        static SAC_N: AtomicUsize = AtomicUsize::new(0);
        if SAC_N.fetch_add(1, AtomicOrd::Relaxed) < 8 {
            if let Some(c) = proc_table.get(caller_nr) {
                Console::write_str(" pid=");
                match c.priv_id {
                    Some(id) => Console::write_hex(id as u64),
                    None => Console::write_str("none"),
                }
                if let Some(id) = c.priv_id {
                    if let Some(kp) = priv_table.get(id) {
                        Console::write_str(" fl=0x");
                        Console::write_hex(kp.flags.s_flags.bits() as u64);
                    }
                }
            }
            Console::write_str(" sys=");
            Console::write_str(if sa_caller_sys { "y" } else { "n" });
            Console::write_str("\n");
        }
    }
    if !sa_caller_sys {
        return KcallResult::Ok(EPERM);
    }

    // C: do_setalarm.c:36 — get timer from priv structure
    let Some(caller_priv_id) = proc_table.get(caller_nr).and_then(|c| c.priv_id) else {
        return KcallResult::Ok(EPERM); // already checked above, defensive
    };

    // C: do_setalarm.c:39-46 — return time left on previous alarm
    // Three branches, exactly as in C:
    //   !tmr_is_set(tp)            → TMR_NEVER
    //   tmr_is_first(uptime, exp)  → exp - uptime (wrap-safe; exp >= uptime here)
    //   otherwise (already expired) → 0
    // (`saturating_sub` was a semantic deviation: an already-expired alarm
    // must report 0, and wrap-around must follow the C comparison.)
    let uptime = clock_state.uptime();
    let time_left = match priv_table.get(caller_priv_id) {
        Some(kpriv) => {
            let tp = &kpriv.runtime.s_alarm_timer;
            if !tp.is_set() {
                TMR_NEVER
            } else if clock::tmr_is_first(uptime, tp.exp_time) {
                // C: `tp->tmr_exp_time - uptime` (clock_t unsigned arithmetic;
                // the branch condition guarantees no wrap on this path).
                tp.exp_time.wrapping_sub(uptime)
            } else {
                0
            }
        }
        None => TMR_NEVER,
    };

    // C: do_setalarm.c:56-62 — set or reset timer
    if !use_abs_time && exp_time == 0 {
        // Reset alarm: C: do_setalarm.c:57 — reset_kernel_timer(tp).
        // Chain-aware: unlinks the node from the clock chain (if linked)
        // and deactivates it. `set_alarm_timer` re-arms first, so no
        // duplicate (entry, id) bookkeeping is needed — node identity is
        // the privilege slot (C: `&priv(caller)->s_alarm_timer`).
        clock::reset_alarm_timer(priv_table, clock_state, caller_priv_id);
    } else {
        // Set alarm: C: do_setalarm.c:59-61 —
        //   if (!use_abs_time) exp_time += uptime;
        //   set_kernel_timer(tp, exp_time, cause_alarm, caller->p_endpoint);
        let actual_exp_time = if use_abs_time {
            exp_time
        } else {
            // C: clock_t unsigned addition (defined wrap-around).
            uptime.wrapping_add(exp_time)
        };

        clock::set_alarm_timer(
            priv_table,
            clock_state,
            caller_priv_id,
            actual_exp_time,
            TimerAction::NotifyAlarm {
                endpoint: proc_table
                    .get(caller_nr)
                    .map(|c| c.p_endpoint)
                    .expect("dispatch_setalarm: caller slot must exist"),
            },
        );
    }

    // C: do_setalarm.c:49 — return current uptime + time_left
    let reply = MessLsysKrnSysSetalarm {
        exp_time,
        time_left,
        uptime,
        abs_time: if use_abs_time { 1 } else { 0 },
        _padding: [0u8; 28],
    };
    msg.debug_check_m_type_any(&[Syscall::Setalarm as i32]);
    // SAFETY: `m_type` verified above (debug) / guaranteed by dispatch (release).
    msg.m_u.m_lsys_krn_sys_setalarm = reply;

    KcallResult::Ok(OK)
}

/// Returns true iff `caller.priv_id` is `Some` AND the matching KPriv
/// entry is a SYS_PROC (i.e. has `ProcessCapability::SYS_PROC` set).
///
/// Used by `dispatch_setalarm` / `dispatch_vtimer` to gate syscalls
/// that only system processes may invoke (Minix3: `do_setalarm.c:33`,
/// `do_vtimer.c:31`).
///
/// Fail-closed: if `priv_id` is `None` we conservatively return `false`
/// so the syscall is rejected rather than silently allowed.
///
/// # Legacy note
///
/// The old `caller_has_sys_proc()` used `PrivTable::new()` which created
/// a fresh empty table — always returning `false` (fail-closed but
/// over-rejecting). This version uses the real PrivTable passed from
/// the dispatcher.
pub(crate) fn caller_has_sys_proc_with_table(caller: &KProcess, priv_table: &PrivTable) -> bool {
    let Some(priv_id) = caller.priv_id else {
        return false;
    };
    priv_table
        .get(priv_id)
        .map(KPriv::is_sys_proc)
        .unwrap_or(false)
}

/// Legacy helper for backward compatibility — uses a fresh PrivTable
/// (always returns false). Prefer `caller_has_sys_proc_with_table`.
#[allow(dead_code)]
pub(crate) fn caller_has_sys_proc(caller: &KProcess) -> bool {
    let Some(priv_id) = caller.priv_id else {
        return false;
    };
    let priv_table = PrivTable::new();
    priv_table
        .get(priv_id)
        .map(KPriv::is_sys_proc)
        .unwrap_or(false)
}

/// Dispatch SYS_STIME.
///
/// C: `do_stime()` — do_stime.c:15-18
///
/// Set the boot time (Unix timestamp when the system was booted).
///
/// # Implementation
///
/// Full implementation matching C `do_stime`:
/// 1. Extract `boot_time` from `m_lsys_krn_sys_stime.boot_time`.
/// 2. Call `ClockState::set_boottime()` which updates both the
///    internal field and the global `CLOCK_BOOTTIME` atomic.
/// 3. Return OK.
pub fn dispatch_stime(
    msg: &Message,
    clock_state: &mut ClockState,
) -> KcallResult {
    // C: do_stime.c:17 — set_boottime(m_ptr->m_lsys_krn_sys_stime.boot_time)
    msg.debug_check_m_type_any(&[Syscall::Stime as i32]);
    // SAFETY: `m_type` verified above (debug) / guaranteed by dispatch (release).
    let req = unsafe { &msg.m_u.m_lsys_krn_sys_stime };
    let boot_time = req.boot_time;

    clock_state.set_boottime(boot_time);

    KcallResult::Ok(OK)
}

/// Dispatch SYS_SETTIME.
///
/// C: `do_settime()` — do_settime.c:18-58
///
/// Set the real-time clock or adjust time gradually (adjtime).
///
/// # Implementation
///
/// Full implementation matching C `do_settime`:
/// 1. Validate `clock_id == CLOCK_REALTIME` (C:25-26).
/// 2. If `now == 0`: adjtime mode — convert sec+nsec to ticks and
///    call `set_adjtime_delta()` (C:29-34).
/// 3. If `now != 0`: set-time mode — compute `timediff_ticks` from
///    `sec - boottime`, validate range, and call `set_realtime()`
///    (C:35-57). If boottime was wrong, correct it.
pub fn dispatch_settime(
    msg: &Message,
    clock_state: &mut ClockState,
) -> KcallResult {
    // C: do_settime.c:25-52 — parameters read inline (C has no extraction)
    msg.debug_check_m_type_any(&[Syscall::Settime as i32]);
    // SAFETY: `m_type` verified above (debug) / guaranteed by dispatch (release).
    let req = unsafe { &msg.m_u.m_lsys_krn_sys_settime };
    let now = req.now;
    let clock_id = req.clock_id;
    let sec = req.sec;
    let nsec = req.nsec;

    // C: do_settime.c:25-26 — only CLOCK_REALTIME allowed
    if clock_id != CLOCK_REALTIME {
        return KcallResult::Ok(EINVAL);
    }

    let hz = clock_state.system_hz();

    // C: do_settime.c:29-34 — adjtime mode (now == 0)
    if now == 0 {
        // Convert delta from seconds + nanoseconds to ticks
        // C: do_settime.c:31-32 — ticks = (sec * system_hz) + (nsec / (1000000000 / system_hz))
        let ticks = (sec as i64 * hz as i64 + nsec / (1_000_000_000 / hz as i64)) as i32;
        clock_state.set_adjtime_delta(ticks);
        return KcallResult::Ok(OK);
    }

    // C: do_settime.c:35-57 — set time mode (now != 0)
    let boottime = clock_state.boottime();

    // C: do_settime.c:39 — timediff = sec - boottime
    let timediff = sec as i64 - boottime as i64;
    // C: do_settime.c:40 — timediff_ticks = timediff * system_hz
    let timediff_ticks = timediff * hz as i64;

    // C: do_settime.c:43-48 — prevent negative realtime
    if sec <= boottime
        || timediff_ticks < i32::MIN as i64 / 2
        || timediff_ticks > i32::MAX as i64 / 2
    {
        // C: boottime was likely wrong, try to correct it
        clock_state.set_boottime(sec);
        clock_state.set_realtime(1);
        return KcallResult::Ok(OK);
    }

    // C: do_settime.c:51-53 — calculate new realtime in ticks
    let newclock = (timediff_ticks + nsec / (1_000_000_000 / hz as i64)) as u64;
    clock_state.set_realtime(newclock);

    KcallResult::Ok(OK)
}

/// Dispatch SYS_VTIMER.
///
/// C: `do_vtimer()` — do_vtimer.c:21-74 (vtimer_check: 81-103)
///
/// Set and/or retrieve the value of a process's virtual or profile timer.
///
/// # Implementation
///
/// Full implementation matching C `do_vtimer`:
/// 1. SYS_PROC permission check via PrivTable.
/// 2. Validate timer type (VT_VIRTUAL=1 / VT_PROF=2).
/// 3. SELF replacement + endpoint validation via ProcessTable.
/// 4. Retrieve old value from `p_virt_left` / `p_prof_left`.
/// 5. If VT_SET: write new value and set/clear MiscFlags.
/// 6. Return old value in reply message.
pub fn dispatch_vtimer(
    caller_nr: ProcNr,
    msg: &mut Message,
    priv_table: &PrivTable,
    proc_table: &ProcessTable,
) -> KcallResult {
    // C: do_vtimer.c:33-71 — M2 parameters read inline (VT_WHICH:33,
    // VT_ENDPT:37, VT_SET:60, VT_VALUE:63/71)
    msg.debug_check_m_type_any(&[Syscall::Vtimer as i32]);
    let m2 = msg_m2(msg);
    let which = m2.m2i1;        // VT_WHICH
    let set = m2.m2i2 != 0;     // VT_SET
    let value = m2.m2l1 as u64; // VT_VALUE
    let endpt = m2.m2l2 as i32; // VT_ENDPT

    // C: do_vtimer.c:31 — SYS_PROC permission check
    if !proc_table
        .get(caller_nr)
        .is_some_and(|c| caller_has_sys_proc_with_table(c, priv_table))
    {
        return KcallResult::Ok(EPERM);
    }

    // C: do_vtimer.c:33-34 — validate timer type
    let vtype = match VtimerType::try_from(which) {
        Ok(v) => v,
        Err(()) => return KcallResult::Ok(EINVAL),
    };

    // C: do_vtimer.c:37-38 — SELF replacement + endpoint validation
    let target_endpoint = if endpt == SELF {
        proc_table
            .get(caller_nr)
            .map(|c| c.p_endpoint)
            .expect("dispatch_vtimer: caller slot must exist")
    } else {
        Endpoint(endpt)
    };

    // C: do_vtimer.c:38 — isokendpt check
    let target_nr = match proc_table.endpoint_to_nr(target_endpoint) {
        Some(nr) => nr,
        None => return KcallResult::Ok(EINVAL),
    };

    // C: do_vtimer.c:39 — rp = proc_addr(proc_nr)
    let target = match proc_table.get(target_nr) {
        Some(rp) => rp,
        None => return KcallResult::Ok(EINVAL),
    };

    // C: do_vtimer.c:45-58 — determine flag/field + retrieve old value
    let (pt_flag, old_value) = match vtype {
        VtimerType::Virtual => {
            let flag = MiscFlagsBits::VIRT_TIMER;
            let old = if target.p_misc_flags.is_set(MiscFlagsBits::VIRT_TIMER) {
                target.p_time.virt_left.load(Ordering::Relaxed)
            } else {
                0
            };
            (flag, old)
        }
        VtimerType::Prof => {
            let flag = MiscFlagsBits::PROF_TIMER;
            let old = if target.p_misc_flags.is_set(MiscFlagsBits::PROF_TIMER) {
                target.p_time.prof_left.load(Ordering::Relaxed)
            } else {
                0
            };
            (flag, old)
        }
    };

    // C: do_vtimer.c:60-69 — set new value if VT_SET
    if set {
        // C: do_vtimer.c:61 — disable timer first
        target.p_misc_flags.clear(pt_flag);

        if value > 0 {
            // C: do_vtimer.c:63-65 — set new timer value + re-enable
            match vtype {
                VtimerType::Virtual => {
                    target.p_time.virt_left.store(value, Ordering::Release);
                }
                VtimerType::Prof => {
                    target.p_time.prof_left.store(value, Ordering::Release);
                }
            }
            target.p_misc_flags.set(pt_flag);
        } else {
            // C: do_vtimer.c:66-68 — clear timer value
            match vtype {
                VtimerType::Virtual => {
                    target.p_time.virt_left.store(0, Ordering::Release);
                }
                VtimerType::Prof => {
                    target.p_time.prof_left.store(0, Ordering::Release);
                }
            }
        }
    }

    // C: do_vtimer.c:71 — return old value in VT_VALUE
    // Write old_value back into the message's m2_l1 field
    // SAFETY: `m_type == SYS_VTIMER` guarantees the M2 format is active.
    // Writing to `m_m2.m2l1` is sound per `#[repr(C)]` union layout.
    msg.m_u.m_m2.m2l1 = old_value as i64;

    KcallResult::Ok(OK)
}

// ── Tests ──

#[cfg(test)]
mod tests {
    use super::*;
    use crate::proc::ProcNr;

    #[test]
    fn test_vtimer_type_values_match_c() {
        // C: com.h:420-421 — VT_VIRTUAL = 1, VT_PROF = 2
        assert_eq!(VtimerType::Virtual as i32, 1);
        assert_eq!(VtimerType::Prof as i32, 2);
    }

    #[test]
    fn test_vtimer_type_try_from() {
        assert_eq!(VtimerType::try_from(1), Ok(VtimerType::Virtual));
        assert_eq!(VtimerType::try_from(2), Ok(VtimerType::Prof));
        assert_eq!(VtimerType::try_from(0), Err(()));
        assert_eq!(VtimerType::try_from(3), Err(()));
    }

    #[test]
    fn test_clock_realtime() {
        assert_eq!(CLOCK_REALTIME, 0);
    }

    // ── KSC-1 / KSC-2 SYS_PROC permission tests ─────────────────────

    use crate::capability::ProcessCapability;
    use crate::proc::KProcess;
    use minix_types::Endpoint;

    fn proc_with_priv_id(priv_id: Option<crate::kpriv::PrivId>) -> KProcess {
        let mut p = KProcess::new(ProcNr(0), Endpoint::from_generation_slot(1, 0));
        p.priv_id = priv_id;
        p
    }

    #[test]
    fn test_ksc_caller_has_sys_proc_no_priv_id_rejected() {
        let p = proc_with_priv_id(None);
        assert!(!caller_has_sys_proc(&p));
    }

    #[test]
    fn test_ksc_caller_has_sys_proc_fresh_table_rejects_all() {
        let p = proc_with_priv_id(Some(crate::kpriv::static_priv_id(ProcNr(0))));
        assert!(!caller_has_sys_proc(&p));
    }

    #[test]
    fn test_ksc_setalarm_non_sys_proc_returns_eperm() {
        // caller-by-nr: the caller is table slot ProcNr(0) with priv_id None.
        let proc_table = crate::test_helpers::test_proc_table();
        let mut priv_table = crate::test_helpers::test_priv_table();
        let mut msg = Message::default();
        msg.m_type = Syscall::Setalarm as i32;
        match dispatch_setalarm(ProcNr(0), &mut msg, &mut priv_table, &mut ClockState::new(), &proc_table) {
            KcallResult::Ok(EPERM) => {}
            other => panic!("expected Ok(EPERM), got {:?}", other),
        }
    }

    #[test]
    fn test_ksc_vtimer_non_sys_proc_returns_eperm() {
        // caller-by-nr: the caller is table slot ProcNr(0) with priv_id None.
        let proc_table = crate::test_helpers::test_proc_table();
        let priv_table = crate::test_helpers::test_priv_table();
        let mut msg = Message::default();
        msg.m_type = Syscall::Vtimer as i32;
        match dispatch_vtimer(ProcNr(0), &mut msg, &priv_table, &proc_table) {
            KcallResult::Ok(EPERM) => {}
            other => panic!("expected Ok(EPERM), got {:?}", other),
        }
    }

    #[test]
    fn test_ksc_priv_flags_sys_proc_bit_definition() {
        // C combos (priv.h:36-49): IDL_F/SRV_F carry SYS_PROC, USR_F does not.
        assert!(ProcessCapability::IDL_F.contains(ProcessCapability::SYS_PROC));
        assert!(ProcessCapability::SRV_F.contains(ProcessCapability::SYS_PROC));
        assert!(!ProcessCapability::USR_F.contains(ProcessCapability::SYS_PROC));
    }

    #[test]
    fn test_dispatch_times_self_replacement() {
        // SELF（endpoint.h:56 权威哨兵）必须替换为 caller 自身 endpoint：
        // caller 槽 ProcNr(0) 携带专属 endpoint 与非零 user_time，仅当替换
        // 真实发生才会把该值读回 reply（历史化石值 -2 使替换谓词永假，
        // 走 else 分支按具体端点解析 → 回 0）。do_times.c:33-34。
        let mut proc_table = crate::test_helpers::test_proc_table();
        // SAFETY: test-only; single-threaded under the harness. Installs a
        // fresh ClockState so the unconditional clock fill (dispatch_times →
        // clock_state_boot_unchecked, syscall_clock.rs:154) does not depend
        // on another test having initialized the global (ordering fragility
        // the old, non-resolving variant of this test silently relied on).
        unsafe {
            *crate::globals::CLOCK_STATE.get() = Some(crate::clock::ClockState::new());
        }
        {
            let slot = proc_table.get_mut(ProcNr(0)).expect("slot 0");
            slot.p_rts_flags.clear(crate::proc::RtsFlagsBits::SLOT_FREE);
            slot.p_endpoint = Endpoint(100);
            slot.p_time.user_time.store(777, Ordering::Relaxed);
        }

        let mut msg = Message::default();
        // Set up the request: endpt = SELF
        msg.m_u.m_lsys_krn_sys_times.endpt = SELF;
        msg.m_type = 25; // SYS_TIMES

        let result = dispatch_times(ProcNr(0), &mut msg, &proc_table);
        assert_eq!(result, KcallResult::Ok(OK));

        // Verify reply fields are populated
        let reply = unsafe { &msg.m_u.m_krn_lsys_sys_times };
        // user_time 必须经 SELF → caller endpoint(100) → 槽 0 解析读回 777；
        // system_time 未填充保持 0。
        assert_eq!(
            reply.user_time, 777,
            "SELF 替换必须解析到 caller 的 user_time"
        );
        assert_eq!(reply.system_time, 0);
    }

    /// C 绝对值 pin：SELF 必须等于 endpoint.h:56 的 `_ENDPOINT_SLOT_TOP - 3`
    /// = 32768 - MAX_NR_TASKS（com.h:55，1023）- 3 = 31742。历史本地化石值
    /// -2 恰为 SYSTEM 任务 endpoint（com.h:50），而 ipc-server/is 实发权威值
    /// （boundary.rs:108 / acquire.rs:353），替换谓词永假。对位
    /// `grant.rs::test_magic_granter_endpoints_match_c_com_h`。
    #[test]
    fn test_sys_times_self_sentinel_authority_pin() {
        assert_eq!(SELF, 31742); // endpoint.h:56 + com.h:55
        assert_eq!(SELF, minix_types::Endpoint::SELF.0);
        assert_ne!(SELF, Endpoint::SYSTEM.0); // 化石值 -2 是 SYSTEM，不是 SELF
    }

    #[test]
    fn test_dispatch_setalarm_reset_timer() {
        // Test that exp_time=0 with !abs_time resets the alarm
        // caller-by-nr: slot ProcNr(0) keeps priv_id None → EPERM check.
        let proc_table = crate::test_helpers::test_proc_table();
        let mut priv_table = crate::test_helpers::test_priv_table();
        let mut msg = Message::default();
        msg.m_type = Syscall::Setalarm as i32;
        let result = dispatch_setalarm(
            ProcNr(0), &mut msg, &mut priv_table, &mut ClockState::new(), &proc_table,
        );
        assert_eq!(result, KcallResult::Ok(EPERM));
    }

    // ── T-6: STIME / SETTIME / SETALARM / VTIMER 非 EPERM 行为测试 ──

    /// SYS_PROC 调用者构造（caller-by-nr 形态）：调用者**占表槽**
    /// `ProcNr(0)`——新 API 按号码回查进程表，独立的 `KProcess` 不再是
    /// 合法调用者。priv_id 指向带 SYS_PROC 能力的静态 priv 槽；SLOT_FREE
    /// 一并清除（`endpoint_to_nr` 跳过自由槽）。
    fn t6_sys_proc_caller() -> (crate::test_helpers::TestProcTable, crate::test_helpers::TestPrivTable) {
        let mut table = crate::test_helpers::test_proc_table();
        let mut privs = crate::test_helpers::test_priv_table();
        let pid = privs.assign_static(ProcNr(0)).expect("static priv slot");
        privs.get_mut(pid).unwrap().flags.s_flags =
            crate::capability::ProcessCapability::SYS_PROC;
        let slot = table.get_mut(ProcNr(0)).unwrap();
        slot.priv_id = Some(pid);
        slot.p_rts_flags.clear(crate::proc::RtsFlagsBits::SLOT_FREE);
        (table, privs)
    }

    #[test]
    fn test_t6_stime_sets_boottime() {
        // C do_stime.c:17 — set_boottime(boot_time) 后 boottime 可读回。
        let mut cs = ClockState::new();
        let mut msg = Message::default();
        msg.m_type = Syscall::Stime as i32;
        // SAFETY: m_type 已设置；测试侧 union 写。
        unsafe { msg.m_u.m_lsys_krn_sys_stime.boot_time = 1000; }
        let result = dispatch_stime(&msg, &mut cs);
        assert_eq!(result, KcallResult::Ok(OK));
        assert_eq!(cs.boottime(), 1000);
    }

    #[test]
    fn test_t6_stime_last_write_wins() {
        // 二次 STIME 覆盖前值（C set_boottime 直接赋值，无累积语义）。
        let mut cs = ClockState::new();
        let mut msg = Message::default();
        msg.m_type = Syscall::Stime as i32;
        unsafe { msg.m_u.m_lsys_krn_sys_stime.boot_time = 1000; }
        let _ = dispatch_stime(&msg, &mut cs);
        unsafe { msg.m_u.m_lsys_krn_sys_stime.boot_time = 2000; }
        let _ = dispatch_stime(&msg, &mut cs);
        assert_eq!(cs.boottime(), 2000);
    }

    #[test]
    fn test_t6_settime_rejects_non_realtime_clock() {
        // C do_settime.c:25-26 — clock_id != CLOCK_REALTIME → EINVAL。
        let mut cs = ClockState::new();
        let mut msg = Message::default();
        msg.m_type = Syscall::Settime as i32;
        unsafe {
            msg.m_u.m_lsys_krn_sys_settime.clock_id = 99; // 非 CLOCK_REALTIME
            msg.m_u.m_lsys_krn_sys_settime.now = 100;
        }
        let result = dispatch_settime(&msg, &mut cs);
        assert_eq!(result, KcallResult::Ok(EINVAL));
    }

    #[test]
    fn test_t6_settime_correction_when_boottime_wrong() {
        // C do_settime.c:43-48 — sec <= boottime 判定 boottime 错误：
        // 纠正 boottime = sec 并置 realtime = 1，仍返回 OK。
        let mut cs = ClockState::new();
        cs.set_boottime(5000);
        let mut msg = Message::default();
        msg.m_type = Syscall::Settime as i32;
        unsafe {
            msg.m_u.m_lsys_krn_sys_settime.clock_id = CLOCK_REALTIME;
            msg.m_u.m_lsys_krn_sys_settime.now = 7;
            msg.m_u.m_lsys_krn_sys_settime.sec = 3000; // < boottime 5000
        }
        let result = dispatch_settime(&msg, &mut cs);
        assert_eq!(result, KcallResult::Ok(OK));
        assert_eq!(cs.boottime(), 3000, "boottime 必须被纠正为 sec");
        assert_eq!(cs.realtime(), 1);
    }

    #[test]
    fn test_t6_settime_adjtime_mode_sets_delta() {
        // C do_settime.c:29-34 — now=0 走 adjtime：ticks = sec*hz + nsec/(1e9/hz)。
        let mut cs = ClockState::new();
        let hz = cs.system_hz();
        let mut msg = Message::default();
        msg.m_type = Syscall::Settime as i32;
        unsafe {
            msg.m_u.m_lsys_krn_sys_settime.clock_id = CLOCK_REALTIME;
            msg.m_u.m_lsys_krn_sys_settime.now = 0; // adjtime 模式
            msg.m_u.m_lsys_krn_sys_settime.sec = 2;
            msg.m_u.m_lsys_krn_sys_settime.nsec = 0;
        }
        let result = dispatch_settime(&msg, &mut cs);
        assert_eq!(result, KcallResult::Ok(OK));
        assert_eq!(cs.adjtime_delta(), 2 * hz);
    }

    #[test]
    fn test_t6_settime_normal_sets_realtime() {
        // C do_settime.c:51-53 — now!=0 正常路径：realtime = (sec-boottime)*hz
        // + nsec 折算 ticks。
        let mut cs = ClockState::new();
        cs.set_boottime(0);
        let hz = cs.system_hz();
        let mut msg = Message::default();
        msg.m_type = Syscall::Settime as i32;
        unsafe {
            msg.m_u.m_lsys_krn_sys_settime.clock_id = CLOCK_REALTIME;
            msg.m_u.m_lsys_krn_sys_settime.now = 7;
            msg.m_u.m_lsys_krn_sys_settime.sec = 100;
            msg.m_u.m_lsys_krn_sys_settime.nsec = 0;
        }
        let result = dispatch_settime(&msg, &mut cs);
        assert_eq!(result, KcallResult::Ok(OK));
        assert_eq!(cs.realtime(), 100 * hz as u64);
    }

    #[test]
    fn test_t6_setalarm_first_set_returns_ok_and_arms() {
        // C do_setalarm.c:56-62 — 相对 100 tick 武装闹钟（uptime=0 时
        // exp_time = 100），前一闹钟未设置故 time_left = TMR_NEVER。
        let (table, mut privs) = t6_sys_proc_caller();
        let mut cs = ClockState::new();
        let mut msg = Message::default();
        msg.m_type = Syscall::Setalarm as i32;
        unsafe {
            msg.m_u.m_lsys_krn_sys_setalarm.exp_time = 100;
            msg.m_u.m_lsys_krn_sys_setalarm.abs_time = 0;
        }
        let result = dispatch_setalarm(ProcNr(0), &mut msg, &mut privs, &mut cs, &table);
        assert_eq!(result, KcallResult::Ok(0));
        let pid = table.get(ProcNr(0)).unwrap().priv_id.unwrap();
        let tp = privs.get(pid).unwrap().runtime.s_alarm_timer;
        assert!(tp.is_set(), "闹钟必须已武装");
        assert_eq!(tp.exp_time, 100);
    }

    #[test]
    fn test_t6_setalarm_second_set_returns_previous_time_left() {
        // C do_setalarm.c:39-46 — 已有闹钟（exp=100 > uptime=0）时再次
        // SETALARM 返回前一闹钟剩余时间 100 - 0 = 100。
        let (table, mut privs) = t6_sys_proc_caller();
        let mut cs = ClockState::new();

        let mut msg = Message::default();
        msg.m_type = Syscall::Setalarm as i32;
        unsafe {
            msg.m_u.m_lsys_krn_sys_setalarm.exp_time = 100;
            msg.m_u.m_lsys_krn_sys_setalarm.abs_time = 0;
        }
        let first = dispatch_setalarm(ProcNr(0), &mut msg, &mut privs, &mut cs, &table);
        assert_eq!(first, KcallResult::Ok(OK));

        let second = dispatch_setalarm(ProcNr(0), &mut msg, &mut privs, &mut cs, &table);
        assert_eq!(second, KcallResult::Ok(OK));
        // time_left 经消息结构体回填（msg.m_lsys_krn_sys_setalarm），非 KcallResult。
        let time_left = unsafe { msg.m_u.m_lsys_krn_sys_setalarm.time_left };
        assert_eq!(time_left, 100, "第二次数值须回填前一闹钟剩余 100-0");
    }

    #[test]
    fn test_t6_vtimer_invalid_type_returns_einval() {
        // C do_vtimer.c:33-34 — VT_WHICH 非 VT_VIRTUAL/VT_PROF → EINVAL。
        // caller-by-nr: the caller IS table slot ProcNr(0) (t6 fixture
        // clears SLOT_FREE so SELF → nr 0 resolves).
        let (proc_table, privs) = t6_sys_proc_caller();
        let mut msg = Message::default();
        msg.m_type = Syscall::Vtimer as i32;
        // SAFETY: m_type 已设置；测试侧 union 写。
        unsafe { msg.m_u.m_m2.m2i1 = 99; } // 非 1/2
        let result = dispatch_vtimer(ProcNr(0), &mut msg, &privs, &proc_table);
        assert_eq!(result, KcallResult::Ok(EINVAL));
    }

    #[test]
    fn test_t6_vtimer_virtual_set_then_get_roundtrip() {
        // C do_vtimer.c:60-71 — VT_SET 写入 virt_left 并置 VIRT_TIMER；
        // 再 VT_GET 返回旧值（m2l1 回填）。
        let (proc_table, privs) = t6_sys_proc_caller();
        let mut msg = Message::default();
        msg.m_type = Syscall::Vtimer as i32;
        unsafe {
            msg.m_u.m_m2.m2i1 = 1; // VT_VIRTUAL
            msg.m_u.m_m2.m2i2 = 1; // VT_SET
            msg.m_u.m_m2.m2l2 = SELF as i64; // VT_ENDPT = SELF → caller
            msg.m_u.m_m2.m2l1 = 500; // VT_VALUE
        }
        let first = dispatch_vtimer(ProcNr(0), &mut msg, &privs, &proc_table);
        assert_eq!(first, KcallResult::Ok(0));

        // VT_GET：set=false → 回填旧值 500。
        let mut msg2 = Message::default();
        msg2.m_type = Syscall::Vtimer as i32;
        unsafe {
            msg2.m_u.m_m2.m2i1 = 1;
            msg2.m_u.m_m2.m2i2 = 0; // VT_GET
            msg2.m_u.m_m2.m2l2 = SELF as i64;
        }
        let second = dispatch_vtimer(ProcNr(0), &mut msg2, &privs, &proc_table);
        assert_eq!(second, KcallResult::Ok(0));
        let old = unsafe { msg2.m_u.m_m2.m2l1 };
        assert_eq!(old, 500, "VT_GET 必须回填先前 VT_SET 的值");
    }
}
