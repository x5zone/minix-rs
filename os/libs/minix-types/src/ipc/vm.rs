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

use crate::ipc::MessageM1;
use crate::{
    EACCES, EFAULT, EINVAL, EIO, ENOENT, ENOMEM, ENOSYS, EPERM, ESRCH, Endpoint, PhysBytes,
    UserSlot, VirBytes,
};

// ============================================================================
// VM Call Numbers
// ============================================================================
// Defined in Minix3: minix/include/minix/com.h

/// Base value for VM request message types.
pub const VM_RQ_BASE: u32 = 0xC00;

// --- PM calls ---

/// Exit process. Sent by PM when a process exits.
pub const VM_EXIT: u32 = VM_RQ_BASE;

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
// VM_MMAP  (PM → VM, VM → PM)
// ---------------------------------------------------------------------------
// C: mess_mmap (ipc.h:1575)
//     #define VMUM_ADDR  m_mmap.addr
//     #define VMUM_LEN   m_mmap.len

/// PM → VM: mmap request.
/// Fields correspond 1:1 to Minix3 `mess_mmap`, plus `caller`
/// which is derived from the IPC source endpoint (m_source).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VmMmapIn {
    pub caller: Endpoint,
    pub forwhom: Endpoint,
    pub addr: VirBytes,
    pub length: VirBytes,
    pub prot: u32,
    pub flags: u32,
    pub fd: i32,
    pub offset: u64,
}

/// VM → PM: mmap reply.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VmMmapOut {
    pub ret_addr: VirBytes,
}

// ---------------------------------------------------------------------------
// VM_MAP_PHYS  (PM → VM, VM → PM)
// ---------------------------------------------------------------------------
// C: mess_lsys_vm_map_phys (ipc.h:1498)

/// PM → VM: map physical memory request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VmMapPhysIn {
    pub caller: Endpoint,
    pub target: Endpoint,
    pub phys_addr: PhysBytes,
    pub length: VirBytes,
}

/// VM → PM: map physical memory reply.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VmMapPhysOut {
    pub virt_addr: VirBytes,
}

// ---------------------------------------------------------------------------
// VM_VFS_MMAP  (VFS → VM, synchronous)
// ---------------------------------------------------------------------------
// C: mess_vm_vfs_mmap (ipc.h:2367)
//     VM_VFS_MMAP = VM_RQ_BASE+46

/// VFS → VM: VFS-initiated file mapping request (synchronous).
/// Used by VFS to map file contents into process address space
/// (e.g. ld.so loading shared libraries).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VmVfsMmapIn {
    pub who: Endpoint,
    pub fd: i32,
    pub offset: u64,
    pub dev: u64,
    pub ino: u64,
    pub vaddr: VirBytes,
    pub length: VirBytes,
    pub flags: u32,
    pub clearend: u16,
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
// VM_UNMAP_PHYS  (driver → VM)
// ---------------------------------------------------------------------------
// C: mess_lsys_vm_unmap_phys (ipc.h:1521)

/// Driver → VM: unmap physical memory mapping.
/// Unlike VM_MUNMAP, the length is derived from the region found at `vaddr`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VmUnmapPhysIn {
    pub target: Endpoint,
    pub vaddr: VirBytes,
}

// ---------------------------------------------------------------------------
// VM_SHM_UNMAP  (process → VM)
// ---------------------------------------------------------------------------
// C: mess_lc_vm_shm_unmap (ipc.h:935)

/// Process → VM: unmap shared memory mapping.
/// Unlike VM_MUNMAP, the length is derived from the region found at `addr`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VmShmUnmapIn {
    pub forwhom: Endpoint,
    pub addr: VirBytes,
}

