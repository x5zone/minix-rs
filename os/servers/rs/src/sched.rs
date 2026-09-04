//! Scheduling initialization and signal-manager update primitives.
//!
//! Mirrors `minix3/minix/servers/rs/utility.c:364-422` (`sched_init_proc`,
//! `update_sig_mgrs`). The external syscalls (`sched_start`'s
//! `sys_schedctl`/`SCHEDULING_START` branch, `sys_getpriv`, `sys_privctl`)
//! are performed by the **shell** (the 19 wiring layer / boot Step 2); this
//! module only holds pure decisions and effects (T5 — the monitor /
//! functional core pattern of 07-rs-period-heartbeat.md). `KernelApi`
//! never appears here; the production wiring is deferred to
//! 19-rs-external-interfaces.md (fail-closed until then — ARCH A-12).

use crate::privilege::Privilege;
use minix_types::Endpoint;

/// Highest priority for user processes. C: `MAX_USER_Q` — config.h:68.
pub const MAX_USER_Q: i32 = 0;
/// Lowest priority for user processes. C: `MIN_USER_Q` — config.h:71
/// (`NR_SCHED_QUEUES - 1`).
pub const MIN_USER_Q: i32 = NR_SCHED_QUEUES - 1;
/// Default scheduling priority for services. C: `USER_Q` — config.h:69:
/// `((MIN_USER_Q - MAX_USER_Q) / 2 + MAX_USER_Q)` = 7.
pub const USER_Q: i32 = (MIN_USER_Q - MAX_USER_Q) / 2 + MAX_USER_Q;
/// Default scheduling quantum for services. C: `USER_QUANTUM` — config.h:74.
pub const USER_QUANTUM: i32 = 200;
/// `NR_SCHED_QUEUES` — config.h:66. Single authority (N9 — todo §11):
/// `slot.rs`'s duplicate definition was deleted; `check_request`
/// (request.c:1275-1279) and `MIN_USER_Q` (config.h:71) now share this one.
pub const NR_SCHED_QUEUES: i32 = 16;

/// Scheduling parameters for one service.
///
/// C: the `r_scheduler`/`r_priority`/`r_quantum`/`r_cpu` fields of
/// `struct rproc` (type.h:89-92) plus the fixed parent (RS_PROC_NR).
/// `sched_init_proc` reads these from the slot and passes them to
/// `sched_start` (lib/libsys/sched_start.c:37-80).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SchedulerConfig {
    /// Scheduler endpoint (`KERNEL` for boot services, priv.h:88).
    pub scheduler: Endpoint,
    /// The process being scheduled.
    pub endpoint: Endpoint,
    /// Parent (RS itself). C: `RS_PROC_NR` — utility.c:374.
    pub parent: Endpoint,
    /// Scheduling priority. C: `r_priority` — type.h:90.
    pub priority: i32,
    /// Scheduling quantum. C: `r_quantum` — type.h:91.
    pub quantum: i32,
    /// CPU affinity. C: `r_cpu` — type.h:92.
    pub cpu: i32,
}

impl SchedulerConfig {
    /// Builds the config from a slot's scheduling fields.
    ///
    /// C: main.c:320-322 + boot Step 2's `sched_init_proc(rp)` (main.c:376).
    /// `endpoint` is `rpub->endpoint` (utility.c:373).
    pub fn from_slot(
        scheduler: Endpoint,
        endpoint: Endpoint,
        priority: i32,
        quantum: i32,
        cpu: i32,
    ) -> SchedulerConfig {
        SchedulerConfig {
            scheduler,
            endpoint,
            parent: Endpoint::RS,
            priority,
            quantum,
            cpu,
        }
    }

    /// Boot-time defaults for a boot image service.
    ///
    /// C: main.c:320-322 — `SRV_OR_USR(rp, SRV_SCH, USR_SCH)` with
    /// `SRV_SCH=KERNEL`, `SRV_Q=USER_Q`, `SRV_QT=USER_QUANTUM` (priv.h:88,
    /// 93, 98). All boot priv entries are `SYS_PROC`, so the SRV_* arm wins;
    /// `r_cpu` is left at the zero-initialized value (main.c:234, no explicit
    /// assignment). `parent` is RS itself (utility.c:374).
    pub fn boot_defaults(endpoint: Endpoint) -> SchedulerConfig {
        SchedulerConfig {
            scheduler: Endpoint::KERNEL,
            endpoint,
            parent: Endpoint::RS,
            priority: USER_Q,
            quantum: USER_QUANTUM,
            cpu: 0,
        }
    }
}

