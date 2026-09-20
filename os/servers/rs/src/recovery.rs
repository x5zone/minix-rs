//! Termination and recovery decisions (pure slice).
//!
//! Mirrors `minix3/minix/servers/rs/manager.c`: `terminate_service` — 1055,
//! `run_script` — 1185, `restart_service` — 1246, `cleanup_service` — 405.
//! 15-rs-terminate-restart.md.
//!
//! The action hooks (`end_update` — 16, `abort_update_proc` — 16,
//! `late_reply` — 06, `unpublish_service` — 11, `cleanup_service`'s
//! `sched_stop`/`srv_kill`/`free_slot` — 19/15, `run_script`'s fork+execle —
//! ARCH A-1, `detach_service` — 15) are stated as call sites. This module
//! owns the pure decision tree of `terminate_service` plus the small
//! helpers: backoff computation, script reason, late-reply result and the
//! two-phase cleanup classification.

use crate::boot::KernelApi;
use crate::privilege::PrivCtlOp;
use crate::process_table::RProcTable;
use crate::service_slot::{Label, RFlags, ServiceSlot, SlotId, SlotMutations, SysFlags};
use minix_types::{Clock, Endpoint, Errno};

/// C: `MAX_DET_RESTART` — const.h:25 (maximum number of detached restarts).
pub const MAX_DET_RESTART: i32 = 10;
/// C: `BACKOFF_BITS` — const.h:50 (`sizeof(long)*8`; 64 on x86-64).
pub const BACKOFF_BITS: i64 = 64;
/// C: `MAX_BACKOFF` — const.h:51 (in `RS_DELTA_T` units).
pub const MAX_BACKOFF: i64 = 30;

/// The `terminate_service` branch to execute.
///
/// C: `terminate_service` — manager.c:1055-1180. Hooks are stated per
/// variant; the slot mutations are returned separately in
/// [`TerminateDecision::mutations`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TerminateAction {
    /// Init failure during an update → roll back (manager.c:1071-1076;
    /// `end_update` hook: 16; the `r_init_err` argument comes from the slot).
    InitUpdateRollback,
    /// `RS_EXITING` path: unpublish + cleanup all instances (+ reincarnate)
    /// (manager.c:1121-1152; hooks: 11/15/16).
    CleanupAll {
        /// C: `norestart` — manager.c:1106 (drives the late-reply result).
        norestart: bool,
        /// `RS_REINCARNATE` was set → reincarnate after cleanup (manager.c:1146-1151).
        reincarnate: bool,
        /// Core service exiting outside shutdown → `_exit(1)` (manager.c:1123-1126).
        core_fatal: bool,
    },
    /// `RS_REFRESHING` path → `restart_service` (manager.c:1154-1156).
    Refresh,
    /// Unexpected exit with restarts → binary backoff (manager.c:1160-1175).
    Backoff { backoff: i64 },
    /// Unexpected exit, first time → `restart_service` (manager.c:1177-1179).
    Restart,
}

/// The `terminate_service` decision plus the flags the caller must set.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TerminateDecision {
    /// Branch to execute.
    pub action: TerminateAction,
    /// Slot mutations implied by the branch (R13). C mutates `r_*` inline
    /// while walking the tree; the 15 caller applies exactly these once.
    pub mutations: SlotMutations,
}

