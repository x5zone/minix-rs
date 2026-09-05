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
use crate::service_slot::{RFlags, ServiceSlot, SlotMutations};
use alloc::vec::Vec;
use minix_types::{Clock, Pid};

/// C: `RS_INIT_T = system_hz * 10` — const.h:48.
pub fn init_timeout(hz: u32) -> i64 {
    hz as i64 * 10
}

/// C: `RS_DELTA_T = system_hz` — const.h:49.
pub fn delta_t(hz: u32) -> i64 {
    hz as i64
}

/// C: `RS_DEFAULT_PREPARE_MAXTIME = 2*RS_DELTA_T` — const.h:58.
pub fn default_prepare_maxtime(hz: u32) -> i64 {
    2 * delta_t(hz)
}

/// C: `UPD_INIT_MAXTIME(&rp->r_upd)` — const.h:116. The update descriptor's
/// `prepare_maxtime` override wins only when it differs from the default
/// (`RS_DEFAULT_PREPARE_MAXTIME`); otherwise the init timeout `RS_INIT_T`
/// applies. `prepare_maxtime` is not modelled yet (16-rs-live-update,
/// update-descriptor timing fields DEFERRED), so callers pass `None` for the
/// unmodelled default; the 16 wiring replaces it with the descriptor value.
pub fn upd_init_maxtime(hz: u32, prepare_maxtime: Option<i64>) -> i64 {
    match prepare_maxtime {
        Some(pm) if pm != default_prepare_maxtime(hz) => pm,
        _ => init_timeout(hz),
    }
}

/// Per-slot decision of `do_period` for one active service.
///
/// C: request.c:975-1038. Each variant corresponds to one branch of the
/// if/else chain; the caller performs the associated side effect.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PeriodAction {
    /// No action for this slot.
    Nothing,
    /// `r_backoff > 0`: decremented, still positive (request.c:975-976); the
    /// decrement is carried as [`PeriodDecision::mutations`] (R13).
    BackoffTick,
    /// `r_backoff` hit zero → `restart_service` (request.c:977-978, 15).
    Restart,
    /// SIGTERM timeout (2×`RS_DELTA_T`) → `crash_service` (request.c:985-989, 15).
    StopTimeoutCrash,
    /// Period expired, no answer pending → `ipc_notify` (request.c:1035-1037);
    /// `r_check_tm = now` travels in [`PeriodDecision::mutations`] (R13).
    PingRequest,
    /// Ping answer overdue (2×period) and no free pass → `crash_service`;
    /// the `NOPINGREPLY` / `r_init_err = EINTR` mutations travel in
    /// [`PeriodDecision::mutations`] (request.c:1004-1028, R13).
    PingTimeoutCrash,
    /// Ping answer overdue but another service is initializing → free pass:
    /// the `r_alive_tm`/`r_check_tm` updates travel in
    /// [`PeriodDecision::mutations`] (request.c:1015-1021, R13).
    FreePass,
}

/// The `do_period` decision for one slot plus the slot mutations it implies.
///
/// R13: C mutates `r_*` inline (request.c:975-1038); the mutations are
/// carried as a typed payload so the 06 caller applies exactly the decision's
/// side effects (`mutations.apply(rp)`) after the action hook.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PeriodDecision {
    /// Branch to execute.
    pub action: PeriodAction,
    /// Slot mutations implied by the branch (R13).
    pub mutations: SlotMutations,
}

/// Computes the effective period for a slot.
///
/// C: request.c:965-969 — initializing slots use `UPD_INIT_MAXTIME` (when
/// updating) or `RS_INIT_T`; otherwise `r_period`.
pub fn effective_period(rp: &ServiceSlot, hz: u32) -> i64 {
    if rp.flags.contains(RFlags::INITIALIZING) {
        if rp.flags.contains(RFlags::UPDATING) {
            // C: `UPD_INIT_MAXTIME(&rp->r_upd)` — const.h:116. The
            // `prepare_maxtime` override lives in the update descriptor
            // (16, DEFERRED); unmodelled → C default branch (`RS_INIT_T`).
            upd_init_maxtime(hz, None)
        } else {
            init_timeout(hz)
        }
    } else {
        rp.period
    }
}

