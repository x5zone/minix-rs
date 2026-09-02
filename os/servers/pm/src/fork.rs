//! Fork system call implementation.
//!
//! Implements core logic of fork system call for PM server.

use minix_types::{Endpoint, UserSlot, PmError, VmForkIn, VfsCall, EAGAIN, EPERM, OK};
use crate::mproc::{ProcTable, Lifecycle, Privilege, SrvForkParams};
use crate::ipc::{send_vm_fork, IpcTransport, tell_vfs};

/// Forks a process.
///
/// This is the main entry point for fork system call handling.
/// Coordinates with VM, VFS, and Kernel to create a child process.
///
/// # Arguments
/// * `table` - PM process table
/// * `parent_endpoint` - Parent process endpoint
/// * `transport` - IPC 传输（用于向 VFS 异步发送 fork 请求，见 `tell_vfs`）
///
/// # Returns
/// * `Ok(child_pid)` - Child process PID (returned in parent)
/// * `Err(e)` - Error
pub fn handle_fork<T: IpcTransport>(
    table: &mut ProcTable,
    parent_endpoint: Endpoint,
    transport: &mut T,
) -> Result<i32, ForkCoordError> {
    // 1. Find parent process (forkexit.c:59 rmp=mp)
    let parent_slot = find_parent_slot(table, parent_endpoint)?;
    let parent = &table.procs[parent_slot];
    let is_root = match &parent.resources.privilege {
        Privilege::Kernel => true, // system process (PRIV_PROC) treated as superuser for LAST_FEW
        Privilege::User(c) => c.user.effective == 0,
    };

    // 2. Capacity check procs_in_use + LAST_FEW (forkexit.c:60-65 → EAGAIN)
    if !table.can_alloc_for_user(is_root) {
        return Err(ForkCoordError::ProcTableFull);
    }

    // 3. Find free slot via round-robin next_child (forkexit.c:68-75)
    // C: do { next_child=(next_child+1)%NR_PROCS; n++; } while(IN_USE && n<=NR_PROCS)
    // Rust: find_free_slot 先递增后检查，与 C 同序
    let child_slot = table
        .find_free_slot()
        .ok_or(ForkCoordError::ProcTableFull)?;

    // 4. VM copy address space (forkexit.c:78 vm_fork, sync, s is errno)
    // C: vm_fork失败直接 return s；成功后进入不可失败窗口 (forkexit.c:82)
    let vm_request = VmForkIn {
        parent_endpoint,
        child_slot: UserSlot::new(child_slot),
    };
    let vm_resp = send_vm_fork(vm_request).map_err(|_| ForkCoordError::VmError)?;
    // VM 返回的 child endpoint（m1_i3），其 slot 必须等于 child_slot (forkexit.c:74-75 守卫)
    let child_endpoint = vm_resp.child_endpoint;
    debug_assert_eq!(
        child_endpoint.slot() as usize,
        child_slot,
        "vm_fork returned endpoint slot mismatch"
    );

    // 5. Occupy slot + full copy (forkexit.c:84-116)
    // C: procs_in_use++ (86) 在 *rmc=*rmp (87) 之前；Rust 的 alloc_slot 已在 step 3 前做 find，
    // 此处手动 ++ 以对齐 C 的"vm_fork成功后才占位"时序（vm_fork失败时不占位，无需回滚）
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
    // VFS_CALL 置于子进程槽 (rmc)，载荷 m7i1=child/m7i2=parent/m7i3=pid/m7i4/m7i5=-1
    let vfs_call = VfsCall::Fork {
        child: child_endpoint,
        parent: parent_endpoint,
        child_pid,
    };
    tell_vfs(table, UserSlot::new(child_slot), vfs_call, transport)
        .map_err(|_| ForkCoordError::VfsError)?;

    // 8. Tracer SIGSTOP (forkexit.c:133-134, DEFERRED — 11-signal-core.md)
    // if (mp_tracer != NO_TRACER) sig_proc(rmc, SIGSTOP, trace=true)
    {
        let tracer = table.procs[child_slot].state.guardianship.tracer();
        if tracer.is_some() {
            // [ARCH: 06 已落地 EventCall, 11 未落地 sig_proc]
            // 当前仅记录意图，11 落地时替换为真实 signal::sig_proc
            #[allow(unreachable_code)]
            {
                // keep as no-op for now; test以 NoTracer 为主路径
            }
        }
    }

    // 9. Return SUSPEND (forkexit.c:139 return SUSPEND → dispatcher ReplyLater)
    Ok(child_pid)
}