/// Walks the `terminate_service` decision tree.
///
/// C: manager.c:1065-1178. Inputs: `flags` (the slot's `r_flags`),
/// `sys_flags` (the service policy flags), `restarts`, `has_script`
/// (`r_script[0] != '\0'`), `shutting_down`, `is_updating`
/// (`SRV_IS_UPDATING` — reads `r_flags & RS_UPDATING`, injected as a bool).
/// The two update aborts — the global `RUPDATE_IS_UPDATING()` one
/// (manager.c:1099-1102) and the `SRV_IS_UPD_SCHEDULED` one
/// (manager.c:1127-1129) — are 16 caller hooks executed around the
/// decision, not modelled here.
pub fn terminate_decision(
    flags: RFlags,
    sys_flags: SysFlags,
    restarts: i32,
    has_script: bool,
    shutting_down: bool,
    is_updating: bool,
) -> TerminateDecision {
    let mut set = RFlags::empty();
    let mut clear = RFlags::empty();
    let mut flags = flags;

    // Init-failure branches (manager.c:1067-1091). C only *sets* flags here
    // and falls through into the main tree (manager.c:1121+): an init
    // failure with `SF_NO_BIN_EXP` becomes the `RS_REFRESHING` path, any
    // other init failure becomes the `RS_EXITING` path. The update rollback
    // is the single early return (manager.c:1071-1076).
    if flags.contains(RFlags::INITIALIZING) {
        if is_updating {
            return TerminateDecision {
                action: TerminateAction::InitUpdateRollback,
                // manager.c:1075 — `r_init_err = ERESTART` after the
                // `end_update(rp->r_init_err, RS_REPLY)` hook (16) consumed
                // the previous value.
                mutations: SlotMutations {
                    init_err: Some(minix_types::ERESTART),
                    ..Default::default()
                },
            };
        }
        if sys_flags.contains(SysFlags::NO_BIN_EXP) {
            set |= RFlags::REFRESHING; // manager.c:1084
            flags |= RFlags::REFRESHING;
        } else {
            set |= RFlags::EXITING; // manager.c:1090
            flags |= RFlags::EXITING;
        }
    }

    // norestart detection (manager.c:1105-1120).
    let norestart = !flags.contains(RFlags::EXITING) && sys_flags.contains(SysFlags::NORESTART);
    if norestart {
        set |= RFlags::EXITING;
        flags |= RFlags::EXITING;
        if sys_flags.contains(SysFlags::DET_RESTART) && restarts < MAX_DET_RESTART {
            set |= RFlags::CLEANUP_DETACH; // manager.c:1111-1114
        }
        if has_script {
            set |= RFlags::CLEANUP_SCRIPT; // manager.c:1116-1119
        }
    }

    if flags.contains(RFlags::EXITING) {
        let core_fatal = sys_flags.contains(SysFlags::CORE_SRV) && !shutting_down; // manager.c:1123-1126
        let reincarnate = flags.contains(RFlags::REINCARNATE); // manager.c:1146-1151
        if reincarnate {
            // manager.c:1147 — `r_flags &= ~RS_REINCARNATE` before
            // `reincarnate_service`; set_flags cannot express a clear, so the
            // mutation payload carries it (R13).
            clear = RFlags::REINCARNATE;
        }
        return TerminateDecision {
            action: TerminateAction::CleanupAll {
                norestart,
                reincarnate,
                core_fatal,
            },
            mutations: SlotMutations {
                set,
                clear,
                ..Default::default()
            },
        };
    }
    if flags.contains(RFlags::REFRESHING) {
        return TerminateDecision {
            action: TerminateAction::Refresh,
            mutations: SlotMutations {
                set,
                ..Default::default()
            },
        };
    }

    // Unexpected exit (manager.c:1158-1179).
    if restarts > 0 {
        let backoff = compute_backoff(
            restarts,
            sys_flags.contains(SysFlags::NO_BIN_EXP),
            sys_flags.contains(SysFlags::USE_COPY),
        );
        return TerminateDecision {
            action: TerminateAction::Backoff { backoff },
            // manager.c:1163-1174 — C writes `r_backoff = 1 << MIN(...)` etc.
            mutations: SlotMutations {
                set,
                backoff: Some(backoff),
                ..Default::default()
            },
        };
    }
    TerminateDecision {
        action: TerminateAction::Restart,
        mutations: SlotMutations {
            set,
            ..Default::default()
        },
    }
}

/// Computes the binary-exponential restart backoff.
///
/// C: `terminate_service` — manager.c:1163-1174:
/// `backoff = 1 << MIN(restarts, BACKOFF_BITS-2)`, capped at `MAX_BACKOFF`;
/// `SF_USE_COPY` services with `backoff > 1` stay at 1 (their image is
/// memory-resident, restarts are cheap); `SF_NO_BIN_EXP` → 1.
pub fn compute_backoff(restarts: i32, no_bin_exp: bool, use_copy: bool) -> i64 {
    if no_bin_exp {
        return 1; // manager.c:1171-1173
    }
    // Clamp negative restarts to 0: C's `1 << MIN(restarts, BACKOFF_BITS-2)`
    // (manager.c:1164) is UB for a negative shift; `terminate_decision`
    // only calls this with `restarts > 0`, but the pub API must fail closed.
    let shift = restarts.max(0).min((BACKOFF_BITS - 2) as i32) as u32;
    let mut backoff = 1i64 << shift;
    backoff = backoff.min(MAX_BACKOFF); // manager.c:1167
    if use_copy && backoff > 1 {
        backoff = 1; // manager.c:1168-1169
    }
    backoff
}

/// The recovery-script reason string.
///
/// C: `run_script` — manager.c:1189-1193: `RS_REFRESHING` → "restart",
/// `RS_NOPINGREPLY` → "no-heartbeat", otherwise "terminated".
pub fn script_reason(flags: RFlags) -> &'static str {
    if flags.contains(RFlags::REFRESHING) {
        "restart"
    } else if flags.contains(RFlags::NOPINGREPLY) {
        "no-heartbeat"
    } else {
        "terminated"
    }
}

/// The `late_reply` result for a terminating service.
///
/// C: `terminate_service` — manager.c:1134-1136: `RS_DOWN`, or
/// `RS_REFRESH` with `norestart`, gets `OK`; anything else `EDEADEPT`.
pub fn late_reply_result(caller_request: i32, norestart: bool) -> i32 {
    if caller_request == minix_types::RS_DOWN
        || (caller_request == minix_types::RS_REFRESH && norestart)
    {
        0 // OK
    } else {
        minix_types::EDEADEPT
    }
}

/// The second-phase cleanup classification.
///
/// C: `cleanup_service` — manager.c:449-490. `script` = `RS_CLEANUP_SCRIPT`
/// (run the recovery script during cleanup); `detach` = `RS_CLEANUP_DETACH`
/// (re-publish under a fresh label instead of freeing the slot). When
/// detaching, the `sched_stop`/`srv_kill` are skipped (manager.c:456-465).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CleanupDecision {
    /// Run the cleanup script. C: manager.c:473-478.
    pub script: bool,
    /// Detach instead of freeing. C: manager.c:480-490.
    pub detach: bool,
}

