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

use minix_types::{Endpoint, Errno};

pub mod access;
pub mod boot;
pub mod dispatch;
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
pub use boot::{BootInit, BootTables, KernelApi, Machine};
pub use exec::{free_exec, has_shared_exec, share_exec, validate_image};
pub use ipc_mask::{IpcListIterator, add_backward_ipc, add_forward_ipc, init_privs};
pub use live_update::{
    AbortAction, EndUpdateRole, LuFlags, RS_CANCEL, RS_REPLY, SEF_INIT_ST, SEF_LU_STATE_NULL,
    SEF_LU_STATE_UNREACHABLE, UpdateChain, UpdateEntry, UpdatePhase, abort_action,
    end_srv_reply_flag, end_update_role, lu_flags_from_rss, resolve_prepare_maxtime, update_phase,
    validate_update_request, vm_default_prealloc,
};
pub use monitor::{
    PeriodAction, PeriodDecision, delta_t, effective_period, has_update_timed_out, init_timeout,
    period_decision, sigchld_cleanup,
};
pub use privilege::{DSRV_I, PrivCtlOp, Privilege, TrapMask};
pub use process_table::{RProcTable, RupdateFlags, ServiceInstances};
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
    StopSignal, check_duplicates, mark_late_reply, shutdown_apply, stop_service, up_init_flags,
};
pub use sef::{SefCallbacks, SefInitInfo, SefInitType};
pub use self_lifecycle::{
    SelfUpgradeRole, SigMgrUpdate, SrvUpdateAction, SwapFlag, is_rs_restart_replica,
    lu_init_invariants, rollback_needs_vm_update, rollback_swap_flag, self_update_sig_mgr_update,
    self_upgrade_role, should_end_update_on_restart, should_pre_swap, sig_mgr_updates,
    srv_update_action,
};
pub use service_create::{
    activate_service, check_create_preconditions, clone_slot, link_replica, mark_child_created,
    rebuild_args, swap_index, swap_slot,
};
pub use service_slot::{
    ARGV_ELEMENTS, IMM_SF, Label, MAX_COMMAND_LEN, MAX_IPC_LIST, MAX_NR_ARGS, MAX_SCRIPT_LEN,
    NR_DOMAIN, NR_IO_RANGE, NR_IRQ, NR_MEM_RANGE, PublicSlot, RFlags, RS_MAX_LABEL_LEN,
    RS_NR_CONTROL, SRV_SF, SRVR_SF, ServiceSlot, SlotId, SlotMutations, SysFlags, VM_SF,
};
pub use slot::{RsStart, RssFlags, build_cmd_dep, check_request};
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
        }
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
    /// C: `main()` loop — main.c:50-131. Skeleton: message receive
    /// (`get_work`, 06) + classification ([`dispatch::classify`]) + request
    /// dispatch. The receive primitive is DEFERRED (06-rs-main-loop.md); the
    /// loop fails closed until then.
    pub fn run(&mut self) -> ! {
        // T1: the main loop operates on the post-boot runtime state. Fail
        // closed (loudly) if boot never completed — an RS that has not
        // finished booting cannot manage services.
        if self.state.is_none() {
            panic!("run() requires a completed fresh boot (init(Fresh))");
        }
        loop {
            // C: rs_idle_period() — main.c:59 (06).
            // C: get_work() → sef_receive_status(ANY) — main.c:62, 826-833 (06).
            let (msg, rcv_sts) = self.get_work();
            // R25: classify takes the notify timestamp (ipc.h:1715). The
            // value lives in the `MessageUnion` — reading it requires
            // `unsafe`, which this crate never uses — so extraction belongs
            // to the safe receive wrapper (06/19); `0` here is unreachable
            // until that lands (get_work fails closed above).
            let _kind = dispatch::classify(&rcv_sts, msg.m_source, msg.m_type, 0);
            // C: message dispatch — main.c:70-127 (mechanisms in 06/07/12-16).
            // 06 wiring: `do_period` reads `state.system_hz`/`state.table`;
            // the RS_DOWN sweep reads/writes `state.shutting_down`.
            let _ = (self.state.as_ref(), _kind);
        }
    }

    /// C: `get_work()` — main.c:826-833. The receive primitive is DEFERRED
    /// (06-rs-main-loop.md); `minix-sys::receive` is a stub and fails closed
    /// until then.
    fn get_work(&mut self) -> (minix_types::Message, dispatch::IpcStatus) {
        let mut msg = minix_types::Message::default();
        // C: sef_receive_status(ANY, &msg, &ipc_status) — main.c:826-833.
        // T4: there is no real ipc_status until receive lands — fabricating
        // `flags: 0` here would misclassify future notify messages (notify
        // bits set) as plain requests once the primitive exists. Fail closed
        // loudly instead; the status word arrives with the receive
        // implementation (06-rs-main-loop.md).
        let _ = minix_sys::receive(minix_types::Endpoint::ANY, &mut msg);
        todo!("get_work: receive primitive DEFERRED (06-rs-main-loop.md)")
    }
}

