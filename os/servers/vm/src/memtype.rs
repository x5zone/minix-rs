//! Memory type system (PFN index model).
//!
//! MemType trait uses `PageSlot + PageFrames` instead of `PhysRegion`.

use minix_types::{Endpoint, VirBytes};
use minix_arch::paging::PageFlags;
use crate::vmproc::ActiveProc;
use crate::region::{PageFrames, PageSlot};

pub(crate) trait MemType: Send + Sync {
    fn name(&self) -> &'static str;

    // C NULL → skip init. Default Ok(()): framework handles page allocation.
    fn ev_new(&self, _region: &mut crate::region::VirRegion) -> Result<(), MemTypeError> {
        Ok(())
    }

    // C NULL → skip cleanup. Default no-op: framework releases resources.
    fn ev_delete(&self, _region: &mut crate::region::VirRegion) {}

    // C NULL → skip reference. PageFrames manages refcount in PFN model, default no-op.
    fn ev_reference(&self, _frames: &mut PageFrames, _slot: PageSlot) -> Result<(), MemTypeError> {
        Ok(())
    }

    // C implementations free physical pages here. PfnAllocator handles this in PFN model, default no-op.
    fn ev_unreference(&self, _frames: &mut PageFrames, _pfn: u32) {}

    // All C types implement this (no NULL). Default Handled; complex types must override.
    fn ev_pagefault(
        &self,
        _proc_endpoint: Endpoint,
        _region: &mut crate::region::VirRegion,
        _frames: &mut PageFrames,
        _offset: VirBytes,
        _write: bool,
    ) -> Result<PagefaultResult, MemTypeError> {
        Ok(PagefaultResult::Handled)
    }

    // C NULL → generic resize logic. Default Ok(()): framework handles page allocation.
    fn ev_resize(
        &self,
        _proc: &mut ActiveProc<'_>,
        _region: &mut crate::region::VirRegion,
        _new_len: VirBytes,
    ) -> Result<(), MemTypeError> {
        Ok(())
    }

    // C NULL → EINVAL (region.c:1164). Default Err(NotSupported).
    fn ev_split(
        &self,
        _proc_endpoint: Endpoint,
        _original: &crate::region::VirRegion,
        _left: &mut crate::region::VirRegion,
        _right: &mut crate::region::VirRegion,
    ) -> Result<(), MemTypeError> {
        Err(MemTypeError::NotSupported)
    }

    // C NULL → EINVAL (region.c:1096). Default Err(NotSupported).
    fn ev_low_shrink(
        &self,
        _region: &mut crate::region::VirRegion,
        _len: VirBytes,
    ) -> Result<(), MemTypeError> {
        Err(MemTypeError::NotSupported)
    }

    // C NULL → skip. No runtime checks needed in PFN model.
    fn ev_sanitycheck(
        &self,
        _frames: &PageFrames,
        _slot: PageSlot,
    ) -> Result<(), MemTypeError> {
        Ok(())
    }

    // All C types implement this (no NULL). Default false; AnonymousMemory overrides for CoW check.
    fn writable(&self, _frames: &PageFrames, _slot: PageSlot, _region: &crate::region::VirRegion) -> bool {
        false
    }

    // C NULL → skip copy specialization. Default Ok(()).
    fn ev_copy(
        &self,
        _src: &crate::region::VirRegion,
        _dst: &mut crate::region::VirRegion,
    ) -> Result<(), MemTypeError> {
        Ok(())
    }

    // C NULL → return 0. Default 0.
    fn region_id(&self, _region: &crate::region::VirRegion) -> u32 {
        0
    }

    // C NULL → return 0. Default 0. AnonymousMemory overrides with 1+remaps.
    fn ref_count(&self, _region: &crate::region::VirRegion) -> i32 {
        0
    }