/// Classifies the second-phase cleanup from the slot flags.
///
/// C: `cleanup_service` — manager.c:450-453: `cleanup_script = r_flags &
/// RS_CLEANUP_SCRIPT; detach = r_flags & RS_CLEANUP_DETACH`.
pub fn cleanup_decision(flags: RFlags) -> CleanupDecision {
    CleanupDecision {
        script: flags.contains(RFlags::CLEANUP_SCRIPT),
        detach: flags.contains(RFlags::CLEANUP_DETACH),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compute_backoff_properties() {
        // E-9: generated restarts across the full i32 range — totality
        // (no panic, Fix #32), bounds [1, MAX_BACKOFF], the use_copy and
        // no_bin_exp overrides, and monotone non-decrease over the
        // meaningful range.
        let mut rng = crate::testutil::XorShift::new(0xB0FF_00F5);
        for _ in 0..5000 {
            let r = rng.next_u64() as i32;
            let b = compute_backoff(r, false, false);
            assert!((1..=MAX_BACKOFF).contains(&b), "restarts {r} → {b}");
            if r <= 0 {
                assert_eq!(b, 1, "negative restarts clamp to shift 0");
            }
            if r >= (BACKOFF_BITS - 2) as i32 {
                assert_eq!(b, MAX_BACKOFF, "shift capped at BACKOFF_BITS-2");
            }
            assert_eq!(compute_backoff(r, true, false), 1, "no_bin_exp → 1");
            if b > 1 {
                assert_eq!(compute_backoff(r, false, true), 1, "use_copy → 1");
            }
        }
        // Monotone non-decrease over 0..=BACKOFF_BITS.
        let mut prev = 0i64;
        for r in 0..=BACKOFF_BITS as i32 {
            let b = compute_backoff(r, false, false);
            assert!(b >= prev, "backoff decreased at restarts {r}");
            prev = b;
        }
    }

    #[test]
    fn test_terminate_init_failure_refresh_falls_through() {
        // C: manager.c:1078-1086 + 1154-1156 — init failure + SF_NO_BIN_EXP
        // sets RS_REFRESHING and falls into the refresh path.
        let d = terminate_decision(
            RFlags::IN_USE | RFlags::INITIALIZING,
            SysFlags::NO_BIN_EXP,
            0,
            false,
            false,
            false,
        );
        assert_eq!(d.action, TerminateAction::Refresh);
        assert!(d.mutations.set.contains(RFlags::REFRESHING));
    }

    #[test]
    fn test_terminate_init_failure_exit_falls_through() {
        // C: manager.c:1087-1091 + 1121-1152 — init failure otherwise sets
        // RS_EXITING and falls into the cleanup-all path.
        let d = terminate_decision(
            RFlags::IN_USE | RFlags::INITIALIZING,
            SysFlags::empty(),
            0,
            false,
            false,
            false,
        );
        assert!(matches!(
            d.action,
            TerminateAction::CleanupAll {
                norestart: false,
                ..
            }
        ));
        assert!(d.mutations.set.contains(RFlags::EXITING));
    }

    #[test]
    fn test_terminate_init_failure_rollback() {
        // C: manager.c:1071-1076 — init failure during update → rollback.
        let d = terminate_decision(
            RFlags::IN_USE | RFlags::INITIALIZING,
            SysFlags::empty(),
            0,
            false,
            false,
            true,
        );
        assert_eq!(d.action, TerminateAction::InitUpdateRollback);
        // manager.c:1075 — r_init_err = ERESTART travels as the payload.
        assert!(d.mutations.set.is_empty());
        assert_eq!(d.mutations.init_err, Some(minix_types::ERESTART));
    }

    #[test]
    fn test_terminate_norestart_flags() {
        // C: manager.c:1105-1120 — NORESTART + DET_RESTART + script.
        let d = terminate_decision(
            RFlags::IN_USE,
            SysFlags::NORESTART | SysFlags::DET_RESTART,
            2,
            true,
            false,
            false,
        );
        assert!(d.mutations.set.contains(RFlags::EXITING));
        assert!(d.mutations.set.contains(RFlags::CLEANUP_DETACH));
        assert!(d.mutations.set.contains(RFlags::CLEANUP_SCRIPT));
        assert!(matches!(
            d.action,
            TerminateAction::CleanupAll {
                norestart: true,
                ..
            }
        ));
    }

    #[test]
    fn test_terminate_detach_restart_limit() {
        // C: manager.c:1111-1114 — restarts >= MAX_DET_RESTART → no detach.
        let d = terminate_decision(
            RFlags::IN_USE,
            SysFlags::NORESTART | SysFlags::DET_RESTART,
            MAX_DET_RESTART,
            false,
            false,
            false,
        );
        assert!(d.mutations.set.contains(RFlags::EXITING));
        assert!(!d.mutations.set.contains(RFlags::CLEANUP_DETACH));
    }

    #[test]
    fn test_terminate_core_fatal() {
        // C: manager.c:1123-1126 — core service exit outside shutdown.
        let d = terminate_decision(
            RFlags::IN_USE | RFlags::EXITING,
            SysFlags::CORE_SRV,
            0,
            false,
            false, // not shutting down
            false,
        );
        assert!(matches!(
            d.action,
            TerminateAction::CleanupAll {
                core_fatal: true,
                ..
            }
        ));
        // Shutting down suppresses the fatal exit.
        let d2 = terminate_decision(
            RFlags::IN_USE | RFlags::EXITING,
            SysFlags::CORE_SRV,
            0,
            false,
            true,
            false,
        );
        assert!(matches!(
            d2.action,
            TerminateAction::CleanupAll {
                core_fatal: false,
                ..
            }
        ));
    }

    #[test]
    fn test_terminate_reincarnate() {
        // C: manager.c:1146-1151 — REINCARNATE survives into CleanupAll.
        let d = terminate_decision(
            RFlags::IN_USE | RFlags::EXITING | RFlags::REINCARNATE,
            SysFlags::empty(),
            0,
            false,
            false,
            false,
        );
        assert!(matches!(
            d.action,
            TerminateAction::CleanupAll {
                reincarnate: true,
                ..
            }
        ));
        // manager.c:1147 — `r_flags &= ~RS_REINCARNATE` before reincarnating
        // (R13: the payload carries the clear — set_flags could not).
        assert!(d.mutations.clear.contains(RFlags::REINCARNATE));
    }

    #[test]
    fn test_terminate_backoff_writes_slot() {
        // manager.c:1163-1174 — C writes `r_backoff = 1 << MIN(...)`,
        // capped, then collapses to 1 for SF_USE_COPY; the mutation payload
        // carries the computed value so the 15 wiring cannot forget it.
        let d = terminate_decision(RFlags::IN_USE, SysFlags::empty(), 4, false, false, false);
        assert!(matches!(d.action, TerminateAction::Backoff { backoff: 16 }));
        assert_eq!(d.mutations.backoff, Some(16));
        // SF_USE_COPY collapses to 1 (manager.c:1168-1169).
        let d = terminate_decision(RFlags::IN_USE, SysFlags::USE_COPY, 4, false, false, false);
        assert!(matches!(d.action, TerminateAction::Backoff { backoff: 1 }));
        assert_eq!(d.mutations.backoff, Some(1));
    }

    #[test]
    fn test_compute_backoff() {
        // C: manager.c:1163-1174 — 1 << min(restarts, 62), capped at 30.
        assert_eq!(compute_backoff(0, false, false), 1);
        assert_eq!(compute_backoff(1, false, false), 2);
        assert_eq!(compute_backoff(4, false, false), 16);
        assert_eq!(compute_backoff(10, false, false), MAX_BACKOFF); // capped
        assert_eq!(compute_backoff(3, true, false), 1); // SF_NO_BIN_EXP
        // SF_USE_COPY: backoff > 1 collapses to 1 (manager.c:1168-1169).
        assert_eq!(compute_backoff(3, false, true), 1);
    }

    #[test]
    fn test_compute_backoff_negative_returns_1() {
        // Negative restarts are unreachable from `terminate_decision` (the
        // `restarts > 0` gate, manager.c:1160), but C's
        // `1 << MIN(restarts, BACKOFF_BITS-2)` (manager.c:1164) is UB for a
        // negative shift; the pub API must fail closed, not inherit the UB.
        assert_eq!(compute_backoff(-1, false, false), 1);
        assert_eq!(compute_backoff(-100, false, false), 1);
        assert_eq!(compute_backoff(-1, true, false), 1); // SF_NO_BIN_EXP
        assert_eq!(compute_backoff(-1, false, true), 1); // SF_USE_COPY
    }

    #[test]
    fn test_script_reason() {
        // C: manager.c:1189-1193 — refresh / no-heartbeat / terminated.
        assert_eq!(script_reason(RFlags::REFRESHING), "restart");
        assert_eq!(script_reason(RFlags::NOPINGREPLY), "no-heartbeat");
        assert_eq!(script_reason(RFlags::IN_USE), "terminated");
    }

    #[test]
    fn test_late_reply_result() {
        // C: manager.c:1134-1136 — RS_DOWN/RS_REFRESH+norestart → OK.
        assert_eq!(late_reply_result(minix_types::RS_DOWN, false), 0);
        assert_eq!(late_reply_result(minix_types::RS_REFRESH, true), 0);
        assert_eq!(
            late_reply_result(minix_types::RS_REFRESH, false),
            minix_types::EDEADEPT
        );
        assert_eq!(
            late_reply_result(minix_types::RS_UP, false),
            minix_types::EDEADEPT
        );
    }

    #[test]
    fn test_cleanup_decision() {
        // C: manager.c:450-453 — script/detach flags.
        assert_eq!(
            cleanup_decision(RFlags::CLEANUP_SCRIPT | RFlags::CLEANUP_DETACH),
            CleanupDecision {
                script: true,
                detach: true
            }
        );
        assert_eq!(
            cleanup_decision(RFlags::IN_USE),
            CleanupDecision {
                script: false,
                detach: false
            }
        );
    }
}

// ── cleanup_service execution (R22a — manager.c:405-495) ────────────────────

/// SIGKILL for the phase-2 PM kill. C: `srv_kill(rp->r_pid, SIGKILL)` —
/// manager.c:469; SIGKILL = 9 (minix/include/signal.h:55).
pub const SIGKILL: i32 = 9;

/// Executes `cleanup_service` — the phase-aware slot teardown.
///
/// C: `cleanup_service` — manager.c:405-495, two phases keyed on `RS_DEAD`:
///
/// * **Phase 1** (first call): unlink the four instance chains (clearing the
///   neighbour's back-link), mark `RS_DEAD`, revoke the right to run
///   (`SYS_PRIV_DISALLOW`) and unblock IPC callers (`SYS_PRIV_CLEAR_IPC_REFS`),
///   clear `RS_ACTIVE`, send the pending late reply (`late_reply(rp, OK)`).
/// * **Phase 2** (second call, `RS_DEAD` set): unless detaching, stop the
///   scheduler and SIGKILL the process (both failures are warnings in C, not
///   fatal); run the cleanup script when `RS_CLEANUP_SCRIPT` asks (the bit is
///   consumed first); finally detach or free the slot — a reincarnating slot
///   is kept for reuse (manager.c:487-494).
///
/// Kernel effects go through `kernel`; `run_script` is the fork+execle seam
/// (ARCH A-1, 19). The detach branch keeps the slot alive under its detached
/// identity — the label republish (`detach_service`, manager.c:497-528) is
/// wired with 13; until then the branch consumes the flag and leaves the slot
/// in use, matching C's "not freed" outcome.
pub fn cleanup_service(
    table: &mut RProcTable,
    rp: SlotId,
    kernel: &mut dyn KernelApi,
    run_script: &mut dyn FnMut(&mut ServiceSlot) -> Result<(), Errno>,
) {
    if !table.get(rp).flags.contains(RFlags::DEAD) {
        // ── Phase 1 (manager.c:411-440) ──
        let (next, prev, new, old) = {
            let s = table.get(rp);
            (s.next_rp, s.prev_rp, s.new_rp, s.old_rp)
        };
        if let Some(n) = next {
            table.get_mut(n).prev_rp = None; // manager.c:423-425
        }
        if let Some(p) = prev {
            table.get_mut(p).next_rp = None; // manager.c:426-428
        }
        if let Some(n) = new {
            table.get_mut(n).old_rp = None; // manager.c:429-431
        }
        if let Some(o) = old {
            table.get_mut(o).new_rp = None; // manager.c:432-434
        }
        let endpoint = {
            let slot = table.get_mut(rp);
            if next.is_some() {
                slot.next_rp = None;
            }
            if prev.is_some() {
                slot.prev_rp = None;
            }
            if new.is_some() {
                slot.new_rp = None;
            }
            if old.is_some() {
                slot.old_rp = None;
            }
            slot.flags.insert(RFlags::DEAD); // manager.c:436
            slot.flags.remove(RFlags::ACTIVE); // manager.c:439
            slot.pub_.endpoint
        };
        // The service can no longer run; IPC callers are unblocked
        // (manager.c:438-439). C ignores both results (best effort).
        let _ = kernel.privctl(endpoint, PrivCtlOp::Disallow, None);
        let _ = kernel.privctl(endpoint, PrivCtlOp::ClearIpcRefs, None);

        // Send a late reply if there is any pending (manager.c:441 →
        // late_reply, utility.c:332-349: a fresh zero message whose
        // m_type = code).
        if table.get(rp).flags.contains(RFlags::LATEREPLY) {
            let _ = kernel.reply(endpoint, 0, &minix_types::Message::default());
            table.get_mut(rp).flags.remove(RFlags::LATEREPLY);
        }
        return;
    }

    // ── Phase 2 (manager.c:441-495) ──
    let cleanup_script = table.get(rp).flags.contains(RFlags::CLEANUP_SCRIPT);
    let detach = table.get(rp).flags.contains(RFlags::CLEANUP_DETACH);
    let reincarnate = table.get(rp).flags.contains(RFlags::REINCARNATE);

    // Cleanup the service when not detaching (manager.c:446-474).
    if !detach {
        // Tell the scheduler this process is finished — a failure is a
        // warning in C, not fatal (manager.c:461-465).
        let (scheduler, endpoint) = {
            let s = table.get(rp);
            (s.scheduler, s.pub_.endpoint)
        };
        let _ = kernel.sched_stop(scheduler, endpoint);

        // Ask PM to exit the service; pid -1 is warned about in C
        // (manager.c:466-473) — both "no pid" shapes skip the kill.
        let pid = table.get(rp).pid;
        if let Some(p) = pid.filter(|p| *p != -1) {
            let _ = kernel.srv_kill(p, SIGKILL);
        }
    }

    // Run the cleanup script when asked; the bit is consumed first
    // (manager.c:476-483). A script failure is a warning, not fatal.
    if cleanup_script {
        table.get_mut(rp).flags.remove(RFlags::CLEANUP_SCRIPT);
        let _ = run_script(table.get_mut(rp));
    }

    if detach {
        // Detach service when asked (manager.c:485-486): the slot stays
        // alive under its detached identity. The label republish
        // (`detach_service`, manager.c:497-528) is wired with 13.
        table.get_mut(rp).flags.remove(RFlags::CLEANUP_DETACH);
    } else if !reincarnate {
        // Free the slot otherwise, unless we're about to reuse it
        // (manager.c:488-494).
        table.free_slot(rp);
    }
}

// ── crash/kill/detach executors (R22b — manager.c:360-378/380-403/497-528) ──

/// The outcome of `crash_service`.
///
/// C: `crash_service` — manager.c:380-403: RS itself `exit(1)`s directly
/// (manager.c:395-397) — a self-termination the caller (06/18 wiring) turns
/// into RS's own run-loop shutdown; every other service is SIGKILLed via PM.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CrashOutcome {
    /// The crashing slot is RS itself: the caller must terminate the run
    /// loop (C: `exit(1)`).
    SelfTerminate,
    /// The service was SIGKILLed through PM.
    Signalled,
}

