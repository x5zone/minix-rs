//! VM mmap implementation.
//!
//! Handles VM_MMAP and VM_VFS_MMAP requests. Supports anonymous
//! mappings, file mappings (via VFS interaction), and handles
//! MAP_FIXED / MAP_CONTIG / MAP_PREALLOC flags.
//!
//! Corresponds to Minix3's `do_mmap()` / `do_vfs_mmap()`
//! and `mmap_file()` / `mmap_file_cont()` in `mmap.c`.
//!
//! ## Design decisions vs Minix3
//!
//! **Separate anonymous and file paths**: Anonymous mappings are
//! handled synchronously in `handle_mmap` with mem_type_anon. File
//! mappings enqueue a `VfsRequestQueue::FdLookup` request with a
//! `mmap_file_cont` callback and return `MmapResult::Suspended`,
//! mirroring Minix3's `vfs_request` + `SUSPEND` + `mmap_file_cont`
//! pattern (C mmap.c:264-268). The request's actual delivery to VFS and
//! the resume reply are gated on the kernel IPC transport
//! (`ipc/transport.rs` — `KernelIpcTransport` pending; 23-vfs-interaction
//! scope), but the queue state machine is real and tested.
//!
//! **PFN index model**: Uses PageFrames/PageSlot instead of
//! Minix3's phys_region/phys_block.
//!
//! ## Permission checks (aligned with C mmap.c)
//!
//! - **MAP_THIRDPARTY**: Only VFS and RS (execpriv) may map memory
//!   into another process's address space. Others receive EPERM.
//!   (C: mmap.c:211-214 `if(!execpriv) return EPERM`)
//!
//! - **MAP_UNINITIALIZED**: Only VFS and RS may create uninitialized
//!   mappings (skip zero-fill). Others receive EINVAL (C returns ENOMEM
//!   via `mmap_region` returning NULL — documented divergence, doc 20 §3.6).
//!   (C: mmap.c:46-50 `if(!execpriv) return NULL`)
//!
//! - **MAP_CONTIG without MAP_PREALLOC**: Contiguous physical memory
//!   must be preallocated. MAP_CONTIG alone returns EINVAL.
//!   (C: mmap.c:242-245 `if((flags&(MAP_CONTIG|MAP_PREALLOC))==MAP_CONTIG) return EINVAL`)
//!
//! - **File mappings**: rejected with ENXIO when file mapping is disabled
//!   (`enable_filemap`) or for writable MAP_SHARED mappings.
//!   (C: mmap.c:255-261)

use core::sync::atomic::{AtomicBool, Ordering};

use minix_types::{Endpoint, VirBytes, VmMmapIn, VmVfsMmapIn};
use crate::vfs_queue::{VfsQueueError, VfsReply, VfsRequest, VfsRequestState, VfsRequestType};
use crate::vmproc::{ActiveProc, EndpointError, VmProcTable};
use crate::region::{PageFrames, VirRegion, VrFlags, VrParam};
use crate::alloc_page::VmPageAllocator;
use crate::memtype::{MemType, MEM_TYPE_ANON, MEM_TYPE_CONTIG_ANON, MEM_TYPE_MAPPED_FILE};
use crate::region::page_state::PAGE_SIZE;

// ── MmapFlags bitflags ──────────────────────────────────────────────

bitflags::bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub(crate) struct MmapFlags: u32 {
        const SHARED      = 0x0001;
        const PRIVATE     = 0x0002;
        const FIXED       = 0x0010;
        const ANONYMOUS   = 0x1000;
        const CONTIG      = 0x100000;
        const PREALLOC    = 0x080000;
        const UNINITIALIZED = 0x040000;
        const LOWER16M    = 0x200000;
        const LOWER1M     = 0x400000;
        const THIRDPARTY  = 0x800000;
        const ALIGNMENT_64KB = 0x01000000;
    }
}

bitflags::bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub(crate) struct ProtFlags: u32 {
        const NONE  = 0x00;
        const READ  = 0x01;
        const WRITE = 0x02;
        const EXEC  = 0x04;
    }
}

impl MmapFlags {
    /// Exactly one of MAP_SHARED / MAP_PRIVATE must be set.
    ///
    /// C `do_mmap` does not validate this (a flags=0 anonymous mapping is
    /// accepted); minix-rs tightens it to fail fast (doc 20 §3.6).
    pub(crate) fn is_valid(&self) -> bool {
        let shared = self.contains(Self::SHARED);
        let private = self.contains(Self::PRIVATE);
        (shared || private) && !(shared && private)
    }
}

impl ProtFlags {
    /// Convert PROT_* + MAP_* flags to region flags.
    ///
    /// Deliberately does NOT set `VrFlags::SHARED` for MAP_SHARED: C never
    /// propagates MAP_SHARED to `VR_SHARED` in `do_mmap` (VR_SHARED is only
    /// set by `do_remap`, mmap.c:413) — user MAP_SHARED regions are copied
    /// COW-style at fork like MAP_PRIVATE ones (doc 20 §2.2/§3.6).
    pub(crate) fn to_vr_flags(self, flags: MmapFlags) -> VrFlags {
        let mut vr = VrFlags::empty();
        if self.contains(Self::WRITE) {
            vr |= VrFlags::WRITABLE;
        }
        if flags.contains(MmapFlags::UNINITIALIZED) {
            vr |= VrFlags::UNINITIALIZED;
        }
        if flags.contains(MmapFlags::PREALLOC) {
            vr |= VrFlags::PREALLOC_MAP;
        }
        if flags.contains(MmapFlags::CONTIG) {
            vr |= VrFlags::PHYS64K;
        }
        if flags.contains(MmapFlags::ALIGNMENT_64KB) {
            vr |= VrFlags::PHYS64K;
        }
        if flags.contains(MmapFlags::LOWER16M) {
            vr |= VrFlags::LOWER16MB;
        }
        if flags.contains(MmapFlags::LOWER1M) {
            vr |= VrFlags::LOWER1MB;
        }
        vr
    }
}

