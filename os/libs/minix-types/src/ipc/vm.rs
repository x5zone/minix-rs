//! VM service IPC message types.
//!
//! Defines the messages exchanged between VM and other services (PM, VFS, RS, Kernel).

use crate::{Endpoint, UserSlot, VirBytes, ESRCH, EINVAL, ENOMEM, EFAULT, EPERM, EIO, ENOSYS, EACCES};

// ============================================================================
// VM Call Numbers
// ============================================================================
// These are the message type numbers for VM requests.
// Defined in Minix3: minix/include/minix/com.h

/// Base value for VM request message types.
pub const VM_RQ_BASE: u32 = 0xC00;

// --- PM calls ---

/// Exit process. Sent by PM when a process exits.
pub const VM_EXIT: u32 = VM_RQ_BASE + 0;

/// Fork process. Sent by PM when a process forks.
pub const VM_FORK: u32 = VM_RQ_BASE + 1;

/// Change heap break. Sent by PM for brk() syscall.
pub const VM_BRK: u32 = VM_RQ_BASE + 2;

/// Exec new memory. Sent by PM for exec() syscall.
pub const VM_EXEC_NEWMEM: u32 = VM_RQ_BASE + 3;

/// Process will exit. Sent by PM before exit cleanup.
pub const VM_WILLEXIT: u32 = VM_RQ_BASE + 5;

// --- General calls ---

/// Memory map. mmap() syscall.
pub const VM_MMAP: u32 = VM_RQ_BASE + 10;

/// Add DMA memory.
pub const VM_ADDDMA: u32 = VM_RQ_BASE + 12;

/// Delete DMA memory.
pub const VM_DELDMA: u32 = VM_RQ_BASE + 13;

/// Get DMA memory.
pub const VM_GETDMA: u32 = VM_RQ_BASE + 14;

/// Map physical memory. Used by system services.
pub const VM_MAP_PHYS: u32 = VM_RQ_BASE + 15;

/// Unmap physical memory.
pub const VM_UNMAP_PHYS: u32 = VM_RQ_BASE + 16;

/// Memory unmap. munmap() syscall.
pub const VM_MUNMAP: u32 = VM_RQ_BASE + 17;

/// Map cache page.
pub const VM_MAPCACHEPAGE: u32 = VM_RQ_BASE + 26;

/// Set cache page.
pub const VM_SETCACHEPAGE: u32 = VM_RQ_BASE + 27;

/// Forget cache page.
pub const VM_FORGETCACHEPAGE: u32 = VM_RQ_BASE + 28;

/// Clear cache.
pub const VM_CLEARCACHE: u32 = VM_RQ_BASE + 29;

/// Remap memory.
pub const VM_REMAP: u32 = VM_RQ_BASE + 33;

/// Shared memory unmap.
pub const VM_SHM_UNMAP: u32 = VM_RQ_BASE + 34;

/// Get physical address.
pub const VM_GETPHYS: u32 = VM_RQ_BASE + 35;

/// Get reference count.
pub const VM_GETREF: u32 = VM_RQ_BASE + 36;

/// Get VM info.
pub const VM_INFO: u32 = VM_RQ_BASE + 40;

/// Remap read-only.
pub const VM_REMAP_RO: u32 = VM_RQ_BASE + 44;

/// Process control (VFS).
pub const VM_PROCCTL: u32 = VM_RQ_BASE + 45;

/// VFS mmap.
pub const VM_VFS_MMAP: u32 = VM_RQ_BASE + 46;

/// Get resource usage.
pub const VM_GETRUSAGE: u32 = VM_RQ_BASE + 47;

// --- RS calls ---

/// RS set privileges.
pub const VM_RS_SET_PRIV: u32 = VM_RQ_BASE + 37;

/// RS update.
pub const VM_RS_UPDATE: u32 = VM_RQ_BASE + 41;

/// RS memory control.
pub const VM_RS_MEMCTL: u32 = VM_RQ_BASE + 42;

/// RS prepare. Used when a system service starts.
pub const VM_RS_PREPARE: u32 = VM_RQ_BASE + 48;

/// Total number of VM calls.
pub const NR_VM_CALLS: u32 = 49;

// --- Special calls ---

/// VFS reply (asynchronous).
pub const VM_VFS_REPLY: u32 = VM_RQ_BASE + 30;