/// Simulates a crash in a system service.
///
/// C: `crash_service` — manager.c:380-403.
pub fn crash_service(
    slot: &ServiceSlot,
    kernel: &mut dyn KernelApi,
) -> Result<CrashOutcome, Errno> {
    if slot.pub_.endpoint == Endpoint::RS {
        return Ok(CrashOutcome::SelfTerminate); // manager.c:395-397
    }
    kernel.sys_kill(slot.pub_.endpoint, SIGKILL)?; // manager.c:399
    Ok(CrashOutcome::Signalled)
}

/// Crashes a system service and marks it as not-to-be-restarted.
///
/// C: `kill_service` — manager.c:360-378: sets `RS_EXITING` ("expect exit")
/// then crashes the service; the `errstr` printf (manager.c:365-367) is the
/// diagnostics face (R31, suppressed while `shutting_down`). C ignores the
/// crash result and returns the input `err` for propagation.
pub fn kill_service(slot: &mut ServiceSlot, kernel: &mut dyn KernelApi, err: Errno) -> Errno {
    slot.flags.insert(RFlags::EXITING); // manager.c:372
    let _ = crash_service(slot, kernel); // manager.c:373 — result ignored
    err
}

/// Detaches the given system service.
///
/// C: `detach_service` — manager.c:497-528: the service survives with a
/// unique `"{counter}.{label}"` identity (republished via DS), keeps running
/// (`RS_IN_USE | RS_ACTIVE`), loses its core/detach policy bits and its
/// monitoring-relevant configuration, and is re-allowed. `counter` is the C
/// static `detach_counter` — owned by the caller (`ServerState`); the
/// `ds_publish_label` effect is injected (11/19).
pub fn detach_service(
    slot: &mut ServiceSlot,
    kernel: &mut dyn KernelApi,
    counter: u64,
    ds_publish_label: &mut dyn FnMut(&Label, Endpoint),
) {
    // manager.c:503-513 — "{++detach_counter}.{label}" (NUL-safe truncation
    // to RS_MAX_LABEL_LEN).
    let old = slot.pub_.label.as_bytes();
    let old_len = old.iter().position(|&b| b == 0).unwrap_or(old.len());
    let mut new_label = [0u8; crate::service_slot::RS_MAX_LABEL_LEN];
    let mut off = 0;
    // Decimal digits of counter+1 (C: `snprintf("%lu.%s", ++detach_counter,
    // label)`), most-significant first, written directly into the label
    // buffer so no intermediate borrow exists.
    let mut digits = [0u8; 20];
    let mut n = counter.wrapping_add(1);
    let mut di = digits.len();
    loop {
        di -= 1;
        digits[di] = b'0' + (n % 10) as u8;
        n /= 10;
        if n == 0 {
            break;
        }
    }
    for &d in &digits[di..] {
        if off >= new_label.len() {
            break;
        }
        new_label[off] = d;
        off += 1;
    }
    if off < new_label.len() {
        new_label[off] = b'.';
        off += 1;
    }
    for &b in &old[..old_len] {
        if off >= new_label.len() {
            break;
        }
        new_label[off] = b;
        off += 1;
    }
    slot.pub_.label = Label::from_bytes(&new_label);
    ds_publish_label(&slot.pub_.label, slot.pub_.endpoint);

    // manager.c:519-524 — alive, demoted from core/detach policy, unmonitored.
    slot.flags = RFlags::IN_USE | RFlags::ACTIVE; // manager.c:520
    slot.pub_
        .sys_flags
        .remove(SysFlags::CORE_SRV | SysFlags::DET_RESTART); // manager.c:521
    slot.period = 0; // manager.c:522
    slot.pub_.dev_nr = 0; // manager.c:523
    slot.pub_.nr_domain = 0; // manager.c:524

    // Allow the service to run (manager.c:526-527) — result ignored in C.
    let _ = kernel.privctl(slot.pub_.endpoint, PrivCtlOp::Allow, None);
}

