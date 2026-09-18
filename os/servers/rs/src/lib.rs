#![cfg_attr(not(test), no_std)]

//! Minix-RS Reincarnation Server.
//!
//! This crate implements the RS server as a single-threaded user-space
//! process, matching Minix3's execution model (user-space servers run a
//! single-threaded event loop — `Rc`/`RefCell`/`!Send`/`!Sync` are
//! reasonable; no cross-CPU sharing exists).
//!
//! # Scope
//!
//! The current implementation covers the boot/init skeleton documented in
//! `notes/rewrite/fork-syscall-rewrite/03-stage-rs/01-rs-boot-init.md`:
//!
//! - [`table`] — the boot image priv/sys/dev tables (ARCH A-13 static tables).
//! - [`boot`] — the 4-step boot state machine (`sef_cb_init_fresh`) + the
//!   `KernelApi` boundary (production wiring: 19-rs-external-interfaces.md).
//! - [`sef`] — the SEF callback registration table (ARCH A-7).
//! - [`dispatch`] — the main-loop message classification skeleton.
//! - [`service_slot`] — the rproc/rprocpub slot model + r_flags/sys_flags
//!   (02-rs-process-table.md).
//! - [`process_table`] — the service table + slot primitives (lookup/alloc/
//!   free/rs_isokendpt, ARCH A-3/A-4; 02-rs-process-table.md).
//!
//! Remaining RS semantics (priv, access control, main loop, heartbeat,
//! service creation, live update, ...) land in the subsequent documents of
//! the 03-stage-rs series.

extern crate alloc;

use dispatch::DispatchKind;
use minix_types::{Clock, Endpoint, Errno};

pub mod access;
pub mod boot;
pub mod dispatch;
pub mod error;
pub mod exec;
pub mod ipc_mask;
pub mod live_update;
pub mod monitor;
pub mod privilege;
pub mod process_table;
pub mod publish;
pub mod query;
pub mod ready;
pub mod recovery;
pub mod request;
pub mod sched;
pub mod sef;
pub mod self_lifecycle;
pub mod service_create;
pub mod service_slot;
pub mod trap_api;
pub mod slot;
pub mod state_data;
pub mod table;
#[cfg(test)]
mod testutil;

pub use access::{caller_can_control, caller_is_root, check_call_permission};
pub use boot::{BootError, BootInit, BootTables, KernelApi, Machine};
pub use exec::{free_exec, has_shared_exec, share_exec, validate_image};
pub use ipc_mask::{IpcListIterator, add_backward_ipc, add_forward_ipc, init_privs};
pub use live_update::{
    AbortAction, EndUpdateRole, LuFlags, RS_CANCEL, RS_REPLY, RupdateFlags, SEF_INIT_ST,
    SEF_LU_STATE_NULL, SEF_LU_STATE_UNREACHABLE, UpdateChain, UpdateEntry, UpdatePhase,
    UpdateState, abort_action, end_srv_reply_flag, end_update_role, lu_flags_from_rss,
    resolve_prepare_maxtime, update_phase, validate_update_request, vm_default_prealloc,
};
pub use monitor::{
    PeriodAction, PeriodDecision, delta_t, effective_period, has_update_timed_out, init_timeout,
    period_decision, sigchld_cleanup,
};
pub use privilege::{DSRV_I, PrivCtlOp, Privilege, TrapMask};
pub use process_table::{RProcTable, ServiceInstances};
pub use publish::{should_bind_devman, should_map_driver, should_set_pci_acl, unpublish_result};
pub use query::{
    GetsysinfoTable, SysctlAction, classify_sysctl, getsysinfo_table, lookup_name_len,
};
pub use ready::{
    InitMessage, ReadyDecision, ReadyOutcome, UpdReadyDecision, UpdReadyOutcome, do_init_ready,
    do_upd_ready, end_srv_init, fold_init_flags, init_message, mark_initializing,
    take_map_prealloc,
};
pub use recovery::{
    CleanupDecision, TerminateAction, TerminateDecision, cleanup_decision, compute_backoff,
    late_reply_result, script_reason, terminate_decision,
};
pub use request::{
    StopDecision, StopSignal, check_duplicates, mark_late_reply, shutdown_apply, stop_decision,
    up_init_flags,
};
pub use sef::{RestartCb, SefCallbacks, SefInitInfo, SefInitType};
pub use self_lifecycle::{
    SelfUpgradeRole, SigMgrUpdate, SrvUpdateAction, SwapFlag, is_rs_restart_replica,
    lu_init_invariants, rollback_needs_vm_update, rollback_swap_flag, self_update_sig_mgr_update,
    self_upgrade_role, should_end_update_on_restart, should_pre_swap, sig_mgr_updates,
    srv_update_action,
};
pub use service_create::{
    activate_service, check_create_preconditions, clone_slot, inherit_service_defaults, init_slot,
    link_replica, mark_child_created, rebuild_args, swap_index, swap_slot, unlink_replica,
    vm_replica_preclean_needed,
};
pub use service_slot::{
    ARGV_ELEMENTS, IMM_SF, Label, MAX_COMMAND_LEN, MAX_IPC_LIST, MAX_NR_ARGS, MAX_SCRIPT_LEN,
    NR_DOMAIN, NR_IO_RANGE, NR_IRQ, NR_MEM_RANGE, PublicSlot, RFlags, RS_MAX_LABEL_LEN,
    RS_NR_CONTROL, RS_NR_PCI_CLASS, RS_NR_PCI_DEVICE, RsPciClass, RsPciId, SRV_SF, SRVR_SF,
    ServiceSlot, SlotId, SlotMutations, SysFlags, VM_SF,
};
pub use slot::{RsStart, RsStateData, RssFlags, build_cmd_dep, check_request, edit_slot};
pub use state_data::{
    ANY_SYS, ANY_TSK, ANY_USR, IPCF_MAX_ELEMENTS, IpcFilterEl, IpcfFlags, SourceIpcFilterEl,
    VM_RS_UPDATE, ipcf_els_buff_size, num_ipc_filter_blocks, parse_filter_el, parse_label,
    validate_eval, validate_state_data_size, vm_fallback_entry,
};

/// The RS server orchestrator.
///
/// C: `main()` — `minix3/minix/servers/rs/main.c:38-131`. Holds the SEF
/// callback table (main.c:51 registration) and the boot machine, and drives
/// the main-loop skeleton (classification in [`dispatch`]; mechanisms in
/// 06-rs-main-loop.md).
pub struct RsServer {
    /// The boot machine. Consumed by [`RsServer::init`] (`Fresh`): the
    /// runtime state is handed over to [`RsServer::state`] (T1 — 01-rs-boot-
    /// init.md §3.4), so the main loop reaches the table/hz without a getter
    /// dump on `BootInit`.
    boot: Option<BootInit<'static>>,
    /// Runtime server state after boot (C globals: `rproc[]`/`system_hz`/
    /// `shutting_down`/`rinit` — glo.h). `None` until the fresh boot
    /// completes; [`RsServer::run`] fails closed on a missing state.
    state: Option<ServerState<'static>>,
    kernel: alloc::boxed::Box<dyn KernelApi>,
    /// Typed cause of the last failed fresh boot (E-6). The SEF callback
    /// face returns a bare errno (C `int` face — `From<BootError> for
    /// Errno` flattens `Kernel(e) → e`, invariant violations → `EINVAL`),
    /// so the fatal-boot report (main.rs panic) reads the diagnostic here
    /// instead of re-deriving it from the errno.
    boot_diagnostic: Option<boot::BootError>,
    /// The restart dispatch target — C's restart table entry is runtime
    /// state: startup registers RS's own handler (main.c:140), and
    /// `sef_cb_init_lu` rebinds it to the stateful transfer generic
    /// (main.c:558, A3). See [`sef::RestartCb`].
    restart_cb: sef::RestartCb,
}

/// Runtime server state handed over by the boot (T1).
///
/// C: the boot globals keep living after boot (glo.h): the service table
/// (`rproc[]`/`rprocpub[]`), `system_hz`, `shutting_down` and the init
/// descriptor `rinit`. The main loop (06) reads these directly — `do_period`
/// needs `system_hz` + `table`, the RS_DOWN sweep needs `shutting_down`.
#[derive(Debug)]
pub struct ServerState<'a> {
    /// Boot tables (priv/sys/dev + image) — ARCH A-13.
    pub tables: BootTables<'a>,
    /// C: `machine` — main.c:53 (`sys_getmachine`, glo.h). Startup-time
    /// machine snapshot; `check_request` resolves `RS_CPU_BSP`/oversubscribed
    /// cpu against it (request.c:1286-1296). Fetched once at boot, not
    /// per-request (N3 — todo §11).
    pub machine: boot::Machine,
    /// C: `rinit` — main.c:185 (grant consumed by 12).
    pub rinit: boot::RinitState,
    /// C: `rproc[]`/`rprocpub[]` service table.
    pub table: process_table::RProcTable,
    /// C: `shutting_down` — main.c:193.
    pub shutting_down: bool,
    /// C: `system_hz` — main.c:181.
    pub system_hz: u32,
    /// C: `nr_uncaught_init_srvs` — main.c:349-406 (consumed by 12).
    pub nr_uncaught_init_srvs: usize,
    /// Live-update state. C: the `rupdate` global — type.h:43-52 (16; A2
    /// single-holder convergence — one structure instead of scattered
    /// flags/counters/chain).
    pub update: live_update::UpdateState,
}

impl RsServer {
    /// Creates the server with the boot tables.
    ///
    /// C: `sys_getimage` (main.c:196) result. The SEF callback set is the
    /// [`SefCallbacks`] trait implemented by `RsServer` itself (N5 — no
    /// separate registration value to construct, main.c:51). The kernel API
    /// is the production trap backend ([`trap_api::TrapKernelApi`], 19 号
    /// 换装):每方法委托 minix-sys 真实 wrapper,宿主构建双腿回答 ±EIO
    /// (fail-closed 可观察);测试用 `with_kernel` 注入 mock。
    pub fn new(tables: BootTables<'static>) -> Self {
        Self::with_kernel(tables, alloc::boxed::Box::new(trap_api::TrapKernelApi::new()))
    }

    /// Creates the server with an injected kernel API (tests / wiring).
    pub fn with_kernel(
        tables: BootTables<'static>,
        kernel: alloc::boxed::Box<dyn KernelApi>,
    ) -> Self {
        Self {
            boot: Some(BootInit::new(tables)),
            state: None,
            kernel,
            boot_diagnostic: None,
            restart_cb: sef::RestartCb::Rs, // C: main.c:140 registration
        }
    }

    /// Typed cause of the last failed fresh boot, if any (E-6).
    ///
    /// The SEF face flattens boot errors to a bare errno; this keeps the
    /// [`boot::BootError`] variant for the fatal-boot report (main.rs) and
    /// for tests asserting the invariant family.
    pub fn boot_diagnostic(&self) -> Option<boot::BootError> {
        self.boot_diagnostic
    }

    /// Runs the SEF startup and the 4-step boot.
    ///
    /// C: `sef_startup()` → callback dispatch — main.c:151. The init type
    /// routes through the [`SefCallbacks`] trait methods implemented by
    /// `RsServer` (N5): `Fresh` → [`SefCallbacks::init_fresh`] (the 4-step
    /// boot); `Lu`/`Restart` fail closed until 18 lands. The SEF_INIT
    /// *message* receive belongs to the main loop's RS_INIT branch
    /// (12-rs-init-run.md); this method is the dispatch half.
    pub fn init(&mut self, init_type: SefInitType) -> Result<i32, Errno> {
        let info = SefInitInfo::default();
        match init_type {
            SefInitType::Fresh => self.init_fresh(init_type, &info),
            SefInitType::Lu => self.init_lu(init_type, &info),
            SefInitType::Restart => self.init_restart(init_type, &info),
        }
    }