/// Decision: whether the kernel should start scheduling `cfg`.
///
/// C: `sched_init_proc` — `minix3/minix/servers/rs/utility.c:364-382`.
///
/// Invariants (C asserts, utility.c:369-371): user processes must have no
/// scheduler (`r_scheduler == NONE` — PM deals with them); system processes
/// must have one. The shell executes the effect (T5):
///
/// ```text
/// match sched_decision(&cfg, is_sys_proc) {
///     SchedAction::Skip  => Ok(Endpoint::NONE),
///     SchedAction::Start(c) => sys.sched_init_proc(c), // returns newscheduler_e
/// }
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SchedAction<'a> {
    /// User process with no scheduler — no kernel call
    /// (sched_start.c:45-47); the caller's scheduler stays `NONE`.
    Skip,
    /// Start scheduling `cfg` via `KernelApi::sched_init_proc`. The kernel
    /// returns the scheduler that actually took over (`*newscheduler_e`,
    /// sched_start.c:37-80 — the scheduler may forward the request).
    Start(&'a SchedulerConfig),
}

/// C: `sched_init_proc` — utility.c:364-382. Pure decision half (T5).
pub fn sched_decision(cfg: &SchedulerConfig, is_sys_proc: bool) -> SchedAction<'_> {
    if !is_sys_proc {
        debug_assert_eq!(
            cfg.scheduler,
            Endpoint::NONE,
            "user process must have no scheduler (utility.c:369)"
        );
    } else {
        debug_assert_ne!(
            cfg.scheduler,
            Endpoint::NONE,
            "system process must have a scheduler (utility.c:370)"
        );
    }
    // C: sched_start — sched_start.c:45-47: no scheduler (user process) → done
    // without any kernel call.
    if cfg.scheduler == Endpoint::NONE {
        return SchedAction::Skip;
    }
    // C: sched_start(..., &rp->r_scheduler) — utility.c:372-378. The kernel
    // API returns the scheduler that actually took over (`*newscheduler_e`).
    SchedAction::Start(cfg)
}

/// Where a `sched_stop` result lands.
///
/// The same nonzero means different things by site: during cleanup there
/// is no slot state left to protect (teardown is best-effort), while
/// during edit the slot is still untouched (abort keeps it so).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StopSite {
    /// `cleanup_service`: teardown. C: manager.c:461-463.
    CleanupService,
    /// `do_edit`: pre-mutation. C: request.c:342-345.
    EditSlot,
}

/// What the caller does with a `sched_stop` result.
///
/// Warnings (`printf`) stay shell-side (T5): the pure layer only names
/// the road — continue or abort with the code.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StopOutcome {
    /// Carry on: success everywhere, failure at cleanup.
    Continue,
    /// Abort the edit, carrying the code. C: `return r` — request.c:345.
    Abort(i32),
}

/// Route a `sched_stop` result by site.
///
/// Zero (OK) continues on both roads — success needs no decision.
/// Nonzero continues at cleanup (warn and go on: nothing to go back to)
/// but aborts an edit (the slot is untouched, so stopping keeps it so).
pub const fn on_stop_result(site: StopSite, result: i32) -> StopOutcome {
    if result == 0 {
        return StopOutcome::Continue;
    }
    match site {
        StopSite::CleanupService => StopOutcome::Continue,
        StopSite::EditSlot => StopOutcome::Abort(result),
    }
}

/// Commit effect of [`set_sig_mgrs`]: `SYS_PRIV_UPDATE_SYS`.
///
/// The shell executes it as
/// `sys.privctl(c.endpoint, PrivCtlOp::UpdateSys, Some(c.priv_))`
/// (T5 — effects are returned, not performed, by the pure layer).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SigMgrCommit<'a> {
    /// C: `rpub->endpoint` — utility.c:407.
    pub endpoint: Endpoint,
    /// The updated priv structure (owned by the caller).
    pub priv_: &'a Privilege,
}