/// Signal class helpers — C `SIGS_IS_*` (sys/sys/signal.h:280-287).
///
/// Lethal: SIGILL(4)/SIGABRT(6)/SIGEMT(7)/SIGFPE(8)/SIGKILL(9)/SIGBUS(10)/
/// SIGSEGV(11); termination adds SIGPIPE(13).
pub fn sigs_is_lethal(signo: i32) -> bool {
    matches!(signo, 4 | 6 | 7 | 8 | 9 | 10 | 11)
}
/// C: `SIGS_IS_TERMINATION` — lethal + SIGPIPE(13).
pub fn sigs_is_termination(signo: i32) -> bool {
    sigs_is_lethal(signo) || signo == 13
}
/// C: `SIGS_IS_STACKTRACE` — lethal except SIGABRT (main.c:681-683).
pub fn sigs_is_stacktrace(signo: i32) -> bool {
    sigs_is_lethal(signo) && signo != 6
}

/// Collects the instance family of a service (rp + its replica chain).
///
/// C: `get_service_instances` — manager.c:1141 (via proto.h): rp, prev, next,
/// old, new — at most five rows.
pub fn get_service_instances(table: &RProcTable, rp: SlotId) -> alloc::vec::Vec<SlotId> {
    let mut instances = alloc::vec::Vec::new();
    instances.push(rp);
    let s = table.get(rp);
    if let Some(p) = s.prev_rp {
        instances.push(p);
    }
    if let Some(n) = s.next_rp {
        instances.push(n);
    }
    if let Some(o) = s.old_rp {
        instances.push(o);
    }
    if let Some(n2) = s.new_rp {
        instances.push(n2);
    }
    instances
}