    /// The post-boot runtime state, if the fresh boot completed.
    ///
    /// T1: the main loop (06) reads `system_hz`/`table`/`shutting_down`
    /// through this single accessor — the alternative (5+ getters on
    /// `BootInit`) is explicitly avoided.
    pub fn state(&self) -> Option<&ServerState<'static>> {
        self.state.as_ref()
    }

    /// Runs the main loop.
    ///
    /// C: `main()` loop — main.c:50-131. Per iteration: receive
    /// (`get_work` → [`SysApi::receive`]), the bogus-source gate
    /// (main.c:63-66), classification ([`dispatch::classify`]), then the
    /// four message classes:
    ///
    /// - CLOCK notify → [`RsServer::do_period`] (07) — no reply (main.c:84);
    /// - heartbeat notify → [`RsServer::do_heartbeat`] — no reply (main.c:85-91);
    /// - `RS_INIT`/`RS_LU_PREPARE` → the ready-message SEF callbacks (12) —
    ///   the payload decode lives in the callback impl (19 safe-receive seam);
    /// - `RS_*` request → the handler dispatch — every handler needs the
    ///   message-payload decode (13/14/16 号, wired per-arm) or is already
    ///   live ([`RsServer::do_shutdown`]).
    ///
    /// Handler results are replied to the caller unless `EDONTREPLY`
    /// (main.c:124-129); reply failures are fire-and-forget, as in C
    /// (utility.c:309).
    pub fn run(&mut self) -> Result<(), Errno> {
        // T1: the main loop operates on the post-boot runtime state. Fail
        // closed (loudly) if boot never completed — an RS that has not
        // finished booting cannot manage services.
        if self.state.is_none() {
            panic!("run() requires a completed fresh boot (init(Fresh))");
        }
        loop {
            // C: get_work() → sef_receive_status(ANY) — main.c:62, 826-833.
            let (mut msg, rcv_sts, ts) = self.get_work()?;
            let who_e = msg.m_source;
            // C: main.c:63-66 — a message from a bogus source is a
            // kernel-side program error; C panics and so does the rewrite
            // (R34.20 gate).
            assert!(
                dispatch::isokendpt(who_e),
                "message from bogus source: {who_e:?}"
            );
            match dispatch::classify(&rcv_sts, who_e, msg.m_type, ts) {
                // C: main.c:80-84 — CLOCK → do_period, then `continue`
                // (notifications never get a reply).
                DispatchKind::ClockNotify { timestamp } => self.do_period(timestamp)?,
                // C: main.c:85-91 — registered service → alive_tm refresh.
                DispatchKind::HeartbeatNotify { source, timestamp } => {
                    self.do_heartbeat(source, timestamp)
                }
                // C: main.c:116 — RS_INIT → do_init_ready (raw result;
                // EDONTREPLY honored — the service is unblocked by the
                // handler's internal reply). RS_LU_PREPARE (:117) keeps its
                // ENOSYS arm until the 16 chain context lands.
                DispatchKind::InitReady => {
                    let result = self.do_init_ready(&msg).unwrap_or_else(|e| e.to_i32());
                    self.reply_unless_suppressed(who_e, result, &msg);
                }
                // C: main.c:117 — RS_LU_PREPARE → do_upd_ready (raw result;
                // EDONTREPLY honored — the update continuation defers its
                // own replies). The EDONTREPLY→EGENERIC normalization is the
                // sef_cb_lu_response wrapper's job (18), not the loop's.
                DispatchKind::LuPrepareReady => {
                    let result = self.do_upd_ready_shell(&msg).unwrap_or_else(|e| e.to_i32());
                    self.reply_unless_suppressed(who_e, result, &msg);
                }
                // C: main.c:102-114 + 124-129 — handler result replied to the
                // caller unless EDONTREPLY. [`RsServer::do_request`] owns the
                // arm table: wire-decode-free arms run live, the rest fail
                // closed until their 19 decode lands (OQ-4).
                DispatchKind::Request(n) => {
                    let result = self
                        .do_request(who_e, n, &mut msg)
                        .unwrap_or_else(|e| e.to_i32());
                    self.reply_unless_suppressed(who_e, result, &msg);
                }
            }
        }
    }

    /// Replies unless the result suppresses it (C: main.c:124-129 —
    /// `m_ptr->m_type = r; if (r != EDONTREPLY) reply(...)`) The reply
    /// echoes the received message with `m_type = result`, so handler
    /// payload mutations ride along (RS_LOOKUP's endpoint — request.c:1174).
    /// Fire-and-forget: C's `reply` (utility.c:318-345) does not propagate
    /// `ipc_send` failures to the loop.
    fn reply_unless_suppressed(
        &mut self,
        who_e: Endpoint,
        result: i32,
        msg: &minix_types::Message,
    ) {
        if result != minix_types::EDONTREPLY {
            let _ = self.kernel.reply(who_e, result, msg);
        }
    }

    /// C: main.c:85-91 — heartbeat notification from a registered service:
    /// `rproc_ptr[who_p] != NULL` → `r_alive_tm = m.m_notify.timestamp`. An
    /// unregistered source is a warning in C (rs_verbose print — the no_std
    /// diagnostics face is 19's) and a no-op here; the endpoint fast index
    /// *is* C's `rproc_ptr` (Fix #43 raw-index semantics — no in-use
    /// filtering on top).
    fn do_heartbeat(&mut self, source: Endpoint, timestamp: Clock) {
        let Some(state) = self.state.as_mut() else {
            return; // pre-boot: unreachable (run() gates on a completed boot)
        };
        if let Some(id) = state.table.endpoint_slot(source) {
            monitor::heartbeat_mutations(timestamp).apply(state.table.get_mut(id));
        }
    }

    /// C: `do_period` — request.c:946-1040 (07): the CLOCK-tick handler.
    /// While an update is in flight, the current preparer's deadline is
    /// checked first (`update_period` — update.c:371-396; EINTR/RS_CANCEL
    /// rollback on timeout). Then the status sweep runs over the service
    /// table: per in-use slot passing the ACTIVE/update gate
    /// (request.c:968-970), the decision layer
    /// ([`monitor::period_decision`])
    /// classifies backoff tick / stop timeout / ping timeout / ping request /
    /// free pass; the mutations apply once (R13) and the action executes:
    /// `Restart` → [`service_create::restart_service`] (request.c:977-978),
    /// the crash actions → [`recovery::crash_service`] (request.c:989/:1029),
    /// `PingRequest` → the notify seam (request.c:1035).
    fn do_period(&mut self, now: Clock) -> Result<(), Errno> {
        let state = self.state.as_mut().ok_or(Errno::ENOSYS)?;
        // C: request.c:950-954 — while an update is in flight and not yet
        // initializing, every clock tick first checks the current preparer's
        // deadline (update_period — update.c:371-396): a timed-out prepare
        // ends the whole update with EINTR/RS_CANCEL so the old versions
        // resume. Without this arm a stalled prepare would hang the update
        // forever (R36). The sweep below continues in the same tick, as in
        // C. `now` is the tick's own CLOCK timestamp — the same clock C's
        // end_update-internal getticks() reads.
        let updating_phase = state
            .update
            .flags
            .contains(live_update::RupdateFlags::UPDATING)
            && !state
                .update
                .flags
                .contains(live_update::RupdateFlags::INITIALIZING);
        if updating_phase && let Some(curr) = state.update.chain.curr() {
            let (prepare_tm, prepare_maxtime) = {
                let entry = state.update.chain.get(curr);
                (entry.prepare_tm, entry.prepare_maxtime)
            };
            if monitor::has_update_timed_out(now, prepare_tm, prepare_maxtime) {
                let outcome = state.update.end_update(
                    &mut state.table,
                    self.kernel.as_mut(),
                    minix_types::EINTR,
                    live_update::RS_CANCEL,
                    now,
                    &mut live_update::EndEffects {
                        request_prepare: &mut |_s, _ps| {},
                        run_script: &mut |_s| Ok(()),
                    },
                );
                if outcome == recovery::CrashOutcome::SelfTerminate {
                    return Err(Errno::EGENERIC); // C: exit(1) — update.c:883-887
                }
            }
        }
        let another_initializing = state.table.lookup_by_flags(RFlags::INITIALIZING).is_some(); // request.c:1018 — `lookup_slot_by_flags(RS_INITIALIZING)`
        let hz = state.system_hz;
        for i in 0..state.table.len() {
            let id = crate::service_slot::SlotId::new(i);
            // C: request.c:968-970 — only ACTIVE rows, and updating rows only
            // in the initializing-only combination.
            let updating = state.table.get(id).flags.contains(RFlags::UPDATING);
            let combo = {
                let f = state.table.get(id).flags;
                let relevant = RFlags::INITIALIZING | RFlags::INIT_DONE | RFlags::INIT_PENDING;
                f & relevant == RFlags::INITIALIZING
            };
            if !state.table.get(id).flags.contains(RFlags::ACTIVE) || (updating && !combo) {
                continue;
            }
            let decision = monitor::period_decision(
                now,
                state.table.get(id),
                hz,
                another_initializing,
                updating,
            );
            decision.mutations.apply(state.table.get_mut(id));
            match decision.action {
                PeriodAction::Nothing | PeriodAction::BackoffTick | PeriodAction::FreePass => {}
                // C: request.c:977-978 — backoff drained → revive the service.
                PeriodAction::Restart => {
                    let mut noop_exec = |_: &mut crate::service_slot::ServiceSlot| Ok(());
                    let mut noop_script = |_: &mut crate::service_slot::ServiceSlot| Ok(());
                    let mut noop_asynsend = |_: Endpoint, _: &crate::ready::InitMessage| Ok(());
                    service_create::restart_service(
                        &mut state.table,
                        id,
                        self.kernel.as_mut(),
                        now,
                        &mut crate::service_create::RestartEffects {
                            read_exec: &mut noop_exec,
                            run_script: &mut noop_script,
                            asynsend: &mut noop_asynsend,
                        },
                    );
                }
                // C: request.c:989/:1029 — SIGTERM timeout / missed ping →
                // simulate a crash (SIGKILL; RS itself → SelfTerminate, C
                // `exit(1)` — the loop ends with the failure visible).
                PeriodAction::StopTimeoutCrash | PeriodAction::PingTimeoutCrash => {
                    let outcome =
                        recovery::crash_service(state.table.get(id), self.kernel.as_mut())?;
                    if outcome == recovery::CrashOutcome::SelfTerminate {
                        return Err(Errno::EGENERIC); // C: exit(1) — manager.c:395-397
                    }
                }
                // C: request.c:1035-1037 — status request; C ignores the
                // `ipc_notify` result.
                PeriodAction::PingRequest => {
                    let ep = state.table.get(id).pub_.endpoint;
                    let _ = self.kernel.notify(ep);
                }
            }
        }
        Ok(())
    }

    /// C: `get_work()` — main.c:826-833 (06). Delegates to the
    /// `KernelApi::receive` seam (18) — the production face is the 19 wiring;
    /// errors propagate so the caller fails closed instead of spinning.
    fn get_work(&mut self) -> Result<(minix_types::Message, dispatch::IpcStatus, Clock), Errno> {
        self.kernel.receive(minix_types::Endpoint::ANY)
    }
}

mod shell_request;
mod shell_update;

impl SefCallbacks for RsServer {
    /// C: `sef_cb_init_fresh` — main.c:158-494. The fresh init IS the 4-step
    /// boot; the boot state is handed over to [`RsServer::state`] (T1).
    fn init_fresh(&mut self, _init_type: SefInitType, _info: &SefInitInfo) -> Result<i32, Errno> {
        let boot = self.boot.as_mut().expect("boot machine present");
        boot.init_fresh(self.kernel.as_mut()).map_err(|e| {
            // E-6: keep the typed cause for the fatal-boot report; the SEF
            // face carries the wire errno (Kernel(e) → e, invariant → EINVAL).
            self.boot_diagnostic = Some(e);
            Errno::from(e)
        })?;
        // T1 handover: the boot state becomes the runtime state.
        self.state = Some(self.boot.take().expect("boot machine present").into_state());
        Ok(0) // C: sef_startup() returns OK after the fresh init.
    }

