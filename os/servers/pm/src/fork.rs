//! Fork system call implementation.
//!
//! Implements core logic of fork system call for PM server.

use minix_types::{Endpoint, UserSlot, PmError, VmForkIn, VfsRequest, KernelRequest};
use crate::mproc::{ProcTable, Lifecycle};
use crate::ipc::{send_vm_fork, send_vfs_request, send_kernel_request};

/// Forks a process.
///
/// This is the main entry point for fork system call handling.
/// Coordinates with VM, VFS, and Kernel to create a child process.
///
/// # Arguments
/// * `table` - PM process table
/// * `parent_endpoint` - Parent process endpoint
///
/// # Returns
/// * `Ok(child_pid)` - Child process PID (returned in parent)
/// * `Err(e)` - Error
pub fn handle_fork(
    table: &mut ProcTable,
    parent_endpoint: Endpoint,
) -> Result<i32, ForkCoordError> {
    // 1. Find parent process
    let parent_slot = find_parent_slot(table, parent_endpoint)?;

    // 2. Allocate child slot
    let child_slot = table.alloc_slot()
        .ok_or(ForkCoordError::ProcTableFull)?;

    // 3. Generate child PID
    let child_pid = table.pid_generator.get_free_pid(table);

    // 4. Generate child endpoint
    let child_endpoint = Endpoint::from_generation_slot(1, child_slot as i32);

    // 5. Request VM to copy address space
    let vm_request = VmForkIn {
        parent_endpoint,
        child_slot: UserSlot::new(child_slot),
    };
    send_vm_fork(vm_request)?;

    // 6. Request VFS to copy file descriptors
    let vfs_request = VfsRequest::Fork {
        parent_endpoint,
        child_endpoint,
    };
    send_vfs_request(vfs_request)?;

    // 7. Request Kernel to create process structure
    let kernel_request = KernelRequest::Fork {
        parent_endpoint,
        child_endpoint,
        child_slot: UserSlot::new(child_slot),
    };
    send_kernel_request(kernel_request)?;

    // 8. Copy mproc fields
    copy_mproc(table, parent_slot, child_slot, child_pid, child_endpoint);

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
const FORK_INHERIT_FLAGS: crate::mproc::RemainingFlags = crate::mproc::RemainingFlags::from_bits_truncate(
    crate::mproc::RemainingFlags::DELAY_CALL.bits()
    | crate::mproc::RemainingFlags::TAINTED.bits()
);

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
        scheduler = parent.resources.scheduler;
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

        let result = handle_fork(&mut table, Endpoint::from_generation_slot(1, 0));
        assert!(result.is_ok());

        let child_pid = result.unwrap();
        assert!(child_pid > 0);
    }

    #[test]
    fn test_handle_fork_parent_not_found() {
        let mut table = ProcTable::new();

        let result = handle_fork(&mut table, Endpoint::from_generation_slot(1, 0));
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), ForkCoordError::InvalidEndpoint);
    }
}