/// The result of a [`terminate_service`] run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TerminateOutcome {
    /// C: `_exit(1)` — a core system service died outside shutdown
    /// (manager.c:1123-1126). The caller ends RS (R27's SelfTerminate shape).
    pub self_terminate: bool,
}

/// Restarts a service as if it were never started before.
///
/// C: `reincarnate_service` — manager.c:1033-1051: clone the slot, reset the
/// flags to bare `RS_IN_USE`, clear the endpoint index, run
/// `start_service(SEF_INIT_FRESH)` keeping the restart count (+1).
fn reincarnate_service(
    table: &mut RProcTable,
    old_rp: SlotId,
    kernel: &mut dyn KernelApi,
    ticks: Clock,
    asynsend: &mut dyn FnMut(Endpoint, &crate::ready::InitMessage) -> Result<(), Errno>,
    read_exec: &mut dyn FnMut(&mut ServiceSlot) -> Result<(), Errno>,
) {
    // manager.c:1036-1040 — clone failure is reported and ignored (the
    // service stays dead; the ping path will pick it up).
    let Ok(rp) = crate::service_create::clone_slot(table, old_rp) else {
        return;
    };
    {
        let r = table.get_mut(rp);
        r.flags = RFlags::IN_USE; // manager.c:1042
    }
    // manager.c:1043 — rproc_ptr[endpoint] = NULL (the fresh instance gets
    // its own endpoint from start_service).
    let ep = table.get(rp).pub_.endpoint;
    table.set_endpoint_index(ep, None);

    let restarts = table.get(rp).restarts; // manager.c:1045-1049
    let mut effects = crate::service_create::CreateEffects {
        asynsend: alloc::boxed::Box::new(asynsend),
        read_exec: alloc::boxed::Box::new(&mut *read_exec),
        ..Default::default()
    };
    let _ = crate::service_create::start_service(
        table,
        rp,
        kernel,
        0, // SEF_INIT_FRESH carries no script flags — fresh incarnation
        ticks,
        &mut effects,
    );
    table.get_mut(rp).restarts = restarts + 1;
}