// ── File mapping enablement ─────────────────────────────────────────

/// Whether file-backed mappings are enabled.
///
/// C: `long enable_filemap` (glo.h:22), default 1, settable via
/// `env_parse("filemap", ...)` (main.c:447-448). The Rust rewrite has no
/// env parsing yet, so the flag is a process-wide atomic defaulting to the
/// C default (enabled); the setter lets tests exercise the ENXIO guard.
static FILEMAP_ENABLED: AtomicBool = AtomicBool::new(true);

/// Test hook mirroring C's `enable_filemap` env switch.
#[cfg(test)]
pub(crate) fn set_filemap_enabled(enabled: bool) {
    FILEMAP_ENABLED.store(enabled, Ordering::Relaxed);
}

fn filemap_enabled() -> bool {
    FILEMAP_ENABLED.load(Ordering::Relaxed)
}

// ── Error & Response types ───────────────────────────────────────────

/// Errors from VM_MMAP / VM_VFS_MMAP operations.
///
/// All variants map to `VmError` via `From<MmapError> for VmError`,
/// then to C errno via `VmError::to_errno()`. Per-error `to_errno()`
/// method is intentionally omitted — the single source of truth is
/// `VmError::to_errno()` in `minix_types::ipc::vm`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MmapError {
    ProcessNotFound,
    InvalidLength,
    BadAddress,
    InvalidFlags,
    PermissionDenied,
    OutOfMemory,
    /// File mapping rejected: either disabled (`enable_filemap`) or a
    /// writable MAP_SHARED file mapping. C returns ENXIO for both
    /// (mmap.c:255-261).
    FileMapDisabled,
}