/// Heartbeat refresh: a service's notify carries the kernel timestamp and
/// the main loop writes it into the slot's alive marker.
///
/// C: main.c:85-91 — `rproc_ptr[who_p]->r_alive_tm = m.m_notify.timestamp`
/// (`m_notify.timestamp` is `u64_t`, ipc.h:1715 — "valid for every notify
/// msg"). R13 payload style: the decision returns the mutation, the 06
/// caller applies it after the endpoint lookup. The NULL-slot branch
/// (main.c:89-90, "unexpected notify" warning) stays with the caller — it
/// owns the table access.
pub fn heartbeat_mutations(timestamp: Clock) -> SlotMutations {
    SlotMutations {
        alive_tm: Some(timestamp),
        ..SlotMutations::default()
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
) -> PeriodDecision {
    // Binary backoff: revive after MAX_BACKOFF periods of repeated exits
    // (request.c:975-978). The decrement is carried as the new absolute value.
    if rp.backoff > 0 {
        return if rp.backoff == 1 {
            PeriodDecision {
                action: PeriodAction::Restart,
                mutations: SlotMutations {
                    backoff: Some(0), // r_backoff -= 1 → 0 (request.c:976)
                    ..Default::default()
                },
            }
        } else {
            PeriodDecision {
                action: PeriodAction::BackoffTick,
                mutations: SlotMutations {
                    backoff: Some(rp.backoff - 1), // request.c:976
                    ..Default::default()
                },
            }
        };
    }

    // SIGTERM without response → SIGKILL (simulated crash, request.c:985-989).
    // R19: C tests `r_pid > 0` (strictly positive) — `Some(0)` is "no pid".
    if rp.stop_tm > 0
        && now.saturating_sub(rp.stop_tm) > 2 * delta_t(hz)
        && rp.pid.is_some_and(|p| p > 0)
    {
        return PeriodDecision {
            action: PeriodAction::StopTimeoutCrash,
            mutations: SlotMutations {
                stop_tm: Some(0), // request.c:988
                ..Default::default()
            },
        };
    }

    let period = effective_period(rp, hz);
    if period == 0 {
        return PeriodDecision {
            action: PeriodAction::Nothing, // no status checks for period-0 services
            mutations: SlotMutations::default(),
        };
    }

    // Answer to a status request is still pending (request.c:1004). R19: C
    // requires `r_pid > 0` (request.c:1007) — a 0 pid is "no process".
    if rp.alive_tm < rp.check_tm {
        let overdue = now.saturating_sub(rp.alive_tm) > 2 * period
            && rp.pid.is_some_and(|p| p > 0)
            && !rp.flags.contains(RFlags::NOPINGREPLY);
        if !overdue {
            return PeriodDecision {
                action: PeriodAction::Nothing,
                mutations: SlotMutations::default(),
            };
        }
        // Free pass while somebody else is initializing (request.c:1015-1017).
        if another_initializing && !is_updating {
            return PeriodDecision {
                action: PeriodAction::FreePass,
                // request.c:1019-1020
                mutations: SlotMutations {
                    alive_tm: Some(now),
                    check_tm: Some(now + 1),
                    ..Default::default()
                },
            };
        }
        return PeriodDecision {
            action: PeriodAction::PingTimeoutCrash,
            // request.c:1024 (`|= RS_NOPINGREPLY`) + request.c:1027-1028
            // (`r_init_err = EINTR` when still initializing).
            mutations: SlotMutations {
                set: RFlags::NOPINGREPLY,
                init_err: rp
                    .flags
                    .contains(RFlags::INITIALIZING)
                    .then_some(minix_types::EINTR),
                ..Default::default()
            },
        };
    }

    // No answer pending: request status when the period expired
    // (request.c:1035-1037). N2: C compares against the **raw** `r_period`
    // here — NOT the effective period computed at request.c:965-969. For an
    // initializing boot slot (`r_period = 0`, main.c:338) the effective
    // period is `RS_INIT_T`, but C pings on every tick (`now - r_check_tm
    // > 0`); using the effective value here would stretch the ping rhythm
    // 10× (todo §11 N2, 07-rs-period-heartbeat.md).
    if now.saturating_sub(rp.check_tm) > rp.period {
        return PeriodDecision {
            action: PeriodAction::PingRequest,
            mutations: SlotMutations {
                check_tm: Some(now), // request.c:1036
                ..Default::default()
            },
        };
    }
    PeriodDecision {
        action: PeriodAction::Nothing,
        mutations: SlotMutations::default(),
    }
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

    fn action(d: PeriodDecision) -> PeriodAction {
        d.action
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
        // C: UPD_INIT_MAXTIME default branch — prepare_maxtime unmodelled → RS_INIT_T.
        assert_eq!(effective_period(&s, 60), 600);
    }

    #[test]
    fn test_upd_init_maxtime() {
        // C: const.h:116 — override wins only when != RS_DEFAULT_PREPARE_MAXTIME.
        assert_eq!(upd_init_maxtime(60, None), 600); // unmodelled → RS_INIT_T
        assert_eq!(upd_init_maxtime(60, Some(300)), 300); // explicit override
        assert_eq!(upd_init_maxtime(60, Some(120)), 600); // == default → RS_INIT_T
    }

    #[test]
    fn test_heartbeat_mutations_refresh_alive_tm() {
        // C: main.c:85-91 — the notify timestamp lands in `r_alive_tm`
        // verbatim; nothing else on the slot may move.
        let mut s = slot();
        let before = (s.flags, s.check_tm, s.stop_tm);
        heartbeat_mutations(777).apply(&mut s);
        assert_eq!(s.alive_tm, 777);
        assert_eq!((s.flags, s.check_tm, s.stop_tm), before);
    }

    #[test]
    fn test_backoff_restart() {
        let mut s = slot();
        s.backoff = 2;
        let d = period_decision(0, &s, 60, false, false);
        assert_eq!(d.action, PeriodAction::BackoffTick);
        assert_eq!(d.mutations.backoff, Some(1)); // r_backoff 2 → 1
        s.backoff = 1;
        let d = period_decision(0, &s, 60, false, false);
        assert_eq!(d.action, PeriodAction::Restart);
        assert_eq!(d.mutations.backoff, Some(0)); // r_backoff 1 → 0
    }

    #[test]
    fn test_stop_timeout() {
        let mut s = slot();
        s.stop_tm = 100;
        // now = 100 + 2*hz + 1 → over 2*RS_DELTA_T (request.c:985).
        let d = period_decision(100 + 2 * 60 + 1, &s, 60, false, false);
        assert_eq!(d.action, PeriodAction::StopTimeoutCrash);
        assert_eq!(d.mutations.stop_tm, Some(0)); // request.c:988
        // Within the window → no stop-crash; a fresh check time also
        // suppresses the ping branch.
        s.check_tm = 1000;
        s.alive_tm = 1000; // replied at check time → no ping pending
        assert_eq!(
            action(period_decision(100 + 60, &s, 60, false, false)),
            PeriodAction::Nothing
        );
    }

    #[test]
    fn test_zero_pid_is_no_process() {
        // R19: C tests `r_pid > 0` (request.c:987/1007) — `Some(0)` must not
        // count as a live process (fail-closed, in case getnpid lands a 0).
        let mut s = slot();
        s.pid = Some(0);
        s.stop_tm = 100;
        s.check_tm = 1000; // suppress the ping branch — isolate the stop path
        s.alive_tm = 1000;
        assert_eq!(
            action(period_decision(100 + 2 * 60 + 1, &s, 60, false, false)),
            PeriodAction::Nothing // stop-timeout crash suppressed
        );
        let mut s2 = slot();
        s2.pid = Some(0);
        s2.check_tm = 10;
        s2.alive_tm = 5;
        assert_eq!(
            action(period_decision(21, &s2, 60, false, false)),
            PeriodAction::Nothing // ping-timeout crash suppressed
        );
    }

    #[test]
    fn test_ping_timeout_with_free_pass() {
        let mut s = slot();
        s.check_tm = 10; // ping sent at 10
        s.alive_tm = 5; // no reply since (alive < check)
        // 2*period = 10 → at now=21 (> 10 past alive_tm) overdue.
        let d = period_decision(21, &s, 60, true, false);
        assert_eq!(d.action, PeriodAction::FreePass);
        // request.c:1019-1020 — r_alive_tm = now, r_check_tm = now+1.
        assert_eq!(d.mutations.alive_tm, Some(21));
        assert_eq!(d.mutations.check_tm, Some(22));
        // No other initializing service → crash.
        let d = period_decision(21, &s, 60, false, false);
        assert_eq!(d.action, PeriodAction::PingTimeoutCrash);
        // request.c:1024 — NOPINGREPLY set; the slot is not initializing
        // here, so no r_init_err write (request.c:1027-1028).
        assert!(d.mutations.set.contains(RFlags::NOPINGREPLY));
        assert_eq!(d.mutations.init_err, None);
        // Initializing slot → r_init_err = EINTR (request.c:1027-1028).
        // The effective period is RS_INIT_T (600 at hz=60), so the overdue
        // threshold is 2*600 — probe far enough past it.
        let mut s_init = s.clone();
        s_init.flags |= RFlags::INITIALIZING;
        let d = period_decision(1300, &s_init, 60, false, false);
        assert_eq!(d.action, PeriodAction::PingTimeoutCrash);
        assert_eq!(d.mutations.init_err, Some(minix_types::EINTR));
        // Service updating → no free pass either.
        assert_eq!(
            action(period_decision(21, &s, 60, true, true)),
            PeriodAction::PingTimeoutCrash
        );
    }

    #[test]
    fn test_ping_request() {
        let mut s = slot();
        s.check_tm = 10;
        s.alive_tm = 10; // replied at 10, check at 10 → not pending
        let d = period_decision(10 + 5 + 1, &s, 60, false, false);
        assert_eq!(d.action, PeriodAction::PingRequest);
        assert_eq!(d.mutations.check_tm, Some(16)); // r_check_tm = now
        // Within the period → nothing.
        assert_eq!(
            action(period_decision(10 + 3, &s, 60, false, false)),
            PeriodAction::Nothing
        );
    }

    #[test]
    fn test_initializing_zero_period_pings_every_tick() {
        // N2: request.c:1035 compares against the raw `r_period`. A boot
        // slot with r_period=0 (main.c:338) is pinged on every tick while
        // initializing — the effective period (RS_INIT_T) gates the branch
        // but does NOT set the ping rhythm.
        let mut s = slot();
        s.flags |= RFlags::INITIALIZING;
        s.period = 0;
        s.check_tm = 100;
        s.alive_tm = 100; // no answer pending
        assert_eq!(
            action(period_decision(101, &s, 60, false, false)),
            PeriodAction::PingRequest
        );
        // Non-initializing period-0 slot → no pings at all (request.c:972
        // `period > 0` gate).
        let mut s2 = slot();
        s2.period = 0;
        s2.check_tm = 100;
        s2.alive_tm = 100;
        assert_eq!(
            action(period_decision(101, &s2, 60, false, false)),
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
            action(period_decision(21, &s, 60, false, false)),
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
