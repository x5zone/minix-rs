//! Service startup and initialization protocol (pure slice).
//!
//! Mirrors `minix3/minix/servers/rs/manager.c` (`run_service` — 923,
//! `start_service` — 950, `end_srv_init` — 328), `utility.c:18-64`
//! (`init_service`), `request.c:462-529` (`do_init_ready`),
//! `request.c:890-938` (`do_upd_ready`) and `main.c:591-626`
//! (`sef_cb_init_response`/`sef_cb_lu_response`), `main.c:784-821`
//! (`catch_boot_init_ready`). 12-rs-init-run.md.
//!
//! The IPC-coupled steps (`rs_asynsend`/`reply` — 06, the `RS_INIT` message
//! exchange — 19, `crash_service` — 15, `rupdate_upd_move`/`end_update`/the
//! `rpupd` chain — 16) are documented hook points. This module owns the pure
//! decisions: the `RS_INIT` payload assembly (ipc.h:1855-1866), the
//! ready-message gate and branch selection, the `end_srv_init` bookkeeping
//! and the SEF response normalization.

use crate::sef::SefInitType;
use crate::service_slot::{RFlags, ServiceSlot, SlotMutations};
use minix_types::{Clock, Endpoint, Errno};

/// C: `SEF_INIT_SCRIPT_RESTART` — sef.h:102 (`0x10`).
pub const SEF_INIT_SCRIPT_RESTART: u32 = 0x10;

/// Extends the init flags when the service has a restart script.
///
/// C: `init_service` — utility.c:54-57: `if(rp->r_pub->sys_flags &
/// SF_USE_SCRIPT) flags |= SEF_INIT_SCRIPT_RESTART;`.
pub fn init_flags(use_script: bool, flags: u32) -> u32 {
    if use_script {
        flags | SEF_INIT_SCRIPT_RESTART
    } else {
        flags
    }
}

/// Marks a slot as initializing before the `RS_INIT` message is sent.
///
/// C: `init_service` — utility.c:18-21:
/// `r_flags |= RS_INITIALIZING; r_alive_tm = getticks();
/// r_check_tm = r_alive_tm + 1` ("expect reply within period"). R14: this is
/// the missing pre-send state transition — without it every `RS_INIT_READY`
/// reply hits the `do_init_ready` gate (request.c:477-483) and is rejected.
pub fn mark_initializing(slot: &mut ServiceSlot, ticks: Clock) {
    slot.flags.insert(RFlags::INITIALIZING); // utility.c:19
    slot.alive_tm = ticks; // utility.c:20
    slot.check_tm = ticks + 1; // utility.c:21
}

/// Folds the caller's init flags into the slot's privilege structure.
///
/// C: `start_service` — manager.c:953: `rp->r_priv.s_init_flags |= init_flags`
/// runs before `create_service`; the replica path is modelled by
/// `service_create::link_replica` (manager.c:751-752). 12 must call this
/// before publishing/starting a fresh or restarted service.
pub fn fold_init_flags(slot: &mut ServiceSlot, init_flags: u32) {
    slot.priv_.init_flags |= init_flags;
}

/// The `RS_INIT` message payload.
///
/// C: `struct mess_rs_init` — `minix3/minix/include/minix/ipc.h:1858-1866`.
/// `rproctab_gid` is `Option` (the grant is created at boot — main.c:185;
/// consumed here, 02 P2-2 tracks the `None` placeholder).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InitMessage {
    /// C: `result` — ipc.h:1856 (0 for requests; ready replies carry it).
    pub result: i32,
    /// C: `type` — ipc.h:1857 (`short`; `SEF_INIT_*`, sef.h:93-95).
    pub init_type: u16,
    /// C: `rproctab_gid` — ipc.h:1858 (`cp_grant_id_t`).
    pub rproctab_gid: Option<u32>,
    /// C: `old_endpoint` — ipc.h:1859.
    pub old_endpoint: Option<Endpoint>,
    /// C: `restarts` — ipc.h:1860 (`short`; `r_restarts + 1`).
    pub restarts: i32,
    /// C: `flags` — ipc.h:1861 (`SEF_INIT_*`).
    pub flags: u32,
    /// C: `buff_addr` — ipc.h:1862 (`vir_bytes`; preallocated mmap, 16).
    pub buff_addr: u64,
    /// C: `buff_len` — ipc.h:1863 (`size_t`).
    pub buff_len: usize,
    /// C: `prepare_state` — ipc.h:1864 (`SEF_LU_STATE_*`, sef.h:213-222).
    pub prepare_state: i32,
}

