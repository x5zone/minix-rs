//! Periodic checks and heartbeat monitoring.
//!
//! Mirrors `minix3/minix/servers/rs/request.c:943-1090` — `do_period`
//! (943-1046) and `do_sigchld` (1051-1090) — plus the update-prepare timeout
//! `update_period` (update.c:371-396). 07-rs-period-heartbeat.md.
//!
//! This module holds the *decision logic* as pure functions over slot state;
//! the side effects (`restart_service`, `crash_service`, `ipc_notify`,
//! `sys_setalarm`, `waitpid`) are owned by 15/10/19 and injected by the
//! future main-loop integration (06).

use crate::process_table::RProcTable;
use crate::service_slot::{RFlags, ServiceSlot};
use alloc::vec::Vec;
use minix_types::Pid;

/// C: `RS_INIT_T = system_hz * 10` — const.h:48.
pub fn init_timeout(hz: u32) -> i64 {
    hz as i64 * 10
}

/// C: `RS_DELTA_T = system_hz` — const.h:49.
pub fn delta_t(hz: u32) -> i64 {
    hz as i64
}

/// C: `MAX_BACKOFF = 30` — const.h:51.
pub const MAX_BACKOFF: u32 = 30;

/// C: `RS_DEFAULT_PREPARE_MAXTIME = 2*RS_DELTA_T` — const.h:58.
pub fn default_prepare_maxtime(hz: u32) -> i64 {
    2 * delta_t(hz)
}

/// Per-slot decision of `do_period` for one active service.
///
/// C: request.c:975-1038. Each variant corresponds to one branch of the
/// if/else chain; the caller performs the associated side effect.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PeriodAction {
    /// No action for this slot.
    Nothing,
    /// `r_backoff > 0`: decremented, still positive (request.c:975-976).
    BackoffTick,
    /// `r_backoff` hit zero → `restart_service` (request.c:977-978, 15).
    Restart,
    /// SIGTERM timeout (2×`RS_DELTA_T`) → `crash_service` (request.c:985-989, 15).
    StopTimeoutCrash,
    /// Period expired, no answer pending → `ipc_notify` + `r_check_tm = now`
    /// (request.c:1035-1037).
    PingRequest,
    /// Ping answer overdue (2×period) and no free pass → `crash_service`;
    /// `NOPINGREPLY` set, `r_init_err = EINTR` if initializing
    /// (request.c:1004-1028).
    PingTimeoutCrash,
    /// Ping answer overdue but another service is initializing → free pass:
    /// `r_alive_tm = now`, `r_check_tm = now+1` (request.c:1015-1021).
    FreePass,
}

/// Computes the effective period for a slot.
///
/// C: request.c:965-969 — initializing slots use `UPD_INIT_MAXTIME` (when
/// updating) or `RS_INIT_T`; otherwise `r_period`.
pub fn effective_period(rp: &ServiceSlot, hz: u32) -> i64 {
    if rp.flags.contains(RFlags::INITIALIZING) {
        if rp.flags.contains(RFlags::UPDATING) {
            // C: `UPD_INIT_MAXTIME(&rp->r_upd)` — const.h:116. The
            // `prepare_maxtime` override lives in the update descriptor (16);
            // the default is `RS_DEFAULT_PREPARE_MAXTIME`.
            default_prepare_maxtime(hz)
        } else {
            init_timeout(hz)
        }
    } else {
        rp.period
    }
}