// ---------------------------------------------------------------------------
// VM_MAPCACHEPAGE / VM_SETCACHEPAGE / VM_FORGETCACHEPAGE / VM_CLEARCACHE
// (VFS → VM)
// ---------------------------------------------------------------------------
// C: m_vmmcp message fields — shared format for all 4 cache requests.
//
//   #define m2_l1 m_vmmcp.dev         // device number
//   #define m2_l2 m_vmmcp.dev_offset  // device offset (u64)
//   #define m2_l1 m_vmmcp.ino         // inode number
//   #define m2_l2 m_vmmcp.ino_offset  // inode offset (u64)
//   #define m2_i1 m_vmmcp.pages       // number of pages
//   #define m2_i2 m_vmmcp.flags       // flags (VMSF_ONCE)
//   #define m2_p1 m_vmmcp.block       // user-space block ptr (setcache only)

/// VFS → VM: cache operation request.
///
/// Shared by `VM_MAPCACHEPAGE`, `VM_SETCACHEPAGE`, `VM_FORGETCACHEPAGE`,
/// and `VM_CLEARCACHE`. Individual handlers read only the fields they need.
///
/// Corresponds to Minix3 `m_vmmcp` union in `mess_2`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VmCacheIn {
    pub dev: u64,
    pub dev_offset: u64,
    pub ino: u64,
    pub ino_offset: u64,
    pub pages: u32,
    pub flags: u32,
    pub block: u64,
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

// ---------------------------------------------------------------------------
// VM_PROCCTL  (VFS → VM)  — DEFERRED
// ---------------------------------------------------------------------------
// C: mess_lc_vm_procctl (ipc.h, uses m9 layout in C)
//     #define VMPCTL_PARAM  m9_l1   // operation (VMPPARAM_CLEAR/SETMCALL/...)
//     #define VMPCTL_WHO    m9_l2   // target endpoint
//     #define VMPCTL_M1     m9_l3   // user-space pointer (m1 sys call index)
//     #define VMPCTL_LEN    m9_l4   // byte count
//
// In the 64-bit Rust rewrite, m1p1/m1p2/m1p3 occupy offsets 16/24/32 (each
// 8 bytes), which lines up with the C 32-bit long fields at the same
// offsets (m9_l1/l2/l3 start at offset 16 after the two 8-byte m9ull
// fields). We use the M1 pointer fields for the wide ints and M1 integer
// field for the small int (param). See `DecodeFromM1` impl for the exact
// field → C-macro mapping.

/// VFS → VM: process control request.
///
/// 64-bit layout (m1p1/m1p2/m1p3 hold the three 64-bit payload words; m1i1
/// is repurposed for the 32-bit `param` since it is small enough):
///
/// | Rust field | m1 offset | C macro      | Width |
/// |------------|-----------|--------------|-------|
/// | `param`    | 0  (m1i1) | m9_l1        | i32   |
/// | `who`      | 16 (m1p1) | m9_l2        | i64   |
/// | `m1`       | 24 (m1p2) | m9_l3        | u64   |
/// | `len`      | 32 (m1p3) | m9_l4        | i32   |
/// | `flags`    | 12 (m1i3) | m9_l5        | i32   |
///
/// See `dispatch_procctl` in `os/servers/vm/src/ipc/dispatcher.rs` for
/// the full DEFERRED implementation path (5 steps).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VmProcctlIn {
    /// VMPCTL_PARAM — operation code (small int).
    pub param: i32,
    /// VMPCTL_WHO — target endpoint.
    pub who: Endpoint,
    /// VMPCTL_M1 — user-space pointer or extra parameter.
    pub m1: u64,
    /// VMPCTL_LEN — byte count.
    pub len: i32,
    /// VMPCTL_FLAGS — write flag for VMPPARAM_HANDLEMEM.
    pub flags: i32,
}

// ---------------------------------------------------------------------------
// VM_REMAP / VM_REMAP_RO  (PM → VM)  — DEFERRED
// ---------------------------------------------------------------------------
// C: mess_lc_vm_remap uses m7 layout (5 ints + 2 pointers) in C. In the
// 64-bit rewrite we use the m1i* and m1p* fields (same offsets as m7_i1..i5
// for the ints and m7_p1/p2 for the pointers, since both layouts start
// with five 4-byte words then pointers at offset 24/32).

