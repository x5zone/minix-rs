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
    /// is fail-closed (`UnimplementedKernelApi`) until the `minix-sys`
    /// wiring lands (19).
    pub fn new(tables: BootTables<'static>) -> Self {
        Self::with_kernel(tables, alloc::boxed::Box::new(boot::UnimplementedKernelApi))
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
            let (msg, rcv_sts, ts) = self.get_work()?;
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
                    self.reply_unless_suppressed(who_e, result);
                }
                DispatchKind::LuPrepareReady => {
                    let result = self.lu_response(&msg).unwrap_or_else(|e| e.to_i32());
                    self.reply_unless_suppressed(who_e, result);
                }
                // C: main.c:102-114 + 124-129 — handler result replied to the
                // caller unless EDONTREPLY. [`RsServer::do_request`] owns the
                // arm table: wire-decode-free arms run live, the rest fail
                // closed until their 19 decode lands (OQ-4).
                DispatchKind::Request(n) => {
                    let result = self
                        .do_request(who_e, n, &msg)
                        .unwrap_or_else(|e| e.to_i32());
                    self.reply_unless_suppressed(who_e, result);
                }
            }
        }
    }

    /// Replies unless the result suppresses it (C: main.c:124-129). The
    /// reply itself is fire-and-forget — C's `reply` (utility.c:309-318)
    /// does not propagate `ipc_send` failures to the loop.
    fn reply_unless_suppressed(&mut self, who_e: Endpoint, result: i32) {
        if result != minix_types::EDONTREPLY {
            let _ = self.kernel.reply(who_e, result);
        }
    }

    /// Dispatches one `RS_*` request (C: main.c:102-114 switch).
    ///
    /// Arms turn live in dependency order as their message-payload decode
    /// lands (the union-arm reads are the 19 safe-receive seam — OQ-4):
    /// `RS_SHUTDOWN` needs only `m_source` and is live; everything else
    /// falls through to [`dispatch::dispatch_request`]'s fail-closed table.
    fn do_request(
        &mut self,
        caller: Endpoint,
        call_nr: i32,
        msg: &minix_types::Message,
    ) -> Result<i32, Errno> {
        match call_nr {
            minix_types::RS_SHUTDOWN => self.do_shutdown(caller),
            minix_types::RS_DOWN => self.do_down(msg),
            n => Ok(dispatch::dispatch_request(n).0),
        }
    }

    /// C: `do_down` — request.c:110-146: decode the target label
    /// (`copy_label` — a 16-byte payload, no structure ABI), resolve the
    /// slot, check permission, then either clean up an already-terminated
    /// service or run the stop flow. The reply is deferred until the service
    /// dies (`RS_LATEREPLY` + late_reply via the sigchld/cleanup path), so
    /// the handler always answers `EDONTREPLY`.
    fn do_down(&mut self, m: &minix_types::Message) -> Result<i32, Errno> {
        let state = self.state.as_mut().ok_or(Errno::ENOSYS)?;
        // C: request.c:121-123 — copy_label(source, m_rs_req.addr, len).
        let Some((addr, len)) = m.rs_req_payload() else {
            return Err(Errno::EINVAL);
        };
        let mut label_buf = [0u8; crate::service_slot::RS_MAX_LABEL_LEN];
        let n = (len as usize).min(label_buf.len() - 1);
        self.kernel
            .safecopy_from(m.m_source, addr as usize, &mut label_buf[..n])?;
        label_buf[n] = 0;
        let label = crate::service_slot::Label::from_bytes(&label_buf[..n]);

        // C: request.c:126-134 — lookup + permission.
        let Some(id) = state.table.lookup_by_label(&label) else {
            return Err(Errno::ESRCH);
        };
        let updating = state
            .table
            .get(id)
            .flags
            .contains(crate::service_slot::RFlags::UPDATING);
        let caller_euid = self.kernel.getnuid(m.m_source);
        crate::access::check_call_permission(
            m.m_source,
            minix_types::RS_DOWN,
            Some(state.table.get(id)),
            &state.table,
            updating,
            caller_euid,
        )?;

        let ticks = self.kernel.get_ticks().unwrap_or(0);
        if state
            .table
            .get(id)
            .flags
            .contains(crate::service_slot::RFlags::TERMINATED)
        {
            // C: request.c:136-141 — a recovery script is bringing down an
            // already-gone service: unpublish + cleanup, reply OK now.
            // C: unpublish_service(rp) — manager.c:864-920 (the DS effect
            // seam is 19; the aggregate decision face is publish.rs, R32).
            let _ = crate::publish::unpublish_result(false, false, false, false);
            crate::recovery::cleanup_service(
                &mut state.table,
                id,
                self.kernel.as_mut(),
                &mut |_| Ok(()),
            );
            return Ok(0);
        }
        // C: request.c:142-145 — stop_service(rp, RS_EXITING) + late reply.
        let decision = crate::request::stop_decision(
            state.table.get(id),
            crate::service_slot::RFlags::EXITING,
            ticks,
        );
        decision.mutations.apply(state.table.get_mut(id));
        state
            .table
            .get_mut(id)
            .flags
            .insert(crate::service_slot::RFlags::LATEREPLY);
        state.table.get_mut(id).caller = m.m_source;
        state.table.get_mut(id).caller_request = minix_types::RS_DOWN;
        match decision.signal {
            crate::request::StopSignal::Hangup => {
                // RS itself (manager.c:1003) — SIGHUP via the PM face.
                let _ = self
                    .kernel
                    .srv_kill(state.table.get(id).pid.unwrap_or(0), 1);
            }
            crate::request::StopSignal::Term => {
                let _ = self
                    .kernel
                    .srv_kill(state.table.get(id).pid.unwrap_or(0), 15);
            }
        }
        Ok(minix_types::EDONTREPLY)
    }

    /// C: `do_shutdown` — request.c:431-455: caller permission, then the
    /// no-restart sweep (`shutting_down` + `RS_EXITING` over the table —
    /// [`request::shutdown_apply`]). The NULL-message *internal* form
    /// (request.c:436 `m_ptr != NULL` gate) is the SIGTERM arm of
    /// `signal_handler` (Fix #57); the message form checks the caller here.
    fn do_shutdown(&mut self, caller: Endpoint) -> Result<i32, Errno> {
        let state = self.state.as_mut().ok_or(Errno::ENOSYS)?;
        let updating = state
            .update
            .flags
            .contains(live_update::RupdateFlags::UPDATING);
        // C: request.c:435-437 — check_call_permission(source, RS_SHUTDOWN,
        // NULL); the euid query is the T5 shell injection (04).
        let caller_euid = self.kernel.getnuid(caller);
        crate::access::check_call_permission(
            caller,
            minix_types::RS_SHUTDOWN,
            None,
            &state.table,
            updating,
            caller_euid,
        )?;
        state.shutting_down = crate::request::shutdown_apply(&mut state.table);
        Ok(0) // C: request.c:454 — return(OK)
    }

    /// C: `do_init_ready` — request.c:462-529 (the `RS_INIT` handler, 12).
    /// The decoded `result` selects the branch; every path ends `EDONTREPLY`
    /// — the service itself is unblocked by the handler's internal reply
    /// (request.c:520-522) or killed (request.c:492).
    fn do_init_ready(&mut self, m: &minix_types::Message) -> Result<i32, Errno> {
        let state = self.state.as_mut().ok_or(Errno::ENOSYS)?;
        // C: request.c:474-475 — `rp = rproc_ptr[who_p]` (registered source).
        let Some(id) = state.table.endpoint_slot(m.m_source) else {
            return Err(Errno::EINVAL);
        };
        // C: request.c:473 — `result = m_ptr->m_rs_init.result`. The typed
        // accessor is total; classify guarantees the RS_INIT arm, so `None`
        // is a program error shaped as EINVAL.
        let Some(result) = m.rs_init_result() else {
            return Err(Errno::EINVAL);
        };
        let updating = state
            .table
            .get(id)
            .flags
            .contains(crate::service_slot::RFlags::UPDATING);
        let decision = crate::ready::do_init_ready(
            state.table.get(id).flags,
            result,
            updating,
            state.update.num_init_ready_pending,
            self.kernel.get_ticks().unwrap_or(0),
        );
        decision.mutations.apply(state.table.get_mut(id));
        match decision.outcome {
            // C: request.c:477-483 — not initializing → EINVAL.
            crate::ready::ReadyOutcome::Unexpected => Err(Errno::EINVAL),
            crate::ready::ReadyOutcome::InitFailed { .. } => {
                // C: request.c:488-497 — crash the service (the REINCARNATE/
                // init_err mutations already applied); RS's own crash ends
                // the loop (C `exit(1)`, manager.c:395-397).
                let outcome =
                    crate::recovery::crash_service(state.table.get(id), self.kernel.as_mut())?;
                if outcome == crate::recovery::CrashOutcome::SelfTerminate {
                    return Err(Errno::EGENERIC);
                }
                Ok(minix_types::EDONTREPLY)
            }
            crate::ready::ReadyOutcome::UpdateInitDone { pending_remaining } => {
                state.update.num_init_ready_pending = pending_remaining;
                if pending_remaining == 0 {
                    // C: request.c:511-514 — end_update(OK, RS_REPLY).
                    let ticks = self.kernel.get_ticks().unwrap_or(0);
                    state.update.end_update(
                        &mut state.table,
                        self.kernel.as_mut(),
                        0, // OK
                        1, // RS_REPLY
                        ticks,
                        &mut |_s, _ps| {},
                        &mut |_s| Ok(()),
                    );
                }
                Ok(minix_types::EDONTREPLY)
            }
            crate::ready::ReadyOutcome::FreshInitDone => {
                // C: request.c:517-524 — unblock the service, then finalize.
                let _ = self.kernel.reply(m.m_source, 0);
                let has_prev = state.table.get(id).prev_rp.is_some();
                crate::ready::end_srv_init(state.table.get_mut(id), has_prev);
                Ok(minix_types::EDONTREPLY)
            }
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

    /// C: `do_period` — request.c:946-1040 (07): the CLOCK-tick status sweep
    /// over the service table. Per in-use slot passing the ACTIVE/update gate
    /// (request.c:968-970): the decision layer ([`monitor::period_decision`])
    /// classifies backoff tick / stop timeout / ping timeout / ping request /
    /// free pass; the mutations apply once (R13) and the action executes:
    /// `Restart` → [`service_create::restart_service`] (request.c:977-978),
    /// the crash actions → [`recovery::crash_service`] (request.c:989/:1029),
    /// `PingRequest` → the notify seam (request.c:1035).
    fn do_period(&mut self, now: Clock) -> Result<(), Errno> {
        let state = self.state.as_mut().ok_or(Errno::ENOSYS)?;
        // 16 号: `RUPDATE_IS_UPDATING() && !RUPDATE_IS_INITIALIZING()` routes
        // the tick into `update_period` first (request.c:952-954) — the LU
        // mid-state checker; deferred with the 16 wiring.
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
                    let mut noop_publish = |_: &RProcTable, _: crate::service_slot::SlotId| Ok(());
                    let mut noop_asynsend = |_: Endpoint, _: &crate::ready::InitMessage| Ok(());
                    service_create::restart_service(
                        &mut state.table,
                        id,
                        self.kernel.as_mut(),
                        now,
                        &mut noop_exec,
                        &mut noop_script,
                        &mut noop_publish,
                        &mut noop_asynsend,
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
                    let ticks = kernel.get_ticks().unwrap_or(0);
                    state.update.end_update(
                        &mut state.table,
                        kernel,
                        minix_types::ERESTART,
                        1,
                        ticks,
                        &mut |_s, _ps| {},
                        &mut |_s| Ok(()),
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
                let ticks = kernel.get_ticks().unwrap_or(0);
                crate::service_create::init_service(
                    state.table.get_mut(new_rs),
                    None,
                    crate::sef::SefInitType::Restart,
                    0,
                    None,
                    crate::live_update::SEF_LU_STATE_NULL,
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
        let ticks = kernel.get_ticks().unwrap_or(0);
        crate::service_create::init_service(
            state.table.get_mut(new_rs),
            None,
            crate::sef::SefInitType::Lu,
            0,
            None,
            crate::live_update::SEF_LU_STATE_NULL,
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

    /// C: `sef_cb_lu_response` — main.c:614-626. The decode half landed with
    /// `rs_init_result`'s sibling pattern (minix-types); the shell still
    /// needs the LU chain context (`do_upd_ready`'s gate + the
    /// complete/rollback orchestration consumers, 16 号) — fail-closed until
    /// that wiring lands.
    fn lu_response(&mut self, _m: &minix_types::Message) -> Result<i32, Errno> {
        Err(Errno::ENOSYS)
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

    /// C: `sef_cb_signal_manager` — main.c:149. DEFERRED until 06 lands;
    /// fail closed, no panic (06-rs-main-loop.md). Signature mirrors
    /// sef.h:270 `(endpoint_t target, int signo)` (R26).
    fn signal_manager(&mut self, _target: Endpoint, _signo: i32) -> Result<i32, Errno> {
        Err(Errno::ENOSYS)
    }
}

#[cfg(test)]
mod signal_handler_tests {
    use super::*;
    use crate::process_table::RProcTable;
    use crate::service_slot::RFlags;
    use minix_types::{Endpoint, SIGNAL_CHILD, SIGNAL_TERMINATE};

    /// A server with a completed boot: table + one in-use VFS service with
    /// pid 700, plus an exited child (pid 700's own child bookkeeping is the
    /// sigchld target).
    fn booted() -> RsServer {
        booted_with(alloc::boxed::Box::new(crate::boot::UnimplementedKernelApi))
    }

    /// Same fixture with an injectable kernel seam (E-10: the waitpid drain
    /// and the second-init panic need a mock / a consumed boot machine).
    fn booted_with(kernel: alloc::boxed::Box<dyn KernelApi>) -> RsServer {
        let mut server = RsServer::with_kernel(crate::boot::BootTables::placeholder(), kernel);
        let mut table = RProcTable::new();
        let id = table.alloc_slot().unwrap();
        {
            let s = table.get_mut(id);
            s.flags = RFlags::IN_USE | RFlags::ACTIVE;
            s.pub_.endpoint = Endpoint::VFS;
            s.pid = Some(700);
        }
        server.state = Some(crate::ServerState {
            tables: crate::boot::BootTables::placeholder(),
            machine: crate::boot::Machine::default(),
            rinit: crate::boot::RinitState::default(),
            table,
            shutting_down: false,
            system_hz: 60,
            nr_uncaught_init_srvs: 0,
            update: crate::live_update::UpdateState::default(),
        });
        server
    }

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

    /// An RS_INIT envelope from the VFS service with the given result.
    /// Union-field *writes* are safe (bit stores); the tagged *read* goes
    /// through `Message::rs_init_result` (minix-types, E-12 decode).
    fn rs_init_envelope(result: i32) -> minix_types::Message {
        let mut m = minix_types::Message {
            m_source: Endpoint::VFS,
            m_type: minix_types::RS_INIT,
            m_u: Default::default(),
        };
        m.m_u.m_rs_init.result = result;
        m
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
    fn test_do_init_ready_failure_crashes_and_records_init_err() {
        // 12 wiring: request.c:488-497 — a failed init crashes the service
        // and records `r_init_err`; the reply is suppressed (EDONTREPLY).
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
            s.pid = Some(700);
        }
        let r = server.do_init_ready(&rs_init_envelope(7)).unwrap();
        assert_eq!(r, minix_types::EDONTREPLY);
        let s = server
            .state
            .as_ref()
            .unwrap()
            .table
            .get(crate::service_slot::SlotId::new(0));
        assert_eq!(s.init_err, 7, "r_init_err records the failure");
    }

    #[test]
    fn test_do_init_ready_unexpected_is_einval() {
        // 12 wiring: request.c:477-483 — an init-ready from a slot that was
        // never asked to initialize → EINVAL.
        let mut server = booted_with(alloc::boxed::Box::new(crate::testutil::MockKernelApi::new(
            60,
        )));
        let r = server.do_init_ready(&rs_init_envelope(0)).unwrap_err();
        assert_eq!(r, Errno::EINVAL);
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
        m.m_u.m_rs_req.addr = 0x4000;
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