/// Assembles the `RS_INIT` request payload.
///
/// C: `init_service` — utility.c:49-64. `old_endpoint` and `prepare_state`
/// are injected: C derives them from `r_old_rp`/`r_prev_rp` and the
/// (unmodelled, 02 P2-3) `r_upd` descriptor (utility.c:33-42).
#[allow(clippy::too_many_arguments)] // 1:1 with C init_service (utility.c:49-64); payload struct deferred to 12's call site
pub fn init_message(
    init_type: SefInitType,
    flags: u32,
    rproctab_gid: Option<u32>,
    old_endpoint: Option<Endpoint>,
    restarts: i32,
    buff_addr: u64,
    buff_len: usize,
    prepare_state: i32,
) -> InitMessage {
    InitMessage {
        result: 0,
        init_type: match init_type {
            SefInitType::Fresh => 0,   // SEF_INIT_FRESH — sef.h:93
            SefInitType::Lu => 1,      // SEF_INIT_LU — sef.h:94
            SefInitType::Restart => 2, // SEF_INIT_RESTART — sef.h:95
        },
        rproctab_gid,
        old_endpoint,
        restarts,
        flags,
        buff_addr,
        buff_len,
        prepare_state,
    }
}

/// The `do_init_ready` outcome after the gate.
///
/// C: `do_init_ready` — request.c:462-529. One of:
/// - gate failure (no `RS_INITIALIZING`) → `EINVAL` (request.c:477-483);
/// - init failure → `crash_service` + `init_err`; `ERESTART` + not updating →
///   `RS_REINCARNATE` (request.c:488-497), reply suppressed (`EDONTREPLY`);
/// - updating → pending decrement, `RS_INIT_DONE`; 0 → `end_update` (16)
///   (request.c:506-513);
/// - fresh → clear `RS_INITIALIZING` + reply + `end_srv_init`
///   (request.c:514-525).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReadyOutcome {
    /// Unexpected init ready — request.c:477-483 → `EINVAL`.
    Unexpected,
    /// Init failed — request.c:488-497 (`crash_service` hook: 15).
    InitFailed { result: i32, reincarnate: bool },
    /// Updating service initialized — request.c:506-513; remaining pending
    /// count; 0 → `end_update` (16).
    UpdateInitDone { pending_remaining: usize },
    /// Fresh init done — request.c:514-525.
    FreshInitDone,
}

/// The `do_init_ready` decision plus the slot mutations it implies.
///
/// R13: C mutates `r_*` inline (request.c:488-525); the mutations travel as
/// a typed payload the 12 caller applies once after the action hook
/// (`crash_service` / `end_update` / reply).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReadyDecision {
    /// Branch to execute.
    pub outcome: ReadyOutcome,
    /// Slot mutations implied by the branch (R13).
    pub mutations: SlotMutations,
}

/// C: `do_init_ready` — request.c:462-529 (pure decisions).
///
/// `is_updating` = `SRV_IS_UPDATING(rp)` (16 flags); `pending` =
/// `rupdate.num_init_ready_pending`; `ticks` = `getticks()` (the fresh
/// branch resets `r_alive_tm` to it, request.c:518).
pub fn do_init_ready(
    flags: RFlags,
    result: i32,
    is_updating: bool,
    pending: usize,
    ticks: Clock,
) -> ReadyDecision {
    if !flags.contains(RFlags::INITIALIZING) {
        return ReadyDecision {
            outcome: ReadyOutcome::Unexpected, // request.c:477-483
            mutations: SlotMutations::default(),
        };
    }
    if result != 0 {
        let reincarnate = result == minix_types::ERESTART && !is_updating;
        return ReadyDecision {
            outcome: ReadyOutcome::InitFailed {
                result,
                reincarnate,
            }, // request.c:488-497
            // request.c:491-492 (`|= RS_REINCARNATE`) + request.c:495
            // (`r_init_err = result`) — carried as a typed payload (R13).
            mutations: SlotMutations {
                set: if reincarnate {
                    RFlags::REINCARNATE
                } else {
                    RFlags::empty()
                },
                init_err: Some(result),
                ..Default::default()
            },
        };
    }
    if is_updating {
        // C: request.c:506-513 — `rupdate.num_init_ready_pending--`; the
        // caller keeps it > 0 (main.c:586 assert), so a 0 here is a program
        // error, not a silent saturate.
        debug_assert!(pending > 0, "do_init_ready: pending underflow");
        ReadyDecision {
            outcome: ReadyOutcome::UpdateInitDone {
                pending_remaining: pending - 1,
            }, // request.c:506-513
            // request.c:509 — `r_flags |= RS_INIT_DONE`.
            mutations: SlotMutations {
                set: RFlags::INIT_DONE,
                ..Default::default()
            },
        }
    } else {
        ReadyDecision {
            outcome: ReadyOutcome::FreshInitDone, // request.c:514-525
            // request.c:516-519 — `&= ~RS_INITIALIZING; r_check_tm = 0;
            // r_alive_tm = getticks()`.
            mutations: SlotMutations {
                clear: RFlags::INITIALIZING,
                check_tm: Some(0),
                alive_tm: Some(ticks),
                ..Default::default()
            },
        }
    }
}

