//! Fork system call implementation.
//!
//! Implements core logic of fork system call for PM server.

use minix_types::{Endpoint, UserSlot, PmError, VfsCall, OK};
use crate::mproc::{ProcTable, Lifecycle, SrvForkParams};
use crate::exit::KernelGateway;
use crate::ipc::{vm_fork, IpcTransport, tell_vfs};

/// Forks a process.
///
/// This is the main entry point for fork system call handling.
/// Coordinates with VM, VFS, and Kernel to create a child process.
///
/// # Arguments
/// * `table` - PM process table
/// * `parent_endpoint` - Parent process endpoint
/// * `transport` - IPC transport (used to send the fork request to VFS
///   asynchronously, see `tell_vfs`)
///
/// # Returns
/// * `Ok(child_pid)` - Child process PID (returned in parent)
/// * `Err(e)` - Error
pub fn do_fork<T: IpcTransport>(
    table: &mut ProcTable,
    parent_endpoint: Endpoint,
    transport: &mut T,
    kern: &mut dyn KernelGateway,
) -> Result<i32, ForkCoordError> {
    // 1. Find parent process (forkexit.c:59 rmp=mp)
    let parent_slot = find_parent_slot(table, parent_endpoint)?;
    let parent = &table.procs[parent_slot];
    let is_root = parent.resources.privilege.credentials().is_superuser();

    // 2. Capacity check procs_in_use + LAST_FEW (forkexit.c:60-65 → EAGAIN)
    if !table.can_alloc_for_user(is_root) {
        return Err(ForkCoordError::ProcTableFull);
    }

    // 3. Find free slot via round-robin next_child (forkexit.c:68-75)
    // C: do { next_child=(next_child+1)%NR_PROCS; n++; } while(IN_USE && n<=NR_PROCS)
    // Rust: find_free_slot increments before checking, same order as C
    let child_slot = table
        .find_free_slot()
        .ok_or(ForkCoordError::ProcTableFull)?;

    // 4. VM copy address space (forkexit.c:77-79 vm_fork, sync taskcall)
    // C: a failed vm_fork returns s directly (errno propagation); success
    // enters the non-failable window (forkexit.c:83 "PM may not fail fork
    // after call to vm_fork()"). Real wire: sendrec(VM, VM_FORK), with the
    // child endpoint in the reply at m1i3 (see the vm_fork docs).
    let child_endpoint = vm_fork(transport, parent_endpoint, UserSlot::new(child_slot))?;
    // VM's sys_fork uses the slot PM passed in, so the reply endpoint's slot
    // must equal next_child; a development-time assertion guards that
    // contract (release trusts VM, same as C).
    debug_assert_eq!(
        child_endpoint.slot() as usize,
        child_slot,
        "vm_fork returned endpoint slot mismatch"
    );

    // 5. Occupy slot + full copy (forkexit.c:84-116)
    // C: procs_in_use++ (86) precedes *rmc=*rmp (87); Rust's alloc_slot has
    // already done the find before step 3, so the ++ is manual here to match
    // C's "occupy only after vm_fork succeeds" timing (a failed vm_fork
    // occupies nothing, so no rollback is needed).
    table.procs_in_use.set(table.procs_in_use.get() + 1);
    // Copy mproc fields (forkexit.c:87-95) — explicit construction, not *rmc=*rmp
    // Temporarily generate pid for copy_mproc to use, but pid will be overwritten
    // after get_free_pid (see step 6). For now use placeholder 0; copy_mproc will
    // set parent relationship and privilege/scheduler, but pid will be replaced.
    // To keep single copy, we first copy with placeholder pid, then assign real pid.
    copy_mproc(table, parent_slot, child_slot, 0, child_endpoint);
    // 6. PID allocation (forkexit.c:119 get_free_pid, after copy, after procs_in_use++)
    let child_pid = table.pid_generator.get_free_pid(table);
    table.procs[child_slot].identity.id.pid = child_pid;

    // 7. VFS copy fd table (forkexit.c:122-130 tell_vfs)
    // VFS_CALL is placed on the child's slot (rmc); payload
    // m7i1=child/m7i2=parent/m7i3=pid/m7i4/m7i5=-1
    let vfs_call = VfsCall::Fork {
        child: child_endpoint,
        parent: parent_endpoint,
        child_pid,
    };
    // A tell_vfs failure panics (same shape as utility.c; an invariant
    // violation or a corrupt transport is not a user-visible fork error code).
    tell_vfs(table, UserSlot::new(child_slot), vfs_call, transport);

    // 8. Tracer SIGSTOP (forkexit.c:132-135)
    // C: if (rmc->mp_tracer != NO_TRACER) sig_proc(rmc, SIGSTOP, TRUE /*trace*/, FALSE);
    // Whether the child holds a tracer is decided by copy_mproc's conditional
    // TO_TRACEFORK inheritance (forkexit.c:91-96); trace=TRUE makes sig_proc
    // take the ptrace stop branch (signal.c:384), so the child enters the
    // stopped state and waits for the tracer to resume it.
    if table.procs[child_slot].state.guardianship.tracer().is_some() {
        // C ignores the sig_proc return value here (forkexit.c:135) — the
        // child was just created, the tracer slot is valid, and SIGSTOP is
        // legal, so a failure is unreachable at this call site.
        //[SD19-gate] PM send/notify/reply: fire and forget
        #[allow(clippy::let_underscore_must_use)]
        let _ = crate::signal::sig_proc(
            table,
            UserSlot::new(child_slot),
            crate::signal::SIGSTOP,
            true,
            false,
            kern,
            transport,
        );
    }

    // 9. Return SUSPEND (forkexit.c:139 return SUSPEND → dispatcher ReplyLater)
    Ok(child_pid)
}