/// PM → VM: remap request.
///
/// Corresponds to Minix3 `do_remap()` in `mmap.c:374`. The `readonly`
/// flag is set by the dispatcher (false for `VM_REMAP`, true for
/// `VM_REMAP_RO`) based on the call number, matching the C side which
/// uses the call number to pick the read-only branch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VmRemapIn {
    /// Caller endpoint (derived from `m_source`).
    pub caller: Endpoint,
    /// Source endpoint whose region is being remapped.
    pub who: Endpoint,
    /// Virtual address in `who`'s address space.
    pub vaddr: VirBytes,
    /// Size of the region in bytes.
    pub length: VirBytes,
    /// Target address in `caller`'s address space (or 0 for any).
    pub target: VirBytes,
    /// Mapping flags (MAP_PRIVATE / MAP_SHARED / MAP_FIXED / ...).
    pub flags: u32,
}

// ---------------------------------------------------------------------------
// VM_VFS_REPLY  (VFS → VM, asynchronous)  — DEFERRED
// ---------------------------------------------------------------------------
// C: mess_vm_vfs_reply (ipc.h, uses m10 layout)
//     #define VMV_ENDPOINT   m10_i1  // endpoint that completed
//     #define VMV_RESULT     m10_i2  // result of the VFS call
//     #define VMV_REQID      m10_i3  // request id (matches VFS_VMCALL_REQID)
//     #define VMV_DEV        m10_i4  // device (for fd resolution)
//     #define VMV_FD         m10_l1  // file descriptor
//     #define VMV_SIZE       m10_l2  // total size
//     #define VMV_SIZE_PAGES m10_l3  // size in pages

/// VFS → VM: VFS call completion reply.
///
/// Sent by VFS in response to an outstanding VM-initiated call (typically
/// the `vfs_vmcall` chain for file-backed mappings). The VM uses this to
/// resume a previously suspended MMAP request and complete the mapping.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VmVfsReplyIn {
    /// Endpoint that completed the call.
    pub endpoint: Endpoint,
    /// Result code (0 = success, otherwise the errno from VFS).
    pub result: i32,
    /// Request id (matches the `reqid` from the original VM→VFS call).
    pub reqid: i32,
    /// Device number (for fd resolution when reopening on resume).
    pub dev: i32,
    /// File descriptor.
    pub fd: i64,
    /// Total size in bytes.
    pub size: i64,
    /// Total size in pages (precomputed by VFS).
    pub size_pages: i64,
}

// ============================================================================
// Unified Reply Type (for dispatcher return value)
// ============================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VmRegionInfo {
    pub vaddr: VirBytes,
    pub length: VirBytes,
    pub flags: u32,
}

/// VM reply — wraps each link's Out type or an error.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VmReply {
    Fork(VmForkOut),
    Brk(VmBrkOut),
    Mmap(VmMmapOut),
    MapPhys(VmMapPhysOut),
    MapCache {
        addr: VirBytes,
    },
    VfsMmap(VmMmapOut),
    Munmap,
    Exit,
    Willexit,
    ExecNewmem(VmExecNewmemOut),
    Ok,
    Suspend,
    RsMemctlAddrLen {
        addr: VirBytes,
        len: usize,
    },
    GetPhys {
        phys_addr: PhysBytes,
    },
    GetRefcount {
        count: u8,
    },
    InfoStats {
        page_size: u64,
        total_pages: u32,
        free_pages: u32,
        largest_contiguous: u32,
    },
    InfoUsage {
        total: VirBytes,
        common: VirBytes,
        shared: VirBytes,
        virtual_total: VirBytes,
        mvirtual: VirBytes,
    },
    InfoRegion {
        regions: [VmRegionInfo; 8],
        count: usize,
        next: usize,
    },
    Getrusage {
        max_rss_kb: u64,
        minor_faults: u64,
        major_faults: u64,
    },
    Error(VmError),
}

// ============================================================================
// Error Type
// ============================================================================

