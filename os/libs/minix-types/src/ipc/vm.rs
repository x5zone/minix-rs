//! VM service IPC message types.
//!
//! # Architecture: Transport Layer + Semantic Layer
//!
//! Minix3 uses a 56-byte `message` union for all IPC. Each message type selects
//! a sub-format (e.g. `mess_1` for VM_FORK) and uses `#define` macros to name
//! fields within that sub-format. There is no per-message struct.
//!
//! This module provides two layers:
//!
//! **Transport Layer** (`message.rs`):
//! `Message`, `MessageM1` etc. — `#[repr(C)]` binary-compatible with
//! Minix3's `message` union. Zero-cost reinterpret.
//!
//! **Semantic Layer** (this file):
//! Per-link `In`/`Out` types (e.g. `VmForkIn`, `VmForkOut`) —
//! type-safe views decoded from the transport layer. All fields
//! are annotated with the corresponding Minix3 `#define` macro.
//!
//! # Codec
//!
//! `DecodeFromM1` / `EncodeToM1` traits connect the two layers.
//! Implementations are `#[inline(always)]` and compile to the same
//! machine code as the C macros (single `mov` instructions).
//!
//! # Naming Convention
//!
//! - `VmXxxIn`  = request received by VM (e.g. PM→VM: VM_FORK)
//! - `VmXxxOut` = reply sent by VM (e.g. VM→PM: VMF_CHILD_ENDPOINT)
//! - `VmReply`  = unified reply enum (for dispatcher return type)
//! - `VmError`  = shared error type (maps to errno)

use crate::{Endpoint, UserSlot, VirBytes, ESRCH, EINVAL, ENOMEM, EFAULT, EPERM, EIO, ENOSYS, EACCES};
use crate::ipc::MessageM1;

// ============================================================================
// VM Call Numbers
// ============================================================================
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
// Semantic Layer — Per-Link IPC Types
// ============================================================================

// ---------------------------------------------------------------------------
// VM_FORK  (PM → VM, VM → PM)
// ---------------------------------------------------------------------------
// C definitions:
//   #define VMF_ENDPOINT        m1_i1   // parent endpoint
//   #define VMF_SLOTNO          m1_i2   // child slot
//   #define VMF_CHILD_ENDPOINT  m1_i3   // result: child endpoint

/// PM → VM: fork request.
///
/// Corresponds to Minix3 `VMF_ENDPOINT` / `VMF_SLOTNO` in `mess_1`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VmForkIn {
    pub parent_endpoint: Endpoint,
    pub child_slot: UserSlot,
}

/// VM → PM: fork reply.
///
/// Corresponds to Minix3 `VMF_CHILD_ENDPOINT` in `mess_1`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VmForkOut {
    pub child_endpoint: Endpoint,
}

// ---------------------------------------------------------------------------
// VM_BRK  (PM → VM, VM → PM)
// ---------------------------------------------------------------------------

/// PM → VM: brk request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VmBrkIn {
    pub endpoint: Endpoint,
    pub new_addr: VirBytes,
}

/// VM → PM: brk reply.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VmBrkOut {
    pub new_addr: VirBytes,
}

// ---------------------------------------------------------------------------
// VM_MUNMAP  (PM → VM)
// ---------------------------------------------------------------------------

/// PM → VM: munmap request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VmMunmapIn {
    pub endpoint: Endpoint,
    pub addr: VirBytes,
    pub length: VirBytes,
}

// ---------------------------------------------------------------------------
// VM_EXIT  (PM → VM)
// ---------------------------------------------------------------------------
// C: #define VME_ENDPOINT m1_i1

/// PM → VM: exit request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VmExitIn {
    pub endpoint: Endpoint,
}

// ---------------------------------------------------------------------------
// VM_WILLEXIT  (PM → VM)
// ---------------------------------------------------------------------------
// C: #define VMWE_ENDPOINT m1_i1

/// PM → VM: willexit request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VmWillexitIn {
    pub endpoint: Endpoint,
}

// ---------------------------------------------------------------------------
// VM_PAGEFAULT  (Kernel → VM)
// ---------------------------------------------------------------------------
// No reply — caller is unblocked by sys_vmctl on success, kernel panics on
// failure. See Minix3 main.c: "do not reply to this call".

/// Kernel → VM: page fault notification.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VmPagefaultIn {
    pub endpoint: Endpoint,
    pub vaddr: VirBytes,
    pub write: bool,
}

// ---------------------------------------------------------------------------
// VM_EXEC_NEWMEM  (PM → VM, VM → PM)
// ---------------------------------------------------------------------------
// C: #define VMEN_ENDPOINT  m1_i1
//     #define VMEN_ARGSPTR   m1_p1
//     #define VMEN_ARGSSIZE  m1_i2
//     #define VMEN_FLAGS     m1_i3    (result)
//     #define VMEN_STACK_TOP m1_p2    (result)