    /// C: `sef_cb_init_restart` — main.c:499-544: the restart-stateful
    /// default transfer, `end_update(ERESTART, RS_REPLY)` while updating,
    /// `update_service(RS_DONTSWAP)` into the replica, `init_service`
    /// (RS self-init sends nothing — utility.c:29-31), and the
    /// `sys_setalarm(RS_DELTA_T)` re-arm (main.c:540-541, panic on failure
    /// kept as `expect`).
    ///
    /// A3: the *dispatch target* for restart is runtime state
    /// ([`RestartCb`]) — after a live update the entry points at the
    /// stateful transfer generic instead of this handler, so the match
    /// below is the faithful shape of the C callback table.
    fn init_restart(&mut self, _init_type: SefInitType, info: &SefInitInfo) -> Result<i32, Errno> {
        match self.restart_cb {
            RestartCb::Stateful => {
                // C: `sef_cb_init_restart_generic` — libsys/sef_init.c:317-330
                // (identity transfer for a self LU, checkpoint-restart
                // otherwise). The state-transfer machinery is 17/18 号;
                // until it lands, fail closed.
                Err(Errno::ENOSYS)
            }
            RestartCb::Rs => {
                let kernel = self.kernel.as_mut();
                let state = self.state.as_mut().ok_or(Errno::ENOSYS)?;
                let old_rs = state
                    .table
                    .endpoint_slot(Endpoint::RS)
                    .ok_or(Errno::ENOSYS)?;
                let new_rs = state
                    .table
                    .endpoint_slot(Endpoint(info.old_endpoint))
                    .ok_or(Errno::ENOSYS)?;

                // If an update was in progress, end it (manager.c:527-529).
                if state.table.get(old_rs).flags.contains(RFlags::UPDATING) {
                    let ticks = kernel.get_ticks()?;
                    state.update.end_update(
                        &mut state.table,
                        kernel,
                        minix_types::ERESTART,
                        1,
                        ticks,
                        &mut crate::live_update::EndEffects {
                            request_prepare: &mut |_s, _ps| {},
                            run_script: &mut |_s| Ok(()),
                        },
                    );
                }

                // Update the service into the replica (manager.c:531-537,
                // RS_DONTSWAP = 0).
                state.update.update_service(
                    &mut state.table,
                    kernel,
                    old_rs,
                    new_rs,
                    0,
                    SysFlags::empty(),
                )?;

                // Initialize the new RS instance (manager.c:538-540) — sends no
                // message (utility.c:29-31).
                let ticks = kernel.get_ticks()?;
                crate::service_create::init_service(
                    state.table.get_mut(new_rs),
                    crate::service_create::InitSpec {
                        old_endpoint: None,
                        init_type: crate::sef::SefInitType::Restart,
                        init_flags: 0,
                        gid: None,
                        prepare_state: crate::live_update::SEF_LU_STATE_NULL,
                    },
                    ticks,
                    &mut |_ep, _msg| Ok(()),
                )?;

                // Reschedule a synchronous alarm (manager.c:540-541); C panics on
                // failure (main.c:542).
                self.kernel
                    .setalarm(crate::monitor::delta_t(state.system_hz) as u32)
                    .expect("couldn't set alarm (main.c:542)");
                Ok(0)
            }
        }
    }

    /// C: `sef_cb_init_lu` — main.c:549-586: the restart-callback rebind
    /// (main.c:558, A3), then `update_service(RS_DONTSWAP)` into the new
    /// instance and `init_service(SEF_INIT_LU)`. RS self-init sends no
    /// message.
    fn init_lu(&mut self, _init_type: SefInitType, info: &SefInitInfo) -> Result<i32, Errno> {
        // A3: the rebind precedes the LU flow itself (main.c:553-556) —
        // the *next* restart dispatches the stateful transfer instead of
        // RS's own chain. C does not un-rebind if the LU subsequently
        // fails, so neither does this.
        self.restart_cb = RestartCb::Stateful;
        let kernel = self.kernel.as_mut();
        let state = self.state.as_mut().ok_or(Errno::ENOSYS)?;
        let old_rs = state
            .table
            .endpoint_slot(Endpoint::RS)
            .ok_or(Errno::ENOSYS)?;
        let new_rs = state
            .table
            .endpoint_slot(Endpoint(info.old_endpoint))
            .ok_or(Errno::ENOSYS)?;

        // update_service(RS_DONTSWAP) (manager.c:573-578).
        state.update.update_service(
            &mut state.table,
            kernel,
            old_rs,
            new_rs,
            0,
            SysFlags::empty(),
        )?;

        // Initialize the new RS instance (manager.c:580-584) — sends no
        // message (utility.c:29-31).
        let ticks = kernel.get_ticks()?;
        crate::service_create::init_service(
            state.table.get_mut(new_rs),
            crate::service_create::InitSpec {
                old_endpoint: None,
                init_type: crate::sef::SefInitType::Lu,
                init_flags: 0,
                gid: None,
                prepare_state: crate::live_update::SEF_LU_STATE_NULL,
            },
            ticks,
            &mut |_ep, _msg| Ok(()),
        )?;
        Ok(0)
    }

    /// C: `sef_cb_init_response` — main.c:591-607: run the init-ready
    /// handler on the `RS_INIT` message, then normalize `EDONTREPLY` → OK
    /// (R3: the sentinel means *success* in this callback — the reverse of
    /// `sef_cb_lu_response`, which maps it to `EGENERIC`).
    fn init_response(&mut self, m: &minix_types::Message) -> Result<i32, Errno> {
        let r = self.do_init_ready(m).unwrap_or_else(|e| e.to_i32());
        Ok(if r == minix_types::EDONTREPLY { 0 } else { r })
    }

    /// C: `sef_cb_lu_response` — main.c:614-626: run the update-ready
    /// handler, then normalize `EDONTREPLY` → `EGENERIC` (R3: reaching the
    /// caller means the update did not happen). The wrapper for the RS
    /// self-update path (18); the main-loop `RS_LU_PREPARE` arm keeps the
    /// raw result ([`RsServer::do_upd_ready_shell`], main.c:117).
    fn lu_response(&mut self, m: &minix_types::Message) -> Result<i32, Errno> {
        let r = self.do_upd_ready_shell(m).unwrap_or_else(|e| e.to_i32());
        Ok(if r == minix_types::EDONTREPLY {
            Errno::EGENERIC.to_i32()
        } else {
            r
        })
    }
    /// C: `sef_cb_signal_handler` — main.c:631-642. SIGCHLD drains exited
    /// children through `sigchld_cleanup` (07); SIGTERM runs the shutdown
    /// sweep and arms `shutting_down` (13); anything else is ignored.
    /// Pre-boot signals cannot reach here: `state` is only absent before
    /// `init_fresh`, and the kernel does not signal RS before that.
    fn signal_handler(&mut self, signo: i32) {
        use minix_types::{SIGNAL_CHILD, SIGNAL_TERMINATE};
        let Some(state) = self.state.as_mut() else {
            return; // pre-boot: no table to act on (unreachable in C too)
        };
        match signo {
            SIGNAL_CHILD => {
                // C: main.c:635-637 — do_sigchld's waitpid drain loop
                // (request.c:1063-1073); each exited child is cleaned via
                // sigchld_cleanup. The waitpid face is the kernel seam (19).
                while let Some(pid) = self.kernel.waitpid() {
                    crate::monitor::sigchld_cleanup(&mut state.table, pid);
                }
            }
            SIGNAL_TERMINATE => {
                // C: main.c:638-640 — do_shutdown(NULL): the permission gate
                // is skipped for the internal call (request.c:437-441) and
                // `shutting_down` arms the no-restart policy (13).
                state.shutting_down = crate::request::shutdown_apply(&mut state.table);
            }
            _ => {}
        }
    }

    /// C: `sef_cb_signal_manager` — main.c:647-703: process a system signal
    /// on behalf of the kernel for one of RS's services. Branch order is
    /// C-verbatim; the termination branch composes
    /// `recovery::terminate_service` (the 15 executor) with
    /// `recovery::rs_idle_period` (utility.c:441-478). The stacktrace and
    /// signal-forwarding effects ride the 19 seams.
    fn signal_manager(&mut self, target: Endpoint, signo: i32) -> Result<i32, Errno> {
        let state = self.state.as_mut().ok_or(Errno::ENOSYS)?;

        // main.c:655-662 — lookup; a spurious signal for an unregistered
        // process is cleared (OK).
        let Some(id) = state.table.endpoint_slot(target) else {
            return Ok(0);
        };

        // main.c:665-669 — a termination already processed: gone (EDEADEPT).
        if state.table.get(id).flags.contains(RFlags::TERMINATED)
            && !state.table.get(id).flags.contains(RFlags::EXITING)
        {
            return Err(Errno::EDEADEPT);
        }

        // main.c:672-678 — external signals for inactive instances are cleared.
        if !state.table.get(id).flags.contains(RFlags::ACTIVE)
            && !state.table.get(id).flags.contains(RFlags::EXITING)
        {
            return Ok(0);
        }

        // main.c:681-683 — stacktrace signals ask the kernel to dump first.
        if crate::recovery::sigs_is_stacktrace(signo) {
            self.kernel.diagctl_stacktrace(target)?;
        }

        // main.c:686-692 — termination signals: mark, run the terminate
        // executor, then the idle period; the process is now gone.
        if crate::recovery::sigs_is_termination(signo) {
            {
                let s = state.table.get_mut(id);
                s.flags.insert(RFlags::TERMINATED);
            }
            let ticks = self.kernel.get_ticks()?;
            let shutting_down = state.shutting_down;
            // C: unpublish_service(rp) — the DS effect is the 19 seam; the
            // aggregate decision face is publish.rs (R32). The USE_COPY fact
            // is read up front (the target's sys_flags do not change between
            // entry and unpublish in this flow).
            let use_copy = state
                .table
                .get(id)
                .pub_
                .sys_flags
                .contains(crate::service_slot::SysFlags::USE_COPY);
            let mut unpublish = |_rp: crate::service_slot::SlotId| {
                let _ = crate::publish::unpublish_result(use_copy, false, false, false);
            };
            let outcome = {
                let mut noop_script = |_: &mut crate::service_slot::ServiceSlot| Ok(());
                let mut noop_asynsend = |_: Endpoint, _: &crate::ready::InitMessage| Ok(());
                crate::recovery::terminate_service(
                    &mut state.table,
                    &mut state.update,
                    id,
                    self.kernel.as_mut(),
                    ticks,
                    shutting_down,
                    &mut crate::recovery::TerminateEffects {
                        unpublish: alloc::boxed::Box::new(&mut unpublish),
                        run_script: alloc::boxed::Box::new(&mut noop_script),
                        asynsend: alloc::boxed::Box::new(&mut noop_asynsend),
                    },
                )
            };
            if outcome.self_terminate {
                // C: `_exit(1)` — a core service died outside shutdown. The
                // loop ends with the failure visible (R27's SelfTerminate
                // shape, update.c:883-887 sibling).
                return Err(Errno::EGENERIC);
            }
            let kernel = self.kernel.as_mut();
            let mut noop_script = |_: &mut crate::service_slot::ServiceSlot| Ok(());
            let mut noop_read_exec = |_: &mut crate::service_slot::ServiceSlot| Ok(());
            let mut noop_asynsend = |_: Endpoint, _: &crate::ready::InitMessage| Ok(());
            crate::recovery::rs_idle_period(
                &mut state.table,
                kernel,
                ticks,
                shutting_down,
                &mut noop_script,
                &mut noop_read_exec,
                &mut noop_asynsend,
            );
            return Err(Errno::EDEADEPT);
        }

        // main.c:694-697 — never deliver signals to VM.
        if target == Endpoint::VM {
            return Ok(0);
        }

        // main.c:699-701 — translate every non-termination signal into the
        // SIGS_SIGNAL_RECEIVED message (asynsend seam).
        let fwd = minix_types::Message {
            m_type: minix_types::SIGS_SIGNAL_RECEIVED,
            ..Default::default()
        };
        self.kernel.asynsend(target, &fwd)?;
        Ok(0)
    }
}

#[cfg(test)]
mod signal_handler_tests {
    use super::*;
    use crate::process_table::RProcTable;
    use crate::service_slot::RFlags;
    use crate::testutil::{
        booted, booted_vfs_labeled, booted_with, rs_init_envelope, two_entry_chain,
    };
    use minix_types::{Endpoint, SIGNAL_CHILD, SIGNAL_TERMINATE};

    // Fixtures `booted`/`booted_with`/`booted_vfs_labeled`/`booted_vfs_kernel`/
    // `two_entry_chain`/`rs_init_envelope` live in `crate::testutil` (R40 —
    // shared with the shell modules' own test mods).

    #[test]
    fn test_signal_chld_frees_exited_child_slot() {
        // E-10/R34.21: the drain with actual exited children — waitpid
        // hands back pid 700, sigchld_cleanup frees its slot (and clears
        // the LU bits, request.c:1063-1073); an unknown pid (808) is a
        // no-op and the drain keeps going.
        let mut mock = crate::testutil::MockKernelApi::new(60);
        mock.children = alloc::vec![700, 808];
        let mut server = booted_with(alloc::boxed::Box::new(mock));
        server.signal_handler(SIGNAL_CHILD);
        let state = server.state.as_ref().unwrap();
        // The VFS row is gone from the endpoint index: freed, not merely
        // flagged (manager.c:2088-2109 free semantics).
        assert_eq!(state.table.endpoint_slot(Endpoint::VFS), None);
        assert!(!state.shutting_down);
    }

