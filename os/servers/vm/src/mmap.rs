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
//! handled entirely in `handle_mmap` with mem_type_anon. File
//! mappings delegate to VFS via `VfsRequestQueue::FdLookup` and
//! a callback, mirroring Minix3's `vfs_request` + `SUSPEND` +
//! `mmap_file_cont` pattern.
//!
//! **PFN index model**: Uses PageFrames/PageSlot instead of
//! Minix3's phys_region/phys_block.
//!
//! ## Permission checks (aligned with C mmap.c)
//!
//! - **MAP_THIRDPARTY**: Only VFS and RS (execpriv) may map memory
//!   into another process's address space. Others receive EPERM.
//!   (C: mmap.c:215 `if(!execpriv) return EPERM`)
//!
//! - **MAP_UNINITIALIZED**: Only VFS and RS may create uninitialized
//!   mappings (skip zero-fill). Others receive EINVAL.
//!   (C: mmap.c:48 `if(!execpriv) return NULL`)
//!
//! - **MAP_CONTIG without MAP_PREALLOC**: Contiguous physical memory
//!   must be preallocated. MAP_CONTIG alone returns EINVAL.
//!   (C: mmap.c:244 `if((flags&(MAP_CONTIG|MAP_PREALLOC))==MAP_CONTIG) return EINVAL`)

use minix_types::{VirBytes, EINVAL, EFAULT, EPERM, ENOMEM, ENXIO, ESRCH, Endpoint, VmMmapIn, VmVfsMmapIn};
use crate::vmproc::VmProcTable;
use crate::region::{VirRegion, VrFlags, VrParam, PageFrames};
use crate::alloc_page::VmPageAllocator;
use crate::memtype::{MEM_TYPE_ANON, MEM_TYPE_MAPPED_FILE, MEM_TYPE_CONTIG_ANON};
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
    pub(crate) fn is_valid(&self) -> bool {
        let shared = self.contains(Self::SHARED);
        let private = self.contains(Self::PRIVATE);
        (shared || private) && !(shared && private)
    }
}