    // C NULL → return 0 (default caching). Default empty(). DirectPhysical etc. override with NO_CACHE.
    fn pt_flags(&self, _region: &crate::region::VirRegion) -> PageFlags {
        PageFlags::empty()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MemTypeError {
    NoMemory,
    InvalidParam,
    NotSupported,
    IoError,
    CopyFailed,
}

impl core::fmt::Display for MemTypeError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::NoMemory => write!(f, "Out of memory"),
            Self::InvalidParam => write!(f, "Invalid parameter"),
            Self::NotSupported => write!(f, "Operation not supported"),
            Self::IoError => write!(f, "IO error"),
            Self::CopyFailed => write!(f, "Copy failed"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PagefaultResult {
    Handled,
    NeedNewPage,
    NeedCow,
    NeedVfsIo,
    AccessViolation,
}

pub(crate) struct AnonymousMemory;

impl AnonymousMemory {
    pub(crate) const fn new() -> Self {
        Self
    }
}

impl Default for AnonymousMemory {
    fn default() -> Self {
        Self::new()
    }
}

impl MemType for AnonymousMemory {
    fn name(&self) -> &'static str {
        "anonymous memory"
    }

    fn writable(&self, frames: &PageFrames, slot: PageSlot, region: &crate::region::VirRegion) -> bool {
        if !slot.is_mapped() {
            return false;
        }
        if region.remaps > 0 {
            return true;
        }
        frames.get(slot.pfn)
            .map(|s| s.refcount == 1)
            .unwrap_or(false)
    }

    fn ev_unreference(&self, _frames: &mut PageFrames, _pfn: u32) {
        // Physical page freeing is the caller's responsibility via PfnAllocator::free_pfn().
        // Minix3's mem_anon.c ev_unreference frees the page here, but in the PFN model
        // the separation of concerns means PageFrames only tracks refcount/flags,
        // and PfnAllocator handles allocation/deallocation.
    }

    fn ev_pagefault(
        &self,
        _proc_endpoint: Endpoint,
        region: &mut crate::region::VirRegion,
        frames: &mut PageFrames,
        offset: VirBytes,
        write: bool,
    ) -> Result<PagefaultResult, MemTypeError> {
        let slot = region.get_slot(offset);

        match slot {
            None => {
                return Ok(PagefaultResult::NeedNewPage);
            }
            Some(s) if !s.is_mapped() => {
                return Ok(PagefaultResult::NeedNewPage);
            }
            _ => {}
        }

        let slot = slot.unwrap();
        let refcount = frames.get(slot.pfn)
            .map(|s| s.refcount)
            .unwrap_or(0);

        if refcount < 2 || !write {
            return Ok(PagefaultResult::Handled);
        }

        if !region.is_writable() {
            return Ok(PagefaultResult::AccessViolation);
        }

        Ok(PagefaultResult::NeedCow)
    }

    fn region_id(&self, region: &crate::region::VirRegion) -> u32 {
        // region.id is i32, always non-negative (valid region IDs), safe for u32
        region.id as u32
    }

    fn ref_count(&self, region: &crate::region::VirRegion) -> i32 {
        1 + region.remaps
    }

    fn ev_split(
        &self,
        _proc_endpoint: Endpoint,
        _original: &crate::region::VirRegion,
        _left: &mut crate::region::VirRegion,
        _right: &mut crate::region::VirRegion,
    ) -> Result<(), MemTypeError> {
        // Minix3's anon_split is a no-op (return).
        Ok(())
    }

    fn ev_low_shrink(
        &self,
        _region: &mut crate::region::VirRegion,
        _len: VirBytes,
    ) -> Result<(), MemTypeError> {
        // Minix3's anon_lowshrink is a no-op (return OK).
        Ok(())
    }
}

pub(crate) struct DirectPhysical;

impl DirectPhysical {
    pub(crate) const fn new() -> Self {
        Self
    }
}

impl Default for DirectPhysical {
    fn default() -> Self {
        Self::new()
    }
}

impl MemType for DirectPhysical {
    fn name(&self) -> &'static str {
        "physical memory mapping"
    }

    fn writable(&self, _frames: &PageFrames, slot: PageSlot, _region: &crate::region::VirRegion) -> bool {
        slot.is_mapped()
    }

    fn ev_pagefault(
        &self,
        _proc_endpoint: Endpoint,
        region: &mut crate::region::VirRegion,
        frames: &mut PageFrames,
        offset: VirBytes,
        _write: bool,
    ) -> Result<PagefaultResult, MemTypeError> {
        if let crate::region::VrParam::Direct { phys: base_phys } = &region.param {
            if base_phys.0 == 0 {
                return Err(MemTypeError::InvalidParam);
            }
            let slot = region.get_slot(offset);
            match slot {
                Some(s) if s.is_mapped() => {
                    return Ok(PagefaultResult::Handled);
                }
                _ => {}
            }
            let phys_addr = minix_types::PhysBytes(base_phys.0 + offset.0);
            let pfn = frames.phys_to_pfn(phys_addr);
            let memtype = region.def_memtype
                .ok_or(MemTypeError::InvalidParam)?;
            region.map_page(frames, offset, pfn, memtype);
            Ok(PagefaultResult::Handled)
        } else {
            Err(MemTypeError::InvalidParam)
        }
    }

    fn ev_copy(
        &self,
        src: &crate::region::VirRegion,
        dst: &mut crate::region::VirRegion,
    ) -> Result<(), MemTypeError> {
        dst.param = src.param.clone();
        Ok(())
    }

    fn ev_unreference(&self, _frames: &mut PageFrames, _pfn: u32) {}

    fn pt_flags(&self, _region: &crate::region::VirRegion) -> PageFlags {
        PageFlags::NO_CACHE
    }

    fn ev_split(
        &self,
        _proc_endpoint: Endpoint,
        _original: &crate::region::VirRegion,
        _left: &mut crate::region::VirRegion,
        _right: &mut crate::region::VirRegion,
    ) -> Result<(), MemTypeError> {
        // Minix3's mem_type_directphys has no ev_split (NULL → EINVAL).
        // Direct physical mapping does not support split; use default Err(NotSupported).
        Err(MemTypeError::NotSupported)
    }
}

pub(crate) struct SharedMemory;

impl SharedMemory {
    pub(crate) const fn new() -> Self {
        Self
    }
}

impl Default for SharedMemory {
    fn default() -> Self {
        Self::new()
    }
}

impl MemType for SharedMemory {
    fn name(&self) -> &'static str {
        "shared memory"
    }