/// The `do_upd_ready` outcome.
///
/// C: `do_upd_ready` — request.c:890-938. The `rpupd` chain gate
/// (request.c:903-910: `!curr_rpupd || rp != rpupd->rp ||
/// RUPDATE_IS_INITIALIZING()`) is injected as `gate_ok` (the chain is
/// modelled in 16). Prepare failure → `end_update(result, RS_REPLY)` (16);
/// otherwise `start_update_prepare_next` (16), then `start_update` (16).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpdReadyOutcome {
    /// Gate failed — request.c:903-910 → `EINVAL`.
    Unexpected,
    /// Prepare failed — request.c:917-922 → `end_update(result)` (16).
    PrepareFailed { result: i32 },
    /// More services to prepare — request.c:930-932 (`start_update_prepare_next`, 16).
    NextPrepare,
    /// Perform the update — request.c:934-935 (`start_update`, 16).
    StartUpdate,
}

/// C: `do_upd_ready` — request.c:890-938 (pure decisions).
pub fn do_upd_ready(result: i32, gate_ok: bool, has_next: bool) -> UpdReadyOutcome {
    if !gate_ok {
        return UpdReadyOutcome::Unexpected; // request.c:903-910
    }
    if result != 0 {
        return UpdReadyOutcome::PrepareFailed { result }; // request.c:917-922
    }
    if has_next {
        return UpdReadyOutcome::NextPrepare; // request.c:930-932
    }
    UpdReadyOutcome::StartUpdate // request.c:934-935
}

/// The `end_srv_init` slot bookkeeping.
///
/// C: `end_srv_init` — manager.c:336-354: `late_reply(rp, OK)` (06 hook);
/// when a prev replica exists, `rupdate_upd_move` (16 hook) +
/// `cleanup_service(prev)` (15 hook), then `restarts += 1` and `prev = None`;
/// always `next = None`. Returns whether a prev replica was handled.
pub fn end_srv_init(rp: &mut ServiceSlot, has_prev: bool) -> bool {
    if has_prev {
        rp.restarts += 1; // manager.c:349
        rp.prev_rp = None; // manager.c:348
    }
    rp.next_rp = None; // manager.c:354
    has_prev
}

/// Whether the boot init-ready catcher replies to the source.
///
/// C: `catch_boot_init_ready` — main.c:812-815: no reply to VM, which sent
/// the reply asynchronously (a synchronous reply could deadlock).
pub fn should_reply_ready(src: Endpoint) -> bool {
    src != Endpoint::VM
}

/// C: `sef_cb_init_response` — main.c:591-609.
///
/// Non-OK result propagates; otherwise the simulated RS-to-RS init runs
/// `do_init_ready`, and `EDONTREPLY` (its normal success) becomes `OK`.
pub fn normalize_init_response(result: i32, ready: Result<(), Errno>) -> i32 {
    if result != 0 {
        return result;
    }
    match ready {
        Ok(()) => 0,
        Err(Errno::EDONTREPLY) => 0,
        Err(e) => e.to_i32(),
    }
}