/// Handles `PM_SRV_FORK` (RS → PM, `forkexit.c:142-240`).
///
/// Differences vs `do_fork` (see 08-pm-srv-fork.md §1.5):
/// - `parent_ep != RS → EPERM` (159-160)
/// - `IN_USE|PRIV_PROC|DELAY_CALL` (199-200, retain PRIV_PROC) vs `IN_USE|DELAY_CALL|TAINTED`
/// - Credentials injected from `params` (206-211) vs inherited
/// - `VFS_PM_SRV_FORK` `REUID/REGID = uid/gid` (227-228) vs `-1`
/// - Immediate `reply(child, OK)` + `Ok(pid)` (237/239) vs `SUSPEND`
pub fn do_srv_fork<T: IpcTransport>(
    table: &mut ProcTable,
    parent_endpoint: Endpoint,
    params: SrvForkParams,
    transport: &mut T,
    kern: &mut dyn KernelGateway,
) -> Result<i32, ForkCoordError> {
    // 1. RS gate (forkexit.c:159-160)
    if parent_endpoint != Endpoint::RS {
        return Err(ForkCoordError::NotPermitted);
    }
    let parent_slot = find_parent_slot(table, parent_endpoint)?;
    let parent = &table.procs[parent_slot];
    let is_root = parent.resources.privilege.credentials().is_superuser();

    // 2. Capacity (162-171)
    if !table.can_alloc_for_user(is_root) {
        return Err(ForkCoordError::ProcTableFull);
    }

    // 3. Find slot (174-181, private static next_child per srv_fork — shared Cell in Rust)
    let child_slot = table.find_free_slot().ok_or(ForkCoordError::ProcTableFull)?;

    // 4. VM fork (183-185) — same as do_fork step 4: real sendrec(VM, VM_FORK)
    let child_endpoint = vm_fork(transport, parent_endpoint, UserSlot::new(child_slot))?;
    debug_assert_eq!(child_endpoint.slot() as usize, child_slot);

    // 5. Occupy + srv copy (187-216) — retain PRIV_PROC, inject uid/gid
    table.procs_in_use.set(table.procs_in_use.get() + 1);
    
    {
        // Use mproc's srv_fork_from for explicit construction
        let child = crate::mproc::Process::srv_fork_from(
            &table.procs[parent_slot],
            child_slot,
            0, // placeholder, will be overwritten after get_free_pid
            child_endpoint,
            parent_slot,
            params,
            // [E6: kernel uptime not wired] C's `getticks()` (forkexit.c:114)
            // depends on kernel uptime; this explicitly takes 0 until
            // minix-sys lands (todo.md §6).
            0,
        );
        table.procs[child_slot] = child;
    }
    // srv_fork_from builds a Normal guardianship; the conditional
    // TO_TRACEFORK inheritance overwrites it here (forkexit.c:187-216 copy +
    // the same conditional clearing at 91-96, see inherit_guardianship).
    let inherited =
        inherit_guardianship(&table.procs[parent_slot].state.guardianship, UserSlot::new(parent_slot));
    table.procs[child_slot].state.guardianship = inherited;
    let child_pid = table.pid_generator.get_free_pid(table);
    table.procs[child_slot].identity.id.pid = child_pid;

    // 6. VFS srv fork (222-230, REUID/REGID = uid/gid)
    let vfs_call = VfsCall::SrvFork {
        child: child_endpoint,
        parent: parent_endpoint,
        child_pid,
        reuid: params.uid as i32,
        regid: params.gid as i32,
    };
    // A tell_vfs failure panics (same shape as utility.c; an invariant
    // violation or a corrupt transport is not a user-visible fork error code).
    tell_vfs(table, UserSlot::new(child_slot), vfs_call, transport);

    // 7. Tracer SIGSTOP (forkexit.c:231-234) — isomorphic to do_fork step 8;
    // the child's tracer inheritance likewise follows the TO_TRACEFORK
    // condition (the *rmc=*rmp copy at forkexit.c:187-216 + the same clearing
    // at 91-96; srv_fork_from's Normal construction is overwritten by the
    // conditional inheritance afterwards).
    if table.procs[child_slot].state.guardianship.tracer().is_some() {
        //[SD19-gate] PM send/notify/reply: fire and forget
        #[allow(clippy::let_underscore_must_use)]
        let _ = crate::signal::sig_proc(
            table,
            UserSlot::new(child_slot),
            crate::signal::SIGSTOP,
            true,
            false,
            kern,
            transport,
        );
    }

    // 8. Immediate reply to child (237) + return pid (239) — not SUSPEND
    let ok_msg = minix_types::Message {
        m_type: OK,
        ..Default::default()
    };
    // Child is now Running with VFS_CALL still set; reply does not clear VFS_CALL
    // (VFS reply 0x988 is empty, 05 handle_vfs_reply SrvFork → {} )
    //[SD19-gate] PM send/notify/reply: fire and forget
    #[allow(clippy::let_underscore_must_use)]
    let _ = transport.send(child_endpoint, &ok_msg);

    Ok(child_pid)
}

