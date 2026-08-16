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

use crate::service_slot::{RFlags, SlotMutations, SysFlags};

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
