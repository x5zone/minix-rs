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
    EACCES, EFAULT, EINVAL, EIO, ENOENT, ENOMEM, ENOSYS, ENXIO, EPERM, ESRCH, Endpoint, Message,
    PhysBytes, UserSlot, VirBytes,
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

/// User process → VM: brk request.
///
/// C: libc `brk()` sends `VM_BRK` directly (`_syscall(VM_PROC_NR, VM_BRK, &m)`,
/// minix3/minix/lib/libc/sys/brk.c) with only `m_lc_vm_brk.addr`; the caller
/// endpoint is `m_source` (kernel-set). PM is NOT involved — unlike VM_FORK.
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
// C: mess_mmap (ipc.h:1582)
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
// C: mess_lsys_vm_map_phys (ipc.h:1504)

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
// C: mess_vm_vfs_mmap (ipc.h:2369)
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

impl VmUnmapPhysIn {
    /// Decode a `VM_UNMAP_PHYS` request (driver → VM).
    ///
    /// Reads the payload from the dedicated `m_lsys_vm_unmap_phys` union
    /// member, matching the C wire format (`mess_lsys_vm_unmap_phys`,
    /// ipc.h:1521-1527) as sent by the driver libraries: `ep` (i32 @ 0),
    /// `vaddr` (u32 @ 4). The target is an explicit message field — the
    /// driver unmaps on behalf of the process it mapped memory for.
    ///
    /// Do NOT decode from `MessageM1` — same wire-format family as
    /// 19-P1-1 / 16-P0-1 / 20-P1-1 (the old M1 decode read `vaddr` from
    /// `m1p1` @ 16, which is past the 4-byte vaddr).
    #[inline(always)]
    pub fn decode_message(msg: &Message) -> Self {
        // SAFETY: `m_lsys_vm_unmap_phys` is the active union arm for
        // VM_UNMAP_PHYS messages.
        let up = unsafe { msg.m_u.m_lsys_vm_unmap_phys };
        Self {
            target: Endpoint(up.ep),
            vaddr: VirBytes(up.vaddr as u64),
        }
    }
}

impl VmShmUnmapIn {
    /// Decode a `VM_SHM_UNMAP` request (process → VM).
    ///
    /// Reads the payload from the dedicated `m_lc_vm_shm_unmap` union
    /// member, matching the C wire format (`mess_lc_vm_shm_unmap`,
    /// ipc.h:934-940) as sent by libc `vm_shm_unmap`: `forwhom` (i32 @ 0),
    /// `addr` (u32 @ 4).
    ///
    /// Do NOT decode from `MessageM1` — same wire-format family as
    /// 19-P1-1 / 16-P0-1 / 20-P1-1.
    #[inline(always)]
    pub fn decode_message(msg: &Message) -> Self {
        // SAFETY: `m_lc_vm_shm_unmap` is the active union arm for
        // VM_SHM_UNMAP messages.
        let sh = unsafe { msg.m_u.m_lc_vm_shm_unmap };
        Self {
            forwhom: Endpoint(sh.forwhom),
            addr: VirBytes(sh.addr as u64),
        }
    }
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
///
/// ARCH: Minix3 packs the fault address in `m1_i1` (`VPF_ADDR`) and the
/// error flags in `m1_i2` (`VPF_FLAGS`), with the faulting endpoint carried
/// in `m_source` (com.h:774-775; `do_pagefaults` reads `m->m_source`,
/// pagefaults.c:242). minix-rs uses the dedicated 64-bit `m_vm_pagefault`
/// union member (`vpf_addr: u64`) instead of the 32-bit `m1_i1`, so this
/// type is decoded from the full `Message`, not from `MessageM1`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VmPagefaultIn {
    pub endpoint: Endpoint,
    pub vaddr: VirBytes,
    pub write: bool,
}