// ── Endpoint-lookup error unification ──
//
// See `munmap.rs` for the full rationale. THIRDPARTY's `vm_isokendpt`
// failure on `forwhom` is reported as `ProcessNotFound` (→ ESRCH),
// matching C mmap.c:215-217.
impl From<EndpointError> for MmapError {
    fn from(_: EndpointError) -> Self {
        MmapError::ProcessNotFound
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct MmapResponse {
    pub mapped_addr: VirBytes,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MmapResult {
    Complete(MmapResponse),
    /// File-backed mapping: an FDLOOKUP request has been queued to VFS.
    /// The main loop must NOT reply; `mmap_file_cont` resumes later.
    Suspended,
}

// ── mmap 64-bit address range ────────────────────────────────────────
// In Minix3 (32-bit), mmap area is runtime-computed:
//   VM_MMAPTOP = VM_STACKTOP - DEFAULT_STACK_LIMIT
//   VM_MMAPBASE = VM_MMAPTOP / 2  (or VM_PAGE_SIZE in non-MAGIC builds)
//
// In minix-rs (64-bit), the address space is 48-bit canonical user
// space ([ARCH: A-6]: 32-bit scarcity → 64-bit headroom). We reserve a
// generous range far from brk/stack:
pub(crate) const MMAP_BASE: u64 = 0x0000_0001_0000_0000;
pub(crate) const MMAP_TOP: u64  = 0x0000_0200_0000_0000;

fn roundup_page(len: u64) -> u64 {
    (len + PAGE_SIZE - 1) & !(PAGE_SIZE - 1)
}

// ── mmap_region (C mmap.c:36-83) ─────────────────────────────────────

/// Resolve a virtual address for a new region honoring MAP_FIXED / hint
/// semantics, then create the region and return its start address.
///
/// C `mmap_region` (mmap.c:36-83):
/// 1. MAP_FIXED with an address → `map_unmap_range` the target range first
///    (mmap.c:60-68), then map exactly there;
/// 2. hint address → try an exact fit at the hint (`map_page_region(addr,
///    0, len)`, mmap.c:70-76);
/// 3. no address / hint taken → `map_page_region(VM_MMAPBASE, VM_MMAPTOP,
///    len)` (mmap.c:78-80).
///
/// minix-rs tightening (doc 20 §3.6): MAP_FIXED with addr == 0 (C would
/// try to map at address 0) or a non-page-aligned addr (POSIX requires
/// page alignment; C doesn't check) is rejected with `BadAddress`.
fn mmap_region(
    active: &mut ActiveProc<'_>,
    page_alloc: &mut VmPageAllocator,
    frames: &mut PageFrames,
    addr: VirBytes,
    vmm_flags: MmapFlags,
    len: VirBytes,
) -> Result<VirBytes, MmapError> {
    debug_assert_eq!(len.0 % PAGE_SIZE, 0);

    if vmm_flags.contains(MmapFlags::FIXED) {
        if addr.0 == 0 {
            return Err(MmapError::BadAddress);
        }
        if !addr.0.is_multiple_of(PAGE_SIZE) {
            return Err(MmapError::BadAddress);
        }
        // C mmap.c:60-68 — unmap whatever occupies [addr, addr+len).
        crate::munmap::unmap_range(active, page_alloc, frames, addr, len)
            .map_err(|_| MmapError::OutOfMemory)?;
        return Ok(addr);
    }

    // Hint address: exact fit at the hint, else fall back to the full
    // mmap range (C mmap.c:70-80). Unaligned hints are treated as "no
    // hint" — the C side would honor them exactly, but region slots are
    // page-aligned by construction in minix-rs (doc 20 §3.6).
    if addr.0 != 0 && addr.0.is_multiple_of(PAGE_SIZE) {
        let end = VirBytes(addr.0 + len.0);
        if active.regions().find_overlap(addr, end).is_none() {
            return Ok(addr);
        }
    }

    active
        .regions()
        .find_slot(VirBytes(MMAP_BASE), VirBytes(MMAP_TOP), len)
        .ok_or(MmapError::OutOfMemory)
}

// ── handle_mmap (C do_mmap, mmap.c:200-269) ─────────────────────────

pub(crate) fn handle_mmap(
    table: &VmProcTable,
    page_alloc: &mut VmPageAllocator,
    frames: &mut PageFrames,
    vfs_queue: &mut crate::vfs_queue::VfsRequestQueue,
    request: &VmMmapIn,
) -> Result<MmapResult, MmapError> {
    let flags = MmapFlags::from_bits_truncate(request.flags);
    let prot = ProtFlags::from_bits_truncate(request.prot);

    // RS and VFS can do slightly more special mmap() things (C mmap.c:208-210)
    let execpriv = request.caller == Endpoint::VFS || request.caller == Endpoint::RS;

    // 1. Target process — MAP_THIRDPARTY requires execpriv (C mmap.c:211-221)
    let target = if flags.contains(MmapFlags::THIRDPARTY) {
        if !execpriv {
            return Err(MmapError::PermissionDenied);
        }
        request.forwhom
    } else {
        request.caller
    };
    let slot = table.vm_isokendpt(target)?;

    // 2. "SUSv3 specifies that mmap() should fail if length is 0" (C mmap.c:224)
    if request.length.0 == 0 {
        return Err(MmapError::InvalidLength);
    }

    // 3. Shared-type flags sanity — minix-rs tightening (doc 20 §3.6):
    //    exactly one of MAP_SHARED / MAP_PRIVATE (C doesn't check).
    if !flags.is_valid() {
        return Err(MmapError::InvalidFlags);
    }

    let aligned_len = VirBytes(roundup_page(request.length.0));

    // 4. Anonymous vs file mapping (C mmap.c:227-268)
    if request.fd == -1 || flags.contains(MmapFlags::ANONYMOUS) {
        // MAP_ANON with a real fd is rejected (C mmap.c:229-233)
        if request.fd != -1 {
            return Err(MmapError::InvalidFlags);
        }
        // Contiguous phys memory has to be preallocated (C mmap.c:242-245)
        if flags.contains(MmapFlags::CONTIG) && !flags.contains(MmapFlags::PREALLOC) {
            return Err(MmapError::InvalidFlags);
        }
        // MAP_UNINITIALIZED is execpriv-only (C mmap_region mmap.c:46-50;
        // C fails with ENOMEM, minix-rs with EINVAL — doc 20 §3.6)
        if flags.contains(MmapFlags::UNINITIALIZED) && !execpriv {
            return Err(MmapError::InvalidFlags);
        }

        let mut active = table.get_active(slot).ok_or(MmapError::ProcessNotFound)?;

        // C anon path passes VR_WRITABLE | VR_ANON unconditionally
        // (mmap.c:247-253); minix-rs derives WRITABLE from PROT_WRITE
        // (POSIX-conformant tightening, doc 20 §3.6).
        let mut vr_flags = prot.to_vr_flags(flags);
        vr_flags |= VrFlags::ANON;
        let mem_type: &'static dyn MemType = if flags.contains(MmapFlags::CONTIG) {
            &MEM_TYPE_CONTIG_ANON
        } else {
            &MEM_TYPE_ANON
        };

        let vaddr = mmap_region(
            &mut active,
            page_alloc,
            frames,
            request.addr,
            flags,
            aligned_len,
        )?;

        let region = VirRegion::with_memtype(vaddr, aligned_len, vr_flags, mem_type);
        active
            .regions_mut()
            .insert(region)
            .expect("mmap: overlap already checked in mmap_region");
        active.add_total(aligned_len);

        Ok(MmapResult::Complete(MmapResponse { mapped_addr: vaddr }))
    } else {
        // File mapping might be disabled (C mmap.c:255)
        if !filemap_enabled() {
            return Err(MmapError::FileMapDisabled);
        }
        // For files, writable MAP_SHARED mappings are not accepted
        // (C mmap.c:258-261)
        if flags.contains(MmapFlags::SHARED) && prot.contains(ProtFlags::WRITE) {
            return Err(MmapError::FileMapDisabled);
        }

        // C: vfs_request(VMVFSREQ_FDLOOKUP, fd, vmp, 0, 0, mmap_file_cont,
        // NULL, m, sizeof(*m)) → SUSPEND (mmap.c:263-268). The request is
        // queued; the VFS reply resumes through `mmap_file_cont`.
        let vreq = VfsRequest {
            request_type: VfsRequestType::FdLookup,
            req_id: 0, // assigned by the queue
            caller_endpoint: target,
            fd: request.fd,
            offset: request.offset,
            length: aligned_len.0 as u32,
            callback: Some(mmap_file_cont),
            state: Some(VfsRequestState::FdLookup { mmap: *request }),
        };
        // C: vfs_request failure → ENXIO (mmap.c:266-268)
        vfs_queue.request(vreq).map_err(|_| MmapError::FileMapDisabled)?;
        Ok(MmapResult::Suspended)
    }
}

// ── File mapping (C mmap_file, mmap.c:84-132) ────────────────────────

/// Parameters for creating a file-backed mapping.
///
/// The two entry points (`do_vfs_mmap` and the `mmap_file_cont` callback)
/// supply these from different sources (VFS message vs. original mmap
/// request + FDLOOKUP reply); the shared `mmap_file` logic lives here.
#[derive(Debug, Clone, Copy)]
struct FileMapParams {
    /// Requested virtual address (hint or MAP_FIXED target).
    addr: VirBytes,
    /// mmap flags (MAP_FIXED / MAP_PRIVATE / MAP_SHARED / ...).
    flags: MmapFlags,
    /// Raw mapping length in bytes (page-rounded inside `mmap_file`).
    len: VirBytes,
    /// File offset; page-aligned down, the remainder becomes the returned
    /// address's page offset (C mmap.c:91-96).
    file_offset: u64,
    /// VFS-provided file descriptor / device / inode.
    fd: i32,
    dev: u64,
    ino: u64,
    /// Zero-padding at the end of the last page (COW block), VFS path only.
    clearend: u16,
    /// Region is writable (C: PROT_WRITE for the user path, MVM_WRITABLE
    /// for the VFS path).
    writable: bool,
    /// May close the fd when the last reference drops (C mmap.c:128 `mayclosefd`).
    mayclosefd: bool,
}

/// C `mmap_file` (mmap.c:84-132): finish a file-backed mapping once VFS
/// has provided the file metadata (fd/dev/ino).
fn mmap_file(
    active: &mut ActiveProc<'_>,
    page_alloc: &mut VmPageAllocator,
    frames: &mut PageFrames,
    vfs_queue: &mut crate::vfs_queue::VfsRequestQueue,
    params: FileMapParams,
) -> Result<MmapResponse, MmapError> {
    // C mmap.c:91-96: page-align the file offset down; the low-order
    // remainder is carried into the returned address so the caller sees
    // the original offset semantics (retaddr = vr->vaddr + page_offset).
    let page_offset = params.file_offset % PAGE_SIZE;
    let file_offset = params.file_offset - page_offset;
    let len = VirBytes(roundup_page(params.len.0 + page_offset));

    // C mmap.c:90: `if(writable) vrflags |= VR_WRITABLE;`
    let mut vr_flags = VrFlags::empty();
    if params.writable {
        vr_flags |= VrFlags::WRITABLE;
    }

    let vaddr = mmap_region(
        active,
        page_alloc,
        frames,
        params.addr,
        params.flags,
        len,
    )?;

    let mut region = VirRegion::with_memtype(vaddr, len, vr_flags, &MEM_TYPE_MAPPED_FILE);

    // C: mappedfile_setfile (mem_file.c:191) — record the file identity so
    // the fdref / page-cache machinery can resolve pages (23-vfs-interaction).
    // fdref_dedup_or_new (fdref.c:161-177) may return a pending close for a
    // newly discovered duplicate fd (same dev+ino, different fd, may_close).
    let fdref_table = crate::fdref::FdRefTable::get_global();
    let (fdref_id, close) =
        fdref_table.dedup_or_new(params.fd, params.dev, params.ino, params.mayclosefd);
    fdref_table.ref_entry(fdref_id);
    if let Some(close) = close {
        // C: fdref_dedup_or_new (fdref.c:167-172) sends VMVFSREQ_FDCLOSE for
        // the duplicate fd and continues — a close failure is only a
        // diagnostic (VFS prints it), not a mapping failure.
        let vreq = crate::vfs_queue::VfsRequest {
            request_type: crate::vfs_queue::VfsRequestType::FdClose,
            req_id: 0, // assigned by the queue
            caller_endpoint: active.endpoint(),
            fd: close.fd,
            offset: 0,
            length: 0,
            callback: None,
            state: None,
        };
        let _ = vfs_queue.request(vreq);
    }
    region.param = VrParam::File {
        inited: true,
        fdref_id: Some(fdref_id),
        offset: file_offset,
        clearend: params.clearend,
    };

    active
        .regions_mut()
        .insert(region)
        .expect("mmap_file: overlap already checked in mmap_region");
    active.add_total(len);

    Ok(MmapResponse {
        mapped_addr: VirBytes(vaddr.0 + page_offset),
    })
}

// ── VFS-initiated mapping (C do_vfs_mmap, mmap.c:135-158) ────────────

pub(crate) fn handle_vfs_mmap(
    table: &VmProcTable,
    page_alloc: &mut VmPageAllocator,
    frames: &mut PageFrames,
    vfs_queue: &mut crate::vfs_queue::VfsRequestQueue,
    request: &VmVfsMmapIn,
) -> Result<MmapResult, MmapError> {
    // C do_vfs_mmap (mmap.c:141): "It might be disabled"
    if !filemap_enabled() {
        return Err(MmapError::FileMapDisabled);
    }

    let slot = table.vm_isokendpt(request.who)?;
    let mut active = table.get_active(slot).ok_or(MmapError::ProcessNotFound)?;

    // C do_vfs_mmap (mmap.c:148-155): mmap_file(vmp, fd, offset,
    // MAP_PRIVATE | MAP_FIXED, ino, dev, LONG_MAX*PAGE_SIZE, vaddr, len,
    // &v, clearend, flags, 0).
    let params = FileMapParams {
        addr: request.vaddr,
        flags: MmapFlags::PRIVATE | MmapFlags::FIXED,
        len: request.length,
        file_offset: request.offset,
        fd: request.fd,
        dev: request.dev,
        ino: request.ino,
        clearend: request.clearend,
        // C: the u16 `flags` field is passed through as the `writable`
        // parameter (mmap.c:155); VFS sets MVM_WRITABLE (0x8000) for
        // PROT_WRITE segments (vfs/exec.c:167-173).
        writable: request.flags != 0,
        mayclosefd: false,
    };

    let response = mmap_file(&mut active, page_alloc, frames, vfs_queue, params)?;
    Ok(MmapResult::Complete(response))
}

// ── VFS callback (C mmap_file_cont, mmap.c:160-190) ──────────────────

/// VFS callback that finishes a suspended user file mapping.
///
/// C `mmap_file_cont` (mmap.c:160-190): invoked from `do_vfs_reply` when
/// the FDLOOKUP request returns. On success it runs `mmap_file` and then
/// unblocks the requesting process with `ipc_send`.
///
/// Transport note: the final `ipc_send` to the suspended caller requires
/// the kernel IPC transport (`ipc/transport.rs` — `KernelIpcTransport`
/// pending). The region is created here so the resume path only needs to
/// deliver the message; the errno for the failure case is preserved in the
/// reply (23-vfs-interaction / transport scope).
pub(crate) fn mmap_file_cont(
    server: &mut crate::vm_server::VmServer,
    reply: &VfsReply,
    state: &VfsRequestState,
) -> Result<(), VfsQueueError> {
    let VfsRequestState::FdLookup { mmap, .. } = state else {
        return Err(VfsQueueError::NoCallbackState);
    };

    // C mmap_file_cont (mmap.c:166-168): writable = PROT_WRITE in the
    // original message.
    let prot = ProtFlags::from_bits_truncate(mmap.prot);
    let writable = prot.contains(ProtFlags::WRITE);

    if reply.result != OK {
        // C: result = replymsg->VMV_RESULT; the process is unblocked with
        // that errno. The resume reply is transport-gated (see note above).
        return Ok(());
    }

    let table = VmProcTable::get_global();
    let (page_alloc, frames, _cache, vfs_queue) = server.parts_mut();

    // C mmap_file: vmp = the target process (forwhom for THIRDPARTY).
    let flags = MmapFlags::from_bits_truncate(mmap.flags);
    let target = if flags.contains(MmapFlags::THIRDPARTY) {
        mmap.forwhom
    } else {
        mmap.caller
    };
    let slot = table.vm_isokendpt(target).map_err(|_| VfsQueueError::InvalidFd)?;
    let mut active = table.get_active(slot).ok_or(VfsQueueError::InvalidFd)?;

    let params = FileMapParams {
        addr: mmap.addr,
        flags,
        len: mmap.length,
        file_offset: mmap.offset,
        fd: reply.fd,
        dev: reply.dev,
        ino: reply.ino,
        clearend: 0,
        writable,
        // C mmap_file_cont (mmap.c:180): mayclosefd = 1 — the user holds
        // the original fd and the mapping may outlive it.
        mayclosefd: true,
    };
    let _ = mmap_file(&mut active, page_alloc, frames, vfs_queue, params)
        .map_err(|_| VfsQueueError::IoError)?;

    Ok(())
}

// C: do_mmap uses `OK` (0) for success in the VFS reply path.
const OK: i32 = 0;

// ── Tests ────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vfs_queue::VfsRequestQueue;
    use crate::vmproc::VmProcTable;
    use crate::phys_mem::{BitmapAllocator, PhysAlloc};
    use crate::region::PAGE_SIZE as REGION_PAGE_SIZE;
    use minix_types::{PhysBytes, UserSlot};

    fn make_frames() -> PageFrames {
        PageFrames::new(PhysBytes(256 * REGION_PAGE_SIZE as u64))
    }

    fn make_page_alloc() -> VmPageAllocator {
        VmPageAllocator::new(PhysAlloc::Bitmap(BitmapAllocator::new_for_test(256)))
    }

    fn init_test_process(slot: UserSlot) -> Endpoint {
        let table = VmProcTable::get_global();
        unsafe { table.reset_slot(slot); }
        let empty = table.get_empty(slot).unwrap();
        let ep = Endpoint::from_generation_slot(1, slot.get() as i32);
        let mut active = empty.activate(ep);
        // Skip init_page_table() — mmap tests don't need page table access,
        // and init_page_table() accesses mock physical memory causing SIGSEGV.
        active.init_regions();
        ep
    }

    fn anon_req(slot: UserSlot, extra_flags: u32, fd: i32) -> VmMmapIn {
        VmMmapIn {
            caller: Endpoint::from_generation_slot(1, slot.get() as i32),
            forwhom: Endpoint::NONE,
            addr: VirBytes(0),
            length: VirBytes(0x1000),
            prot: ProtFlags::READ.bits() | ProtFlags::WRITE.bits(),
            flags: MmapFlags::PRIVATE.bits() | MmapFlags::ANONYMOUS.bits() | extra_flags,
            fd,
            offset: 0,
        }
    }

    #[test]
    fn test_mmap_anonymous_basic() {
        let table = VmProcTable::get_global();
        let mut page_alloc = make_page_alloc();
        let mut frames = make_frames();
        let mut queue = VfsRequestQueue::new();
        let slot = UserSlot::new(70);
        let ep = init_test_process(slot);

        let req = anon_req(slot, 0, -1);
        let result = handle_mmap(table, &mut page_alloc, &mut frames, &mut queue, &req);
        assert!(result.is_ok());
        let _ = ep;
    }

    #[test]
    fn test_mmap_zero_length_fails() {
        let table = VmProcTable::get_global();
        let mut page_alloc = make_page_alloc();
        let mut frames = make_frames();
        let mut queue = VfsRequestQueue::new();
        let slot = UserSlot::new(87);
        let ep = init_test_process(slot);

        let req = VmMmapIn {
            caller: ep,
            forwhom: Endpoint::NONE,
            addr: VirBytes(0),
            length: VirBytes(0),
            prot: 0,
            flags: MmapFlags::PRIVATE.bits() | MmapFlags::ANONYMOUS.bits(),
            fd: -1,
            offset: 0,
        };

        let result = handle_mmap(table, &mut page_alloc, &mut frames, &mut queue, &req);
        assert!(matches!(result, Err(MmapError::InvalidLength)));
    }

    #[test]
    fn test_mmap_flags_validation() {
        let table = VmProcTable::get_global();
        let mut page_alloc = make_page_alloc();
        let mut frames = make_frames();
        let mut queue = VfsRequestQueue::new();
        let slot = UserSlot::new(71);
        init_test_process(slot);

        let req = VmMmapIn {
            caller: Endpoint::from_generation_slot(1, slot.get() as i32),
            forwhom: Endpoint::NONE,
            addr: VirBytes(0),
            length: VirBytes(0x1000),
            prot: ProtFlags::READ.bits(),
            flags: 0,
            fd: -1,
            offset: 0,
        };

        let result = handle_mmap(table, &mut page_alloc, &mut frames, &mut queue, &req);
        assert!(matches!(result, Err(MmapError::InvalidFlags)));
    }

    #[test]
    fn test_mmap_anon_with_fd_rejected() {
        // C mmap.c:229-233 — MAP_ANON with a real fd is EINVAL.
        let table = VmProcTable::get_global();
        let mut page_alloc = make_page_alloc();
        let mut frames = make_frames();
        let mut queue = VfsRequestQueue::new();
        let slot = UserSlot::new(75);
        init_test_process(slot);

        let req = anon_req(slot, 0, 5);
        let result = handle_mmap(table, &mut page_alloc, &mut frames, &mut queue, &req);
        assert!(matches!(result, Err(MmapError::InvalidFlags)));
    }

    #[test]
    fn test_mmap_error_to_errno() {
        // Tests the full error path: MmapError → From<MmapError> for VmError → VmError::to_errno()
        use minix_types::{VmError, EFAULT, EINVAL, ENOMEM, EPERM, ENXIO, ESRCH};
        assert_eq!(VmError::from(MmapError::InvalidLength).to_errno(), EINVAL);      // InvalidLength → InvalidParam → EINVAL (C mmap.c:224)
        assert_eq!(VmError::from(MmapError::InvalidFlags).to_errno(), EINVAL);       // InvalidFlags → InvalidParam → EINVAL (C mmap.c:229-245)
        assert_eq!(VmError::from(MmapError::BadAddress).to_errno(), EFAULT);         // BadAddress → InvalidAddress → EFAULT
        assert_eq!(VmError::from(MmapError::OutOfMemory).to_errno(), ENOMEM);        // OutOfMemory → OutOfMemory → ENOMEM
        assert_eq!(VmError::from(MmapError::PermissionDenied).to_errno(), EPERM);    // PermissionDenied → PermissionDenied → EPERM
        assert_eq!(VmError::from(MmapError::FileMapDisabled).to_errno(), ENXIO);     // FileMapDisabled → NoDevice → ENXIO (C mmap.c:255-261)
        assert_eq!(VmError::from(MmapError::ProcessNotFound).to_errno(), ESRCH);     // ProcessNotFound → InvalidEndpoint → ESRCH (C mmap.c:217)
    }

    #[test]
    fn test_mmap_contig_without_prealloc_fails() {
        let table = VmProcTable::get_global();
        let mut page_alloc = make_page_alloc();
        let mut frames = make_frames();
        let mut queue = VfsRequestQueue::new();
        let slot = UserSlot::new(72);
        init_test_process(slot);

        let req = anon_req(slot, MmapFlags::CONTIG.bits(), -1);
        let result = handle_mmap(table, &mut page_alloc, &mut frames, &mut queue, &req);
        assert!(matches!(result, Err(MmapError::InvalidFlags)));
    }

    #[test]
    fn test_mmap_thirdparty_no_priv() {
        let table = VmProcTable::get_global();
        let mut page_alloc = make_page_alloc();
        let mut frames = make_frames();
        let mut queue = VfsRequestQueue::new();
        let slot = UserSlot::new(73);
        init_test_process(slot);

        let req = anon_req(slot, MmapFlags::THIRDPARTY.bits(), -1);
        let result = handle_mmap(table, &mut page_alloc, &mut frames, &mut queue, &req);
        assert!(matches!(result, Err(MmapError::PermissionDenied)));
    }

    #[test]
    fn test_mmap_uninitialized_no_priv() {
        let table = VmProcTable::get_global();
        let mut page_alloc = make_page_alloc();
        let mut frames = make_frames();
        let mut queue = VfsRequestQueue::new();
        let slot = UserSlot::new(74);
        init_test_process(slot);

        let req = anon_req(slot, MmapFlags::UNINITIALIZED.bits(), -1);
        let result = handle_mmap(table, &mut page_alloc, &mut frames, &mut queue, &req);
        assert!(matches!(result, Err(MmapError::InvalidFlags)));
    }

    #[test]
    fn test_mmap_fixed_unmaps_existing() {
        // MAP_FIXED replaces whatever occupies the range (C mmap.c:60-68).
        let table = VmProcTable::get_global();
        let mut page_alloc = make_page_alloc();
        let mut frames = make_frames();
        let mut queue = VfsRequestQueue::new();
        let slot = UserSlot::new(76);
        let ep = init_test_process(slot);

        // First mapping: hint at a page-aligned address inside the mmap range.
        let req1 = VmMmapIn {
            caller: ep,
            forwhom: Endpoint::NONE,
            addr: VirBytes(0x0000_0001_0000_1000),
            length: VirBytes(0x1000),
            prot: ProtFlags::READ.bits() | ProtFlags::WRITE.bits(),
            flags: MmapFlags::PRIVATE.bits() | MmapFlags::ANONYMOUS.bits(),
            fd: -1,
            offset: 0,
        };
        let r1 = handle_mmap(table, &mut page_alloc, &mut frames, &mut queue, &req1);
        assert!(matches!(r1, Ok(MmapResult::Complete(_))));

        // MAP_FIXED at the same address must succeed and replace.
        let req2 = VmMmapIn {
            caller: ep,
            forwhom: Endpoint::NONE,
            addr: VirBytes(0x0000_0001_0000_1000),
            length: VirBytes(0x1000),
            prot: ProtFlags::READ.bits() | ProtFlags::WRITE.bits(),
            flags: MmapFlags::PRIVATE.bits() | MmapFlags::ANONYMOUS.bits() | MmapFlags::FIXED.bits(),
            fd: -1,
            offset: 0,
        };
        let r2 = handle_mmap(table, &mut page_alloc, &mut frames, &mut queue, &req2);
        match r2 {
            Ok(MmapResult::Complete(resp)) => assert_eq!(resp.mapped_addr.0, 0x0000_0001_0000_1000),
            other => panic!("MAP_FIXED over existing mapping must succeed, got {:?}", other),
        }
    }

    #[test]
    fn test_mmap_fixed_zero_addr_rejected() {
        // minix-rs tightening: C would try to map at address 0.
        let table = VmProcTable::get_global();
        let mut page_alloc = make_page_alloc();
        let mut frames = make_frames();
        let mut queue = VfsRequestQueue::new();
        let slot = UserSlot::new(77);
        let ep = init_test_process(slot);

        let req = VmMmapIn {
            caller: ep,
            forwhom: Endpoint::NONE,
            addr: VirBytes(0),
            length: VirBytes(0x1000),
            prot: ProtFlags::READ.bits() | ProtFlags::WRITE.bits(),
            flags: MmapFlags::PRIVATE.bits() | MmapFlags::ANONYMOUS.bits() | MmapFlags::FIXED.bits(),
            fd: -1,
            offset: 0,
        };
        let result = handle_mmap(table, &mut page_alloc, &mut frames, &mut queue, &req);
        assert!(matches!(result, Err(MmapError::BadAddress)));
    }

    #[test]
    fn test_mmap_hint_exact_fit() {
        let table = VmProcTable::get_global();
        let mut page_alloc = make_page_alloc();
        let mut frames = make_frames();
        let mut queue = VfsRequestQueue::new();
        let slot = UserSlot::new(78);
        let ep = init_test_process(slot);

        let req = VmMmapIn {
            caller: ep,
            forwhom: Endpoint::NONE,
            addr: VirBytes(0x0000_0001_0000_2000),
            length: VirBytes(0x2000),
            prot: ProtFlags::READ.bits() | ProtFlags::WRITE.bits(),
            flags: MmapFlags::PRIVATE.bits() | MmapFlags::ANONYMOUS.bits(),
            fd: -1,
            offset: 0,
        };
        match handle_mmap(table, &mut page_alloc, &mut frames, &mut queue, &req) {
            Ok(MmapResult::Complete(resp)) => assert_eq!(resp.mapped_addr.0, 0x0000_0001_0000_2000),
            other => panic!("free hint must map exactly there, got {:?}", other),
        }
    }

    #[test]
    fn test_mmap_hint_taken_falls_back() {
        let table = VmProcTable::get_global();
        let mut page_alloc = make_page_alloc();
        let mut frames = make_frames();
        let mut queue = VfsRequestQueue::new();
        let slot = UserSlot::new(79);
        let ep = init_test_process(slot);

        // Occupy the hint range first.
        let req1 = VmMmapIn {
            caller: ep,
            forwhom: Endpoint::NONE,
            addr: VirBytes(0x0000_0001_0000_3000),
            length: VirBytes(0x1000),
            prot: ProtFlags::READ.bits() | ProtFlags::WRITE.bits(),
            flags: MmapFlags::PRIVATE.bits() | MmapFlags::ANONYMOUS.bits(),
            fd: -1,
            offset: 0,
        };
        handle_mmap(table, &mut page_alloc, &mut frames, &mut queue, &req1).unwrap();

        // Same hint again: must fall back to a different address.
        let req2 = VmMmapIn {
            caller: ep,
            forwhom: Endpoint::NONE,
            addr: VirBytes(0x0000_0001_0000_3000),
            length: VirBytes(0x1000),
            prot: ProtFlags::READ.bits() | ProtFlags::WRITE.bits(),
            flags: MmapFlags::PRIVATE.bits() | MmapFlags::ANONYMOUS.bits(),
            fd: -1,
            offset: 0,
        };
        match handle_mmap(table, &mut page_alloc, &mut frames, &mut queue, &req2) {
            Ok(MmapResult::Complete(resp)) => {
                assert_ne!(resp.mapped_addr.0, 0x0000_0001_0000_3000);
                assert!(resp.mapped_addr.0 >= MMAP_BASE && resp.mapped_addr.0 < MMAP_TOP);
            }
            other => panic!("occupied hint must fall back, got {:?}", other),
        }
    }

    #[test]
    fn test_mmap_file_disabled() {
        let table = VmProcTable::get_global();
        let mut page_alloc = make_page_alloc();
        let mut frames = make_frames();
        let mut queue = VfsRequestQueue::new();
        let slot = UserSlot::new(80);
        init_test_process(slot);

        let ep = Endpoint::from_generation_slot(1, slot.get() as i32);
        let req = VmMmapIn {
            caller: ep,
            forwhom: Endpoint::NONE,
            addr: VirBytes(0),
            length: VirBytes(0x1000),
            prot: ProtFlags::READ.bits(),
            flags: MmapFlags::PRIVATE.bits(),
            fd: 3,
            offset: 0,
        };

        set_filemap_enabled(false);
        let result = handle_mmap(table, &mut page_alloc, &mut frames, &mut queue, &req);
        set_filemap_enabled(true);
        assert!(matches!(result, Err(MmapError::FileMapDisabled)));
    }

    #[test]
    fn test_mmap_file_shared_write_rejected() {
        // C mmap.c:258-261 — writable MAP_SHARED file mappings → ENXIO.
        let table = VmProcTable::get_global();
        let mut page_alloc = make_page_alloc();
        let mut frames = make_frames();
        let mut queue = VfsRequestQueue::new();
        let slot = UserSlot::new(81);
        init_test_process(slot);

        let ep = Endpoint::from_generation_slot(1, slot.get() as i32);
        let req = VmMmapIn {
            caller: ep,
            forwhom: Endpoint::NONE,
            addr: VirBytes(0),
            length: VirBytes(0x1000),
            prot: ProtFlags::READ.bits() | ProtFlags::WRITE.bits(),
            flags: MmapFlags::SHARED.bits(),
            fd: 3,
            offset: 0,
        };
        let result = handle_mmap(table, &mut page_alloc, &mut frames, &mut queue, &req);
        assert!(matches!(result, Err(MmapError::FileMapDisabled)));
    }

    #[test]
    fn test_mmap_file_enqueues_vfs_request() {
        // A file mapping must enqueue an FDLOOKUP request and suspend
        // (C mmap.c:263-268) — not silently complete.
        let table = VmProcTable::get_global();
        let mut page_alloc = make_page_alloc();
        let mut frames = make_frames();
        let mut queue = VfsRequestQueue::new();
        let slot = UserSlot::new(82);
        init_test_process(slot);

        let ep = Endpoint::from_generation_slot(1, slot.get() as i32);
        let req = VmMmapIn {
            caller: ep,
            forwhom: Endpoint::NONE,
            addr: VirBytes(0),
            length: VirBytes(0x1000),
            prot: ProtFlags::READ.bits(),
            flags: MmapFlags::PRIVATE.bits(),
            fd: 3,
            offset: 0x1000,
        };
        let result = handle_mmap(table, &mut page_alloc, &mut frames, &mut queue, &req);
        assert!(matches!(result, Ok(MmapResult::Suspended)));
        assert!(queue.has_active());
        assert_eq!(queue.active_req_id(), Some(1));
    }

    #[test]
    fn test_vfs_mmap_basic() {
        let table = VmProcTable::get_global();
        let mut page_alloc = make_page_alloc();
        let mut frames = make_frames();
        let mut queue = VfsRequestQueue::new();
        let slot = UserSlot::new(83);
        let ep = init_test_process(slot);

        let req = VmVfsMmapIn {
            who: ep,
            fd: 9,
            offset: 0,
            dev: 0xABCD,
            ino: 42,
            vaddr: VirBytes(0x0000_0001_0000_4000),
            length: VirBytes(0x2000),
            flags: 0, // read-only segment
            clearend: 0,
        };
        match handle_vfs_mmap(table, &mut page_alloc, &mut frames, &mut queue, &req) {
            Ok(MmapResult::Complete(resp)) => {
                assert_eq!(resp.mapped_addr.0, 0x0000_0001_0000_4000);
                let active = table.get_active(table.vm_isokendpt(ep).unwrap()).unwrap();
                let region = active.regions().find_overlap(
                    VirBytes(0x0000_0001_0000_4000),
                    VirBytes(0x0000_0001_0000_6000),
                ).expect("region must exist");
                assert!(!region.flags.contains(VrFlags::WRITABLE), "read-only VFS_MMAP must not be writable");
                assert!(matches!(region.param, VrParam::File { inited: true, .. }));
            }
            other => panic!("vfs_mmap must complete, got {:?}", other),
        }
    }

    #[test]
    fn test_vfs_mmap_writable_flag() {
        // VFS sets MVM_WRITABLE (0x8000) for PROT_WRITE segments.
        let table = VmProcTable::get_global();
        let mut page_alloc = make_page_alloc();
        let mut frames = make_frames();
        let mut queue = VfsRequestQueue::new();
        let slot = UserSlot::new(84);
        let ep = init_test_process(slot);

        let req = VmVfsMmapIn {
            who: ep,
            fd: 9,
            offset: 0,
            dev: 0xABCD,
            ino: 43,
            vaddr: VirBytes(0x0000_0001_0000_5000),
            length: VirBytes(0x1000),
            flags: 0x8000,
            clearend: 0,
        };
        handle_vfs_mmap(table, &mut page_alloc, &mut frames, &mut queue, &req).unwrap();
        let active = table.get_active(table.vm_isokendpt(ep).unwrap()).unwrap();
        let region = active.regions().find_overlap(
            VirBytes(0x0000_0001_0000_5000),
            VirBytes(0x0000_0001_0000_6000),
        ).expect("region must exist");
        assert!(region.flags.contains(VrFlags::WRITABLE));
    }

    #[test]
    fn test_vfs_mmap_page_offset() {
        // C mmap_file (mmap.c:91-96): a non-page-aligned file offset is
        // carried into the returned address (retaddr = vaddr + page_offset).
        let table = VmProcTable::get_global();
        let mut page_alloc = make_page_alloc();
        let mut frames = make_frames();
        let mut queue = VfsRequestQueue::new();
        let slot = UserSlot::new(85);
        let ep = init_test_process(slot);

        let req = VmVfsMmapIn {
            who: ep,
            fd: 9,
            offset: 0x100, // 256-byte page offset (below PAGE_SIZE)
            dev: 0xABCD,
            ino: 44,
            vaddr: VirBytes(0x0000_0001_0000_6000),
            length: VirBytes(0x1000),
            flags: 0,
            clearend: 0,
        };
        match handle_vfs_mmap(table, &mut page_alloc, &mut frames, &mut queue, &req) {
            Ok(MmapResult::Complete(resp)) => {
                // page_offset = 0x100 → retaddr = vaddr + 0x100
                assert_eq!(resp.mapped_addr.0, 0x0000_0001_0000_6100);
                let active = table.get_active(table.vm_isokendpt(ep).unwrap()).unwrap();
                let region = active.regions().find_overlap(
                    VirBytes(0x0000_0001_0000_6000),
                    VirBytes(0x0000_0001_0000_8000),
                ).expect("region must exist");
                // len = roundup(0x1000 + 0x100) = 0x2000; region covers
                // [0x6000, 0x8000).
                assert_eq!(region.length.0, 0x2000);
            }
            other => panic!("vfs_mmap must complete, got {:?}", other),
        }
    }

    #[test]
    fn test_vfs_mmap_disabled() {
        let table = VmProcTable::get_global();
        let mut page_alloc = make_page_alloc();
        let mut frames = make_frames();
        let mut queue = VfsRequestQueue::new();
        let slot = UserSlot::new(86);
        let ep = init_test_process(slot);

        let req = VmVfsMmapIn {
            who: ep,
            fd: 9,
            offset: 0,
            dev: 0xABCD,
            ino: 45,
            vaddr: VirBytes(0x0000_0001_0000_9000),
            length: VirBytes(0x1000),
            flags: 0,
            clearend: 0,
        };
        set_filemap_enabled(false);
        let result = handle_vfs_mmap(table, &mut page_alloc, &mut frames, &mut queue, &req);
        set_filemap_enabled(true);
        assert!(matches!(result, Err(MmapError::FileMapDisabled)));
    }
}