    fn writable(&self, _frames: &PageFrames, slot: PageSlot, _region: &crate::region::VirRegion) -> bool {
        slot.is_mapped()
    }

    fn ev_pagefault(
        &self,
        _proc_endpoint: Endpoint,
        region: &mut crate::region::VirRegion,
        frames: &mut PageFrames,
        offset: VirBytes,
        _write: bool,
    ) -> Result<PagefaultResult, MemTypeError> {
        // Check if the page is already mapped. If so, no action needed.
        if let Some(slot) = region.get_slot(offset) {
            if slot.is_mapped() {
                return Ok(PagefaultResult::Handled);
            }
        }

        // Page not mapped. In Minix3, shared_pagefault() (mem_shared.c:122) does:
        //   1. getsrc() → look up the source process and source region
        //   2. map_pf() on the source → ensure source has the page
        //   3. pb_link() → link current phys_region to the same phys_block
        //
        // This requires cross-process region lookup (VmProcTable → ActiveProc →
        // RegionMap::find). The ev_pagefault signature doesn't carry VmProcTable,
        // so this must be handled by the caller (dispatch_pagefault in vm_server.rs).
        //
        // For now: fail closed. Real implementation must:
        //   - Extract VrParam::Shared { ep, vaddr, id } from region.param
        //   - Look up source process by endpoint (vm_isokendpt + get_active)
        //   - Find source VirRegion by vaddr
        //   - Get source PFN at the matching offset
        //   - Map current PageSlot to the same PFN (pb_link equivalent)
        Err(MemTypeError::NotSupported)
    }

    fn ev_unreference(&self, _frames: &mut PageFrames, _pfn: u32) {}

    fn ev_copy(
        &self,
        src: &crate::region::VirRegion,
        dst: &mut crate::region::VirRegion,
    ) -> Result<(), MemTypeError> {
        dst.param = src.param.clone();
        Ok(())
    }