/// Finds parent process slot by endpoint.
fn find_parent_slot(
    table: &ProcTable,
    endpoint: Endpoint,
) -> Result<usize, ForkCoordError> {
    for (i, proc) in table.procs.iter().enumerate() {
        if proc.endpoint() == endpoint && proc.is_in_use() {
            return Ok(i);
        }
    }
    Err(ForkCoordError::InvalidEndpoint)
}

/// Fork-inheritable flags mask.
///
/// Corresponds to Minix3's:
/// ```c
/// rmc->mp_flags &= (IN_USE|DELAY_CALL|TAINTED);
/// ```
/// `IN_USE` is expressed by `Lifecycle::Running`; `DELAY_CALL` is modeled by
/// `BlockState::ipc_blocked` and deliberately reset at fork (a process with
/// DELAY_CALL set is mid-send in the kernel and cannot execute fork, so the
/// C inheritance is an artifact of the whole-slot copy). Only `TAINTED` is
/// inherited.
const FORK_INHERIT_FLAGS: crate::mproc::RemainingFlags = crate::mproc::RemainingFlags::TAINTED;

/// Copies parent's mproc fields to child.
///
/// Corresponds to Minix3's `do_fork` mproc copy logic.
/// Child guardianship inheritance decision (the Rust expression of
/// forkexit.c:87-96, shared by `do_fork` and `do_srv_fork`).
///
/// C copies `mp_tracer`/`mp_trace_flags`/`mp_sigtrace` wholesale via
/// `*rmc = *rmp`, then clears them conditionally by `TO_TRACEFORK`: only
/// when the parent's trace_flags **lack** `TO_TRACEFORK` is the child's
/// tracer set to NO_TRACER. On the Rust side a parent that is `Traced` with
/// `trace_options` containing `TRACEFORK` → the child inherits `Traced`
/// (`TRACE_EXIT` is not inherited — C's `FORK_INHERIT_FLAGS` lacks that bit,
/// and a new Rust child's forced-exit flag must be false); every other case
/// is `Normal`.
fn inherit_guardianship(
    parent: &crate::mproc::Guardianship,
    parent_slot: UserSlot,
) -> crate::mproc::Guardianship {
    match parent {
        crate::mproc::Guardianship::Traced {
            parent,
            tracer,
            trace_options,
            ..
        } if trace_options.contains(crate::mproc::TraceOptions::TRACEFORK) => {
            crate::mproc::Guardianship::Traced {
                parent: *parent,
                tracer: *tracer,
                                trace_options: *trace_options,
            }
        }
        _ => crate::mproc::Guardianship::Normal {
            parent: parent_slot,
        },
    }
}