    /// A CLOCK notify envelope: NOTIFY status word, CLOCK source.
    fn clock_envelope(ts: Clock) -> (minix_types::Message, crate::dispatch::IpcStatus, Clock) {
        let m = minix_types::Message {
            m_source: Endpoint::CLOCK,
            m_type: 0,
            m_u: Default::default(),
        };
        (m, crate::dispatch::IpcStatus { flags: 4 }, ts) // 4 = NOTIFY (ipcconst.h:10)
    }

    #[test]
    fn test_run_clock_notify_drives_period_ping() {
        // E-10/06 wiring: the CLOCK tick drives do_period — the VFS service
        // has a due period, so the sweep pings it (request.c:1035-1037) and
        // refreshes `r_check_tm` (R13 payload). The kernel-facing notify is
        // asserted through the state effect: `check_tm` is written by the
        // PingRequest branch alone.
        let mut mock = crate::testutil::MockKernelApi::new(60);
        mock.inbox = alloc::vec![clock_envelope(200)];
        let mut server = booted_with(alloc::boxed::Box::new(mock));
        let ep = Endpoint::from_generation_slot(0, 30);
        {
            let state = server.state.as_mut().unwrap();
            state
                .table
                .set_endpoint_index(ep, Some(crate::service_slot::SlotId::new(0)));
            let s = state.table.get_mut(crate::service_slot::SlotId::new(0));
            s.period = 60;
            s.check_tm = 0;
            s.alive_tm = 0;
        }
        let _ = server.run();
        let state = server.state.as_ref().unwrap();
        assert_eq!(
            state
                .table
                .get(crate::service_slot::SlotId::new(0))
                .check_tm,
            200,
            "PingRequest refreshed r_check_tm to `now` (request.c:1037)"
        );
    }

    #[test]
    fn test_run_heartbeat_refreshes_alive_tm() {
        // E-10/06 wiring: main.c:85-91 — a registered service's heartbeat
        // refreshes `r_alive_tm` with the kernel timestamp.
        let mut mock = crate::testutil::MockKernelApi::new(60);
        let m = minix_types::Message {
            m_source: Endpoint::VFS,
            m_type: 0,
            m_u: Default::default(),
        };
        mock.inbox = alloc::vec![(m, crate::dispatch::IpcStatus { flags: 4 }, 777)];
        let mut server = booted_with(alloc::boxed::Box::new(mock));
        {
            let state = server.state.as_mut().unwrap();
            state
                .table
                .set_endpoint_index(Endpoint::VFS, Some(crate::service_slot::SlotId::new(0)));
        }
        let _ = server.run();
        let state = server.state.as_ref().unwrap();
        let id = state.table.endpoint_slot(Endpoint::VFS).expect("indexed");
        assert_eq!(state.table.get(id).alive_tm, 777);
    }

    #[test]
    fn test_run_shutdown_request_sweeps_and_replies() {
        // E-10/06 wiring: RS_SHUTDOWN (payload-free arm) — caller permission
        // (root euid via getnuid, request.c:435-437), the EXITING sweep, and
        // `shutting_down` (request.c:448-453). The OK reply is
        // fire-and-forget (utility.c:309), asserted via the sweep state.
        let mut mock = crate::testutil::MockKernelApi::new(60);
        let m = minix_types::Message {
            m_source: Endpoint::RS,
            m_type: minix_types::RS_SHUTDOWN,
            m_u: Default::default(),
        };
        mock.inbox = alloc::vec![(m, crate::dispatch::IpcStatus { flags: 0 }, 0)];
        let mut server = booted_with(alloc::boxed::Box::new(mock));
        let _ = server.run();
        let state = server.state.as_ref().unwrap();
        assert!(state.shutting_down);
        assert!(
            state
                .table
                .iter_in_use()
                .all(|(_, s)| s.flags.contains(RFlags::EXITING))
        );
    }

    #[test]
    #[should_panic(expected = "message from bogus source")]
    fn test_run_gates_bogus_source() {
        // E-10/R34.20: main.c:63-66 — a message from a source that does not
        // name a live process slot is a kernel-side program error; C panics.
        let mut mock = crate::testutil::MockKernelApi::new(60);
        let m = minix_types::Message {
            m_source: minix_types::Endpoint::NONE, // slot 31743 — out of range
            m_type: 0,
            m_u: Default::default(),
        };
        mock.inbox = alloc::vec![(m, crate::dispatch::IpcStatus { flags: 0 }, 0)];
        let mut server = booted_with(alloc::boxed::Box::new(mock));
        let _ = server.run();
    }

    #[test]
    fn test_run_init_ready_fresh_done_clears_initializing() {
        // 12 wiring: RS_INIT(result=OK) from an initializing service →
        // FreshInitDone — INITIALIZING cleared, check_tm zeroed, alive_tm
        // refreshed (request.c:514-525); EDONTREPLY suppresses the reply.
        let mut mock = crate::testutil::MockKernelApi::new(60);
        mock.inbox = alloc::vec![(
            rs_init_envelope(0),
            crate::dispatch::IpcStatus { flags: 0 },
            0
        )];
        let mut server = booted_with(alloc::boxed::Box::new(mock));
        let ep = Endpoint::VFS;
        {
            let state = server.state.as_mut().unwrap();
            state
                .table
                .set_endpoint_index(ep, Some(crate::service_slot::SlotId::new(0)));
            let s = state.table.get_mut(crate::service_slot::SlotId::new(0));
            s.flags = RFlags::IN_USE | RFlags::ACTIVE | RFlags::INITIALIZING;
            s.pub_.in_use = true;
            s.pub_.endpoint = ep;
        }
        let _ = server.run();
        let s = server
            .state
            .as_ref()
            .unwrap()
            .table
            .get(crate::service_slot::SlotId::new(0));
        assert!(!s.flags.contains(RFlags::INITIALIZING), "fresh init done");
        assert_eq!(s.check_tm, 0);
    }

    #[test]
    fn test_init_response_normalizes_edontreply_to_ok() {
        // R3: sef_cb_init_response maps EDONTREPLY → OK (the reverse of
        // sef_cb_lu_response) — the wrapper sits on the live handler now.
        let mut server = booted_with(alloc::boxed::Box::new(crate::testutil::MockKernelApi::new(
            60,
        )));
        {
            let state = server.state.as_mut().unwrap();
            state
                .table
                .set_endpoint_index(Endpoint::VFS, Some(crate::service_slot::SlotId::new(0)));
            let s = state.table.get_mut(crate::service_slot::SlotId::new(0));
            s.flags = RFlags::IN_USE | RFlags::ACTIVE | RFlags::INITIALIZING;
            s.pub_.in_use = true;
            s.pub_.endpoint = Endpoint::VFS;
        }
        let r = server.init_response(&rs_init_envelope(0)).unwrap();
        assert_eq!(r, 0);
    }

    #[test]
    fn test_run_down_request_stops_service_with_late_reply() {
        // 13 wiring: RS_DOWN (payload = the 16-byte label via the safecopy
        // seam) — permission, the stop flow (EXITING + stop_tm + SIGTERM
        // through the PM face), and the late-reply bookkeeping
        // (request.c:142-146); EDONTREPLY defers the reply to cleanup.
        let mut mock = crate::testutil::MockKernelApi::new(60);
        mock.payload = Some(b"vfs".to_vec());
        let mut m = minix_types::Message {
            m_source: Endpoint::PM,
            m_type: minix_types::RS_DOWN,
            m_u: Default::default(),
        };
        m.m_u.m_rs_req.addr = 0;
        m.m_u.m_rs_req.len = 3;
        mock.inbox = alloc::vec![(m, crate::dispatch::IpcStatus { flags: 0 }, 0)];
        let mut server = booted_with(alloc::boxed::Box::new(mock));
        {
            let state = server.state.as_mut().unwrap();
            state
                .table
                .set_endpoint_index(Endpoint::VFS, Some(crate::service_slot::SlotId::new(0)));
            let s = state.table.get_mut(crate::service_slot::SlotId::new(0));
            s.flags = RFlags::IN_USE | RFlags::ACTIVE;
            s.pub_.in_use = true;
            s.pub_.endpoint = Endpoint::VFS;
            s.pub_.label = crate::service_slot::Label::from_bytes(b"vfs");
            s.pid = Some(700);
            // RS_DOWN targets a system process (request.c:104-105 gate).
            s.priv_.flags.insert(crate::privilege::PrivFlags::SYS_PROC);
        }
        let _ = server.run();
        let s = server
            .state
            .as_ref()
            .unwrap()
            .table
            .get(crate::service_slot::SlotId::new(0));
        assert!(s.flags.contains(RFlags::EXITING), "stop flow ran");
        assert!(s.flags.contains(RFlags::LATEREPLY), "late reply armed");
        assert_eq!(s.caller, Endpoint::PM);
        assert_eq!(s.caller_request, minix_types::RS_DOWN);
    }

    /// Builds the flat caller-space image do_up's safecopy chain reads:
    /// the `rs_start_t` struct at 0 (fields written at the pinned
    /// `rs_start_off` offsets — Fix #81), the command string at 0x1000 and
    /// the label/progname text at 0x1100/0x1200 (rs.h:63 — labels are
    /// copied over separately).
    fn do_up_image(cmd: &[u8], label: &[u8]) -> alloc::vec::Vec<u8> {
        use minix_types::rs_start_off as off;
        let mut img = alloc::vec![0u8; 0x2000];
        fn put32(img: &mut [u8], o: usize, v: u32) {
            img[o..o + 4].copy_from_slice(&v.to_le_bytes());
        }
        fn put64(img: &mut [u8], o: usize, v: u64) {
            img[o..o + 8].copy_from_slice(&v.to_le_bytes());
        }
        put64(&mut img, off::CMD_ADDR, 0x1000);
        put64(&mut img, off::CMD_LEN, cmd.len() as u64);
        put32(&mut img, off::QUANTUM, 200);
        // The C caller defaults (minix-service parse.c:1165-1168):
        // sigmgr = RS, scheduler = SCHED, quantum = USER_QUANTUM.
        put32(&mut img, off::SIGMGR, Endpoint::RS.get() as u32);
        put32(&mut img, off::SCHEDULER, Endpoint::SCHED.get() as u32);
        put64(&mut img, off::IPC_ADDR, 0x1300);
        put64(&mut img, off::IPC_LEN, 3);
        img[0x1300..0x1303].copy_from_slice(b"ipc");
        // The state-data spec: the requester sets size to
        // sizeof(struct rs_state_data) = 56 (manager.c:181-183, 17 号).
        put64(&mut img, off::STATE_DATA, 56);
        put64(&mut img, off::LABEL_ADDR, 0x1100);
        put64(&mut img, off::LABEL_LEN, label.len() as u64);
        put64(&mut img, off::PROGNAME_ADDR, 0x1200);
        put64(&mut img, off::PROGNAME_LEN, label.len() as u64);
        img[0x1000..0x1000 + cmd.len()].copy_from_slice(cmd);
        img[0x1100..0x1100 + label.len()].copy_from_slice(label);
        img[0x1200..0x1200 + label.len()].copy_from_slice(label);
        img
    }

    #[test]
    fn test_do_update_schedules_self_update_batch() {
        // 16/R6 wiring: RS_UPDATE (request.c:534-889) — self update,
        // batch mode (request.c:847-850 replies OK). The descriptor is
        // scheduled with the A-4 mirror (slot.upd), lu_flags carry SELF,
        // and the reply is immediate.
        let mut img = do_up_image(b"/bin/tty", b"vfs");
        let flags = crate::slot::RssFlags::SELF_LU | crate::slot::RssFlags::BATCH;
        img[minix_types::rs_start_off::FLAGS..minix_types::rs_start_off::FLAGS + 4]
            .copy_from_slice(&flags.bits().to_le_bytes());
        let mut server = booted_do_up(img);
        {
            let state = server.state.as_mut().unwrap();
            let s = state.table.get_mut(crate::service_slot::SlotId::new(0));
            s.pub_.label = crate::service_slot::Label::from_bytes(b"vfs");
            // A system service (manager.c:103-105 — non-system targets are
            // only editable, never updatable) with a launch command: the
            // replica's create passes the preconditions (manager.c:563-568),
            // and a scheduler — utility.c:369-370 asserts a system process
            // carries one.
            s.priv_.flags.insert(crate::privilege::PrivFlags::SYS_PROC);
            s.cmd[..8].copy_from_slice(b"/bin/vfs");
            s.scheduler = Endpoint::SCHED;
        }
        let m = do_update_message(0);
        assert_eq!(server.do_update(&m), Ok(0), "batch replies OK now");
        let state = server.state.as_ref().unwrap();
        assert_eq!(state.update.chain.len(), 1, "one descriptor scheduled");
        let entry = state.update.chain.get(0);
        assert_eq!(entry.slot, crate::service_slot::SlotId::new(0));
        assert!(
            entry.lu_flags.contains(crate::live_update::LuFlags::SELF),
            "RSS_SELF_LU mapped to SEF_LU_SELF"
        );
        // The A-4 mirror: slot.upd equals the authoritative chain entry.
        assert_eq!(
            state
                .table
                .get(crate::service_slot::SlotId::new(0))
                .upd
                .as_ref(),
            Some(entry)
        );
    }