/// The per-slot decision for a single `do_period` pass.
///
/// C: request.c:975-1038. `another_initializing` is the caller's
/// `lookup_slot_by_flags(RS_INITIALIZING) != NULL` probe
/// (request.c:1013), and `is_updating` is `SRV_IS_UPDATING(rp)`
/// (const.h:114).
pub fn period_decision(
    now: i64,
    rp: &ServiceSlot,
    hz: u32,
    another_initializing: bool,
    is_updating: bool,
) -> PeriodAction {
    // Binary backoff: revive after MAX_BACKOFF periods of repeated exits
    // (request.c:975-978).
    if rp.backoff > 0 {
        return if rp.backoff == 1 {
            PeriodAction::Restart
        } else {
            PeriodAction::BackoffTick
        };
    }

    // SIGTERM without response → SIGKILL (simulated crash, request.c:985-989).
    if rp.stop_tm > 0 && now.saturating_sub(rp.stop_tm) > 2 * delta_t(hz) && rp.pid.is_some() {
        return PeriodAction::StopTimeoutCrash;
    }

    let period = effective_period(rp, hz);
    if period == 0 {
        return PeriodAction::Nothing; // no status checks for period-0 services
    }

    // Answer to a status request is still pending (request.c:1004).
    if rp.alive_tm < rp.check_tm {
        let overdue = now.saturating_sub(rp.alive_tm) > 2 * period
            && rp.pid.is_some()
            && !rp.flags.contains(RFlags::NOPINGREPLY);
        if !overdue {
            return PeriodAction::Nothing;
        }
        // Free pass while somebody else is initializing (request.c:1015-1017).
        if another_initializing && !is_updating {
            return PeriodAction::FreePass;
        }
        return PeriodAction::PingTimeoutCrash;
    }

    // No answer pending: request status when the period expired
    // (request.c:1035-1037).
    if now.saturating_sub(rp.check_tm) > period {
        return PeriodAction::PingRequest;
    }
    PeriodAction::Nothing
}

/// Whether the update-preparation phase timed out.
///
/// C: `update_period` — update.c:385-386: `prepare_maxtime > 0 && now -
/// prepare_tm > prepare_maxtime` → `end_update(EINTR, RS_CANCEL)` (16).
pub fn has_update_timed_out(now: i64, prepare_tm: i64, prepare_maxtime: i64) -> bool {
    prepare_maxtime > 0 && now.saturating_sub(prepare_tm) > prepare_maxtime
}

/// Outcome of a `do_sigchld` pass for one exited pid.
///
/// C: request.c:1051-1090 — `waitpid(-1, &status, WNOHANG)`; the slot is
/// looked up by pid; all instances of the service are freed and, if any was
/// updating, the update chain is cleared (`rupdate_clear_upds`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SigchldOutcome {
    /// Whether an updating instance had its update bits cleared
    /// (`found` — request.c:1080-1081).
    pub update_cleared: bool,
}

