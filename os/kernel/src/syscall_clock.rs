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
use crate::proc::{KProcess, MiscFlagsBits};
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

const SELF: i32 = -2;

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
    caller: &mut KProcess,
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
        caller.p_endpoint
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

    // C: do_times.c:40-42 — always fill these fields
    let reply = MessKrnLsysSysTimes {
        boot_ticks: clock::get_monotonic(),
        real_ticks: clock::get_realtime(),
        user_time,
        system_time: sys_time,
        boot_time: clock::get_boottime(),
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
    caller: &mut KProcess,
    msg: &mut Message,
    priv_table: &mut PrivTable,
    clock_state: &mut ClockState,
) -> KcallResult {
    // C: do_setalarm.c:31-32 — extract parameters
    msg.debug_check_m_type_any(&[Syscall::Setalarm as i32]);
    // SAFETY: `m_type` verified above (debug) / guaranteed by dispatch (release).
    let req = unsafe { &msg.m_u.m_lsys_krn_sys_setalarm };
    let exp_time = req.exp_time;
    let use_abs_time = req.abs_time != 0;

    // C: do_setalarm.c:33 — SYS_PROC permission check
    if !caller_has_sys_proc_with_table(caller, priv_table) {
        return KcallResult::Ok(EPERM);
    }

    // C: do_setalarm.c:36 — get timer from priv structure
    let caller_priv_id = match caller.priv_id {
        Some(id) => id,
        None => return KcallResult::Ok(EPERM), // already checked above, defensive
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
                endpoint: caller.p_endpoint,
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
    _caller: &mut KProcess,
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
    _caller: &mut KProcess,
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
    caller: &mut KProcess,
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
    if !caller_has_sys_proc_with_table(caller, priv_table) {
        return KcallResult::Ok(EPERM);
    }

    // C: do_vtimer.c:33-34 — validate timer type
    let vtype = match VtimerType::try_from(which) {
        Ok(v) => v,
        Err(()) => return KcallResult::Ok(EINVAL),
    };

    // C: do_vtimer.c:37-38 — SELF replacement + endpoint validation
    let target_endpoint = if endpt == SELF {
        caller.p_endpoint
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
        let mut p = proc_with_priv_id(None);
        let mut msg = Message::default();
        msg.m_type = Syscall::Setalarm as i32;
        match dispatch_setalarm(&mut p, &mut msg, &mut crate::test_helpers::test_priv_table(), &mut ClockState::new()) {
            KcallResult::Ok(EPERM) => {}
            other => panic!("expected Ok(EPERM), got {:?}", other),
        }
    }

    #[test]
    fn test_ksc_vtimer_non_sys_proc_returns_eperm() {
        let mut p = proc_with_priv_id(None);
        let mut msg = Message::default();
        msg.m_type = Syscall::Vtimer as i32;
        match dispatch_vtimer(&mut p, &mut msg, &crate::test_helpers::test_priv_table(), &crate::test_helpers::test_proc_table()) {
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
        // Test that SELF (-2) is replaced with caller's endpoint
        let mut p = KProcess::new(ProcNr(0), Endpoint::from_generation_slot(1, 5));
        let mut msg = Message::default();
        // Set up the request: endpt = SELF
        msg.m_u.m_lsys_krn_sys_times.endpt = SELF;
        msg.m_type = 25; // SYS_TIMES

        let proc_table = crate::test_helpers::test_proc_table();
        let result = dispatch_times(&mut p, &mut msg, &proc_table);
        assert_eq!(result, KcallResult::Ok(OK));

        // Verify reply fields are populated
        let reply = unsafe { &msg.m_u.m_krn_lsys_sys_times };
        // boot_ticks should be get_monotonic() (may be 0 if no tick yet)
        // real_ticks should be get_realtime()
        // boot_time should be get_boottime()
        // user_time and system_time should be 0 (process not found in empty table)
        assert_eq!(reply.user_time, 0);
        assert_eq!(reply.system_time, 0);
    }

    #[test]
    fn test_dispatch_setalarm_reset_timer() {
        // Test that exp_time=0 with !abs_time resets the alarm
        let mut p = KProcess::new(ProcNr(0), Endpoint::from_generation_slot(1, 0));
        p.priv_id = None; // Will be rejected by EPERM check
        let mut msg = Message::default();
        msg.m_type = Syscall::Setalarm as i32;
        let result = dispatch_setalarm(
            &mut p, &mut msg, &mut crate::test_helpers::test_priv_table(), &mut ClockState::new(),
        );
        assert_eq!(result, KcallResult::Ok(EPERM));
    }
}