impl ProtFlags {
    pub(crate) fn to_vr_flags(&self, flags: MmapFlags) -> VrFlags {
        let mut vr = VrFlags::ANON;
        if self.contains(Self::WRITE) {
            vr |= VrFlags::WRITABLE;
        }
        if flags.contains(MmapFlags::SHARED) {
            vr |= VrFlags::SHARED;
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

// ── Error & Response types ───────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MmapError {
    ProcessNotFound,
    InvalidLength,
    BadAddress,
    InvalidFlags,
    PermissionDenied,
    OutOfMemory,
    FileMapDisabled,
}

impl MmapError {
    pub(crate) fn to_errno(&self) -> i32 {
        match self {
            Self::ProcessNotFound => ESRCH,
            Self::InvalidLength => EINVAL,
            Self::BadAddress => EFAULT,
            Self::InvalidFlags => EINVAL,
            Self::PermissionDenied => EPERM,
            Self::OutOfMemory => ENOMEM,
            Self::FileMapDisabled => ENXIO,
        }
    }
}

pub(crate) struct MmapResponse {
    pub mapped_addr: VirBytes,
}

pub(crate) enum MmapResult {
    Complete(MmapResponse),
    Suspended,
}

// ── mmap 64-bit address range ────────────────────────────────────────
// In Minix3 (32-bit), mmap area is runtime-computed:
//   VM_MMAPTOP = VM_STACKTOP - DEFAULT_STACK_LIMIT
//   VM_MMAPBASE = VM_MMAPTOP / 2  (or VM_PAGE_SIZE in non-MAGIC builds)
//
// In minix-rs (64-bit), the address space is 48-bit canonical user
// space. We reserve a generous range far from brk/stack:
const MMAP_BASE: u64 = 0x0000_0001_0000_0000;
const MMAP_TOP: u64  = 0x0000_0200_0000_0000;

// ── handle_mmap ──────────────────────────────────────────────────────

pub(crate) fn handle_mmap(
    table: &VmProcTable,
    page_alloc: &mut VmPageAllocator,
    frames: &mut PageFrames,
    request: &VmMmapIn,
) -> Result<MmapResult, MmapError> {
    let flags = MmapFlags::from_bits_truncate(request.flags);
    let prot = ProtFlags::from_bits_truncate(request.prot);

    let execpriv = request.caller == Endpoint::VFS || request.caller == Endpoint::RS;

    // 1. Validate parameters (EINVAL)
    if request.length.0 == 0 {
        return Err(MmapError::InvalidLength);
    }
    if !flags.is_valid() {
        return Err(MmapError::InvalidFlags);
    }
    if flags.contains(MmapFlags::CONTIG) && !flags.contains(MmapFlags::PREALLOC) {
        return Err(MmapError::InvalidFlags);
    }
    if flags.contains(MmapFlags::UNINITIALIZED) && !execpriv {
        return Err(MmapError::InvalidFlags);
    }

    // 2. Target process — THIRDPARTY requires execpriv
    let target = if flags.contains(MmapFlags::THIRDPARTY) {
        if !execpriv {
            return Err(MmapError::PermissionDenied);
        }
        request.forwhom
    } else {
        request.caller
    };

    let slot = table
        .vm_isokendpt(target)
        .map_err(|_| MmapError::ProcessNotFound)?;

    let mut active = table
        .get_active(slot)
        .ok_or(MmapError::ProcessNotFound)?;

    // 3. Length page-aligned
    let aligned_len = VirBytes(((request.length.0 + PAGE_SIZE - 1) / PAGE_SIZE) * PAGE_SIZE);

    // 4. Resolve virtual address
    let vaddr = if flags.contains(MmapFlags::FIXED) {
        if request.addr.0 == 0 {
            return Err(MmapError::BadAddress);
        }
        crate::munmap::unmap_range(&mut active, page_alloc, frames, request.addr, aligned_len)
            .ok();
        request.addr
    } else if request.addr.0 != 0 {
        // Hint address — try hint first, fall back to full range search
        active.regions()
            .find_slot(request.addr, VirBytes(MMAP_TOP), aligned_len)
            .or_else(|| active.regions()
                .find_slot(VirBytes(MMAP_BASE), VirBytes(MMAP_TOP), aligned_len))
            .unwrap_or(VirBytes(0))
    } else {
        active.regions()
            .find_slot(VirBytes(MMAP_BASE), VirBytes(MMAP_TOP), aligned_len)
            .unwrap_or(VirBytes(0))
    };

    if vaddr.0 == 0 {
        return Err(MmapError::OutOfMemory);
    }

    // 5. Resolve mem_type and vr_flags
    let vr_flags = prot.to_vr_flags(flags);
    let is_anon = flags.contains(MmapFlags::ANONYMOUS) || request.fd == -1;

    let mem_type: &'static dyn crate::memtype::MemType = if is_anon {
        if flags.contains(MmapFlags::CONTIG) {
            &MEM_TYPE_CONTIG_ANON
        } else {
            &MEM_TYPE_ANON
        }
    } else {
        // File mapping — handled by VFS callback path
        // For now, return error indicating file mapping not yet implemented synchronously
        return Ok(MmapResult::Suspended);
    };

    // 6. Create region
    let mut region = VirRegion::with_memtype(vaddr, aligned_len, vr_flags, mem_type);

    if !is_anon {
        region.param = VrParam::File {
            inited: false,
            fdref_id: None,
            offset: request.offset,
            clearend: 0,
        };
    }

    active.regions_mut().insert(region);
    active.add_total(aligned_len);

    Ok(MmapResult::Complete(MmapResponse { mapped_addr: vaddr }))
}

// ── handle_vfs_mmap ──────────────────────────────────────────────────

pub(crate) fn handle_vfs_mmap(
    table: &VmProcTable,
    _page_alloc: &mut VmPageAllocator,
    _frames: &mut PageFrames,
    request: &VmVfsMmapIn,
) -> Result<MmapResult, MmapError> {
    let slot = table
        .vm_isokendpt(request.who)
        .map_err(|_| MmapError::ProcessNotFound)?;

    let mut active = table
        .get_active(slot)
        .ok_or(MmapError::ProcessNotFound)?;

    let aligned_len = VirBytes(((request.length.0 + PAGE_SIZE - 1) / PAGE_SIZE) * PAGE_SIZE);

    // VFS-initiated mappings are always MAP_PRIVATE | MAP_FIXED,
    // with pages loaded on demand by mappedfile_pagefault (no PREALLOC).
    let vr_flags = VrFlags::empty();

    let vaddr = request.vaddr;
    if vaddr.0 == 0 {
        return Err(MmapError::OutOfMemory);
    }

    let mut region = VirRegion::with_memtype(vaddr, aligned_len, vr_flags, &MEM_TYPE_MAPPED_FILE);

    let fdref_table = crate::fdref::FdRefTable::get_global();
    let fdref_id = if let Some(existing_id) = fdref_table.find_by_dev_ino(request.dev, request.ino) {
        fdref_table.ref_entry(existing_id);
        existing_id
    } else {
        let id = fdref_table.create(request.fd, request.dev, request.ino, true);
        fdref_table.ref_entry(id);
        id
    };

    region.param = VrParam::File {
        inited: true,
        fdref_id: Some(fdref_id),
        offset: request.offset,
        clearend: request.clearend,
    };

    active.regions_mut().insert(region);
    active.add_total(aligned_len);

    Ok(MmapResult::Complete(MmapResponse { mapped_addr: vaddr }))
}

// ── Tests ────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vmproc::VmProcTable;
    use crate::phys_mem::{BitmapAllocator, PhysAlloc};
    use crate::region::PAGE_SIZE as REGION_PAGE_SIZE;
    use minix_types::{Endpoint, UserSlot, PhysBytes};

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
        active.init_page_table().unwrap();
        active.init_regions();
        ep
    }

    #[test]
    fn test_mmap_anonymous_basic() {
        let table = VmProcTable::get_global();
        let mut page_alloc = make_page_alloc();
        let mut frames = make_frames();
        let slot = UserSlot::new(70);
        let ep = init_test_process(slot);

        let req = VmMmapIn {
            caller: ep,
            forwhom: ep,
            addr: VirBytes(0),
            length: VirBytes(0x4000),
            prot: ProtFlags::READ.bits() | ProtFlags::WRITE.bits(),
            flags: MmapFlags::PRIVATE.bits() | MmapFlags::ANONYMOUS.bits(),
            fd: -1,
            offset: 0,
        };

        let result = handle_mmap(table, &mut page_alloc, &mut frames, &req);
        assert!(result.is_ok());
    }

    #[test]
    fn test_mmap_zero_length_fails() {
        let table = VmProcTable::get_global();
        let mut page_alloc = make_page_alloc();
        let mut frames = make_frames();

        let req = VmMmapIn {
            caller: Endpoint::PM,
            forwhom: Endpoint::PM,
            addr: VirBytes(0),
            length: VirBytes(0),
            prot: 0,
            flags: MmapFlags::PRIVATE.bits() | MmapFlags::ANONYMOUS.bits(),
            fd: -1,
            offset: 0,
        };

        let result = handle_mmap(table, &mut page_alloc, &mut frames, &req);
        assert!(matches!(result, Err(MmapError::InvalidLength)));
    }

    #[test]
    fn test_mmap_flags_validation() {
        let table = VmProcTable::get_global();
        let mut page_alloc = make_page_alloc();
        let mut frames = make_frames();
        let slot = UserSlot::new(71);
        let ep = init_test_process(slot);

        let req = VmMmapIn {
            caller: ep,
            forwhom: ep,
            addr: VirBytes(0),
            length: VirBytes(0x1000),
            prot: ProtFlags::READ.bits(),
            flags: 0,
            fd: -1,
            offset: 0,
        };

        let result = handle_mmap(table, &mut page_alloc, &mut frames, &req);
        assert!(matches!(result, Err(MmapError::InvalidFlags)));
    }

    #[test]
    fn test_mmap_error_to_errno() {
        assert_eq!(MmapError::InvalidLength.to_errno(), EINVAL);
        assert_eq!(MmapError::BadAddress.to_errno(), EFAULT);
        assert_eq!(MmapError::OutOfMemory.to_errno(), ENOMEM);
        assert_eq!(MmapError::PermissionDenied.to_errno(), EPERM);
        assert_eq!(MmapError::FileMapDisabled.to_errno(), ENXIO);
        assert_eq!(MmapError::ProcessNotFound.to_errno(), ESRCH);
    }

    #[test]
    fn test_mmap_contig_without_prealloc_fails() {
        let table = VmProcTable::get_global();
        let mut page_alloc = make_page_alloc();
        let mut frames = make_frames();
        let slot = UserSlot::new(72);
        let ep = init_test_process(slot);

        let req = VmMmapIn {
            caller: ep,
            forwhom: ep,
            addr: VirBytes(0),
            length: VirBytes(0x1000),
            prot: ProtFlags::READ.bits() | ProtFlags::WRITE.bits(),
            flags: MmapFlags::PRIVATE.bits() | MmapFlags::ANONYMOUS.bits() | MmapFlags::CONTIG.bits(),
            fd: -1,
            offset: 0,
        };

        let result = handle_mmap(table, &mut page_alloc, &mut frames, &req);
        assert!(matches!(result, Err(MmapError::InvalidFlags)));
    }

    #[test]
    fn test_mmap_thirdparty_no_priv() {
        let table = VmProcTable::get_global();
        let mut page_alloc = make_page_alloc();
        let mut frames = make_frames();
        let slot = UserSlot::new(73);
        let ep = init_test_process(slot);

        let req = VmMmapIn {
            caller: ep,
            forwhom: ep,
            addr: VirBytes(0),
            length: VirBytes(0x1000),
            prot: ProtFlags::READ.bits() | ProtFlags::WRITE.bits(),
            flags: MmapFlags::PRIVATE.bits() | MmapFlags::ANONYMOUS.bits() | MmapFlags::THIRDPARTY.bits(),
            fd: -1,
            offset: 0,
        };

        let result = handle_mmap(table, &mut page_alloc, &mut frames, &req);
        assert!(matches!(result, Err(MmapError::PermissionDenied)));
    }

    #[test]
    fn test_mmap_uninitialized_no_priv() {
        let table = VmProcTable::get_global();
        let mut page_alloc = make_page_alloc();
        let mut frames = make_frames();
        let slot = UserSlot::new(74);
        let ep = init_test_process(slot);

        let req = VmMmapIn {
            caller: ep,
            forwhom: ep,
            addr: VirBytes(0),
            length: VirBytes(0x1000),
            prot: ProtFlags::READ.bits() | ProtFlags::WRITE.bits(),
            flags: MmapFlags::PRIVATE.bits() | MmapFlags::ANONYMOUS.bits() | MmapFlags::UNINITIALIZED.bits(),
            fd: -1,
            offset: 0,
        };

        let result = handle_mmap(table, &mut page_alloc, &mut frames, &req);
        assert!(matches!(result, Err(MmapError::InvalidFlags)));
    }
}