fn copy_mproc(
    table: &mut ProcTable,
    parent_slot: usize,
    child_slot: usize,
    child_pid: i32,
    child_endpoint: Endpoint,
) {
    // Extract values from parent first to avoid borrow conflict
    let procgrp;
    let credentials;
    let nice;
    let scheduler;
    let parent_flags;
    let signal_actions;
    let signal_mask;
    // Tracer inheritance decision (forkexit.c:87-96) — computed in the
    // parent-borrow block to keep the child write section conflict-free.
    let child_guardianship;

    {
        let parent = &table.procs[parent_slot];
        procgrp = parent.procgrp();
        credentials = parent.resources.privilege.credentials().clone();
        nice = parent.resources.nice;
        // forkexit.c:96-100: a PRIV_PROC parent's regular-fork child is a
        // *user* process scheduled by SCHED; other children inherit.
        scheduler = if parent.resources.privilege.is_kernel() {
            Endpoint::SCHED
        } else {
            parent.resources.scheduler
        };
        parent_flags = parent.resources.flags;
        // Clone signal actions (corresponds to Minix3's mp_sigact copy)
        signal_actions = parent.resources.signals.actions.clone();
        signal_mask = parent.resources.signals.mask;
        // C: *rmc=*rmp copies mp_tracer/mp_trace_flags/mp_sigtrace wholesale,
        // then clears them conditionally — only when the parent's trace_flags
        // lack TO_TRACEFORK is the child's tracer cleared (forkexit.c:91-96).
        // The decision is expressed by the shared helper.
        child_guardianship =
            inherit_guardianship(&parent.state.guardianship, UserSlot::new(parent_slot));
    }

    // Now modify child
    let child = &mut table.procs[child_slot];

    // Copy identity fields
    child.identity.id.pid = child_pid;
    child.identity.id.index = UserSlot::new(child_slot);
    child.identity.endpoint = child_endpoint;
    child.identity.procgrp = procgrp;

    child.state.guardianship = child_guardianship;

    // Copy credentials (forkexit.c:87 copies mp_realuid etc.; PRIV_PROC is
    // dropped by the :106 mask, so a regular-fork child is always a *user*
    // process carrying the parent's credential values).
    child.resources.privilege = crate::mproc::Privilege::User(credentials);

    // Set lifecycle to running
    child.state.lifecycle = Lifecycle::Running;

    // Copy other fields as needed
    child.resources.nice = nice;
    child.resources.scheduler = scheduler;

    // Inherit only specific flags (corresponds to Minix3's flag filtering)
    child.resources.flags = parent_flags & FORK_INHERIT_FLAGS;

    // Copy signal actions (corresponds to Minix3's mp_sigact copy)
    child.resources.signals.actions = signal_actions;
    child.resources.signals.mask = signal_mask;

    // Reset interval timers (corresponds to Minix3's mp_interval reset)
    child.resources.intervals = [0; crate::mproc::NR_ITIMERS];
}