    fn ev_split(
        &self,
        _proc_endpoint: Endpoint,
        _original: &crate::region::VirRegion,
        _left: &mut crate::region::VirRegion,
        _right: &mut crate::region::VirRegion,
    ) -> Result<(), MemTypeError> {
        // Minix3's mem_type_shared has no ev_split (NULL → EINVAL).
        // Shared memory does not support split; use default Err(NotSupported).
        Err(MemTypeError::NotSupported)
    }

    fn ev_low_shrink(
        &self,
        _region: &mut crate::region::VirRegion,
        _len: VirBytes,
    ) -> Result<(), MemTypeError> {
        // Minix3's mem_type_shared has no ev_lowshrink (NULL → EINVAL).
        // Shared memory does not support low_shrink; use default Err(NotSupported).
        Err(MemTypeError::NotSupported)
    }
}

pub(crate) struct ContiguousAnonymous;

impl ContiguousAnonymous {
    pub(crate) const fn new() -> Self {
        Self
    }
}

impl Default for ContiguousAnonymous {
    fn default() -> Self {
        Self::new()
    }
}

impl MemType for ContiguousAnonymous {
    fn name(&self) -> &'static str {
        "contiguous anonymous memory"
    }

    fn writable(&self, frames: &PageFrames, slot: PageSlot, region: &crate::region::VirRegion) -> bool {
        // Minix3's anon_contig_writable delegates to anon_writable (refcount+remaps
        // check). Currently ev_copy returns NotSupported so refcount is always 1 and
        // remaps is always 0, making this equivalent to slot.is_mapped(). However, if
        // fork support is added for ContiguousAnonymous in the future, the full check
        // must remain to correctly determine CoW eligibility.
        if !slot.is_mapped() {
            return false;
        }
        if region.remaps > 0 {
            return true;
        }
        frames.get(slot.pfn)
            .map(|s| s.refcount == 1)
            .unwrap_or(false)
    }

    fn ev_reference(&self, _frames: &mut PageFrames, _slot: PageSlot) -> Result<(), MemTypeError> {
        Err(MemTypeError::NotSupported)
    }

    fn ev_resize(
        &self,
        _proc: &mut ActiveProc<'_>,
        _region: &mut crate::region::VirRegion,
        _new_len: VirBytes,
    ) -> Result<(), MemTypeError> {
        Err(MemTypeError::NotSupported)
    }

    fn ev_copy(
        &self,
        _src: &crate::region::VirRegion,
        _dst: &mut crate::region::VirRegion,
    ) -> Result<(), MemTypeError> {
        Err(MemTypeError::NotSupported)
    }

    fn ev_new(&self, region: &mut crate::region::VirRegion) -> Result<(), MemTypeError> {
        let pages = region.physblocks.len();
        if pages == 0 {
            return Ok(());
        }
        // TODO: Minix3's anon_contig_new pre-allocates contiguous physical pages.
        // Current implementation is a no-op; contiguous physical memory is not yet
        // guaranteed for ContiguousAnonymous regions.
        Ok(())
    }

    fn ev_pagefault(
        &self,
        _proc_endpoint: Endpoint,
        _region: &mut crate::region::VirRegion,
        _frames: &mut PageFrames,
        _offset: VirBytes,
        _write: bool,
    ) -> Result<PagefaultResult, MemTypeError> {
        Ok(PagefaultResult::NeedNewPage)
    }

    fn ev_unreference(&self, _frames: &mut PageFrames, _pfn: u32) {
        // Physical page freeing is the caller's responsibility via PfnAllocator::free_pfn().
    }

    fn pt_flags(&self, _region: &crate::region::VirRegion) -> PageFlags {
        PageFlags::NO_CACHE
    }

    fn ev_split(
        &self,
        _proc_endpoint: Endpoint,
        _original: &crate::region::VirRegion,
        _left: &mut crate::region::VirRegion,
        _right: &mut crate::region::VirRegion,
    ) -> Result<(), MemTypeError> {
        // Minix3's anon_contig_split is a no-op (return).
        Ok(())
    }
}

pub(crate) struct CacheMemory;

impl CacheMemory {
    pub(crate) const fn new() -> Self {
        Self
    }
}

impl Default for CacheMemory {
    fn default() -> Self {
        Self::new()
    }
}