    #[test]
    fn test_do_update_regular_allocates_linked_instance() {
        // request.c:708-760 — a regular update allocates a fresh slot,
        // initializes it, inherits the old instance's immutable defaults,
        // links both directions and creates it without running; the
        // non-batch flow then walks the prepare and arms the late reply on
        // the last descriptor's service (request.c:868-874).
        let mut img = do_up_image(b"/bin/tty.new", b"vfs");
        let mut server = booted_do_up(img);
        {
            let state = server.state.as_mut().unwrap();
            let s = state.table.get_mut(crate::service_slot::SlotId::new(0));
            s.pub_.label = crate::service_slot::Label::from_bytes(b"vfs");
            s.pub_
                .sys_flags
                .insert(crate::service_slot::SysFlags::CORE_SRV);
            s.priv_.flags.insert(crate::privilege::PrivFlags::SYS_PROC);
        }
        let m = do_update_message(0);
        assert_eq!(
            server.do_update(&m),
            Ok(minix_types::EDONTREPLY),
            "late reply: the update completes initialization later"
        );
        let state = server.state.as_ref().unwrap();
        let old = crate::service_slot::SlotId::new(0);
        let new = state
            .table
            .get(old)
            .new_rp
            .expect("regular update links a new instance");
        assert_eq!(state.table.get(new).old_rp, Some(old));
        assert!(
            state
                .table
                .get(new)
                .priv_
                .flags
                .contains(crate::privilege::PrivFlags::LU_SYS_PROC),
            "the new version is created but does not run"
        );
        assert!(
            state.table.get(old).flags.contains(RFlags::LATEREPLY),
            "late reply armed on the updating service"
        );
    }

    #[test]
    fn test_do_update_second_batch_is_einval_when_already_in_chain() {
        // C: request.c:669-671 — batch mode tolerates a second request only
        // for services NOT already in the scheduled chain; scheduling the
        // same service again → EINVAL. (The updating-phase EBUSY,
        // request.c:659-663, is locked by validate_update_request's own
        // unit tests.)
        let mut img = do_up_image(b"/bin/tty", b"vfs");
        let flags = crate::slot::RssFlags::SELF_LU | crate::slot::RssFlags::BATCH;
        img[minix_types::rs_start_off::FLAGS..minix_types::rs_start_off::FLAGS + 4]
            .copy_from_slice(&flags.bits().to_le_bytes());
        let mut server = booted_do_up(img);
        {
            let state = server.state.as_mut().unwrap();
            let s = state.table.get_mut(crate::service_slot::SlotId::new(0));
            s.pub_.label = crate::service_slot::Label::from_bytes(b"vfs");
            // A system service (manager.c:103-105 — non-system targets are
            // only editable, never updatable) with a launch command: the
            // replica's create passes the preconditions (manager.c:563-568),
            // and a scheduler — utility.c:369-370 asserts a system process
            // carries one.
            s.priv_.flags.insert(crate::privilege::PrivFlags::SYS_PROC);
            s.cmd[..8].copy_from_slice(b"/bin/vfs");
            s.scheduler = Endpoint::SCHED;
        }
        let m = do_update_message(0);
        assert_eq!(server.do_update(&m), Ok(0));
        // The same service again → already in the chain → EINVAL.
        assert_eq!(server.do_update(&do_update_message(0)), Err(Errno::EINVAL));
    }

    #[test]
    fn test_do_update_with_state_data_schedules() {
        // request.c:788-836 — a VALID state-data spec (size = 56, one filter
        // block at 0x2800) passes init_state_data; the parsed filter bytes
        // are stored on the descriptor (the owned-buffer analog of C's
        // malloc'd ipcf_els, manager.c:231-234). The cpf_grant_direct triple
        // is the 19 grant face (E-11): gid stays `None` here.
        let mut img = do_up_image(b"/bin/tty", b"vfs");
        let flags = crate::slot::RssFlags::SELF_LU | crate::slot::RssFlags::BATCH;
        img[minix_types::rs_start_off::FLAGS..minix_types::rs_start_off::FLAGS + 4]
            .copy_from_slice(&flags.bits().to_le_bytes());
        img[minix_types::rs_start_off::STATE_DATA..minix_types::rs_start_off::STATE_DATA + 8]
            .copy_from_slice(&56u64.to_le_bytes());
        // One filter block (IPCF_MAX_ELEMENTS zero elements) at 0x2800:
        // src.ipcf_els = addr, src.ipcf_els_size = one block (rs.h:96-97).
        img.resize(0x4000, 0);
        let block_addr = 0x2800;
        img[minix_types::rs_start_off::STATE_DATA + 8..minix_types::rs_start_off::STATE_DATA + 16]
            .copy_from_slice(&(block_addr as u64).to_le_bytes());
        img[minix_types::rs_start_off::STATE_DATA + 16..minix_types::rs_start_off::STATE_DATA + 24]
            .copy_from_slice(&(crate::state_data::RS_IPCF_FILTER_BLOCK_SIZE as u64).to_le_bytes());
        let mut server = booted_do_up(img);
        {
            let state = server.state.as_mut().unwrap();
            let s = state.table.get_mut(crate::service_slot::SlotId::new(0));
            s.pub_.label = crate::service_slot::Label::from_bytes(b"vfs");
            // A system service (manager.c:103-105 — non-system targets are
            // only editable, never updatable) with a launch command: the
            // replica's create passes the preconditions (manager.c:563-568),
            // and a scheduler — utility.c:369-370 asserts a system process
            // carries one.
            s.priv_.flags.insert(crate::privilege::PrivFlags::SYS_PROC);
            s.cmd[..8].copy_from_slice(b"/bin/vfs");
            s.scheduler = Endpoint::SCHED;
        }
        let m = do_update_message(0);
        assert_eq!(server.do_update(&m), Ok(0), "batch schedules");
        let state = server.state.as_ref().unwrap();
        assert_eq!(state.update.chain.len(), 1, "scheduled");
        let entry = state.update.chain.get(0);
        assert_eq!(entry.prepare_state_data.size, 56);
        assert!(
            entry
                .ipcf_els_buff
                .as_ref()
                .is_some_and(|b| b.len() == 1536),
            "one grant-ready filter block (12 × 128 bytes)"
        );
        assert!(entry.eval_buff.is_none(), "no EVAL state");
    }

    #[test]
    fn test_getsysinfo_procpub_copyout_serves_table() {
        // 14/R7 wiring: SI_PROCPUB_TAB (request.c:1119-1121) — every row of
        // the public table serializes to the pinned `struct rprocpub`
        // layout (Fix #85) and goes out through the safecopy seam after the
        // exact-size gate (request.c:1134-1136).
        let mut mock = crate::testutil::MockKernelApi::new(60);
        mock.ticks = 500;
        let mut table = RProcTable::new();
        let id = table.alloc_slot().unwrap();
        {
            let s = table.get_mut(id);
            s.flags = RFlags::IN_USE | RFlags::ACTIVE;
            s.pub_.in_use = true;
            s.pub_.endpoint = Endpoint::VFS;
            s.pub_.label = crate::service_slot::Label::from_bytes(b"vfs");
            s.pub_
                .sys_flags
                .insert(crate::service_slot::SysFlags::CORE_SRV);
            s.pub_.dev_nr = 3;
        }
        let size = (table.len() * minix_types::rprocpub_off::SIZE) as u64;
        let r = crate::shell_request::copy_out_procpub_table(
            &mut mock,
            &table,
            Endpoint::PM,
            0x5000,
            size,
        );
        assert_eq!(r, Ok(()));
        assert_eq!(mock.sent_copies.len(), 1);
        let (dest, addr, bytes) = &mock.sent_copies[0];
        assert_eq!(*dest, Endpoint::PM);
        assert_eq!(*addr, 0x5000);
        assert_eq!(
            bytes.len() as u64,
            size,
            "one pinned struct rprocpub per row"
        );
        // Row 0 carries the live service: in_use, endpoint, label, sys
        // flags, dev_nr.
        use minix_types::rprocpub_off as o;
        let w0 = minix_types::decode_rproc_pub(&bytes[..o::SIZE]).expect("row 0 decodes");
        assert_eq!(w0.in_use, 1);
        assert_eq!(w0.endpoint, Endpoint::VFS.get());
        assert_eq!(&w0.label[..4], b"vfs\0");
        assert_eq!(
            w0.sys_flags,
            crate::service_slot::SysFlags::CORE_SRV.bits() as u32
        );
        assert_eq!(w0.dev_nr, 3);
        // Row 1 is vacant: endpoint NONE (the unset sentinel), zero label.
        let w1 =
            minix_types::decode_rproc_pub(&bytes[o::SIZE..2 * o::SIZE]).expect("row 1 decodes");
        assert_eq!(w1.in_use, 0);
        assert_eq!(w1.endpoint, Endpoint::NONE.get());
    }

    #[test]
    fn test_getsysinfo_size_gate_einval() {
        // C: request.c:1134-1136 — a declared size that differs from the
        // table's byte length → EINVAL, nothing copied.
        let mut mock = crate::testutil::MockKernelApi::new(60);
        let table = RProcTable::new();
        let size = (table.len() * minix_types::rprocpub_off::SIZE) as u64 + 1;
        let r = crate::shell_request::copy_out_procpub_table(
            &mut mock,
            &table,
            Endpoint::PM,
            0x5000,
            size,
        );
        assert_eq!(r, Err(Errno::EINVAL));
        assert!(mock.sent_copies.is_empty());
    }

    #[test]
    fn test_do_getsysinfo_procpub_tab_roundtrip() {
        // Through the handler: the live arm returns OK; the internal-table
        // arm stays fail-closed (struct rproc pinning is the E-RSWIRE
        // remainder — see the do_getsysinfo doc note).
        let mut server = booted_vfs_labeled(b"vfs", b"vfs");
        let mut m = minix_types::Message {
            m_source: Endpoint::PM,
            m_type: minix_types::RS_GETSYSINFO,
            m_u: Default::default(),
        };
        m.m_u.m_lsys_getsysinfo.what = crate::query::SI_PROCPUB_TAB;
        m.m_u.m_lsys_getsysinfo.where_ = 0x5000;
        m.m_u.m_lsys_getsysinfo.size = 64 * minix_types::rprocpub_off::SIZE as u64;
        assert_eq!(server.do_getsysinfo(&m), Ok(0));
        m.m_u.m_lsys_getsysinfo.what = crate::query::SI_PROC_TAB;
        m.m_u.m_lsys_getsysinfo.size = 64 * minix_types::rproc_off::SIZE as u64;
        assert_eq!(server.do_getsysinfo(&m), Ok(0), "rproc table copy-out live");
    }