/// Fork coordination error type.
///
/// This error type covers the IPC coordination phase of fork (communicating
/// with VM, VFS, and Kernel). Distinct from `mproc::fork::ForkError` which
/// covers the slot allocation phase.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ForkCoordError {
    /// Parent process not found.
    NoProc,
    /// Out of memory.
    NoMem,
    /// Invalid endpoint.
    InvalidEndpoint,
    /// Not permitted (e.g., non-RS calling srv_fork → EPERM).
    NotPermitted,
    /// Process table full.
    ProcTableFull,
    /// Slot already in use.
    SlotInUse,
    /// VM service error.
    VmError,
    /// VFS service error.
    /// Kernel error.
    KernelError,
}

impl From<ForkCoordError> for PmError {
    fn from(e: ForkCoordError) -> Self {
        match e {
            ForkCoordError::NoProc => PmError::InvalidEndpoint,
            ForkCoordError::NoMem => PmError::OutOfMemory,
            ForkCoordError::InvalidEndpoint => PmError::InvalidEndpoint,
            ForkCoordError::NotPermitted => PmError::PermissionDenied,
            ForkCoordError::ProcTableFull => PmError::ProcTableFull,
            ForkCoordError::SlotInUse => PmError::SlotInUse,
            ForkCoordError::VmError => PmError::InternalError,
            ForkCoordError::KernelError => PmError::InternalError,
        }
    }
}

#[cfg(test)]
mod tests {

    /// Kernel gateway mock for fork.rs tests (scripted sys_times accounting
    /// values).
    struct TestKernelTimes {
        pub user: minix_types::Clock,
        pub sys: minix_types::Clock,
    }
    impl Default for TestKernelTimes {
        fn default() -> Self {
            Self { user: 30, sys: 12 }
        }
    }
    impl crate::exit::KernelGateway for TestKernelTimes {
        fn sys_sigsend(&mut self, _ep: minix_types::Endpoint, _sigmsg: &minix_sys::syscall::SigMsgWire) -> Result<(), i32> { Ok(()) }
        fn get_ksig(&mut self) -> Result<Option<(minix_types::Endpoint, minix_types::SigSetBits)>, i32> { Ok(None) }
    fn end_ksig(&mut self, _ep: minix_types::Endpoint, _sig: i32) -> Result<(), i32> { Ok(()) }
    fn sys_diagctl_stacktrace(&mut self, _ep: minix_types::Endpoint) -> Result<(), i32> { Ok(()) }
        fn sys_trace(&mut self, _req: i32, _ep: minix_types::Endpoint, _addr: u64, _data: &mut i64) -> Result<(), i32> { Ok(()) }
    fn sys_vircopy(&mut self, _src_ep: minix_types::Endpoint, _src: u64, _dst_ep: minix_types::Endpoint, _dst: u64, _len: u64) -> Result<(), i32> { Ok(()) }
    fn copy_from_user(&mut self, _src_ep: minix_types::Endpoint, _src: u64, _bytes: &mut [u8]) -> Result<(), i32> { Ok(()) }
        fn sys_delay_stop(&mut self, _ep: minix_types::Endpoint) -> Result<(), i32> { Ok(()) }
        fn sys_kill(&mut self, _ep: Endpoint, _sig: i32) -> Result<(), i32> { Ok(()) }
        fn sys_clear(&mut self, _ep: Endpoint) -> Result<(), i32> { Ok(()) }
        fn sys_abort(&mut self, _how: i32) -> Result<(), i32> { Ok(()) }
        fn copy_to_user(&mut self, _bytes: &[u8], _dst_ep: Endpoint, _dst_addr: u64) -> Result<(), i32> {
            Ok(())
        }
        fn sys_resume(&mut self, _ep: Endpoint) -> Result<(), i32> { Ok(()) }
        fn proc_times(&mut self, _ep: Endpoint) -> Result<(minix_types::Clock, minix_types::Clock), i32> {
            Ok((self.user, self.sys))
        }
    }
    use super::*;