/// Executes the [`terminate_decision`] for one service.
///
/// C: `terminate_service` — manager.c:1055-1166. The decision face
/// ([`terminate_decision`]) owns the flag tree; this executor runs its
/// effects: init-failure rollback (16 hook), global-update abort
/// ([`abort_update_proc`]), norestart arming, the EXITING path (core fatal,
/// late reply, unpublish hook, per-instance cleanup, reincarnate), the
/// REFRESHING restart, and the backoff branch.
/// The termination-family seam bundle (A-2 — §20.3): owned closures
/// (`Box<dyn FnMut>`) so a caller can pass real effects while tests use
/// noops. C: the implicit globals unpublish_service/run_script/rs_asynsend
/// (manager.c:1140/:1209, utility.c:223).
pub struct TerminateEffects<'a> {
    /// DS unpublish hook. C: unpublish_service — manager.c:1140 (11/19).
    pub unpublish: alloc::boxed::Box<dyn FnMut(SlotId) + 'a>,
    /// Recovery script hook. C: run_script — manager.c:1209 (15/19).
    pub run_script: alloc::boxed::Box<crate::service_create::SlotEffectFn<'a>>,
    /// Binary image load for the restart arms (refresh / first unexpected
    /// exit) and the reincarnate tail. C: read_exec — manager.c:1372, the
    /// implicit global in the same family as run_script/rs_asynsend.
    pub read_exec: alloc::boxed::Box<crate::service_create::SlotEffectFn<'a>>,
    /// RS_INIT async send. C: rs_asynsend — utility.c:223 (19).
    pub asynsend: alloc::boxed::Box<crate::service_create::AsynsendFn<'a>>,
}