/// PM → VM: exec newmem request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VmExecNewmemIn {
    pub endpoint: Endpoint,
    pub text_addr: VirBytes,
    pub text_len: VirBytes,
    pub data_addr: VirBytes,
    pub data_len: VirBytes,
    pub pc: VirBytes,
}

/// VM → PM: exec newmem reply.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VmExecNewmemOut {
    pub flags: i32,
    pub stack_top: VirBytes,
}

// ============================================================================
// Unified Reply Type (for dispatcher return value)
// ============================================================================

/// VM reply — wraps each link's Out type or an error.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VmReply {
    Fork(VmForkOut),
    Brk(VmBrkOut),
    Munmap,
    Exit,
    Willexit,
    ExecNewmem(VmExecNewmemOut),
    Error(VmError),
}

// ============================================================================
// Error Type
// ============================================================================

/// VM error types.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VmError {
    InvalidEndpoint,
    SlotInUse,
    OutOfMemory,
    InvalidAddress,
    PermissionDenied,
    AccessViolation,
    PageNotMapped,
    MemType,
    PageTableError,
    InternalError,
    NotImplemented,
}

impl VmError {
    pub fn to_errno(&self) -> i32 {
        match self {
            Self::InvalidEndpoint => ESRCH,
            Self::SlotInUse => EINVAL,
            Self::OutOfMemory => ENOMEM,
            Self::InvalidAddress => EFAULT,
            Self::PermissionDenied => EPERM,
            Self::AccessViolation => EACCES,
            Self::PageNotMapped => EFAULT,
            Self::MemType => EIO,
            Self::PageTableError => EIO,
            Self::InternalError => EIO,
            Self::NotImplemented => ENOSYS,
        }
    }
}

// ============================================================================
// Codec Traits — Transport ↔ Semantic
// ============================================================================

/// Decode from `MessageM1` (mess_1 format).
///
/// All implementations are `#[inline(always)]` — compiles to the same
/// `mov` instructions as the C `#define` macros.
pub trait DecodeFromM1: Sized {
    fn decode(m1: &MessageM1) -> Self;
}

/// Encode into `MessageM1` (mess_1 format).
///
/// All implementations are `#[inline(always)]` — compiles to the same
/// store instructions as the C `#define` macros.
pub trait EncodeToM1 {
    fn encode(&self, m1: &mut MessageM1);
}

impl DecodeFromM1 for VmForkIn {
    #[inline(always)]
    fn decode(m1: &MessageM1) -> Self {
        Self {
            parent_endpoint: Endpoint(m1.m1i1),
            child_slot: UserSlot(m1.m1i2 as usize),
        }
    }
}

impl EncodeToM1 for VmForkOut {
    #[inline(always)]
    fn encode(&self, m1: &mut MessageM1) {
        m1.m1i3 = self.child_endpoint.0;
    }
}

impl DecodeFromM1 for VmBrkIn {
    #[inline(always)]
    fn decode(m1: &MessageM1) -> Self {
        Self {
            endpoint: Endpoint(m1.m1i1),
            new_addr: VirBytes(m1.m1p1),
        }
    }
}

impl EncodeToM1 for VmBrkOut {
    #[inline(always)]
    fn encode(&self, m1: &mut MessageM1) {
        m1.m1i1 = self.new_addr.0 as i32;
    }
}

impl DecodeFromM1 for VmMunmapIn {
    #[inline(always)]
    fn decode(m1: &MessageM1) -> Self {
        Self {
            endpoint: Endpoint(m1.m1i1),
            addr: VirBytes(m1.m1p1),
            length: VirBytes(m1.m1p2),
        }
    }
}

impl DecodeFromM1 for VmExitIn {
    #[inline(always)]
    fn decode(m1: &MessageM1) -> Self {
        Self {
            endpoint: Endpoint(m1.m1i1),
        }
    }
}

impl DecodeFromM1 for VmWillexitIn {
    #[inline(always)]
    fn decode(m1: &MessageM1) -> Self {
        Self {
            endpoint: Endpoint(m1.m1i1),
        }
    }
}

impl DecodeFromM1 for VmPagefaultIn {
    #[inline(always)]
    fn decode(m1: &MessageM1) -> Self {
        Self {
            endpoint: Endpoint(m1.m1i1),
            vaddr: VirBytes(m1.m1p1),
            write: m1.m1i2 != 0,
        }
    }
}

impl DecodeFromM1 for VmExecNewmemIn {
    #[inline(always)]
    fn decode(m1: &MessageM1) -> Self {
        Self {
            endpoint: Endpoint(m1.m1i1),
            text_addr: VirBytes(m1.m1p1),
            text_len: VirBytes(m1.m1i2 as u64),
            data_addr: VirBytes(m1.m1p2),
            data_len: VirBytes(m1.m1i3 as u64),
            pc: VirBytes(m1.m1p3),
        }
    }
}