    fn create_test_table_with_parent() -> ProcTable {
        let mut table = ProcTable::new();

        // Initialize a parent process
        let parent_slot = 0;
        table.procs[parent_slot].identity.endpoint = Endpoint::from_generation_slot(1, 0);
        table.procs[parent_slot].identity.id.pid = 100;
        table.procs[parent_slot].state.lifecycle = Lifecycle::Running;

        table
    }

    /// Enqueue one VM_FORK reply on the mock transport (OK + child endpoint in
    /// m1i3).
    ///
    /// In the real protocol VM's sys_fork uses the slot PM passed in and bumps
    /// the generation; the test passes the expected child endpoint explicitly
    /// as `child_endpoint`, and `vm_fork`'s debug_assert slot guard relies on
    /// the two agreeing.
    fn queue_vm_fork_reply(transport: &mut crate::ipc::TestIpcTransport, child_endpoint: Endpoint) {
        let mut reply = minix_types::Message { m_type: OK, ..minix_types::Message::default() };
        reply.m_u.m_m1.m1i3 = child_endpoint.0;
        transport.queue_sendrec_reply(reply);
    }

    #[test]
    fn test_find_parent_slot_success() {
        let table = create_test_table_with_parent();

        let result = find_parent_slot(&table, Endpoint::from_generation_slot(1, 0));
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 0);
    }

    #[test]
    fn test_find_parent_slot_not_found() {
        let table = ProcTable::new();

        let result = find_parent_slot(&table, Endpoint::from_generation_slot(1, 0));
        assert!(result.is_err());
    }

    #[test]
    fn test_do_fork_success() {
        let mut table = create_test_table_with_parent();
        let mut transport = crate::ipc::TestIpcTransport::default();
        let mut kern = TestKernelTimes::default();
        // Scripted VM_FORK reply: empty table + cursor 0 → first free slot is
        // 1; the child endpoint generation = parent(1) + 1 (VM sys_fork's
        // generation-bump contract).
        queue_vm_fork_reply(&mut transport, Endpoint::from_generation_slot(2, 1));

        let result = do_fork(
            &mut table,
            Endpoint::from_generation_slot(1, 0),
            &mut transport,
            &mut kern,
        );
        assert!(result.is_ok());

        let child_pid = result.unwrap();
        assert!(child_pid > 0);
        // Real wire: the VM_FORK task call has been sent (the sendrec is
        // recorded in sent).
        assert_eq!(transport.sent()[0].0, Endpoint::VM);
        assert_eq!(transport.sent()[0].1.m_type, minix_types::VM_FORK as i32);
    }

    #[test]
    fn test_do_fork_parent_not_found() {
        let mut table = ProcTable::new();
        let mut transport = crate::ipc::TestIpcTransport::default();
        let mut kern = TestKernelTimes::default();

        let result = do_fork(
            &mut table,
            Endpoint::from_generation_slot(1, 0),
            &mut transport,
            &mut kern,
        );
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), ForkCoordError::InvalidEndpoint);
    }

    #[test]
    fn test_do_srv_fork_eperm() {
        // Non-RS parent → EPERM
        let mut table = ProcTable::new();
        table.procs[5].identity.endpoint = Endpoint::from_generation_slot(1, 5);
        table.procs[5].identity.id.pid = 100;
        table.procs[5].state.lifecycle = Lifecycle::Running;
        let mut transport = crate::ipc::TestIpcTransport::default();
        let mut kern = TestKernelTimes::default();
        let params = crate::mproc::SrvForkParams { uid: 0, gid: 0 };
        let result = do_srv_fork(
            &mut table,
            Endpoint::from_generation_slot(1, 5),
            params,
            &mut transport,
            &mut kern,
        );
        assert_eq!(result.unwrap_err(), ForkCoordError::NotPermitted);
    }

    #[test]
    fn test_do_srv_fork_success() {
        let mut table = ProcTable::new();
        // RS at slot 2
        table.procs[2].identity.endpoint = Endpoint::RS;
        table.procs[2].identity.id.pid = 2;
        table.procs[2].state.lifecycle = Lifecycle::Running;
        table.procs[2].resources.privilege = crate::mproc::Privilege::Kernel(crate::mproc::Credentials::default());
        table.procs[2].resources.scheduler = Endpoint::NONE;

        let mut transport = crate::ipc::TestIpcTransport::default();
        let mut kern = TestKernelTimes::default();
        queue_vm_fork_reply(&mut transport, Endpoint::from_generation_slot(2, 1));
        let params = crate::mproc::SrvForkParams { uid: 1000, gid: 100 };
        let result = do_srv_fork(&mut table, Endpoint::RS, params, &mut transport, &mut kern);
        assert!(result.is_ok());
        let child_pid = result.unwrap();
        assert!(child_pid > 0);
        // VFS_CALL on child + immediate reply to child
        let vfs_sent = transport.sent().iter().any(|(ep, msg)| *ep == Endpoint::VFS && msg.m_type == minix_types::VFS_PM_SRV_FORK);
        assert!(vfs_sent, "VFS_PM_SRV_FORK not sent");
        // Child immediate reply (OK) — transport should have sent to child endpoint
        let child_replied = transport.sent().iter().any(|(_, msg)| msg.m_type == minix_types::OK);
        assert!(child_replied, "child not replied OK");
        // Child should have injected credentials
        let child_slot = table.find_proc(child_pid).expect("child not found").get();
        let child = &table.procs[child_slot];
        // Child retains PRIV_PROC (forkexit.c:199-200) *and* carries injected
        // credentials (forkexit.c:206-211).
        assert!(child.is_kernel_process(), "srv_fork child retains PRIV_PROC");
        assert_eq!(child.resources.privilege.credentials().user.real, 1000);
    }

    #[test]
    fn test_do_srv_fork_vfs_call() {
        let mut table = ProcTable::new();
        table.procs[2].identity.endpoint = Endpoint::RS;
        table.procs[2].identity.id.pid = 2;
        table.procs[2].state.lifecycle = Lifecycle::Running;
        table.procs[2].resources.privilege = crate::mproc::Privilege::Kernel(crate::mproc::Credentials::default());
        let mut transport = crate::ipc::TestIpcTransport::default();
        let mut kern = TestKernelTimes::default();
        queue_vm_fork_reply(&mut transport, Endpoint::from_generation_slot(2, 1));
        let params = crate::mproc::SrvForkParams { uid: 42, gid: 43 };
        //[SD19-gate] PM send/notify/reply: fire and forget
        #[allow(clippy::let_underscore_must_use)]
        let _ = do_srv_fork(&mut table, Endpoint::RS, params, &mut transport, &mut kern).unwrap();
        // VFS call should carry real uid/gid, not -1
        let vfs_msg = transport
            .sent()
            .iter()
            .find(|(ep, _)| *ep == Endpoint::VFS)
            .unwrap()
            .1;
        let m7 = unsafe { vfs_msg.m_u.m_m7 };
        assert_eq!(m7.m7i4, 42);
        assert_eq!(m7.m7i5, 43);
    }

    // ── tracer SIGSTOP and conditional TO_TRACEFORK inheritance
    // ── (forkexit.c:87-96/132-135)

    use crate::mproc::{Guardianship, TraceOptions};

    /// Seed the `parent` slot with a process traced by `tracer`.
    fn seed_traced_parent(table: &mut ProcTable, parent: usize, tracer: usize, tracefork: bool) {
        let mut opts = TraceOptions::empty();
        if tracefork {
            opts |= TraceOptions::TRACEFORK;
        }
        table.procs[parent].identity.endpoint = Endpoint::from_generation_slot(1, parent as i32);
        table.procs[parent].identity.id.pid = 100 + parent as i32;
        table.procs[parent].state.lifecycle = Lifecycle::Running;
        table.procs[parent].state.guardianship = Guardianship::Traced {
            parent: UserSlot::new(0),
            tracer: UserSlot::new(tracer),
                        trace_options: opts,
        };
    }

    #[test]
    fn test_do_fork_tracefork_child_inherits_tracer_and_stops() {
        // C: parent Traced + TO_TRACEFORK → the child inherits the tracer
        // (the 91-96 condition does not clear it), and do_fork calls
        // sig_proc(SIGSTOP, trace=TRUE) on the child (132-135) → the child
        // enters the ptrace-stopped state, with sigtrace recording SIGSTOP.
        let mut table = ProcTable::new();
        seed_traced_parent(&mut table, 3, 7, true);
        let mut transport = crate::ipc::TestIpcTransport::default();
        let mut kern = TestKernelTimes::default();
        queue_vm_fork_reply(&mut transport, Endpoint::from_generation_slot(2, 1));

        let child_pid = do_fork(&mut table, Endpoint::from_generation_slot(1, 3), &mut transport, &mut kern)
            .expect("fork with TO_TRACEFORK should succeed");
        let child_slot = table.find_proc(child_pid).expect("child not found").get();

        let child = &table.procs[child_slot];
        assert_eq!(
            child.state.guardianship.tracer(),
            Some(UserSlot::new(7)),
            "TO_TRACEFORK child must inherit the tracer"
        );
        assert!(child.state.trace.stopped, "child must be ptrace-stopped");
        assert_eq!(
            child.resources.signals.trace_mask,
            1u64 << (crate::signal::SIGSTOP - 1),
            "sigtrace must record the SIGSTOP delivery"
        );
    }

    #[test]
    fn test_do_fork_without_tracefork_child_untraced_and_running() {
        // C: parent Traced but without TO_TRACEFORK → the child's tracer is
        // cleared (91-96), no SIGSTOP is sent, and the child stays Running.
        let mut table = ProcTable::new();
        seed_traced_parent(&mut table, 3, 7, false);
        let mut transport = crate::ipc::TestIpcTransport::default();
        let mut kern = TestKernelTimes::default();
        queue_vm_fork_reply(&mut transport, Endpoint::from_generation_slot(2, 1));

        let child_pid = do_fork(&mut table, Endpoint::from_generation_slot(1, 3), &mut transport, &mut kern)
            .expect("fork without TO_TRACEFORK should succeed");
        let child_slot = table.find_proc(child_pid).expect("child not found").get();

        let child = &table.procs[child_slot];
        assert!(
            matches!(
                &child.state.guardianship,
                Guardianship::Normal { parent } if *parent == UserSlot::new(3)
            ),
            "child tracer must be cleared without TO_TRACEFORK"
        );
        assert!(!child.state.trace.stopped, "child must not be stopped");
        assert_eq!(child.resources.signals.trace_mask, 0);
    }

    #[test]
    fn test_do_srv_fork_tracefork_child_inherits_tracer_and_stops() {
        // C: do_srv_fork's copy section has the same shape (187-216 +
        // 231-234) — an srv child likewise inherits by TO_TRACEFORK and
        // receives the trace SIGSTOP.
        let mut table = ProcTable::new();
        table.procs[2].identity.endpoint = Endpoint::RS;
        table.procs[2].identity.id.pid = 2;
        table.procs[2].state.lifecycle = Lifecycle::Running;
        table.procs[2].resources.privilege = crate::mproc::Privilege::Kernel(crate::mproc::Credentials::default());
        table.procs[2].resources.scheduler = Endpoint::NONE;
        // RS (the parent here) is traced by tracer=7 and carries
        // TO_TRACEFORK.
        table.procs[2].state.guardianship = Guardianship::Traced {
            parent: UserSlot::new(0),
            tracer: UserSlot::new(7),
                        trace_options: TraceOptions::TRACEFORK,
        };

        let mut transport = crate::ipc::TestIpcTransport::default();
        let mut kern = TestKernelTimes::default();
        queue_vm_fork_reply(&mut transport, Endpoint::from_generation_slot(2, 1));
        let params = crate::mproc::SrvForkParams { uid: 1000, gid: 100 };

        let child_pid = do_srv_fork(&mut table, Endpoint::RS, params, &mut transport, &mut kern)
            .expect("srv_fork with TO_TRACEFORK should succeed");
        let child_slot = table.find_proc(child_pid).expect("child not found").get();

        let child = &table.procs[child_slot];
        assert_eq!(child.state.guardianship.tracer(), Some(UserSlot::new(7)));
        assert!(child.state.trace.stopped);
        assert_eq!(child.resources.signals.trace_mask, 1u64 << (crate::signal::SIGSTOP - 1));
    }
}