impl MemType for CacheMemory {
    fn name(&self) -> &'static str {
        "cache memory"
    }

    fn writable(&self, _frames: &PageFrames, slot: PageSlot, _region: &crate::region::VirRegion) -> bool {
        slot.is_mapped()
    }

    fn ev_pagefault(
        &self,
        _proc_endpoint: Endpoint,
        _region: &mut crate::region::VirRegion,
        _frames: &mut PageFrames,
        _offset: VirBytes,
        _write: bool,
    ) -> Result<PagefaultResult, MemTypeError> {
        Ok(PagefaultResult::NeedNewPage)
    }

    fn ev_unreference(&self, _frames: &mut PageFrames, _pfn: u32) {
    }

    fn ev_resize(
        &self,
        _proc: &mut ActiveProc<'_>,
        _region: &mut crate::region::VirRegion,
        _new_len: VirBytes,
    ) -> Result<(), MemTypeError> {
        Err(MemTypeError::NotSupported)
    }

    fn ev_delete(&self, region: &mut crate::region::VirRegion) {
        // Minix3's mem_type_cache has no ev_delete callback (NULL). Cache cleanup
        // happens in do_forgetcache/rmcache. In the PFN index model, we clear the
        // cached pfn to prevent dangling references — if the region outlives the
        // cache entry, a stale pfn would point to a freed or reused page.
        if let crate::region::VrParam::PbCache { pfn } = &mut region.param {
            *pfn = 0;
        }
    }

    fn ev_low_shrink(
        &self,
        _region: &mut crate::region::VirRegion,
        _len: VirBytes,
    ) -> Result<(), MemTypeError> {
        Ok(())
    }
}

pub(crate) struct MappedFile;

impl MappedFile {
    pub(crate) const fn new() -> Self {
        Self
    }
}

impl Default for MappedFile {
    fn default() -> Self {
        Self::new()
    }
}

impl MemType for MappedFile {
    fn name(&self) -> &'static str {
        "mapped file"
    }

    fn writable(&self, _frames: &PageFrames, _slot: PageSlot, _region: &crate::region::VirRegion) -> bool {
        false
    }

    fn ev_pagefault(
        &self,
        _proc_endpoint: Endpoint,
        region: &mut crate::region::VirRegion,
        _frames: &mut PageFrames,
        offset: VirBytes,
        write: bool,
    ) -> Result<PagefaultResult, MemTypeError> {
        // _proc and _frames unused here: page table update and frame allocation
        // happen in the VFS reply callback (mappedfile_pf_cont), not in this
        // initial page-fault check which only determines the action needed.
        if let crate::region::VrParam::File { inited, .. } = &region.param {
            if !inited {
                return Ok(PagefaultResult::NeedNewPage);
            }
        }

        let slot = region.get_slot(offset);

        match slot {
            Some(s) if s.is_mapped() => {
                if write {
                    Ok(PagefaultResult::NeedCow)
                } else {
                    Ok(PagefaultResult::Handled)
                }
            }
            _ => Ok(PagefaultResult::NeedVfsIo),
        }
    }

    fn ev_unreference(&self, _frames: &mut PageFrames, _pfn: u32) {}

    fn ev_copy(
        &self,
        src: &crate::region::VirRegion,
        dst: &mut crate::region::VirRegion,
    ) -> Result<(), MemTypeError> {
        dst.param = src.param.clone();
        Ok(())
    }

    fn ev_split(
        &self,
        _proc_endpoint: Endpoint,
        original: &crate::region::VirRegion,
        left: &mut crate::region::VirRegion,
        right: &mut crate::region::VirRegion,
    ) -> Result<(), MemTypeError> {
        if let crate::region::VrParam::File { inited: true, fdref_id, offset, clearend } = &original.param {
            let fdref_id = *fdref_id;
            let orig_offset = *offset;
            let orig_clearend = *clearend;

            left.param = crate::region::VrParam::File {
                inited: true,
                fdref_id,
                offset: orig_offset,
                clearend: 0,
            };

            right.param = crate::region::VrParam::File {
                inited: true,
                fdref_id,
                offset: orig_offset + left.length.get(),
                clearend: orig_clearend,
            };
        }
        Ok(())
    }

    fn ev_low_shrink(
        &self,
        region: &mut crate::region::VirRegion,
        len: VirBytes,
    ) -> Result<(), MemTypeError> {
        if let crate::region::VrParam::File { offset, .. } = &mut region.param {
            *offset += len.get();
        }
        Ok(())
    }

    fn ev_delete(&self, region: &mut crate::region::VirRegion) {
        if let crate::region::VrParam::File { inited, fdref_id, .. } = &mut region.param {
            *inited = false;
            *fdref_id = None;
        }
    }
}