/// Page fault. Sent by kernel, not a normal request.
pub const VM_PAGEFAULT: u32 = VM_RQ_BASE + 0xFF;

// ============================================================================
// VM Message Types
// ============================================================================

/// VM request message types.
///
/// These are the requests that VM receives from other services.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VmRequest {
    /// Fork request from PM.
    ///
    /// PM sends this when a process calls fork(). VM must duplicate
    /// the parent's address space to the child slot.
    Fork {
        /// Parent process endpoint.
        parent_endpoint: Endpoint,
        /// Child process slot (allocated by PM).
        child_slot: UserSlot,
        /// Child process endpoint (assigned by kernel).
        child_endpoint: Endpoint,
    },

    /// Brk request - change process heap break address.
    ///
    /// Sent by the process directly (via syscall) or by PM.
    /// VM adjusts the heap region to the new break address.
    Brk {
        /// Process endpoint requesting the brk change.
        endpoint: Endpoint,
        /// New heap break address.
        new_addr: VirBytes,
    },

    /// Munmap request - unmap a virtual address range.
    ///
    /// Sent by the process to release a memory mapping.
    Munmap {
        /// Process endpoint requesting the unmap.
        endpoint: Endpoint,
        /// Start address of the range to unmap.
        addr: VirBytes,
        /// Length of the range to unmap.
        length: VirBytes,
    },

    /// Exit request - process is exiting.
    ///
    /// Sent by PM when a process exits. VM releases all process resources.
    Exit {
        /// Endpoint of the exiting process.
        endpoint: Endpoint,
    },

    /// Willexit request - process will exit soon.
    ///
    /// Sent by PM before exit cleanup. VM marks the process as exiting.
    Willexit {
        /// Endpoint of the process that will exit.
        endpoint: Endpoint,
    },

    /// Page fault notification from kernel.
    ///
    /// Kernel sends this when a process accesses an unmapped or protected page.
    Pagefault {
        /// Endpoint of the faulting process.
        endpoint: Endpoint,
        /// Virtual address that caused the fault.
        vaddr: VirBytes,
        /// Whether the fault was caused by a write access.
        write: bool,
    },

    /// Exec newmem request - set up new address space for exec.
    ///
    /// Sent by PM during exec() to replace the process address space.
    ExecNewmem {
        /// Endpoint of the process executing exec.
        endpoint: Endpoint,
        /// Text segment start address.
        text_addr: VirBytes,
        /// Text segment length.
        text_len: VirBytes,
        /// Data segment start address.
        data_addr: VirBytes,
        /// Data segment length.
        data_len: VirBytes,
        /// Program counter / entry point.
        pc: VirBytes,
    },
}

/// VM response message types.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VmResponse {
    /// Fork succeeded.
    ForkOk {
        /// Child process endpoint.
        child_endpoint: Endpoint,
    },
    /// Brk succeeded.
    BrkOk {
        /// New heap break address.
        new_addr: VirBytes,
    },
    /// Munmap succeeded.
    MunmapOk,
    /// Exit succeeded.
    ExitOk,
    /// Willexit succeeded.
    WillexitOk,
    /// Pagefault handled.
    PagefaultOk,
    /// Exec newmem succeeded.
    ExecNewmemOk,
    /// Operation failed.
    Error(VmError),
}

/// VM error types.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VmError {
    /// Process not found (invalid endpoint).
    InvalidEndpoint,
    /// Slot is already in use.
    SlotInUse,
    /// Out of memory.
    OutOfMemory,
    /// Invalid address.
    InvalidAddress,
    /// Permission denied.
    PermissionDenied,
    /// Access violation (e.g., write to read-only page).
    AccessViolation,
    /// Page table operation failed.
    PageTableError,
    /// Internal error.
    InternalError,
    /// Operation not implemented.
    NotImplemented,
}