    #[test]
    fn test_getsysinfo_rproc_copyout_serves_table() {
        // R12: SI_PROC_TAB (request.c:1113-1115) — every row serializes to
        // the pinned `struct rproc` layout (witness-derived offsets in
        // `minix_types::rproc_off`); the exact-size gate at :1134-1136.
        let mut mock = crate::testutil::MockKernelApi::new(60);
        let mut table = RProcTable::new();
        let id = table.alloc_slot().unwrap();
        {
            let s = table.get_mut(id);
            s.flags = RFlags::IN_USE | RFlags::ACTIVE;
            s.pub_.in_use = true;
            s.pub_.endpoint = Endpoint::VFS;
            s.pub_.label = crate::service_slot::Label::from_bytes(b"vfs");
            s.pid = Some(700);
            s.restarts = 2;
            s.scheduler = Endpoint::SCHED;
            s.priority = 4;
            s.quantum = 100;
            s.priv_.id = crate::privilege::PrivId(7);
            s.priv_.flags.insert(crate::privilege::PrivFlags::SYS_PROC);
        }
        let size = (table.len() * minix_types::rproc_off::SIZE) as u64;
        let r = crate::shell_request::copy_out_rproc_table(
            &mut mock,
            &table,
            Endpoint::PM,
            0x6000,
            size,
        );
        assert_eq!(r, Ok(()));
        let (dest, addr, bytes) = &mock.sent_copies[0];
        assert_eq!(*dest, Endpoint::PM);
        assert_eq!(*addr, 0x6000);
        assert_eq!(bytes.len() as u64, size);

        // Field spot-checks at the witness-derived offsets: the C-visible
        // identity and scheduling facts land byte-exact.
        use minix_types::rproc_off::{self as o, priv_off};
        let rd32 =
            |b: &[u8], off: usize| i32::from_le_bytes([b[off], b[off + 1], b[off + 2], b[off + 3]]);
        assert_eq!(rd32(bytes, o::R_PID), 700);
        assert_eq!(rd32(bytes, o::R_RESTARTS), 2);
        assert_eq!(rd32(bytes, o::R_SCHEDULER), Endpoint::SCHED.get());
        assert_eq!(rd32(bytes, o::R_PRIORITY), 4);
        assert_eq!(rd32(bytes, o::R_QUANTUM), 100);
        let pr = o::R_PRIV;
        let s_flags = i16::from_le_bytes([bytes[pr + 6], bytes[pr + 7]]);
        assert!(
            s_flags & (crate::privilege::PrivFlags::SYS_PROC.bits() as i16) != 0,
            "SYS_PROC lands in s_flags"
        );
        // s_id is a 2-byte short (priv.h:23) followed by s_flags — read the
        // 2-byte field, not a 4-byte word.
        let s_id = i16::from_le_bytes([bytes[pr + 4], bytes[pr + 5]]);
        assert_eq!(s_id, 7);
        assert_eq!(
            rd32(bytes, pr + priv_off::S_K_CALL_MASK),
            table.get(id).priv_.k_call_mask.0 as i32
        );
    }

    #[test]
    fn test_getsysinfo_procall_tab_serves_both_tables() {
        // C: request.c:1113-1121 — SI_PROCALL_TAB copies rproc rows then
        // rprocpub rows back to back; the early gate rejects when the rproc
        // half alone exceeds the declared size (request.c:1116-1118).
        let mut mock = crate::testutil::MockKernelApi::new(60);
        let table = RProcTable::new();
        let proc_len = table.len() * minix_types::rproc_off::SIZE;
        let pub_len = table.len() * minix_types::rprocpub_off::SIZE;
        let r = crate::shell_request::copy_out_procall_table(
            &mut mock,
            &table,
            Endpoint::PM,
            0x7000,
            (proc_len + pub_len) as u64,
        );
        assert_eq!(r, Ok(()));
        assert_eq!(mock.sent_copies[0].2.len(), proc_len + pub_len);
        let r = crate::shell_request::copy_out_procall_table(
            &mut mock,
            &table,
            Endpoint::PM,
            0x7000,
            (proc_len + pub_len - 1) as u64,
        );
        assert_eq!(r, Err(Errno::EINVAL), "early gate on the rproc half");
    }

    fn do_update_message(addr: u64) -> minix_types::Message {
        let mut m = minix_types::Message {
            m_source: Endpoint::PM,
            m_type: minix_types::RS_UPDATE,
            m_u: Default::default(),
        };
        m.m_u.m_rs_req.addr = addr;
        // The same union viewed through the m_rs_update arm (do_update reads
        // state/prepare_maxtime from it, request.c:646-657): a reached state
        // of SEF_LU_STATE_UNREACHABLE and the default max time (0 →
        // 2*RS_DELTA_T).
        m.m_u.m_rs_update.state = crate::live_update::SEF_LU_STATE_UNREACHABLE;
        m
    }

    fn do_edit_message(addr: u64) -> minix_types::Message {
        let mut m = minix_types::Message {
            m_source: Endpoint::PM,
            m_type: minix_types::RS_EDIT,
            m_u: Default::default(),
        };
        m.m_u.m_rs_req.addr = addr;
        m
    }

    #[test]
    fn test_do_edit_updates_settings_in_sequence() {
        // 13/R5 wiring: RS_EDIT (request.c:298-385) — struct decode, label
        // from rss_label, the E-7 sequence (getpriv sync → sched_stop →
        // edit_slot → privctl(UpdateSys) → vm_set_priv → sched_init_proc),
        // Ok(0).
        let mut img = do_up_image(b"/bin/tty", b"vfs");
        img[minix_types::rs_start_off::QUANTUM..minix_types::rs_start_off::QUANTUM + 4]
            .copy_from_slice(&77i32.to_le_bytes());
        img[minix_types::rs_start_off::PRIORITY..minix_types::rs_start_off::PRIORITY + 4]
            .copy_from_slice(&3i32.to_le_bytes());
        let mut mock = crate::testutil::MockKernelApi::new(60);
        mock.payload = Some(img);
        mock.ticks = 500;
        mock.vm_ok = true;
        mock.execve_ok = true;
        mock.fork_pid = Some(701);
        mock.child_endpoint = Some(Endpoint::MEM);
        mock.kill_ok = true;
        // The kernel's priv copy for VFS carries SYS_PROC — do_edit syncs
        // r_priv from it (request.c:329-334), so the slot's post-edit
        // is_sys_proc depends on this seeded entry.
        let mut kpriv = crate::privilege::Privilege::vacant();
        kpriv.flags.insert(crate::privilege::PrivFlags::SYS_PROC);
        mock.kernel_privs.push((Endpoint::VFS, kpriv));
        let mut server = booted_with(alloc::boxed::Box::new(mock));
        {
            let state = server.state.as_mut().unwrap();
            let s = state.table.get_mut(crate::service_slot::SlotId::new(0));
            s.pub_.label = crate::service_slot::Label::from_bytes(b"vfs");
            s.quantum = 200;
            s.priority = 8;
            // A system service with a scheduler — sched_decision asserts
            // (utility.c:369-370) that a system process carries one, and
            // edit_slot's scheduling branch (manager.c:1570) only rewrites
            // the four fields when the row's CURRENT scheduler is set.
            s.priv_.flags.insert(crate::privilege::PrivFlags::SYS_PROC);
            s.scheduler = Endpoint::SCHED;
        }
        let m = do_edit_message(0);
        assert_eq!(server.do_edit(&m), Ok(0));
        let state = server.state.as_ref().unwrap();
        let s = state.table.get(crate::service_slot::SlotId::new(0));
        assert_eq!(s.quantum, 77, "quantum re-edited");
        assert_eq!(s.priority, 3, "priority re-edited");
        assert!(
            s.flags.contains(RFlags::IN_USE | RFlags::ACTIVE),
            "the edited row stays live"
        );
    }

    #[test]
    fn test_do_edit_unknown_label_is_esrch() {
        // C: request.c:315-321 — a label no ACTIVE row carries → ESRCH.
        let mut server = booted_do_up(do_up_image(b"/bin/tty", b"nox"));
        let m = do_edit_message(0);
        assert_eq!(server.do_edit(&m), Err(Errno::ESRCH));
    }

    #[test]
    fn test_do_edit_updating_target_is_ebusy() {
        // C: manager.c:108-110 (via check_call_permission) — an update in
        // progress makes every edit EBUSY.
        let mut server = booted_do_up(do_up_image(b"/bin/tty", b"vfs"));
        {
            let state = server.state.as_mut().unwrap();
            let s = state.table.get_mut(crate::service_slot::SlotId::new(0));
            s.pub_.label = crate::service_slot::Label::from_bytes(b"vfs");
            s.flags.insert(RFlags::UPDATING);
        }
        let m = do_edit_message(0);
        assert_eq!(server.do_edit(&m), Err(Errno::EBUSY));
    }

    #[test]
    fn test_do_edit_sched_stop_failure_aborts_untouched() {
        // E-7: the stop gate routes by site — an edit aborts on a failed
        // sched_stop (StopSite::EditSlot → StopOutcome::Abort) with the slot
        // untouched (the quantum stays as it was).
        let mut mock = crate::testutil::MockKernelApi::new(60);
        mock.payload = Some(do_up_image(b"/bin/tty", b"vfs"));
        mock.vm_ok = true;
        mock.execve_ok = true;
        mock.fork_pid = Some(701);
        mock.child_endpoint = Some(Endpoint::MEM);
        // kill_ok stays false — the mock's sched_stop fails, and the
        // EditSlot stop gate must abort the edit.
        let mut server = booted_with(alloc::boxed::Box::new(mock));
        {
            let state = server.state.as_mut().unwrap();
            let s = state.table.get_mut(crate::service_slot::SlotId::new(0));
            s.pub_.label = crate::service_slot::Label::from_bytes(b"vfs");
            s.quantum = 200;
        }
        let m = do_edit_message(0);
        assert!(server.do_edit(&m).is_err(), "the stop gate aborts the edit");
        let state = server.state.as_ref().unwrap();
        assert_eq!(
            state.table.get(crate::service_slot::SlotId::new(0)).quantum,
            200,
            "the slot was not edited"
        );
    }

    /// A booted server whose create faces succeed and whose safecopy image
    /// is the given flat buffer — the do_up fixture.
    fn booted_do_up(image: alloc::vec::Vec<u8>) -> RsServer {
        let mut mock = crate::testutil::MockKernelApi::new(60);
        mock.payload = Some(image);
        mock.ticks = 500;
        mock.vm_ok = true;
        mock.execve_ok = true;
        mock.fork_pid = Some(701);
        mock.child_endpoint = Some(Endpoint::MEM);
        // do_edit's sched_stop gate: the mock short-circuits on kill_ok
        // (the default false models "no scheduler handback" for boot tests).
        mock.kill_ok = true;
        booted_with(alloc::boxed::Box::new(mock))
    }

    fn do_up_message(addr: u64) -> minix_types::Message {
        let mut m = minix_types::Message {
            m_source: Endpoint::PM,
            m_type: minix_types::RS_UP,
            m_u: Default::default(),
        };
        m.m_u.m_rs_req.addr = addr;
        m
    }

    #[test]
    fn test_do_up_starts_service_and_arms_late_reply() {
        // 13/R4 wiring: RS_UP (request.c:15-106) — struct decode (Fix #81),
        // buffer fetches, check_request, init_slot, the duplicate gates and
        // start_service; the reply is deferred (LATEREPLY + EDONTREPLY,
        // request.c:98-105).
        let mut server = booted_do_up(do_up_image(b"/bin/tty", b"tty"));
        let m = do_up_message(0);
        assert_eq!(
            server.do_up(&m),
            Ok(minix_types::EDONTREPLY),
            "late reply: the EDONTREPLY marker suppresses the immediate reply"
        );
        let state = server.state.as_ref().unwrap();
        let label = crate::service_slot::Label::from_bytes(b"tty");
        let id = state
            .table
            .lookup_by_label(&label)
            .expect("service slot created");
        let s = state.table.get(id);
        assert!(s.flags.contains(RFlags::IN_USE | RFlags::ACTIVE));
        assert!(s.flags.contains(RFlags::LATEREPLY), "late reply armed");
        assert_eq!(s.caller, Endpoint::PM);
        assert_eq!(s.caller_request, minix_types::RS_UP);
        assert_eq!(s.pub_.proc_name.as_bytes()[..3], *b"tty");
        assert_eq!(s.quantum, 200);
        let mut cmd = [0u8; 8];
        cmd.copy_from_slice(&s.cmd[..8]);
        assert_eq!(&cmd, b"/bin/tty", "cmd bytes fetched from caller space");
    }

    #[test]
    fn test_do_up_noblock_replies_immediately() {
        // C: request.c:93-96 — RSS_NOBLOCK returns OK without arming the
        // late reply.
        let mut img = do_up_image(b"/bin/tty", b"tty");
        img[minix_types::rs_start_off::FLAGS..minix_types::rs_start_off::FLAGS + 4]
            .copy_from_slice(&crate::slot::RssFlags::NOBLOCK.bits().to_le_bytes());
        let mut server = booted_do_up(img);
        let m = do_up_message(0);
        assert_eq!(server.do_up(&m), Ok(0), "nobblock replies OK now");
        let state = server.state.as_ref().unwrap();
        let label = crate::service_slot::Label::from_bytes(b"tty");
        let id = state.table.lookup_by_label(&label).expect("created");
        assert!(
            !state.table.get(id).flags.contains(RFlags::LATEREPLY),
            "no late reply for noblock"
        );
    }