impl VmPagefaultIn {
    /// Decode a `VM_PAGEFAULT` message (kernel → VM).
    ///
    /// Reads the endpoint from `m_source` and the fault address / error
    /// flags from the dedicated `m_vm_pagefault` union member, which the
    /// minix-rs kernel populates in `build_vm_pagefault_msg`
    /// (os/kernel/src/page_fault.rs:142-166).
    #[inline(always)]
    pub fn decode_message(msg: &Message) -> Self {
        // SAFETY: `m_vm_pagefault` is the active union arm for VM_PAGEFAULT
        // messages — the kernel writes it via build_vm_pagefault_msg.
        let pf = unsafe { msg.m_u.m_vm_pagefault };
        Self {
            endpoint: msg.m_source,
            vaddr: VirBytes(pf.vpf_addr),
            // C: PFERR_WRITE(err) — arch/i386/pagetable.h:38; bit 1 = W.
            write: (pf.vpf_flags & 2) != 0,
        }
    }
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
// VM_PROCCTL  (VFS/RS → VM)
// ---------------------------------------------------------------------------
// C: VM_PROCCTL fields use the m9 union layout (`message.m_m9`)
//     #define VMPCTL_PARAM  m9_l1   // operation (VMPPARAM_CLEAR/SETMCALL/...)
//     #define VMPCTL_WHO    m9_l2   // target endpoint
//     #define VMPCTL_M1     m9_l3   // user-space pointer (m1 sys call index)
//     #define VMPCTL_LEN    m9_l4   // byte count
//     #define VMPCTL_FLAGS  m9_l5   // write flag
//
// Wire layout is the C `mess_9` layout (32-bit longs): param@16, who@20,
// m1@24, len@28, flags@32. Decode goes through the dedicated
// `MessageUnion::m_lc_vm_procctl` overlay (`MessLcVmProcctl`) so the
// offsets match the C senders (libsys `vm_procctl.c`, vfs `comm.c`).

/// VFS → VM: process control request.
///
/// Decoded via [`VmProcctlIn::decode_message`] from the C-compatible
/// `MessLcVmProcctl` overlay.
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
// C: mess_lsys_vm_vmremap (ipc.h:1537) — destination/source endpoints are
// explicit message fields, NOT derived from m_source (the IPC server
// remaps into its client, servers/ipc/shm.c:159).

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
    /// Destination endpoint that receives the shared region.
    /// C: `mess_lsys_vm_vmremap.destination`
    pub destination: Endpoint,
    /// Source endpoint whose region is being remapped.
    pub who: Endpoint,
    /// Virtual address in `who`'s address space.
    pub vaddr: VirBytes,
    /// Size of the region in bytes.
    pub length: VirBytes,
    /// Requested address in `destination`'s address space (or 0 for any).
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
    /// Invalid request parameter (C: `EINVAL`).
    ///
    /// C's mmap path returns `EINVAL` for `len <= 0`, bad flag
    /// combinations, and MAP_ANON-with-fd (mmap.c:215/226/244); a bare
    /// `EINVAL` is distinct from the `InvalidProcess` EINVAL used by
    /// `vm_isokendpt` failures.
    InvalidParam,
    PermissionDenied,
    AccessViolation,
    PageNotMapped,
    MemType,
    PageTableError,
    InternalError,
    NotImplemented,
    /// No such device or address (C: `ENXIO`).
    ///
    /// C's mmap file path returns `ENXIO` when file mapping is disabled
    /// and for writable MAP_SHARED file mappings (mmap.c:255-259).
    NoDevice,
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
            Self::InvalidParam => EINVAL,
            Self::PermissionDenied => EPERM,
            Self::AccessViolation => EACCES,
            Self::PageNotMapped => EFAULT,
            Self::MemType => EIO,
            Self::PageTableError => EIO,
            Self::InternalError => EIO,
            Self::NotImplemented => ENOSYS,
            Self::NoDevice => ENXIO,
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

impl VmBrkIn {
    /// Decode a `VM_BRK` request (user process → VM).
    ///
    /// Reads the endpoint from `m_source` and the address from the dedicated
    /// `m_lc_vm_brk` union member, matching the C wire format
    /// (`mess_lc_vm_brk.addr` at payload offset 0, ipc.h:918-926).
    ///
    /// Do NOT decode from `MessageM1` — C's `do_brk` uses `m_source`, not
    /// `m1_i1`; a real libc sender leaves `m1_i1` zeroed (brk.c memsets the
    /// message), so the old M1 decode would read endpoint 0 → EINVAL for
    /// every brk() (same family as 16-P0-1 VM_PAGEFAULT wire format).
    #[inline(always)]
    pub fn decode_message(msg: &Message) -> Self {
        // SAFETY: `m_lc_vm_brk` is the active union arm for VM_BRK messages —
        // the sender writes only the addr field (libc brk.c).
        let brk = unsafe { msg.m_u.m_lc_vm_brk };
        Self {
            endpoint: msg.m_source,
            new_addr: VirBytes(brk.addr),
        }
    }
}

impl EncodeToM1 for VmBrkOut {
    #[inline(always)]
    fn encode(&self, m1: &mut MessageM1) {
        m1.m1p1 = self.new_addr.0;
    }
}

impl VmMunmapIn {
    /// Decode a `VM_MUNMAP` request (user process → VM).
    ///
    /// Reads the caller from `m_source` (C: `target = m->m_source` for
    /// VM_MUNMAP, mmap.c:518-525) and the payload from the dedicated
    /// `m_mmap` union member, matching the C wire format as sent by libc
    /// `munmap()` (`m.VMUM_ADDR = addr; m.VMUM_LEN = len;`,
    /// libc/sys/mmap.c:79-86): `addr` (u32 @ 8), `len` (u32 @ 12).
    ///
    /// Do NOT decode from `MessageM1` — same wire-format family as
    /// 19-P1-1 / 16-P0-1 / 20-P1-1 (the old M1 decode read the endpoint
    /// from `m_mmap.offset`'s low bits and addr/len from prot/flags/fd).
    #[inline(always)]
    pub fn decode_message(msg: &Message) -> Self {
        // SAFETY: `m_mmap` is the active union arm for VM_MUNMAP messages
        // (com.h:650-651 reuses `mess_mmap` for VMUM_ADDR/VMUM_LEN).
        let mm = unsafe { msg.m_u.m_mmap };
        Self {
            endpoint: msg.m_source,
            addr: VirBytes(mm.addr as u64),
            length: VirBytes(mm.len as u64),
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

impl VmProcctlIn {
    /// Decode a `VM_PROCCTL` request (VFS/RS → VM) from the C-compatible
    /// m9 overlay.
    ///
    /// Reads `MessageUnion::m_lc_vm_procctl` (C `mess_9` layout, ipc.h:77-83).
    /// Field offsets match the C senders: `param`@16, `who`@20, `m1`@24,
    /// `len`@28, `flags`@32 (com.h:753-757, 32-bit `long`).
    ///
    /// Do NOT decode from `MessageM1` — same wire-format family as
    /// 21-P1-1 / 19-P1-1 / 16-P0-1 (the old M1 decode re-mapped the five
    /// fields to different offsets, breaking C-layout compatibility).
    #[inline(always)]
    pub fn decode_message(msg: &Message) -> Self {
        // SAFETY: `m_lc_vm_procctl` is the active union arm for VM_PROCCTL
        // messages (C: `message.m_m9`, com.h:753-757).
        let p = unsafe { msg.m_u.m_lc_vm_procctl };
        Self {
            param: p.param,
            who: Endpoint(p.who),
            m1: p.m1 as u64,
            len: p.len,
            flags: p.flags,
        }
    }
}

impl VmRemapIn {
    /// Decode a `VM_REMAP` / `VM_REMAP_RO` request (driver → VM).
    ///
    /// Reads the endpoints from the dedicated `m_lsys_vm_vmremap` union
    /// member, matching the C wire format (`mess_lsys_vm_vmremap`,
    /// ipc.h:1537-1545): `destination` and `source` are explicit message
    /// fields. `caller` comes from `m_source` for ACL purposes.
    ///
    /// Do NOT decode from `MessageM1` — the C sender (libc `vm_remap`,
    /// libc/sys/mmap.c) memsets the message and writes the fields at the
    /// union payload offsets, so an M1 read would misplace every field
    /// (same family as 19-P1-1 / 16-P0-1 wire-format fixes).
    #[inline(always)]
    pub fn decode_message(msg: &Message) -> Self {
        // SAFETY: `m_lsys_vm_vmremap` is the active union arm for
        // VM_REMAP / VM_REMAP_RO messages.
        let r = unsafe { msg.m_u.m_lsys_vm_vmremap };
        Self {
            caller: msg.m_source,
            destination: Endpoint(r.destination),
            who: Endpoint(r.source),
            vaddr: VirBytes(r.src_addr as u64),
            length: VirBytes(r.size as u64),
            target: VirBytes(r.dest_addr as u64),
            flags: 0,
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
// DecodeFromM1 for the reply encoders below; requests use dedicated
// `decode_message` overlays matching the C wire formats.
// ---------------------------------------------------------------------------

impl VmMmapIn {
    /// Decode a `VM_MMAP` request (user process → VM).
    ///
    /// Reads `caller` from `m_source` (kernel-set, spoof-proof) and the
    /// payload from the dedicated `m_mmap` union member, matching the C
    /// wire format (`mess_mmap`, ipc.h:1582-1592) as sent by libc
    /// `minix_mmap_for` (libc/sys/mmap.c): `offset` (u64 @ 0), `addr`,
    /// `len`, `prot`, `flags`, `fd`, `forwhom` as 32-bit fields.
    ///
    /// Do NOT decode from `MessageM1` — the real libc sender memsets the
    /// message and writes the fields at the union payload offsets, so the
    /// old M1 decode read prot/flags/fd/offset as 0 for every mmap()
    /// (same family as 19-P1-1 / 16-P0-1 wire-format fixes).
    #[inline(always)]
    pub fn decode_message(msg: &Message) -> Self {
        // SAFETY: `m_mmap` is the active union arm for VM_MMAP messages.
        let mm = unsafe { msg.m_u.m_mmap };
        Self {
            caller: msg.m_source,
            forwhom: Endpoint(mm.forwhom),
            addr: VirBytes(mm.addr as u64),
            length: VirBytes(mm.len as u64),
            prot: mm.prot as u32,
            flags: mm.flags as u32,
            fd: mm.fd,
            offset: mm.offset,
        }
    }
}

impl VmMapPhysIn {
    /// Decode a `VM_MAP_PHYS` request (driver → VM).
    ///
    /// Reads `caller` from `m_source` and the payload from the dedicated
    /// `m_lsys_vm_map_phys` union member, matching the C wire format
    /// (`mess_lsys_vm_map_phys`, ipc.h:1504-1510) as sent by the driver
    /// libraries: `ep` (i32 @ 0), `phaddr` (u32 @ 4), `len` (u32 @ 8).
    ///
    /// Do NOT decode from `MessageM1` — same wire-format family as
    /// 19-P1-1 / 16-P0-1.
    #[inline(always)]
    pub fn decode_message(msg: &Message) -> Self {
        // SAFETY: `m_lsys_vm_map_phys` is the active union arm for
        // VM_MAP_PHYS messages.
        let mp = unsafe { msg.m_u.m_lsys_vm_map_phys };
        Self {
            caller: msg.m_source,
            target: Endpoint(mp.ep),
            phys_addr: PhysBytes(mp.phaddr as u64),
            length: VirBytes(mp.len as u64),
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

impl VmVfsMmapIn {
    /// Decode a `VM_VFS_MMAP` request (VFS → VM, synchronous).
    ///
    /// Reads the payload from the dedicated `m_vm_vfs_mmap` union member,
    /// matching the C wire format (`mess_vm_vfs_mmap`, ipc.h:2369-2380)
    /// as sent by `minix_vfs_mmap` (libc/sys/mmap.c): `offset`/`dev`/`ino`
    /// as u64 at offsets 0/8/16, then `who`/`vaddr`/`len`/`flags`/`fd` as
    /// 32-bit fields and `clearend` as u16.
    ///
    /// Do NOT decode from `MessageM1` — same wire-format family as
    /// 19-P1-1 / 16-P0-1 (the old M1 decode zeroed offset/dev/ino/flags/
    /// clearend for every VFS_MMAP).
    #[inline(always)]
    pub fn decode_message(msg: &Message) -> Self {
        // SAFETY: `m_vm_vfs_mmap` is the active union arm for
        // VM_VFS_MMAP messages.
        let v = unsafe { msg.m_u.m_vm_vfs_mmap };
        Self {
            who: Endpoint(v.who),
            fd: v.fd as i32,
            offset: v.offset,
            dev: v.dev,
            ino: v.ino,
            vaddr: VirBytes(v.vaddr as u64),
            length: VirBytes(v.len as u64),
            flags: v.flags,
            clearend: v.clearend,
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
    fn test_vm_brk_in_decode_message() {
        // C wire format: libc brk() sends only m_lc_vm_brk.addr; the caller
        // endpoint is m_source (kernel-set). On the 32-bit wire, addr sits at
        // payload offset 0 — the same bytes as m1_i1 — so the OLD M1 decode
        // (endpoint = m1i1) read the low 32 bits of addr → garbage endpoint
        // → EINVAL for every brk() (19-P1-1, same family as 16-P0-1).
        use crate::ipc::{Message, MessLcVmBrk};
        let mut msg = Message::default();
        msg.m_source = Endpoint::PM;
        msg.m_type = 0xC02; // VM_BRK
        unsafe {
            msg.m_u.m_lc_vm_brk = MessLcVmBrk { addr: 0x4000_0000, ..MessLcVmBrk::default() };
        }
        // m1_i1 (payload bytes 0-3) == low 32 bits of addr == 0x4000_0000,
        // deliberately non-zero: if the decode read m1i1 as endpoint it
        // would be Endpoint(0x4000_0000), not PM.
        let req = VmBrkIn::decode_message(&msg);
        assert_eq!(req.endpoint, Endpoint::PM, "endpoint must come from m_source, not m1i1");
        assert_eq!(req.new_addr.0, 0x4000_0000);
    }

    #[test]
    fn test_vm_mmap_in_decode_message() {
        // C wire format (mess_mmap, ipc.h:1582): offset (u64 @ 0), addr
        // (u32 @ 8), len (u32 @ 12), prot (i32 @ 16), flags (i32 @ 20),
        // fd (i32 @ 24), forwhom (i32 @ 28). The old M1 decode zeroed
        // prot/flags/fd/offset — the regression asserts all fields survive.
        use crate::ipc::{Message, MessMmap};
        let mut msg = Message::default();
        msg.m_source = Endpoint::PM;
        msg.m_type = 0xC03; // VM_MMAP
        unsafe {
            msg.m_u.m_mmap = MessMmap {
                offset: 0x2000,
                addr: 0x4000_0000,
                len: 0x3000,
                prot: 3,
                flags: 0x1002,
                fd: 7,
                forwhom: 42,
                ..MessMmap::default()
            };
        }
        let req = VmMmapIn::decode_message(&msg);
        assert_eq!(req.caller, Endpoint::PM, "caller must come from m_source");
        assert_eq!(req.forwhom.0, 42);
        assert_eq!(req.addr.0, 0x4000_0000);
        assert_eq!(req.length.0, 0x3000);
        assert_eq!(req.prot, 3);
        assert_eq!(req.flags, 0x1002);
        assert_eq!(req.fd, 7);
        assert_eq!(req.offset, 0x2000);
    }

    #[test]
    fn test_vm_vfs_mmap_in_decode_message() {
        // C wire format (mess_vm_vfs_mmap, ipc.h:2369): offset/dev/ino as
        // u64 at 0/8/16, who/vaddr/len/flags/fd as u32, clearend as u16.
        use crate::ipc::{Message, MessVmVfsMmap};
        let mut msg = Message::default();
        msg.m_source = Endpoint::VFS;
        msg.m_type = 0xC2E; // VM_VFS_MMAP = VM_RQ_BASE+46
        unsafe {
            msg.m_u.m_vm_vfs_mmap = MessVmVfsMmap {
                offset: 0x1000,
                dev: 0xABCD,
                ino: 0x1234_5678,
                who: 77,
                vaddr: 0x2000_0000,
                len: 0x4000,
                flags: 1,
                fd: 9,
                clearend: 0x100,
                ..MessVmVfsMmap::default()
            };
        }
        let req = VmVfsMmapIn::decode_message(&msg);
        assert_eq!(req.who.0, 77);
        assert_eq!(req.fd, 9);
        assert_eq!(req.offset, 0x1000);
        assert_eq!(req.dev, 0xABCD);
        assert_eq!(req.ino, 0x1234_5678);
        assert_eq!(req.vaddr.0, 0x2000_0000);
        assert_eq!(req.length.0, 0x4000);
        assert_eq!(req.flags, 1);
        assert_eq!(req.clearend, 0x100);
    }

    #[test]
    fn test_vm_map_phys_in_decode_message() {
        // C wire format (mess_lsys_vm_map_phys, ipc.h:1504): ep (i32 @ 0),
        // phaddr (u32 @ 4), len (u32 @ 8).
        use crate::ipc::{Message, MessLsysVmMapPhys};
        let mut msg = Message::default();
        msg.m_source = Endpoint::MEM;
        msg.m_type = 0xC04; // VM_MAP_PHYS
        unsafe {
            msg.m_u.m_lsys_vm_map_phys = MessLsysVmMapPhys {
                ep: 55,
                phaddr: 0xB8000,
                len: 0x1000,
                ..MessLsysVmMapPhys::default()
            };
        }
        let req = VmMapPhysIn::decode_message(&msg);
        assert_eq!(req.caller, Endpoint::MEM, "caller must come from m_source");
        assert_eq!(req.target.0, 55);
        assert_eq!(req.phys_addr.0, 0xB8000);
        assert_eq!(req.length.0, 0x1000);
    }

    #[test]
    fn test_vm_munmap_in_decode_message() {
        // 21-P1-1 regression: VM_MUNMAP reuses mess_mmap (com.h:650-651)
        // and the caller is m_source (mmap.c:518-525). The old M1 decode
        // read the endpoint from m_mmap.offset's low bits and addr/len
        // from prot/flags — all wrong.
        use crate::ipc::{Message, MessMmap};
        let mut msg = Message::default();
        msg.m_source = Endpoint::from_generation_slot(1, 33);
        msg.m_type = VM_MUNMAP as i32;
        unsafe {
            msg.m_u.m_mmap = MessMmap {
                offset: 0xDEAD_BEEF, // must be ignored
                addr: 0x1000,
                len: 0x2000,
                ..MessMmap::default()
            };
        }
        let req = VmMunmapIn::decode_message(&msg);
        assert_eq!(req.endpoint, msg.m_source, "endpoint must come from m_source");
        assert_eq!(req.addr.0, 0x1000);
        assert_eq!(req.length.0, 0x2000);
    }

    #[test]
    fn test_vm_unmap_phys_in_decode_message() {
        // 21-P1-1 regression: mess_lsys_vm_unmap_phys (ipc.h:1521) has
        // ep (i32 @ 0), vaddr (u32 @ 4). The old M1 decode read vaddr
        // from m1p1 @ 16 — past the 4-byte field.
        use crate::ipc::{Message, MessLsysVmUnmapPhys};
        let mut msg = Message::default();
        msg.m_source = Endpoint::MEM;
        msg.m_type = VM_UNMAP_PHYS as i32;
        unsafe {
            msg.m_u.m_lsys_vm_unmap_phys = MessLsysVmUnmapPhys {
                ep: 66,
                vaddr: 0x8000_0000,
                ..MessLsysVmUnmapPhys::default()
            };
        }
        let req = VmUnmapPhysIn::decode_message(&msg);
        assert_eq!(req.target.0, 66);
        assert_eq!(req.vaddr.0, 0x8000_0000);
    }

    #[test]
    fn test_vm_shm_unmap_in_decode_message() {
        // 21-P1-1 regression: mess_lc_vm_shm_unmap (ipc.h:934) has
        // forwhom (i32 @ 0), addr (u32 @ 4). The old M1 decode read addr
        // from m1p1 @ 16.
        use crate::ipc::{Message, MessLcVmShmUnmap};
        let mut msg = Message::default();
        msg.m_source = Endpoint::PM;
        msg.m_type = VM_SHM_UNMAP as i32;
        unsafe {
            msg.m_u.m_lc_vm_shm_unmap = MessLcVmShmUnmap {
                forwhom: 88,
                addr: 0x7000_0000,
                ..MessLcVmShmUnmap::default()
            };
        }
        let req = VmShmUnmapIn::decode_message(&msg);
        assert_eq!(req.forwhom.0, 88);
        assert_eq!(req.addr.0, 0x7000_0000);
    }

    #[test]
    fn test_vm_remap_in_decode_message() {
        // C wire format (mess_lsys_vm_vmremap, ipc.h:1537): destination
        // (i32 @ 0), source (i32 @ 4), dest_addr (u32 @ 8), src_addr
        // (u32 @ 12), size (u32 @ 16). destination is an explicit message
        // field, NOT the caller.
        use crate::ipc::{Message, MessLsysVmVmremap};
        let mut msg = Message::default();
        msg.m_source = Endpoint(100); // IPC server
        msg.m_type = 0xC0D; // VM_REMAP
        unsafe {
            msg.m_u.m_lsys_vm_vmremap = MessLsysVmVmremap {
                destination: 88,
                source: 99,
                dest_addr: 0x3000_0000,
                src_addr: 0x1000,
                size: 0x4000,
                ..MessLsysVmVmremap::default()
            };
        }
        let req = VmRemapIn::decode_message(&msg);
        assert_eq!(req.caller, Endpoint(100));
        assert_eq!(req.destination.0, 88, "destination must come from the message field, not m_source");
        assert_eq!(req.who.0, 99);
        assert_eq!(req.vaddr.0, 0x1000);
        assert_eq!(req.length.0, 0x4000);
        assert_eq!(req.target.0, 0x3000_0000);
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
    fn test_vm_procctl_in_decode_message() {
        // Wire layout is the C m9 overlay (param@16/who@20/m1@24/len@28/
        // flags@32, com.h:753-757). The sender writes the dedicated
        // `m_lc_vm_procctl` union arm.
        let mut msg = Message::default();
        unsafe {
            msg.m_u.m_lc_vm_procctl.param = 2; // VMPPARAM_HANDLEMEM
            msg.m_u.m_lc_vm_procctl.who = 1234;
            msg.m_u.m_lc_vm_procctl.m1 = 0x1000;
            msg.m_u.m_lc_vm_procctl.len = 4096;
            msg.m_u.m_lc_vm_procctl.flags = 1;
        }

        let req = VmProcctlIn::decode_message(&msg);
        assert_eq!(req.param, 2);
        assert_eq!(req.who, Endpoint(1234));
        assert_eq!(req.m1, 0x1000);
        assert_eq!(req.len, 4096);
        assert_eq!(req.flags, 1);
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
    fn test_vm_pagefault_in_decode_message() {
        // Kernel writes the dedicated m_vm_pagefault union member
        // (build_vm_pagefault_msg, os/kernel/src/page_fault.rs:142-166).
        let mut msg = Message::default();
        msg.m_source = Endpoint::PM;
        msg.m_type = VM_PAGEFAULT as i32;
        unsafe {
            msg.m_u.m_vm_pagefault.vpf_addr = 0x1234_0000;
            msg.m_u.m_vm_pagefault.vpf_flags = 0x2; // x86 PFE_W bit
        }

        let req = VmPagefaultIn::decode_message(&msg);
        assert_eq!(req.endpoint, Endpoint::PM);
        assert_eq!(req.vaddr.0, 0x1234_0000);
        assert!(req.write);
    }

    #[test]
    fn test_vm_pagefault_in_decode_read_fault() {
        let mut msg = Message::default();
        msg.m_source = Endpoint::PM;
        msg.m_type = VM_PAGEFAULT as i32;
        unsafe {
            msg.m_u.m_vm_pagefault.vpf_addr = 0x1000;
            msg.m_u.m_vm_pagefault.vpf_flags = 0x1; // P bit only, no W
        }

        let req = VmPagefaultIn::decode_message(&msg);
        assert_eq!(req.vaddr.0, 0x1000);
        assert!(!req.write);
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