impl VmError {
    /// Converts error to errno value.
    pub fn to_errno(&self) -> i32 {
        match self {
            Self::InvalidEndpoint => ESRCH,
            Self::SlotInUse => EINVAL,
            Self::OutOfMemory => ENOMEM,
            Self::InvalidAddress => EFAULT,
            Self::PermissionDenied => EPERM,
            Self::AccessViolation => EACCES,
            Self::PageTableError => EIO,
            Self::InternalError => EIO,
            Self::NotImplemented => ENOSYS,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vm_fork_request() {
        let req = VmRequest::Fork {
            parent_endpoint: Endpoint::PM,
            child_slot: UserSlot::new(1),
            child_endpoint: Endpoint::from_generation_slot(1, 1),
        };

        match req {
            VmRequest::Fork { parent_endpoint, child_slot, child_endpoint } => {
                assert_eq!(parent_endpoint, Endpoint::PM);
                assert_eq!(child_slot.get(), 1);
                assert_eq!(child_endpoint.slot(), 1);
            }
            _ => panic!("expected Fork"),
        }
    }

    #[test]
    fn test_vm_brk_request() {
        let req = VmRequest::Brk {
            endpoint: Endpoint::PM,
            new_addr: VirBytes(0x4000_0000),
        };

        match req {
            VmRequest::Brk { endpoint, new_addr } => {
                assert_eq!(endpoint, Endpoint::PM);
                assert_eq!(new_addr.0, 0x4000_0000);
            }
            _ => panic!("expected Brk"),
        }
    }

    #[test]
    fn test_vm_munmap_request() {
        let req = VmRequest::Munmap {
            endpoint: Endpoint::PM,
            addr: VirBytes(0x1000),
            length: VirBytes(0x2000),
        };

        match req {
            VmRequest::Munmap { endpoint, addr, length } => {
                assert_eq!(endpoint, Endpoint::PM);
                assert_eq!(addr.0, 0x1000);
                assert_eq!(length.0, 0x2000);
            }
            _ => panic!("expected Munmap"),
        }
    }

    #[test]
    fn test_vm_exit_request() {
        let req = VmRequest::Exit {
            endpoint: Endpoint::PM,
        };

        match req {
            VmRequest::Exit { endpoint } => {
                assert_eq!(endpoint, Endpoint::PM);
            }
            _ => panic!("expected Exit"),
        }
    }

    #[test]
    fn test_vm_pagefault_request() {
        let req = VmRequest::Pagefault {
            endpoint: Endpoint::PM,
            vaddr: VirBytes(0x1000),
            write: true,
        };

        match req {
            VmRequest::Pagefault { endpoint, vaddr, write } => {
                assert_eq!(endpoint, Endpoint::PM);
                assert_eq!(vaddr.0, 0x1000);
                assert!(write);
            }
            _ => panic!("expected Pagefault"),
        }
    }

    #[test]
    fn test_vm_exec_newmem_request() {
        let req = VmRequest::ExecNewmem {
            endpoint: Endpoint::PM,
            text_addr: VirBytes(0x1000),
            text_len: VirBytes(0x2000),
            data_addr: VirBytes(0x4000),
            data_len: VirBytes(0x1000),
            pc: VirBytes(0x1000),
        };

        match req {
            VmRequest::ExecNewmem { endpoint, text_addr, text_len, data_addr, data_len, pc } => {
                assert_eq!(endpoint, Endpoint::PM);
                assert_eq!(text_addr.0, 0x1000);
                assert_eq!(text_len.0, 0x2000);
                assert_eq!(data_addr.0, 0x4000);
                assert_eq!(data_len.0, 0x1000);
                assert_eq!(pc.0, 0x1000);
            }
            _ => panic!("expected ExecNewmem"),
        }
    }

    #[test]
    fn test_vm_response_fork_ok() {
        let resp = VmResponse::ForkOk {
            child_endpoint: Endpoint::from_generation_slot(1, 1),
        };

        match resp {
            VmResponse::ForkOk { child_endpoint } => {
                assert_eq!(child_endpoint.slot(), 1);
            }
            _ => panic!("expected ForkOk"),
        }
    }

    #[test]
    fn test_vm_response_brk_ok() {
        let resp = VmResponse::BrkOk {
            new_addr: VirBytes(0x4000_1000),
        };

        match resp {
            VmResponse::BrkOk { new_addr } => {
                assert_eq!(new_addr.0, 0x4000_1000);
            }
            _ => panic!("expected BrkOk"),
        }
    }

    #[test]
    fn test_vm_error_to_errno() {
        assert_eq!(VmError::InvalidEndpoint.to_errno(), ESRCH);
        assert_eq!(VmError::OutOfMemory.to_errno(), ENOMEM);
        assert_eq!(VmError::NotImplemented.to_errno(), ENOSYS);
        assert_eq!(VmError::AccessViolation.to_errno(), EACCES);
    }
}