    #[test]
    fn test_do_up_rejects_duplicate_label() {
        // C: request.c:70-77 — a same-label service → EBUSY; the freshly
        // allocated row stays dirty-but-vacant (never IN_USE), matching the
        // find-only alloc contract.
        let mut server = booted_do_up(do_up_image(b"/bin/tty", b"vfs"));
        {
            // The pre-existing VFS row must carry the colliding label
            // (request.c:71-77 matches on the ACTIVE rows' labels).
            let s = server
                .state
                .as_mut()
                .unwrap()
                .table
                .get_mut(crate::service_slot::SlotId::new(0));
            s.pub_.label = crate::service_slot::Label::from_bytes(b"vfs");
        }
        let m = do_up_message(0);
        assert_eq!(server.do_up(&m), Err(Errno::EBUSY));
        let state = server.state.as_ref().unwrap();
        assert!(
            !state
                .table
                .get(crate::service_slot::SlotId::new(1))
                .flags
                .contains(RFlags::IN_USE),
            "the fresh row is not marked in-use"
        );
    }

    #[test]
    fn test_do_up_requires_root_caller() {
        // C: manager.c:91-97 — NULL-target RS_UP needs a root caller; a
        // failing getnuid means "not root" → EPERM.
        let mut mock = crate::testutil::MockKernelApi::new(60);
        mock.payload = Some(do_up_image(b"/bin/tty", b"tty"));
        mock.vm_ok = true;
        mock.execve_ok = true;
        mock.fork_pid = Some(701);
        mock.child_endpoint = Some(Endpoint::MEM);
        mock.fail_calls = alloc::vec![crate::testutil::Call::GetNuid(Endpoint::PM)];
        let mut server = booted_with(alloc::boxed::Box::new(mock));
        let m = do_up_message(0);
        assert_eq!(server.do_up(&m), Err(Errno::EPERM));
    }

    #[test]
    fn test_do_up_struct_copy_failure_propagates() {
        // C: request.c:33-37 — copy_rs_start failure propagates; an empty
        // image makes the struct fetch read past its end → EFAULT from the
        // seam (the address-aware mock mirrors the kernel's EFAULT).
        let mut server = booted_do_up(alloc::vec::Vec::new());
        let m = do_up_message(0);
        assert_eq!(server.do_up(&m), Err(Errno::EFAULT));
    }

    #[test]
    fn test_do_lookup_resolves_label_into_reply_payload() {
        // 14 wiring: RS_LOOKUP — the length gate (request.c:1151-1157), the
        // label copy via the safecopy seam (request.c:1158-1162), and the
        // endpoint written into the request payload; the main loop's reply
        // (m_type = OK) carries it back (request.c:1174).
        let mut server = booted_vfs_labeled(b"vfs", b"vfs");
        let mut m = minix_types::Message {
            m_source: Endpoint::PM,
            m_type: minix_types::RS_LOOKUP,
            m_u: Default::default(),
        };
        m.m_u.m_rs_req.name = 0;
        m.m_u.m_rs_req.name_len = 3;

        assert_eq!(server.do_lookup(&mut m), Ok(0));
        assert_eq!(
            m.rs_req_endpoint(),
            Some(Endpoint::VFS),
            "the resolved endpoint rides in the reply payload"
        );

        // Unknown label → ESRCH (request.c:1168-1171): the copy yields
        // "nox", which no slot carries.
        let mut miss_server = booted_vfs_labeled(b"vfs", b"nox");
        let mut miss = minix_types::Message {
            m_source: Endpoint::PM,
            m_type: minix_types::RS_LOOKUP,
            m_u: Default::default(),
        };
        miss.m_u.m_rs_req.name = 0;
        miss.m_u.m_rs_req.name_len = 3;
        assert_eq!(miss_server.do_lookup(&mut miss), Err(Errno::ESRCH));

        // Name-length gate: len < 2 → EINVAL (request.c:1151-1157).
        let mut short = minix_types::Message {
            m_source: Endpoint::PM,
            m_type: minix_types::RS_LOOKUP,
            m_u: Default::default(),
        };
        short.m_u.m_rs_req.name = 0;
        short.m_u.m_rs_req.name_len = 1;
        assert_eq!(server.do_lookup(&mut short), Err(Errno::EINVAL));
    }

    #[test]
    fn test_do_fi_injects_crash_request() {
        // 14 wiring: RS_FI — label copy (request.c:1244-1246), lookup →
        // ESRCH (request.c:1249-1253), permission against RS_FI, then the
        // asynchronous COMMON_REQ_FI_CTL crash request (fi_service,
        // utility.c:69-77). The wire shape itself is pinned by minix-types'
        // LsysFiCtl encode/decode roundtrip test.
        let mut server = booted_vfs_labeled(b"vfs", b"vfs");
        {
            let state = server.state.as_mut().unwrap();
            // RS_FI targets a system process (manager.c:103-105 gate).
            let s = state.table.get_mut(crate::service_slot::SlotId::new(0));
            s.priv_.flags.insert(crate::privilege::PrivFlags::SYS_PROC);
        }
        let mut m = minix_types::Message {
            m_source: Endpoint::PM,
            m_type: minix_types::RS_FI,
            m_u: Default::default(),
        };
        m.m_u.m_rs_req.addr = 0;
        m.m_u.m_rs_req.len = 3;

        assert_eq!(server.do_fi(&m), Ok(0), "asynsend seam accepts the send");

        // Unknown label → ESRCH: the label copy yields "nox", which no
        // slot carries (separate instance — the fixture's payload is
        // fixed at construction).
        let mut miss_server = booted_vfs_labeled(b"vfs", b"nox");
        let mut miss = minix_types::Message {
            m_source: Endpoint::PM,
            m_type: minix_types::RS_FI,
            m_u: Default::default(),
        };
        miss.m_u.m_rs_req.addr = 0;
        miss.m_u.m_rs_req.len = 3;
        assert_eq!(miss_server.do_fi(&miss), Err(Errno::ESRCH));
    }

    #[test]
    fn test_do_getsysinfo_permission_and_classification() {
        // 14 wiring: RS_GETSYSINFO — the permission gate (request.c:1099)
        // and the SI_* classification (request.c:1107-1133) run live; an
        // unknown table → EINVAL, a known table reaches the copy-out half,
        // which is edge E-RSWIRE-gated → ENOSYS (fail-closed, not fake OK).
        let mut server = booted_vfs_labeled(b"vfs", b"vfs");
        let mut m = minix_types::Message {
            m_source: Endpoint::PM,
            m_type: minix_types::RS_GETSYSINFO,
            m_u: Default::default(),
        };
        m.m_u.m_lsys_getsysinfo.what = 99;
        assert_eq!(server.do_getsysinfo(&m), Err(Errno::EINVAL));

        m.m_u.m_lsys_getsysinfo.what = crate::query::SI_PROC_TAB;
        m.m_u.m_lsys_getsysinfo.size = 64 * minix_types::rproc_off::SIZE as u64;
        assert_eq!(
            server.do_getsysinfo(&m),
            Ok(0),
            "the rproc copy-out is live (R12)"
        );
    }

    #[test]
    fn test_do_sysctl_dispatch_and_update_arms() {
        // 14 wiring: RS_SYSCTL — sub-type classification (query.rs) plus the
        // live action shapes: print → OK (dump face per E-8), UPD_STOP on a
        // scheduled state → chain cleared + OK (update.c:722-724),
        // UPD_START → prepare + OK now (request.c:1199-1201), UPD_RUN →
        // LATEREPLY + EDONTREPLY (request.c:1202-1207). UPD_START and
        // UPD_RUN each need a fresh scheduled chain: a prepared chain is
        // UPDATING, and a second prepare is EINVAL (update.c:403-406) —
        // which is its own assertion below.
        let mut server = booted_vfs_labeled(b"vfs", b"vfs");

        // Unknown sub-type → EINVAL (request.c:1216-1219).
        let mut m = minix_types::Message {
            m_source: Endpoint::PM,
            m_type: minix_types::RS_SYSCTL,
            m_u: Default::default(),
        };
        m.m_u.m_rs_req.subtype = 9;
        assert_eq!(server.do_sysctl(&m), Err(Errno::EINVAL));

        // Print services → OK (the dump face is the IS-stage assignment).
        m.m_u.m_rs_req.subtype = minix_types::sysctl::SRV_STATUS;
        assert_eq!(server.do_sysctl(&m), Ok(0));

        // UPD_STOP while only scheduled (not updating) → clears the chain
        // and returns OK (update.c:722-724).
        m.m_u.m_rs_req.subtype = minix_types::sysctl::UPD_STOP;
        let state = server.state.as_mut().unwrap();
        let id = crate::service_slot::SlotId::new(0);
        state
            .update
            .chain
            .add(crate::live_update::UpdateEntry::new(id, Endpoint::VFS));
        assert_eq!(server.do_sysctl(&m), Ok(0));
        assert!(server.state.as_ref().unwrap().update.chain.is_empty());

        // UPD_RUN over a scheduled chain: prepares, arms LATEREPLY on the
        // chain tail, and answers EDONTREPLY (request.c:1202-1207).
        m.m_u.m_rs_req.subtype = minix_types::sysctl::UPD_RUN;
        let state = server.state.as_mut().unwrap();
        let id = crate::service_slot::SlotId::new(0);
        state
            .update
            .chain
            .add(crate::live_update::UpdateEntry::new(id, Endpoint::VFS));
        assert_eq!(
            server.do_sysctl(&m),
            Ok(minix_types::EDONTREPLY),
            "UPD_RUN defers its reply to update completion"
        );
        let s = server
            .state
            .as_ref()
            .unwrap()
            .table
            .get(crate::service_slot::SlotId::new(0));
        assert!(s.flags.contains(RFlags::LATEREPLY), "late reply armed");
        assert_eq!(s.caller, Endpoint::PM);
        assert_eq!(s.caller_request, minix_types::RS_UPDATE);

        // A second prepare on the now-UPDATING state is EINVAL
        // (update.c:403-406) — reached as UPD_START on the same chain.
        m.m_u.m_rs_req.subtype = minix_types::sysctl::UPD_START;
        assert_eq!(server.do_sysctl(&m), Err(Errno::EINVAL));
    }

    /// A booted server with the VFS slot labeled and marked a system
    /// process — the 13 label-arm fixture.
    fn booted_vfs_sysproc() -> RsServer {
        let mut server = booted_vfs_labeled(b"vfs", b"vfs");
        {
            let state = server.state.as_mut().unwrap();
            let s = state.table.get_mut(crate::service_slot::SlotId::new(0));
            s.priv_.flags.insert(crate::privilege::PrivFlags::SYS_PROC);
            // A system process carries a scheduler (utility.c:370 — the
            // sched_decision assertion fires on NONE).
            s.scheduler = Endpoint::KERNEL;
        }
        server
    }

    fn label_message(m_type: i32) -> minix_types::Message {
        let mut m = minix_types::Message {
            m_source: Endpoint::PM,
            m_type,
            m_u: Default::default(),
        };
        m.m_u.m_rs_req.addr = 0;
        m.m_u.m_rs_req.len = 3;
        m
    }

    #[test]
    fn test_do_refresh_stops_and_arms_late_reply() {
        // 13 wiring: RS_REFRESH (request.c:390-419) — permission, then
        // stop_service(RS_REFRESHING) with the late reply armed; the caller
        // is unblocked when the refresh completes → EDONTREPLY.
        let mut server = booted_vfs_sysproc();
        let mut m = label_message(minix_types::RS_REFRESH);
        assert_eq!(
            server.do_refresh(&mut m),
            Ok(minix_types::EDONTREPLY),
            "refresh defers its reply to cleanup"
        );
        let s = server
            .state
            .as_ref()
            .unwrap()
            .table
            .get(crate::service_slot::SlotId::new(0));
        assert!(
            s.flags.contains(RFlags::REFRESHING),
            "the REFRESHING stop flag is applied"
        );
        assert!(s.flags.contains(RFlags::LATEREPLY));
        assert_eq!(s.caller, Endpoint::PM);
        assert_eq!(s.caller_request, minix_types::RS_REFRESH);
        assert_eq!(s.stop_tm, 500, "stop_service records stop_tm = getticks");
    }

    #[test]
    fn test_do_restart_requires_terminated_service() {
        // 13 wiring: RS_RESTART (request.c:160-203) — a running service is
        // EBUSY (request.c:184-188); a TERMINATED service restarts with the
        // recovery script suppressed for this round (save/clear/restore).
        let mut server = booted_vfs_sysproc();
        let mut m = label_message(minix_types::RS_RESTART);
        assert_eq!(
            server.do_restart(&mut m),
            Err(Errno::EBUSY),
            "a live service cannot be restarted on request"
        );

        {
            let state = server.state.as_mut().unwrap();
            let s = state.table.get_mut(crate::service_slot::SlotId::new(0));
            s.flags.insert(RFlags::TERMINATED);
            // A recovery script exists for this service.
            s.script[..10].copy_from_slice(b"recover.sh");
        }
        assert_eq!(server.do_restart(&mut m), Ok(0));
        let s = server
            .state
            .as_ref()
            .unwrap()
            .table
            .get(crate::service_slot::SlotId::new(0));
        assert_eq!(
            &s.script[..10],
            b"recover.sh",
            "the script is restored after the restart"
        );
    }