impl EncodeToM1 for VmExecNewmemOut {
    #[inline(always)]
    fn encode(&self, m1: &mut MessageM1) {
        m1.m1i3 = self.flags;
        m1.m1p2 = self.stack_top.0;
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vm_fork_in() {
        let req = VmForkIn {
            parent_endpoint: Endpoint::PM,
            child_slot: UserSlot::new(1),
        };
        assert_eq!(req.parent_endpoint, Endpoint::PM);
        assert_eq!(req.child_slot.get(), 1);
    }

    #[test]
    fn test_vm_fork_out() {
        let out = VmForkOut {
            child_endpoint: Endpoint::from_generation_slot(1, 1),
        };
        assert_eq!(out.child_endpoint.slot(), 1);
    }

    #[test]
    fn test_vm_fork_codec_roundtrip() {
        let mut m1 = MessageM1::default();
        let req = VmForkIn {
            parent_endpoint: Endpoint::PM,
            child_slot: UserSlot::new(5),
        };

        m1.m1i1 = req.parent_endpoint.0;
        m1.m1i2 = req.child_slot.get() as i32;

        let decoded = VmForkIn::decode(&m1);
        assert_eq!(decoded.parent_endpoint, Endpoint::PM);
        assert_eq!(decoded.child_slot.get(), 5);

        let out = VmForkOut {
            child_endpoint: Endpoint::from_generation_slot(2, 7),
        };
        out.encode(&mut m1);
        assert_eq!(m1.m1i3, Endpoint::from_generation_slot(2, 7).0);
    }

    #[test]
    fn test_vm_brk_in() {
        let req = VmBrkIn {
            endpoint: Endpoint::PM,
            new_addr: VirBytes(0x4000_0000),
        };
        assert_eq!(req.endpoint, Endpoint::PM);
        assert_eq!(req.new_addr.0, 0x4000_0000);
    }

    #[test]
    fn test_vm_brk_out() {
        let out = VmBrkOut {
            new_addr: VirBytes(0x4000_1000),
        };
        assert_eq!(out.new_addr.0, 0x4000_1000);
    }

    #[test]
    fn test_vm_munmap_in() {
        let req = VmMunmapIn {
            endpoint: Endpoint::PM,
            addr: VirBytes(0x1000),
            length: VirBytes(0x2000),
        };
        assert_eq!(req.endpoint, Endpoint::PM);
        assert_eq!(req.addr.0, 0x1000);
        assert_eq!(req.length.0, 0x2000);
    }

    #[test]
    fn test_vm_exit_in() {
        let req = VmExitIn { endpoint: Endpoint::PM };
        assert_eq!(req.endpoint, Endpoint::PM);
    }

    #[test]
    fn test_vm_willexit_in() {
        let req = VmWillexitIn { endpoint: Endpoint::PM };
        assert_eq!(req.endpoint, Endpoint::PM);
    }

    #[test]
    fn test_vm_pagefault_in() {
        let req = VmPagefaultIn {
            endpoint: Endpoint::PM,
            vaddr: VirBytes(0x1000),
            write: true,
        };
        assert_eq!(req.endpoint, Endpoint::PM);
        assert_eq!(req.vaddr.0, 0x1000);
        assert!(req.write);
    }

    #[test]
    fn test_vm_exec_newmem_in() {
        let req = VmExecNewmemIn {
            endpoint: Endpoint::PM,
            text_addr: VirBytes(0x1000),
            text_len: VirBytes(0x2000),
            data_addr: VirBytes(0x4000),
            data_len: VirBytes(0x1000),
            pc: VirBytes(0x1000),
        };
        assert_eq!(req.endpoint, Endpoint::PM);
        assert_eq!(req.text_addr.0, 0x1000);
        assert_eq!(req.text_len.0, 0x2000);
        assert_eq!(req.data_addr.0, 0x4000);
        assert_eq!(req.data_len.0, 0x1000);
        assert_eq!(req.pc.0, 0x1000);
    }

    #[test]
    fn test_vm_exec_newmem_out() {
        let out = VmExecNewmemOut {
            flags: 1,
            stack_top: VirBytes(0x7FFF_0000),
        };
        assert_eq!(out.flags, 1);
        assert_eq!(out.stack_top.0, 0x7FFF_0000);
    }

    #[test]
    fn test_vm_reply_fork() {
        let reply = VmReply::Fork(VmForkOut {
            child_endpoint: Endpoint::from_generation_slot(1, 1),
        });
        match reply {
            VmReply::Fork(out) => assert_eq!(out.child_endpoint.slot(), 1),
            _ => panic!("expected Fork"),
        }
    }

    #[test]
    fn test_vm_reply_brk() {
        let reply = VmReply::Brk(VmBrkOut {
            new_addr: VirBytes(0x4000_1000),
        });
        match reply {
            VmReply::Brk(out) => assert_eq!(out.new_addr.0, 0x4000_1000),
            _ => panic!("expected Brk"),
        }
    }

    #[test]
    fn test_vm_reply_error() {
        let reply = VmReply::Error(VmError::InvalidEndpoint);
        match reply {
            VmReply::Error(e) => assert_eq!(e.to_errno(), ESRCH),
            _ => panic!("expected Error"),
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