/// Pure core of `update_sig_mgrs` — applies the synced privilege structure
/// and the new signal managers, and returns the commit.
///
/// C: `update_sig_mgrs` — `minix3/minix/servers/rs/utility.c:387-422`.
/// The shell owns C's fixed order of the two kernel calls (T5):
///
/// 1. `sys.getpriv(endpoint)` → `synced` (utility.c:393-396);
/// 2. execute the returned [`SigMgrCommit`] (utility.c:407-408).
///
/// `SELF` expansion (`sig_mgr == SELF ? endpoint : sig_mgr`,
/// utility.c:397-398) happens at the call site (12/16).
pub fn set_sig_mgrs(
    priv_: &mut Privilege,
    synced: Privilege,
    endpoint: Endpoint,
    sig_mgr: Endpoint,
    bak_sig_mgr: Endpoint,
) -> SigMgrCommit<'_> {
    // utility.c:393-396: the shell synced the privilege structure; apply it.
    *priv_ = synced;

    // utility.c:399-400: set signal managers.
    priv_.sig_mgr = sig_mgr;
    priv_.bak_sig_mgr = bak_sig_mgr;

    // utility.c:401-406: update privilege structure.
    SigMgrCommit { endpoint, priv_ }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::privilege::{PrivFlags, Privilege};

    #[test]
    fn test_sched_decision_sys_proc_starts() {
        let cfg = SchedulerConfig::from_slot(Endpoint::KERNEL, Endpoint::PM, 3, 200, 0);
        match sched_decision(&cfg, true) {
            SchedAction::Start(c) => assert_eq!(c.endpoint, Endpoint::PM),
            SchedAction::Skip => panic!("system process with a scheduler must start"),
        }
    }

    #[test]
    fn test_sched_decision_user_proc_none_skips() {
        // User process with NONE scheduler: the C code returns OK without
        // calling the scheduler (sched_start.c:45-47) — no kernel call.
        let cfg = SchedulerConfig::from_slot(Endpoint::NONE, Endpoint::INIT, 3, 200, 0);
        assert_eq!(sched_decision(&cfg, false), SchedAction::Skip);
    }

    #[test]
    fn test_sched_decision_passes_full_config() {
        // S4: the KernelApi receives scheduler/priority/quantum/cpu, not just
        // the endpoint (C: sched_start.c:37-80 forwards all four).
        let cfg = SchedulerConfig::from_slot(Endpoint::SCHED, Endpoint::VFS, 7, 200, -1);
        match sched_decision(&cfg, true) {
            SchedAction::Start(c) => {
                assert_eq!(c.scheduler, Endpoint::SCHED);
                assert_eq!(c.parent, Endpoint::RS);
                assert_eq!(c.priority, 7);
                assert_eq!(c.quantum, 200);
                assert_eq!(c.cpu, -1);
            }
            SchedAction::Skip => panic!("system process with a scheduler must start"),
        }
    }

    #[test]
    fn test_boot_defaults_match_c() {
        // C: main.c:320-322 — SRV_SCH=KERNEL (priv.h:88), SRV_Q=USER_Q
        // (priv.h:93, config.h:69 → 7), SRV_QT=USER_QUANTUM (priv.h:98,
        // config.h:74 → 200); parent = RS (utility.c:374); r_cpu stays 0.
        let cfg = SchedulerConfig::boot_defaults(Endpoint::PM);
        assert_eq!(cfg.scheduler, Endpoint::KERNEL);
        assert_eq!(cfg.parent, Endpoint::RS);
        assert_eq!(cfg.priority, USER_Q);
        assert_eq!(USER_Q, 7);
        assert_eq!(cfg.quantum, USER_QUANTUM);
        assert_eq!(USER_QUANTUM, 200);
        assert_eq!(cfg.cpu, 0);
    }

    #[test]
    fn test_set_sig_mgrs_applies_and_commits() {
        // T5: the pure core applies the synced priv + managers and returns
        // the commit; the shell owns `sys.getpriv` (before) and the
        // `SYS_PRIV_UPDATE_SYS` execution (after) in C's fixed order
        // (utility.c:393-408).
        let mut priv_ = Privilege::vacant();
        let synced = Privilege::boot_priv(PrivFlags::SYS_PROC, Endpoint::PM.slot());
        let synced_id = synced.id;
        let commit = set_sig_mgrs(
            &mut priv_,
            synced,
            Endpoint::PM,
            Endpoint::RS,
            Endpoint::NONE,
        );
        // The commit borrows the caller's updated structure — read the
        // applied values through it (the synced priv carries the id, the
        // managers are set on top).
        assert_eq!(commit.priv_.id, synced_id);
        assert_eq!(commit.priv_.sig_mgr, Endpoint::RS);
        assert_eq!(commit.priv_.bak_sig_mgr, Endpoint::NONE);
        // The commit names the target.
        assert_eq!(commit.endpoint, Endpoint::PM);
    }

    #[test]
    fn test_boot_priv_sys_flags() {
        // sanity: Privilege + PrivFlags wiring compiles together.
        let p = Privilege::boot_priv(PrivFlags::SYS_PROC, 0);
        assert!(p.is_sys_proc());
    }

    #[test]
    fn test_stop_ok_continues() {
        // Zero (OK) continues on both roads — success needs no decision
        // (manager.c:461 `s == OK` falls through; request.c:342 likewise).
        assert_eq!(
            on_stop_result(StopSite::CleanupService, 0),
            StopOutcome::Continue
        );
        assert_eq!(on_stop_result(StopSite::EditSlot, 0), StopOutcome::Continue);
    }

    #[test]
    fn test_cleanup_warns_on() {
        // Cleanup failure warns and goes on: teardown has no slot state
        // left to protect (manager.c:461-463).
        assert_eq!(
            on_stop_result(StopSite::CleanupService, 1),
            StopOutcome::Continue
        );
    }

    #[test]
    fn test_edit_aborts_on() {
        // Edit failure aborts with the code: the slot is untouched, so
        // stopping keeps it so (request.c:342-345). The code rides
        // through untouched — any nonzero, here 5.
        assert_eq!(on_stop_result(StopSite::EditSlot, 5), StopOutcome::Abort(5));
    }
}