impl SefCallbacks for RsServer {
    /// C: `sef_cb_init_fresh` — main.c:158-494. The fresh init IS the 4-step
    /// boot; the boot state is handed over to [`RsServer::state`] (T1).
    fn init_fresh(&mut self, _init_type: SefInitType, _info: &SefInitInfo) -> Result<i32, Errno> {
        let boot = self.boot.as_mut().expect("boot machine present");
        boot.init_fresh(self.kernel.as_mut())?;
        // T1 handover: the boot state becomes the runtime state.
        self.state = Some(self.boot.take().expect("boot machine present").into_state());
        Ok(0) // C: sef_startup() returns OK after the fresh init.
    }

    /// C: `sef_cb_init_restart` — main.c:140. DEFERRED until 18 lands; fail
    /// closed (18-rs-self-lifecycle.md).
    fn init_restart(&mut self, _init_type: SefInitType, _info: &SefInitInfo) -> Result<i32, Errno> {
        Err(Errno::ENOSYS)
    }

    /// C: `sef_cb_init_lu` — main.c:141. DEFERRED until 18 lands; fail
    /// closed (18-rs-self-lifecycle.md).
    fn init_lu(&mut self, _init_type: SefInitType, _info: &SefInitInfo) -> Result<i32, Errno> {
        Err(Errno::ENOSYS)
    }

    /// C: `sef_cb_init_response` — main.c:144. DEFERRED until 12 lands; fail
    /// closed (12-rs-init-run.md).
    fn init_response(&mut self, _m: &minix_types::Message) -> Result<i32, Errno> {
        Err(Errno::ENOSYS)
    }

    /// C: `sef_cb_lu_response` — main.c:145. DEFERRED until 12 lands; fail
    /// closed (12-rs-init-run.md).
    fn lu_response(&mut self, _m: &minix_types::Message) -> Result<i32, Errno> {
        Err(Errno::ENOSYS)
    }

    /// C: `sef_cb_signal_handler` — main.c:148. Unreachable until the main
    /// loop lands (06); no Result channel to fail closed through, so keep the
    /// loud marker (06-rs-main-loop.md, T7 gate).
    fn signal_handler(&mut self, _signo: i32) {
        unimplemented!("sef_cb_signal_handler body: 06-rs-main-loop.md (unreachable until 06)")
    }

    /// C: `sef_cb_signal_manager` — main.c:149. DEFERRED until 06 lands;
    /// fail closed, no panic (06-rs-main-loop.md). Signature mirrors
    /// sef.h:270 `(endpoint_t target, int signo)` (R26).
    fn signal_manager(&mut self, _target: Endpoint, _signo: i32) -> Result<i32, Errno> {
        Err(Errno::ENOSYS)
    }
}