pub(crate) static MEM_TYPE_ANON: AnonymousMemory = AnonymousMemory::new();
pub(crate) static MEM_TYPE_DIRECT: DirectPhysical = DirectPhysical::new();
pub(crate) static MEM_TYPE_SHARED: SharedMemory = SharedMemory::new();
pub(crate) static MEM_TYPE_CONTIG_ANON: ContiguousAnonymous = ContiguousAnonymous::new();
pub(crate) static MEM_TYPE_CACHE: CacheMemory = CacheMemory::new();
pub(crate) static MEM_TYPE_MAPPED_FILE: MappedFile = MappedFile::new();

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_anonymous_memory_name() {
        let anon = AnonymousMemory::new();
        assert_eq!(anon.name(), "anonymous memory");
    }

    #[test]
    fn test_direct_physical_name() {
        let direct = DirectPhysical::new();
        assert_eq!(direct.name(), "physical memory mapping");
    }

    #[test]
    fn test_shared_memory_name() {
        let shared = SharedMemory::new();
        assert_eq!(shared.name(), "shared memory");
    }

    #[test]
    fn test_static_instances() {
        assert_eq!(MEM_TYPE_ANON.name(), "anonymous memory");
        assert_eq!(MEM_TYPE_DIRECT.name(), "physical memory mapping");
        assert_eq!(MEM_TYPE_SHARED.name(), "shared memory");
        assert_eq!(MEM_TYPE_CONTIG_ANON.name(), "contiguous anonymous memory");
        assert_eq!(MEM_TYPE_CACHE.name(), "cache memory");
        assert_eq!(MEM_TYPE_MAPPED_FILE.name(), "mapped file");
    }

    #[test]
    fn test_anon_writable() {
        use crate::region::{PfnAllocator, PfnAllocError};

        struct TestAlloc { next: u32 }
        impl PfnAllocator for TestAlloc {
            fn alloc_pfn(&mut self) -> Result<u32, PfnAllocError> {
                let pfn = self.next;
                self.next += 1;
                Ok(pfn)
            }
            fn free_pfn(&mut self, _pfn: u32) {}
        }

        let mut frames = PageFrames::new(minix_types::PhysBytes(4096 * 4));
        let mut alloc = TestAlloc { next: 0 };
        let pfn = alloc.alloc_pfn().unwrap();
        let mut region = crate::region::VirRegion::new(
            minix_types::VirBytes(0x1000),
            minix_types::VirBytes(0x1000),
            crate::region::VrFlags::empty(),
        );
        region.map_page(&mut frames, minix_types::VirBytes(0), pfn, &MEM_TYPE_ANON);
        let slot = region.get_slot(minix_types::VirBytes(0)).unwrap();

        assert!(MEM_TYPE_ANON.writable(&frames, *slot, &region));

        frames.get_mut(pfn).unwrap().refcount = 2;
        assert!(!MEM_TYPE_ANON.writable(&frames, *slot, &region));
    }

    #[test]
    fn test_mapped_file_copy() {
        let mf = MappedFile::new();
        let src = crate::region::VirRegion::new(
            minix_types::VirBytes(0x1000),
            minix_types::VirBytes(0x1000),
            crate::region::VrFlags::empty(),
        );
        let mut dst = crate::region::VirRegion::new(
            minix_types::VirBytes(0x2000),
            minix_types::VirBytes(0x1000),
            crate::region::VrFlags::empty(),
        );
        assert_eq!(mf.ev_copy(&src, &mut dst), Ok(()));
    }
}