pub fn terminate_service(
    table: &mut RProcTable,
    upd: &mut crate::live_update::UpdateState,
    rp: SlotId,
    kernel: &mut dyn KernelApi,
    ticks: Clock,
    shutting_down: bool,
    effects: &mut TerminateEffects,
) -> TerminateOutcome {
    let outcome = TerminateOutcome {
        self_terminate: false,
    };

    // The decision owns C's whole flag tree (manager.c:1067-1179); this
    // executor applies its payload and runs the effects. `SRV_IS_UPDATING`
    // is the slot-level `RS_UPDATING` flag.
    let slot_updating = table.get(rp).flags.contains(RFlags::UPDATING);
    let d = {
        let s = table.get(rp);
        terminate_decision(
            s.flags,
            s.pub_.sys_flags,
            s.restarts,
            !s.script.is_empty() && s.script[0] != 0,
            shutting_down,
            slot_updating,
        )
    };
    d.mutations.apply(table.get_mut(rp));

    match d.action {
        // manager.c:1071-1076 — end_update(r_init_err, RS_REPLY), then
        // r_init_err = ERESTART; the rollback is this round's whole job.
        TerminateAction::InitUpdateRollback => {
            let init_err = table.get(rp).init_err;
            let mut noop_req = |_: &ServiceSlot, _: i32| {};
            upd.end_update(
                table,
                kernel,
                init_err,
                crate::live_update::RS_REPLY,
                ticks,
                &mut crate::live_update::EndEffects {
                    request_prepare: &mut noop_req,
                    run_script: &mut effects.run_script,
                },
            );
            table.get_mut(rp).init_err = minix_types::ERESTART;
        }
        // manager.c:1154-1156 — refresh path: restart in place.
        TerminateAction::Refresh => {
            crate::service_create::restart_service(
                table,
                rp,
                kernel,
                ticks,
                &mut crate::service_create::RestartEffects {
                    read_exec: &mut *effects.read_exec,
                    run_script: &mut effects.run_script,
                    asynsend: &mut effects.asynsend,
                },
            );
        }
        // manager.c:1160-1175 — wait out the binary backoff (do_period
        // restarts when it drains).
        TerminateAction::Backoff { backoff } => {
            table.get_mut(rp).backoff = backoff;
        }
        // manager.c:1177-1179 — first unexpected exit: immediate restart.
        TerminateAction::Restart => {
            crate::service_create::restart_service(
                table,
                rp,
                kernel,
                ticks,
                &mut crate::service_create::RestartEffects {
                    read_exec: &mut *effects.read_exec,
                    run_script: &mut effects.run_script,
                    asynsend: &mut effects.asynsend,
                },
            );
        }
        TerminateAction::CleanupAll {
            norestart,
            reincarnate,
            core_fatal,
        } => {
            // manager.c:1093-1097 — end a running update before any recovery.
            if upd
                .flags
                .contains(crate::live_update::RupdateFlags::UPDATING)
            {
                let _ = crate::live_update::abort_update_proc(
                    upd,
                    table,
                    kernel,
                    minix_types::ERESTART,
                    ticks,
                    &mut effects.run_script,
                );
            }

            // manager.c:1121-1126 — a core service exiting outside shutdown
            // is fatal for RS itself (`_exit(1)`).
            if core_fatal && !shutting_down {
                return TerminateOutcome {
                    self_terminate: true,
                };
            }

            // manager.c:1128-1133 — abort a scheduled update when one of its
            // services is exiting.
            if !upd.chain.is_empty()
                && !upd
                    .flags
                    .contains(crate::live_update::RupdateFlags::UPDATING)
            {
                let _ = crate::live_update::abort_update_proc(
                    upd,
                    table,
                    kernel,
                    minix_types::EDEADSRCDST,
                    ticks,
                    &mut effects.run_script,
                );
            }

            // manager.c:1135-1138 — the late reply: OK for RS_DOWN (and a
            // norestart RS_REFRESH), EDEADEPT otherwise.
            let r = {
                let s = table.get(rp);
                if s.caller_request == minix_types::RS_DOWN
                    || (s.caller_request == minix_types::RS_REFRESH && norestart)
                {
                    0
                } else {
                    minix_types::EDEADEPT
                }
            };
            let caller = table.get(rp).caller;
            let _ = kernel.reply(caller, r, &minix_types::Message::default());

            // manager.c:1140 — unpublish (the DS effect is the 11/19 seam hook).
            (effects.unpublish)(rp);

            // manager.c:1141-1143 — cleanup every instance of the service.
            for inst in get_service_instances(table, rp) {
                cleanup_service(table, inst, kernel, &mut effects.run_script);
            }

            // manager.c:1145-1151 — reincarnate after cleanup (the decision
            // already cleared RS_REINCARNATE via `mutations.clear`).
            if reincarnate {
                reincarnate_service(
                    table,
                    rp,
                    kernel,
                    ticks,
                    &mut effects.asynsend,
                    &mut *effects.read_exec,
                );
            }
        }
    }
    outcome
}

/// Cleans up dead services and recreates missing replicas when RS is idle.
///
/// C: `rs_idle_period` — utility.c:441-478. During shutdown the idle gate is
/// overridden (dead services must be cleaned to avoid deadlocks); otherwise
/// the replica pass is skipped too.
pub fn rs_idle_period(
    table: &mut RProcTable,
    kernel: &mut dyn KernelApi,
    ticks: Clock,
    shutting_down: bool,
    run_script: &mut dyn FnMut(&mut ServiceSlot) -> Result<(), Errno>,
    read_exec: &mut dyn FnMut(&mut ServiceSlot) -> Result<(), Errno>,
    _asynsend: &mut dyn FnMut(Endpoint, &crate::ready::InitMessage) -> Result<(), Errno>,
) {
    // utility.c:448-453 — not much to do when RS is not idle (the shutdown
    // override keeps dead-service cleanup running).
    if !shutting_down && !table.iter_in_use().all(|(_, s)| s.flags.is_idle()) {
        return;
    }

    // utility.c:455-462 — clean up dead services.
    let dead: alloc::vec::Vec<SlotId> = table
        .iter_all()
        .filter(|(_, s)| s.flags.contains(RFlags::IN_USE) && s.flags.contains(RFlags::DEAD))
        .map(|(id, _)| id)
        .collect();
    for id in dead {
        cleanup_service(table, id, kernel, run_script);
    }

    if shutting_down {
        return;
    }

    // utility.c:464-477 — create missing replicas (one at a time for VM
    // during/after an update).
    let need: alloc::vec::Vec<SlotId> = table
        .iter_in_use()
        .filter(|(_, s)| {
            s.flags.contains(RFlags::ACTIVE)
                && s.pub_.sys_flags.contains(SysFlags::USE_REPL)
                && s.next_rp.is_none()
        })
        .map(|(id, _)| id)
        .collect();
    for id in need {
        let (vm_pending, ep) = {
            let s = table.get(id);
            (s.old_rp.is_some() || s.new_rp.is_some(), s.pub_.endpoint)
        };
        if ep == Endpoint::VM && vm_pending {
            continue; // utility.c:470-473
        }
        let _ = crate::service_create::clone_service(
            table,
            id,
            kernel,
            crate::privilege::PrivFlags::RST_SYS_PROC,
            0,
            ticks,
            read_exec,
        );
    }
}