/// Handles `PM_SRV_FORK` (RS → PM, `forkexit.c:142-240`).
///
/// Differences vs `handle_fork` (see 08-pm-srv-fork.md §1.5):
/// - `parent_ep != RS → EPERM` (159-160)
/// - `IN_USE|PRIV_PROC|DELAY_CALL` (199-200, retain PRIV_PROC) vs `IN_USE|DELAY_CALL|TAINTED`
/// - Credentials injected from `params` (206-211) vs inherited
/// - `VFS_PM_SRV_FORK` `REUID/REGID = uid/gid` (227-228) vs `-1`
/// - Immediate `reply(child, OK)` + `Ok(pid)` (237/239) vs `SUSPEND`
pub fn handle_srv_fork<T: IpcTransport>(
    table: &mut ProcTable,
    parent_endpoint: Endpoint,
    params: SrvForkParams,
    transport: &mut T,
) -> Result<i32, ForkCoordError> {
    // 1. RS gate (forkexit.c:159-160)
    if parent_endpoint != Endpoint::RS {
        return Err(ForkCoordError::NotPermitted);
    }
    let parent_slot = find_parent_slot(table, parent_endpoint)?;
    let parent = &table.procs[parent_slot];
    let is_root = match &parent.resources.privilege {
        Privilege::Kernel => true,
        Privilege::User(c) => c.user.effective == 0,
    };

    // 2. Capacity (162-171)
    if !table.can_alloc_for_user(is_root) {
        return Err(ForkCoordError::ProcTableFull);
    }

    // 3. Find slot (174-181, private static next_child per srv_fork — shared Cell in Rust)
    let child_slot = table.find_free_slot().ok_or(ForkCoordError::ProcTableFull)?;

    // 4. VM fork (183-185)
    let vm_request = VmForkIn {
        parent_endpoint,
        child_slot: UserSlot::new(child_slot),
    };
    let vm_resp = send_vm_fork(vm_request).map_err(|_| ForkCoordError::VmError)?;
    let child_endpoint = vm_resp.child_endpoint;
    debug_assert_eq!(child_endpoint.slot() as usize, child_slot);

    // 5. Occupy + srv copy (187-216) — retain PRIV_PROC, inject uid/gid
    table.procs_in_use.set(table.procs_in_use.get() + 1);
    let child_pid;
    {
        // Use mproc's srv_fork_from for explicit construction
        let child = crate::mproc::Process::srv_fork_from(
            &table.procs[parent_slot],
            child_slot,
            0, // placeholder, will be overwritten after get_free_pid
            child_endpoint,
            parent_slot,
            params,
        );
        table.procs[child_slot] = child;
    }
    child_pid = table.pid_generator.get_free_pid(table);
    table.procs[child_slot].identity.id.pid = child_pid;

    // 6. VFS srv fork (222-230, REUID/REGID = uid/gid)
    let vfs_call = VfsCall::SrvFork {
        child: child_endpoint,
        parent: parent_endpoint,
        child_pid,
        reuid: params.uid as i32,
        regid: params.gid as i32,
    };
    tell_vfs(table, UserSlot::new(child_slot), vfs_call, transport)
        .map_err(|_| ForkCoordError::VfsError)?;

    // 7. Tracer SIGSTOP (232-234, DEFERRED)
    {
        let tracer = table.procs[child_slot].state.guardianship.tracer();
        if tracer.is_some() {
            // DEFERRED: sig_proc SIGSTOP — see 11-signal-core.md
        }
    }

    // 8. Immediate reply to child (237) + return pid (239) — not SUSPEND
    let ok_msg = minix_types::Message {
        m_type: OK,
        ..Default::default()
    };
    // Child is now Running with VFS_CALL still set; reply does not clear VFS_CALL
    // (VFS reply 0x988 is empty, 05 handle_vfs_reply SrvFork → {} )
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

    {
        let parent = &table.procs[parent_slot];
        procgrp = parent.procgrp();
        credentials = parent.resources.privilege.credentials().cloned();
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
    }

    // Now modify child
    let child = &mut table.procs[child_slot];

    // Copy identity fields
    child.identity.id.pid = child_pid;
    child.identity.id.index = UserSlot::new(child_slot);
    child.identity.endpoint = child_endpoint;
    child.identity.procgrp = procgrp;

    // Copy parent relationship
    child.state.guardianship = crate::mproc::Guardianship::Normal {
        parent: UserSlot::new(parent_slot),
    };

    // Copy credentials
    if let Some(creds) = credentials {
        child.resources.privilege = crate::mproc::Privilege::User(creds);
    }

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
    VfsError,
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
            ForkCoordError::VfsError => PmError::InternalError,
            ForkCoordError::KernelError => PmError::InternalError,
        }
    }
}