/// C: `sef_cb_lu_response` — main.c:614-626.
///
/// `do_upd_ready` normally returns `EDONTREPLY`; reaching the caller means
/// the update did not happen → `EGENERIC` (sys/errno.h:200).
pub fn normalize_lu_response(ready: Result<(), Errno>) -> i32 {
    match ready {
        Ok(()) => 0,
        Err(Errno::EDONTREPLY) => Errno::EGENERIC.to_i32(),
        Err(e) => e.to_i32(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::service_slot::RFlags;

    #[test]
    fn test_init_flags_script() {
        // C: utility.c:44-47 — SF_USE_SCRIPT adds SEF_INIT_SCRIPT_RESTART.
        assert_eq!(init_flags(true, 0), SEF_INIT_SCRIPT_RESTART);
        assert_eq!(init_flags(true, 0x2), 0x2 | SEF_INIT_SCRIPT_RESTART);
        assert_eq!(init_flags(false, 0x4), 0x4);
    }

    #[test]
    fn test_init_message_fields() {
        // C: utility.c:49-64 — type/flags/rproctab_gid/old_endpoint/restarts/buff.
        let m = init_message(
            SefInitType::Restart,
            0x10,
            Some(7),
            Some(Endpoint::VFS),
            3,
            0x1000,
            4096,
            2,
        );
        assert_eq!(m.init_type, 2); // SEF_INIT_RESTART
        assert_eq!(m.flags, 0x10);
        assert_eq!(m.rproctab_gid, Some(7));
        assert_eq!(m.old_endpoint, Some(Endpoint::VFS));
        assert_eq!(m.restarts, 3);
        assert_eq!(m.buff_addr, 0x1000);
        assert_eq!(m.buff_len, 4096);
        assert_eq!(m.prepare_state, 2);
        assert_eq!(m.result, 0);
    }

    #[test]
    fn test_mark_initializing() {
        // R14: C init_service — utility.c:18-21. Without this pre-send
        // transition every RS_INIT_READY hits the do_init_ready gate.
        let mut s = ServiceSlot::vacant();
        mark_initializing(&mut s, 4242);
        assert!(s.flags.contains(RFlags::INITIALIZING)); // utility.c:19
        assert_eq!(s.alive_tm, 4242); // utility.c:20
        assert_eq!(s.check_tm, 4243); // utility.c:21 — reply within period
    }

    #[test]
    fn test_fold_init_flags() {
        // R14: C start_service — manager.c:953 `s_init_flags |= init_flags`.
        let mut s = ServiceSlot::vacant();
        s.priv_.init_flags = 0x2;
        fold_init_flags(&mut s, 0x10);
        assert_eq!(s.priv_.init_flags, 0x12); // OR, not replace
    }

    #[test]
    fn test_do_init_ready_gate() {
        // C: request.c:477-483 — no RS_INITIALIZING → EINVAL.
        let d = do_init_ready(RFlags::IN_USE, 0, false, 0, 100);
        assert_eq!(d.outcome, ReadyOutcome::Unexpected);
        assert_eq!(d.mutations, SlotMutations::default());
    }

    #[test]
    fn test_do_init_ready_failed_reincarnate() {
        // C: request.c:488-497 — ERESTART + not updating → REINCARNATE.
        let d = do_init_ready(
            RFlags::IN_USE | RFlags::INITIALIZING,
            minix_types::ERESTART,
            false,
            0,
            100,
        );
        assert_eq!(
            d.outcome,
            ReadyOutcome::InitFailed {
                result: minix_types::ERESTART,
                reincarnate: true
            }
        );
        // request.c:491-492 + 495 — REINCARNATE set, r_init_err = result.
        assert!(d.mutations.set.contains(RFlags::REINCARNATE));
        assert_eq!(d.mutations.init_err, Some(minix_types::ERESTART));
        // Not ERESTART → no reincarnate.
        let d = do_init_ready(RFlags::IN_USE | RFlags::INITIALIZING, 5, false, 0, 100);
        assert_eq!(
            d.outcome,
            ReadyOutcome::InitFailed {
                result: 5,
                reincarnate: false
            }
        );
        assert!(d.mutations.set.is_empty());
        assert_eq!(d.mutations.init_err, Some(5));
        // ERESTART during update → no reincarnate (request.c:492).
        let d = do_init_ready(
            RFlags::IN_USE | RFlags::INITIALIZING,
            minix_types::ERESTART,
            true,
            1,
            100,
        );
        assert_eq!(
            d.outcome,
            ReadyOutcome::InitFailed {
                result: minix_types::ERESTART,
                reincarnate: false
            }
        );
    }

    #[test]
    fn test_do_init_ready_update_done() {
        // C: request.c:506-513 — pending decrements; 0 → end_update (16).
        let d = do_init_ready(RFlags::IN_USE | RFlags::INITIALIZING, 0, true, 1, 100);
        assert_eq!(
            d.outcome,
            ReadyOutcome::UpdateInitDone {
                pending_remaining: 0
            }
        );
        assert!(d.mutations.set.contains(RFlags::INIT_DONE)); // request.c:509
        let d = do_init_ready(RFlags::IN_USE | RFlags::INITIALIZING, 0, true, 3, 100);
        assert_eq!(
            d.outcome,
            ReadyOutcome::UpdateInitDone {
                pending_remaining: 2
            }
        );
    }

    #[test]
    fn test_do_init_ready_fresh() {
        // C: request.c:514-525 — fresh path replies + end_srv_init.
        let d = do_init_ready(RFlags::IN_USE | RFlags::INITIALIZING, 0, false, 99, 4242);
        assert_eq!(d.outcome, ReadyOutcome::FreshInitDone);
        // request.c:516-519 — clear INITIALIZING, r_check_tm = 0,
        // r_alive_tm = getticks().
        assert!(d.mutations.clear.contains(RFlags::INITIALIZING));
        assert_eq!(d.mutations.check_tm, Some(0));
        assert_eq!(d.mutations.alive_tm, Some(4242));
    }

    #[test]
    #[should_panic(expected = "pending underflow")]
    fn test_do_init_ready_pending_underflow_panics() {
        // C: main.c:586 — `num_init_ready_pending > 0` before decrement; a
        // zero pending is a program error, not a silent saturate.
        let _ = do_init_ready(RFlags::IN_USE | RFlags::INITIALIZING, 0, true, 0, 100);
    }

    #[test]
    fn test_do_upd_ready() {
        // C: request.c:903-937 — gate / prepare-fail / next / start.
        assert_eq!(do_upd_ready(0, false, false), UpdReadyOutcome::Unexpected);
        assert_eq!(
            do_upd_ready(9, true, false),
            UpdReadyOutcome::PrepareFailed { result: 9 }
        );
        assert_eq!(do_upd_ready(0, true, true), UpdReadyOutcome::NextPrepare);
        assert_eq!(do_upd_ready(0, true, false), UpdReadyOutcome::StartUpdate);
    }

    #[test]
    fn test_end_srv_init_bookkeeping() {
        // C: manager.c:336-354 — restarts++, prev/next cleared.
        let mut rp = ServiceSlot::vacant();
        rp.restarts = 2;
        rp.prev_rp = Some(crate::service_slot::SlotId::new(4));
        rp.next_rp = Some(crate::service_slot::SlotId::new(5));
        assert!(end_srv_init(&mut rp, true));
        assert_eq!(rp.restarts, 3);
        assert_eq!(rp.prev_rp, None);
        assert_eq!(rp.next_rp, None);
    }

    #[test]
    fn test_end_srv_init_no_prev() {
        // C: manager.c:354 — without prev only next is cleared.
        let mut rp = ServiceSlot::vacant();
        rp.restarts = 1;
        rp.next_rp = Some(crate::service_slot::SlotId::new(5));
        assert!(!end_srv_init(&mut rp, false));
        assert_eq!(rp.restarts, 1);
        assert_eq!(rp.next_rp, None);
    }

    #[test]
    fn test_should_reply_ready_vm_exception() {
        // C: main.c:812-815 — VM is not replied to.
        assert!(!should_reply_ready(Endpoint::VM));
        assert!(should_reply_ready(Endpoint::VFS));
        assert!(should_reply_ready(Endpoint::PM));
    }

    #[test]
    fn test_normalize_init_response() {
        // C: main.c:591-609 — result wins; EDONTREPLY → OK.
        assert_eq!(normalize_init_response(5, Ok(())), 5);
        assert_eq!(normalize_init_response(0, Err(Errno::EDONTREPLY)), 0);
        assert_eq!(normalize_init_response(0, Err(Errno::from_i32(42))), 42);
        assert_eq!(normalize_init_response(0, Ok(())), 0);
    }

    #[test]
    fn test_normalize_lu_response() {
        // C: main.c:614-626 — EDONTREPLY → EGENERIC.
        assert_eq!(
            normalize_lu_response(Err(Errno::EDONTREPLY)),
            Errno::EGENERIC.to_i32()
        );
        assert_eq!(normalize_lu_response(Err(Errno::from_i32(3))), 3);
        assert_eq!(normalize_lu_response(Ok(())), 0);
    }
}