/// VM error types.
///
/// # `InvalidEndpoint` vs `InvalidProcess`
///
/// Both represent "bad endpoint" but map to different errno values,
/// matching Minix3 C source behavior:
///
/// - `InvalidEndpoint` → `ESRCH`: Used when C's `vm_isokendpt` failure
///   returns `ESRCH` (e.g. `do_mmap` third-party mapping: mmap.c:216,
///   `do_getrusage`: utility.c:442)
///
/// - `InvalidProcess` → `EINVAL`: Used when C's `vm_isokendpt` failure
///   returns `EINVAL` (most services: fork.c:44, break.c:53, exit.c:69,
///   mmap.c:329, rs.c:44/94/165/361, utility.c:110)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VmError {
    InvalidEndpoint,
    InvalidProcess,
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
    /// Cache entry not found (C: ENOENT).
    /// Returned by do_mapcache when the requested cache page does not exist.
    NotFound,
}

impl VmError {
    pub fn to_errno(&self) -> i32 {
        match self {
            Self::InvalidEndpoint => ESRCH,
            Self::InvalidProcess => EINVAL,
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
            Self::NotFound => ENOENT,
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
        m1.m1p1 = self.new_addr.0;
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

// ── Decoders for the 3 DEFERRED stubs ────────────────────
//
// These decoders re-purpose the `m1` fields to hold the C `m7`/`m9`/`m10`
// payload words. The mapping is documented on each struct; see
// `dispatcher.rs::dispatch_procctl` for the corresponding layout audit.

impl DecodeFromM1 for VmProcctlIn {
    /// Decode VMPCTL_PARAM/WHO/M1/LEN/FLAGS from the m1 payload.
    ///
    /// Field mapping (C `mess_lc_vm_procctl` → Rust m1):
    /// - `VMPCTL_PARAM` is a small int → stored in `m1i1` (offset 0)
    /// - `VMPCTL_WHO` is an endpoint (i32) → stored in low 4 bytes of
    ///   `m1p1` (offset 16, 8 bytes wide for future extension)
    /// - `VMPCTL_M1` is a `vir_bytes` (u64) → stored in `m1p2` (offset 24)
    /// - `VMPCTL_LEN` is an int → stored in `m1p3` low 4 bytes (offset 32)
    /// - `VMPCTL_FLAGS` is an int → stored in `m1i3` (offset 12)
    #[inline(always)]
    fn decode(m1: &MessageM1) -> Self {
        Self {
            param: m1.m1i1,
            who: Endpoint(m1.m1p1 as i32),
            m1: m1.m1p2,
            len: m1.m1p3 as i32,
            flags: m1.m1i3,
        }
    }
}

impl DecodeFromM1 for VmRemapIn {
    /// Decode VM_REMAP / VM_REMAP_RO payload.
    ///
    /// Field mapping (C `mess_lc_vm_remap` → Rust m1):
    /// - `caller` is derived from `m_source` and passed in separately (the
    ///   decoder takes it as a parameter through the surrounding
    ///   dispatcher's `msg.m_source`); we still expose it as a field for
    ///   handler ergonomics. The decoder itself only fills the rest.
    /// - `who` → m1.m1i1
    /// - `vaddr` → m1.m1p1
    /// - `length` → m1.m1i2 as u64
    /// - `target` → m1.m1p2
    /// - `flags` → m1.m1i3 as u32
    #[inline(always)]
    fn decode(m1: &MessageM1) -> Self {
        Self {
            // `caller` is set by `dispatch_remap` from `msg.m_source`
            // before decoding; we use `Endpoint::NONE` as a placeholder
            // and let the caller overwrite it.
            caller: Endpoint::NONE,
            who: Endpoint(m1.m1i1),
            vaddr: VirBytes(m1.m1p1),
            length: VirBytes(m1.m1i2 as u64),
            target: VirBytes(m1.m1p2),
            flags: m1.m1i3 as u32,
        }
    }
}

impl DecodeFromM1 for VmVfsReplyIn {
    /// Decode VM_VFS_REPLY payload.
    ///
    /// Field mapping (C `mess_vm_vfs_reply` → Rust m1):
    /// - `VMV_ENDPOINT` → m1.m1i1
    /// - `VMV_RESULT` → m1.m1i2
    /// - `VMV_REQID` → m1.m1i3
    /// - `VMV_DEV` → m1.m1p1 low 4 bytes
    /// - `VMV_FD` → m1.m1p1 (re-using the 8-byte field for fd+dev)
    /// - `VMV_SIZE` → m1.m1p2
    /// - `VMV_SIZE_PAGES` → m1.m1p3
    #[inline(always)]
    fn decode(m1: &MessageM1) -> Self {
        // m1p1 holds both dev (i32) and fd (i64) in the C layout; we pack
        // them as `dev = m1p1 as i32`, `fd = m1p1 as i64` — the high 4
        // bytes overlap with the dev field of the next call. This matches
        // the C side's m10_i4 + m10_l1 aliasing (m10_i4 is the low 4
        // bytes of m10_l1 in little-endian).
        let p1 = m1.m1p1;
        Self {
            endpoint: Endpoint(m1.m1i1),
            result: m1.m1i2,
            reqid: m1.m1i3,
            dev: p1 as i32,
            fd: p1 as i64,
            size: m1.m1p2 as i64,
            size_pages: m1.m1p3 as i64,
        }
    }
}

// ---------------------------------------------------------------------------
// Stub DecodeFromM1 / EncodeToM1 for types that use extended message formats
// (mess_lsys_vm_mmap, etc.) — these require a dedicated message format
// beyond MessageM1. For now, decode from M1 with best-effort field mapping.
// TODO: Add proper message format types (e.g. MessageLsysVmMmap) and
// implement DecodeFrom those formats instead.
// ---------------------------------------------------------------------------

impl DecodeFromM1 for VmMmapIn {
    fn decode(m1: &MessageM1) -> Self {
        Self {
            caller: Endpoint(m1.m1i1),
            forwhom: Endpoint(m1.m1i2),
            addr: VirBytes(m1.m1p1),
            length: VirBytes(m1.m1p2),
            prot: 0,
            flags: 0,
            fd: 0,
            offset: 0,
        }
    }
}

impl EncodeToM1 for VmMmapOut {
    fn encode(&self, m1: &mut MessageM1) {
        m1.m1p1 = self.ret_addr.0;
    }
}

impl DecodeFromM1 for VmMapPhysIn {
    fn decode(m1: &MessageM1) -> Self {
        Self {
            caller: Endpoint(m1.m1i1),
            target: Endpoint(m1.m1i2),
            phys_addr: PhysBytes(m1.m1p1),
            length: VirBytes(m1.m1p2),
        }
    }
}

impl EncodeToM1 for VmMapPhysOut {
    fn encode(&self, m1: &mut MessageM1) {
        m1.m1p1 = self.virt_addr.0;
    }
}

impl DecodeFromM1 for VmVfsMmapIn {
    fn decode(m1: &MessageM1) -> Self {
        Self {
            who: Endpoint(m1.m1i1),
            fd: m1.m1i2,
            offset: 0,
            dev: 0,
            ino: 0,
            vaddr: VirBytes(m1.m1p1),
            length: VirBytes(m1.m1p2),
            flags: 0,
            clearend: 0,
        }
    }
}

impl DecodeFromM1 for VmCacheIn {
    fn decode(m1: &MessageM1) -> Self {
        Self {
            dev: m1.m1p1,
            dev_offset: m1.m1p2,
            ino: 0,
            ino_offset: 0,
            pages: m1.m1i1 as u32,
            flags: m1.m1i2 as u32,
            block: 0,
        }
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
        let req = VmExitIn {
            endpoint: Endpoint::PM,
        };
        assert_eq!(req.endpoint, Endpoint::PM);
    }

    #[test]
    fn test_vm_willexit_in() {
        let req = VmWillexitIn {
            endpoint: Endpoint::PM,
        };
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