/// Applies the `do_sigchld` instance logic to the table.
///
/// Pure table pass: frees every instance slot of the service with the given
/// pid (C: `get_service_instances` + `free_slot`, request.c:1077-1083) and
/// reports whether any was updating (`SRV_IS_UPDATING`, const.h:114). The
/// caller owns `rupdate_clear_upds` (16) and the actual `waitpid` loop.
pub fn sigchld_cleanup(table: &mut RProcTable, pid: Pid) -> Option<SigchldOutcome> {
    let target = table.lookup_by_pid(pid)?;
    let instances: Vec<_> = table.instances_of(target).collect();
    let mut update_cleared = false;
    for id in instances {
        let rp = table.get_mut(id);
        if rp.flags.contains(RFlags::UPDATING) {
            // C: request.c:1080 — clear the per-slot update bits.
            rp.flags.remove(RFlags::UPDATING);
            rp.flags.remove(RFlags::PREPARE_DONE);
            rp.flags.remove(RFlags::INIT_DONE);
            rp.flags.remove(RFlags::INIT_PENDING);
            update_cleared = true;
        }
        table.free_slot(id); // manager.c:2088-2109 (15)
    }
    Some(SigchldOutcome { update_cleared })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::service_slot::{Label, SlotId};

    fn slot() -> ServiceSlot {
        let mut s = ServiceSlot::vacant();
        s.flags = RFlags::IN_USE | RFlags::ACTIVE;
        s.pid = Some(100);
        s.period = 5;
        s
    }

    #[test]
    fn test_effective_period_normal() {
        let s = slot();
        assert_eq!(effective_period(&s, 60), 5);
    }

    #[test]
    fn test_effective_period_initializing() {
        let mut s = slot();
        s.flags |= RFlags::INITIALIZING;
        assert_eq!(effective_period(&s, 60), 600); // RS_INIT_T = hz*10
        s.flags |= RFlags::UPDATING;
        assert_eq!(effective_period(&s, 60), 120); // default prepare maxtime
    }

    #[test]
    fn test_backoff_restart() {
        let mut s = slot();
        s.backoff = 2;
        assert_eq!(
            period_decision(0, &s, 60, false, false),
            PeriodAction::BackoffTick
        );
        s.backoff = 1;
        assert_eq!(
            period_decision(0, &s, 60, false, false),
            PeriodAction::Restart
        );
    }

    #[test]
    fn test_stop_timeout() {
        let mut s = slot();
        s.stop_tm = 100;
        // now = 100 + 2*hz + 1 → over 2*RS_DELTA_T (request.c:985).
        assert_eq!(
            period_decision(100 + 2 * 60 + 1, &s, 60, false, false),
            PeriodAction::StopTimeoutCrash
        );
        // Within the window → no stop-crash; a fresh check time also
        // suppresses the ping branch.
        s.check_tm = 1000;
        s.alive_tm = 1000; // replied at check time → no ping pending
        assert_eq!(
            period_decision(100 + 60, &s, 60, false, false),
            PeriodAction::Nothing
        );
    }

    #[test]
    fn test_ping_timeout_with_free_pass() {
        let mut s = slot();
        s.check_tm = 10; // ping sent at 10
        s.alive_tm = 5; // no reply since (alive < check)
        // 2*period = 10 → at now=21 (> 10 past alive_tm) overdue.
        assert_eq!(
            period_decision(21, &s, 60, true, false),
            PeriodAction::FreePass
        );
        // No other initializing service → crash.
        assert_eq!(
            period_decision(21, &s, 60, false, false),
            PeriodAction::PingTimeoutCrash
        );
        // Service updating → no free pass either.
        assert_eq!(
            period_decision(21, &s, 60, true, true),
            PeriodAction::PingTimeoutCrash
        );
    }

    #[test]
    fn test_ping_request() {
        let mut s = slot();
        s.check_tm = 10;
        s.alive_tm = 10; // replied at 10, check at 10 → not pending
        assert_eq!(
            period_decision(10 + 5 + 1, &s, 60, false, false),
            PeriodAction::PingRequest
        );
        // Within the period → nothing.
        assert_eq!(
            period_decision(10 + 3, &s, 60, false, false),
            PeriodAction::Nothing
        );
    }

    #[test]
    fn test_nopingreply_blocks_crash() {
        let mut s = slot();
        s.check_tm = 10;
        s.alive_tm = 5;
        s.flags |= RFlags::NOPINGREPLY;
        assert_eq!(
            period_decision(21, &s, 60, false, false),
            PeriodAction::Nothing // NOPINGREPLY → no further pings (request.c:1006)
        );
    }

    #[test]
    fn test_update_timeout() {
        assert!(has_update_timed_out(100, 50, 30));
        assert!(!has_update_timed_out(80, 50, 30));
        assert!(!has_update_timed_out(100, 50, 0)); // maxtime 0 = no timeout
    }

    #[test]
    fn test_sigchld_cleanup() {
        let mut t = RProcTable::new();
        let a = t.alloc_slot().unwrap();
        t.get_mut(a).flags = RFlags::IN_USE | RFlags::ACTIVE;
        let b = t.alloc_slot().unwrap();
        // Active instance (pid 100) + updating replica (same service).
        t.get_mut(a).pid = Some(100);
        t.get_mut(a).pub_.proc_name = Label::from_bytes(b"tty");
        t.get_mut(b).flags = RFlags::IN_USE | RFlags::UPDATING | RFlags::PREPARE_DONE;
        t.get_mut(b).pid = Some(100);
        t.get_mut(b).pub_.proc_name = Label::from_bytes(b"tty");
        t.get_mut(a).next_rp = Some(SlotId::new(1));
        t.get_mut(b).prev_rp = Some(SlotId::new(0));

        let out = sigchld_cleanup(&mut t, 100).unwrap();
        assert!(out.update_cleared);
        // Both instance slots freed.
        assert!(!t.get(a).flags.contains(RFlags::IN_USE));
        assert!(!t.get(b).flags.contains(RFlags::IN_USE));
    }
}
