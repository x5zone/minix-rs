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

pub use access::{caller_can_control, caller_is_root, check_call_permission};
pub use boot::{BootInit, BootTables, KernelApi, Machine};
pub use exec::{free_exec, has_shared_exec, share_exec, validate_image};
pub use ipc_mask::{IpcListIterator, add_backward_ipc, add_forward_ipc, init_privs};
pub use live_update::{
    AbortAction, EndUpdateRole, LuFlags, RS_CANCEL, RS_REPLY, SEF_INIT_ST, SEF_LU_STATE_NULL,
    SEF_LU_STATE_UNREACHABLE, UpdateChain, UpdateEntry, UpdatePhase, abort_action,
    default_prepare_maxtime, end_srv_reply_flag, end_update_role, lu_flags_from_rss, update_phase,
    validate_update_request, vm_default_prealloc,
};
pub use monitor::{
    PeriodAction, delta_t, effective_period, has_update_timed_out, init_timeout, period_decision,
    sigchld_cleanup,
};
pub use privilege::{PrivCtlOp, Privilege};
pub use process_table::{RProcTable, RupdateDescriptor, RupdateFlags, ServiceInstances};
pub use publish::{should_bind_devman, should_map_driver, should_set_pci_acl, unpublish_result};
pub use query::{
    GetsysinfoTable, SysctlAction, classify_sysctl, getsysinfo_table, lookup_name_len,
};
pub use ready::{
    InitMessage, ReadyOutcome, do_init_ready, do_upd_ready, end_srv_init, init_message,
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
    lu_init_invariants, rollback_needs_vm_update, rollback_swap_flag, self_upgrade_role,
    should_end_update_on_restart, should_pre_swap, sig_mgr_updates, srv_update_action,
};
pub use service_create::{
    activate_service, check_create_preconditions, clone_slot, link_replica, mark_child_created,
    rebuild_args, swap_index, swap_slot,
};
pub use service_slot::{
    ARGV_ELEMENTS, IMM_SF, Label, MAX_COMMAND_LEN, MAX_IPC_LIST, MAX_NR_ARGS, MAX_SCRIPT_LEN,
    NR_DOMAIN, NR_IO_RANGE, NR_IRQ, NR_MEM_RANGE, PublicSlot, RFlags, RS_MAX_LABEL_LEN,
    RS_NR_CONTROL, SRV_SF, SRVR_SF, ServiceSlot, SlotId, SysFlags, VM_SF,
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
    // Held for the SEF_INIT *message* dispatch path (12-rs-init-run.md):
    // `RsServer::init` drives the fresh boot directly via `BootInit`, so the
    // registered callback table is not read until the main loop's RS_INIT
    // branch merges the two paths (doc §3.3).
    #[allow(dead_code)]
    callbacks: SefCallbacks,
    boot: BootInit<'static>,
    kernel: alloc::boxed::Box<dyn KernelApi>,
}

impl RsServer {
    /// Creates the server with the full SEF callback table and boot tables.
    ///
    /// C: `sef_local_startup()` (main.c:51) + `sys_getimage` (main.c:196)
    /// results. The kernel API is fail-closed (`UnimplementedKernelApi`)
    /// until the `minix-sys` wiring lands (19).
    pub fn new(callbacks: SefCallbacks, tables: BootTables<'static>) -> Self {
        Self::with_kernel(
            callbacks,
            tables,
            alloc::boxed::Box::new(boot::UnimplementedKernelApi),
        )
    }

    /// Creates the server with an injected kernel API (tests / wiring).
    pub fn with_kernel(
        callbacks: SefCallbacks,
        tables: BootTables<'static>,
        kernel: alloc::boxed::Box<dyn KernelApi>,
    ) -> Self {
        Self {
            callbacks,
            boot: BootInit::new(tables),
            kernel,
        }
    }

    /// Runs the SEF startup and the 4-step boot.
    ///
    /// C: `sef_startup()` → `sef_cb_init_fresh()` — main.c:151, 158-494. The
    /// fresh init IS the 4-step boot ([`BootInit::init_fresh`]); the SEF_INIT
    /// *message* path (receive + [`SefCallbacks::startup`] dispatch by init
    /// type) belongs to the main loop's RS_INIT branch (12-rs-init-run.md).
    /// LU/restart inits are DEFERRED until 18 lands; they fail closed.
    pub fn init(&mut self, init_type: SefInitType) -> Result<i32, i32> {
        match init_type {
            SefInitType::Fresh => {
                self.boot.init_fresh(self.kernel.as_mut())?;
                Ok(0) // C: sef_startup() returns OK after the fresh init.
            }
            SefInitType::Lu | SefInitType::Restart => {
                // C: sef_cb_init_lu / sef_cb_init_restart — 18-rs-self-lifecycle.md.
                // DEFERRED until 18 lands; fail closed.
                Err(minix_types::ENOSYS)
            }
        }
    }

    /// Runs the main loop.
    ///
    /// C: `main()` loop — main.c:50-131. Skeleton: message receive
    /// (`get_work`, 06) + classification ([`dispatch::classify`]) + request
    /// dispatch. The receive primitive is DEFERRED (06-rs-main-loop.md); the
    /// loop fails closed until then.
    pub fn run(&mut self) -> ! {
        loop {
            // C: rs_idle_period() — main.c:59 (06).
            // C: get_work() → sef_receive_status(ANY) — main.c:62, 826-833 (06).
            let (msg, rcv_sts) = self.get_work();
            let _kind = dispatch::classify(&rcv_sts, msg.m_source, msg.m_type);
            // C: message dispatch — main.c:70-127 (mechanisms in 06/07/12-16).
        }
    }

    /// C: `get_work()` — main.c:826-833. The receive primitive is DEFERRED
    /// (06-rs-main-loop.md); `minix-sys::receive` is a stub and fails closed
    /// until then.
    fn get_work(&mut self) -> (minix_types::Message, dispatch::IpcStatus) {
        let mut msg = minix_types::Message::default();
        minix_sys::receive(minix_types::Endpoint::ANY, &mut msg)
            .expect("ipc_receive() failed (receive primitive DEFERRED: 06-rs-main-loop.md)");
        // C: the ipc_status word — com.h:92 (is_ipc_notify). Full parsing: 06.
        (msg, dispatch::IpcStatus { flags: 0 })
    }
}