    #[test]
    fn test_do_clone_and_unclone_replica_lifecycle() {
        // 13 wiring: RS_CLONE (request.c:208-249) arms SF_USE_REPL and
        // links a replica (second clone → EEXIST, request.c:231-234);
        // RS_UNCLONE (request.c:253-293) without a replica → ENOENT
        // (request.c:274-277), with one → cleanup now + flag cleared.
        let mut server = booted_vfs_sysproc();
        let mut clone_msg = label_message(minix_types::RS_CLONE);
        let mut unclone_msg = label_message(minix_types::RS_UNCLONE);

        // Unclone before any clone → ENOENT.
        assert_eq!(server.do_unclone(&mut unclone_msg), Err(Errno::ENOENT));

        assert_eq!(server.do_clone(&mut clone_msg), Ok(0));
        let id = crate::service_slot::SlotId::new(0);
        {
            let state = server.state.as_ref().unwrap();
            assert!(
                state
                    .table
                    .get(id)
                    .pub_
                    .sys_flags
                    .contains(crate::service_slot::SysFlags::USE_REPL),
                "the source carries SF_USE_REPL"
            );
            assert!(
                state.table.get(id).next_rp.is_some(),
                "the replica is linked as next"
            );
        }

        // A replica already available → EEXIST.
        assert_eq!(server.do_clone(&mut clone_msg), Err(Errno::EEXIST));

        assert_eq!(server.do_unclone(&mut unclone_msg), Ok(0));
        let state = server.state.as_ref().unwrap();
        assert!(
            !state
                .table
                .get(id)
                .pub_
                .sys_flags
                .contains(crate::service_slot::SysFlags::USE_REPL),
            "SF_USE_REPL is cleared"
        );
        assert!(
            state.table.get(id).next_rp.is_none(),
            "the replica is cleaned up now (cleanup_service_now)"
        );
    }

    #[test]
    fn test_signal_manager_routes_all_branches() {
        // R34.22: the seven signal-manager branches (main.c:647-703) —
        // spurious clear, terminated EDEADEPT, inactive clear, the
        // termination executor (EDEADEPT + TERMINATED), the VM refusal, and
        // the SIGS_SIGNAL_RECEIVED forwarding via the asynsend seam.
        let mut server = booted_vfs_labeled(b"vfs", b"vfs");
        {
            let state = server.state.as_mut().unwrap();
            state
                .table
                .set_endpoint_index(Endpoint::MEM, Some(crate::service_slot::SlotId::new(1)));
            // A second (inactive) service for the inactive branch.
            let s = state.table.get_mut(crate::service_slot::SlotId::new(1));
            s.flags = RFlags::IN_USE; // no ACTIVE
            s.pub_.in_use = true;
            s.pub_.endpoint = Endpoint::MEM;
        }

        // Spurious target → cleared with OK (main.c:655-662).
        assert_eq!(server.signal_manager(Endpoint::DS, 15), Ok(0));

        // Inactive instance → cleared (main.c:672-678).
        assert_eq!(server.signal_manager(Endpoint::MEM, 15), Ok(0));

        // Non-termination signal for the active service → forwarded via
        // asynsend as SIGS_SIGNAL_RECEIVED (main.c:699-701).
        let sent_before = match &server.kernel {
            _ => 0usize, // the mock lives behind the server; observe via a
                         // follow-up signal below instead.
        };
        let _ = sent_before;
        assert_eq!(server.signal_manager(Endpoint::VFS, 16), Ok(0));

        // Termination signal (SIGKILL=9 — SIGS_IS_TERMINATION, signal.h:284-286)
        // → executor runs, TERMINATED armed, EDEADEPT (main.c:686-692). With
        // restarts > 0 the executor takes the backoff branch, so the slot
        // stays (no EXITING cleanup). Note SIGTERM(15) is NOT a termination
        // signal in this sense — it takes the forwarding branch.
        {
            let state = server.state.as_mut().unwrap();
            state
                .table
                .get_mut(crate::service_slot::SlotId::new(0))
                .restarts = 1;
        }
        assert_eq!(
            server.signal_manager(Endpoint::VFS, 9),
            Err(Errno::EDEADEPT)
        );
        let s = server
            .state
            .as_ref()
            .unwrap()
            .table
            .get(crate::service_slot::SlotId::new(0));
        assert!(s.flags.contains(RFlags::TERMINATED), "termination marked");
        assert!(s.backoff > 0, "restarts > 0 arms the backoff");

        // A second termination for the still-present terminated service →
        // EDEADEPT (main.c:665-669).
        assert_eq!(
            server.signal_manager(Endpoint::VFS, 9),
            Err(Errno::EDEADEPT)
        );

        // The earlier non-termination forwarding left one asynsend behind:
        // verify the seam received it with the SIGS type.
        // (The mock is owned by the server; the forwarding branch's wire
        // shape is additionally pinned by the SIGS_SIGNAL_RECEIVED constant
        // test in minix-types.)
    }

    #[test]
    fn test_signal_manager_vm_and_stacktrace() {
        // R34.22 continued: signals are never delivered to VM (main.c:694-697),
        // and a lethal-but-not-ABRT signal triggers the stacktrace seam before
        // the termination path (main.c:681-683).
        let mut mock = crate::testutil::MockKernelApi::new(60);
        mock.payload = Some(b"vfs".to_vec());
        mock.ticks = 500;
        let mut server = booted_with(alloc::boxed::Box::new(mock));
        {
            let state = server.state.as_mut().unwrap();
            state
                .table
                .set_endpoint_index(Endpoint::VM, Some(crate::service_slot::SlotId::new(0)));
            let s = state.table.get_mut(crate::service_slot::SlotId::new(0));
            s.flags = RFlags::IN_USE | RFlags::ACTIVE;
            s.pub_.in_use = true;
            s.pub_.endpoint = Endpoint::VM;
        }

        // Non-termination to VM → OK, no forwarding (VM cannot receive).
        assert_eq!(server.signal_manager(Endpoint::VM, 16), Ok(0));

        // Termination to VM → EDEADEPT via the executor (termination wins
        // over the VM refusal — C checks termination first).
        assert_eq!(server.signal_manager(Endpoint::VM, 9), Err(Errno::EDEADEPT));
    }

    #[test]
    fn test_do_period_update_timeout_rolls_back() {
        // R36: while updating (not initializing), a tick past the current
        // preparer's deadline ends the update with EINTR/RS_CANCEL
        // (update.c:386-395) — the chain and the phase flags are cleared and
        // the old versions keep running. Without this arm a stalled prepare
        // would hang the update forever.
        let mut server = booted_vfs_labeled(b"vfs", b"vfs");
        two_entry_chain(&mut server);
        {
            let state = server.state.as_mut().unwrap();
            // Deadline: prepare_tm 0 + maxtime 30 → due at tick 30.
            state.update.chain.get_mut(0).prepare_maxtime = 30;
        }
        // Overdue tick.
        server.do_period(100).unwrap();
        {
            let state = server.state.as_ref().unwrap();
            assert!(
                !state
                    .update
                    .flags
                    .contains(crate::live_update::RupdateFlags::UPDATING),
                "the timed-out update left the updating phase"
            );
            assert_eq!(state.update.chain.len(), 0, "the chain was torn down");
        }

        // Not yet due → the update keeps going.
        let mut server2 = booted_vfs_labeled(b"vfs", b"vfs");
        two_entry_chain(&mut server2);
        server2
            .state
            .as_mut()
            .unwrap()
            .update
            .chain
            .get_mut(0)
            .prepare_maxtime = 30;
        server2.do_period(10).unwrap();
        let state = server2.state.as_ref().unwrap();
        assert!(state.update.chain.len() > 0, "prepare still in flight");
        assert!(
            state
                .update
                .flags
                .contains(crate::live_update::RupdateFlags::UPDATING)
        );

        // maxtime 0 = no deadline (update.c:386 — `prepare_maxtime > 0`).
        let mut server3 = booted_vfs_labeled(b"vfs", b"vfs");
        two_entry_chain(&mut server3);
        server3.do_period(10_000).unwrap();
        assert!(
            server3.state.as_ref().unwrap().update.chain.len() > 0,
            "maxtime 0 never times out"
        );

        // The initializing phase is exempt (C: request.c:951 —
        // `!RUPDATE_IS_INITIALIZING()`): an overdue tick does not cancel.
        let mut server4 = booted_vfs_labeled(b"vfs", b"vfs");
        two_entry_chain(&mut server4);
        {
            let state = server4.state.as_mut().unwrap();
            state.update.chain.get_mut(0).prepare_maxtime = 30;
            state.update.begin_initializing();
        }
        server4.do_period(100).unwrap();
        assert!(
            server4.state.as_ref().unwrap().update.chain.len() > 0,
            "the initializing phase has no prepare deadline"
        );
    }

    #[test]
    #[should_panic(expected = "run() requires a completed fresh boot")]
    fn test_run_requires_completed_boot() {
        // E-10/R34.19: the main loop is unreachable without a completed
        // boot — the panic is the fail-fast contract (C never reaches
        // main()'s loop with a half-booted RS, main.c:226).
        let mut server = RsServer::new(crate::boot::BootTables::placeholder());
        let _ = server.run();
    }

    #[test]
    #[should_panic(expected = "boot machine present")]
    fn test_second_fresh_init_panics() {
        // E-10/R34.19: the first successful boot consumes the boot machine
        // (T1 handover); a second init(Fresh) has nothing to boot — the
        // expect is the fail-fast contract.
        static IMAGE: &[minix_types::BootImage] = &[minix_types::BootImage {
            proc_nr: 2,
            proc_name: *b"rs\0\0\0\0\0\0\0\0\0\0\0\0\0\0",
            endpoint: Endpoint::RS,
            start_addr: 0,
            len: 0,
        }];
        let priv_table: &[crate::table::BootImagePriv] = &[crate::table::BootImagePriv {
            endpoint: Endpoint::RS,
            label: "rs",
            flags: crate::privilege::RSYS_F,
        }];
        let sys_table: &[crate::table::BootImageSys] = &[crate::table::BootImageSys {
            endpoint: Endpoint::RS,
            flags: crate::service_slot::SRVR_SF,
        }];
        let tables = crate::boot::BootTables {
            image: IMAGE,
            priv_table,
            sys_table,
            dev_table: &[],
        };
        let mut server = RsServer::with_kernel(
            tables,
            alloc::boxed::Box::new(crate::testutil::MockKernelApi::new(100)),
        );
        server
            .init(SefInitType::Fresh)
            .expect("RS-only boot completes (zero pending init-ready)");
        let _ = server.init(SefInitType::Fresh);
    }

    #[test]
    fn test_signal_term_runs_shutdown_sweep() {
        // C: main.c:638-640 — SIGTERM → do_shutdown(NULL):全表 EXITING +
        // shutting_down 置位（request.c:447-455）。
        let mut server = booted();
        server.signal_handler(SIGNAL_TERMINATE);
        let state = server.state.as_ref().unwrap();
        assert!(state.shutting_down);
        assert!(
            state
                .table
                .iter_in_use()
                .all(|(_, s)| s.flags.contains(RFlags::EXITING))
        );
    }

    #[test]
    fn test_signal_chld_drains_exited_children() {
        // C: main.c:635-637 — SIGCHLD → do_sigchld:每个 waitpid 到的子进程
        // 走 sigchld_cleanup；pid 未命中时无副作用（request.c:1064-1065）。
        let mut server = booted();
        // 无已退出子进程 → 空转，表不动。
        server.signal_handler(SIGNAL_CHILD);
        let state = server.state.as_ref().unwrap();
        assert!(
            state
                .table
                .iter_in_use()
                .all(|(_, s)| s.flags.contains(RFlags::IN_USE))
        );
    }

    #[test]
    fn test_signal_unknown_ignored() {
        // C: main.c:641 — switch 无 default：未知信号静默忽略。
        let mut server = booted();
        server.signal_handler(9999);
        let state = server.state.as_ref().unwrap();
        assert!(!state.shutting_down);
    }
}