#[cfg(test)]
mod tests {
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
    fn test_handle_fork_success() {
        let mut table = create_test_table_with_parent();
        let mut transport = crate::ipc::TestIpcTransport::default();

        let result = handle_fork(
            &mut table,
            Endpoint::from_generation_slot(1, 0),
            &mut transport,
        );
        assert!(result.is_ok());

        let child_pid = result.unwrap();
        assert!(child_pid > 0);
    }

    #[test]
    fn test_handle_fork_parent_not_found() {
        let mut table = ProcTable::new();
        let mut transport = crate::ipc::TestIpcTransport::default();

        let result = handle_fork(
            &mut table,
            Endpoint::from_generation_slot(1, 0),
            &mut transport,
        );
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), ForkCoordError::InvalidEndpoint);
    }

    #[test]
    fn test_handle_srv_fork_eperm() {
        // Non-RS parent → EPERM
        let mut table = ProcTable::new();
        table.procs[5].identity.endpoint = Endpoint::from_generation_slot(1, 5);
        table.procs[5].identity.id.pid = 100;
        table.procs[5].state.lifecycle = Lifecycle::Running;
        let mut transport = crate::ipc::TestIpcTransport::default();
        let params = crate::mproc::SrvForkParams { uid: 0, gid: 0 };
        let result = handle_srv_fork(
            &mut table,
            Endpoint::from_generation_slot(1, 5),
            params,
            &mut transport,
        );
        assert_eq!(result.unwrap_err(), ForkCoordError::NotPermitted);
    }

    #[test]
    fn test_handle_srv_fork_success() {
        let mut table = ProcTable::new();
        // RS at slot 2
        table.procs[2].identity.endpoint = Endpoint::RS;
        table.procs[2].identity.id.pid = 2;
        table.procs[2].state.lifecycle = Lifecycle::Running;
        table.procs[2].resources.privilege = crate::mproc::Privilege::Kernel;
        table.procs[2].resources.scheduler = Endpoint::NONE;

        let mut transport = crate::ipc::TestIpcTransport::default();
        let params = crate::mproc::SrvForkParams { uid: 1000, gid: 100 };
        let result = handle_srv_fork(&mut table, Endpoint::RS, params, &mut transport);
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
        assert_eq!(child.resources.privilege.credentials().unwrap().user.real, 1000);
    }

    #[test]
    fn test_handle_srv_fork_vfs_call() {
        let mut table = ProcTable::new();
        table.procs[2].identity.endpoint = Endpoint::RS;
        table.procs[2].identity.id.pid = 2;
        table.procs[2].state.lifecycle = Lifecycle::Running;
        table.procs[2].resources.privilege = crate::mproc::Privilege::Kernel;
        let mut transport = crate::ipc::TestIpcTransport::default();
        let params = crate::mproc::SrvForkParams { uid: 42, gid: 43 };
        let _ = handle_srv_fork(&mut table, Endpoint::RS, params, &mut transport).unwrap();
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
}
