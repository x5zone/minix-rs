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

use minix_types::{Clock, Endpoint, Errno};

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
pub use sef::{SefCallbacks, SefInitInfo, SefInitType};
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
    /// C: `main()` loop — main.c:50-131. Skeleton: message receive
    /// (`get_work`, 06) + classification ([`dispatch::classify`]) + request
    /// dispatch. The receive primitive is DEFERRED (06-rs-main-loop.md); the
    /// loop fails closed until then.
    pub fn run(&mut self) -> Result<(), Errno> {
        // T1: the main loop operates on the post-boot runtime state. Fail
        // closed (loudly) if boot never completed — an RS that has not
        // finished booting cannot manage services.
        if self.state.is_none() {
            panic!("run() requires a completed fresh boot (init(Fresh))");
        }
        loop {
            // C: rs_idle_period() — main.c:59 (06).
            // C: get_work() → sef_receive_status(ANY) — main.c:62, 826-833 (06).
            let (msg, rcv_sts, ts) = self.get_work()?;
            let _kind = dispatch::classify(&rcv_sts, msg.m_source, msg.m_type, ts);
            // C: message dispatch — main.c:70-127 (mechanisms in 06/07/12-16).
            // 06 wiring: `do_period` reads `state.system_hz`/`state.table`;
            // the RS_DOWN sweep reads/writes `state.shutting_down`.
            let _ = (self.state.as_ref(), _kind);
        }
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
    fn init_restart(&mut self, _init_type: SefInitType, info: &SefInitInfo) -> Result<i32, Errno> {
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

    /// C: `sef_cb_init_lu` — main.c:549-586: `update_service(RS_DONTSWAP)`
    /// into the new instance, then `init_service(SEF_INIT_LU)` (the
    /// callback-table rebind of main.c:558 pairs with A3's restart_cb note
    /// in 18). RS self-init sends no message.
    fn init_lu(&mut self, _init_type: SefInitType, info: &SefInitInfo) -> Result<i32, Errno> {
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

    /// C: `sef_cb_init_response` — main.c:591-609. **EDGE（19 接线）**：决策
    /// 面（`do_init_ready` 四参数 + pending 持有 + normalize 包装）全部就绪；
    /// 缺的是消息载荷解码——`m_rs_init.result` 位于 union 臂，安全提取归 19 的
    /// receive 包装（R25 同源）。落地形态：
    /// `let result = decode.result; if result != 0 { return Err(...) };
    /// do_init_ready(flags, 0, is_updating, pending, ticks)` + mutations +
    /// pending 回写（UpdateInitDone）。
    fn init_response(&mut self, _m: &minix_types::Message) -> Result<i32, Errno> {
        Err(Errno::ENOSYS)
    }

    /// C: `sef_cb_lu_response` — main.c:614-626. **EDGE（19 接线）**：同上，
    /// 决策面 `do_upd_ready(result, gate_ok, has_next)` 就绪（gate 由
    /// `state.update` + RS 槽 `upd` 判定），载荷解码归 19。落地形态：
    /// `do_upd_ready(result, gate_ok, true)` → R24 载荷 → Unexpected 时
    /// EINVAL，EDONTREPLY → EGENERIC（main.c:622-624）。
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
        let mut server = RsServer::new(crate::boot::BootTables::placeholder());
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
