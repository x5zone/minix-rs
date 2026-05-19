//! Memory type system (方案三：PFN 索引模型).
//!
//! MemType trait uses `PageSlot + PageFrames` instead of `PhysRegion`.

use minix_types::VirBytes;
use crate::vmproc::ActiveProc;
use crate::region::{PageFrames, PageSlot};

pub(crate) trait MemType: Send + Sync {
    fn name(&self) -> &'static str;

    fn ev_new(&self, _region: &mut crate::region::VirRegion) -> Result<(), MemTypeError> {
        Ok(())
    }

    fn ev_delete(&self, _region: &mut crate::region::VirRegion) {}

    fn ev_reference(&self, _frames: &mut PageFrames, _slot: PageSlot) {}

    fn ev_unreference(&self, _frames: &mut PageFrames, _pfn: u32) {}

    fn ev_pagefault(
        &self,
        _proc: &ActiveProc<'_>,
        _region: &mut crate::region::VirRegion,
        _frames: &mut PageFrames,
        _offset: VirBytes,
        _write: bool,
    ) -> Result<PagefaultResult, MemTypeError> {
        Ok(PagefaultResult::Handled)
    }

    fn ev_resize(
        &self,
        _proc: &mut ActiveProc<'_>,
        _region: &mut crate::region::VirRegion,
        _new_len: VirBytes,
    ) -> Result<(), MemTypeError> {
        Ok(())
    }

    fn ev_split(
        &self,
        _proc: &ActiveProc<'_>,
        _original: &crate::region::VirRegion,
        _left: &mut crate::region::VirRegion,
        _right: &mut crate::region::VirRegion,
    ) {
    }

    fn ev_low_shrink(
        &self,
        _region: &mut crate::region::VirRegion,
        _len: VirBytes,
    ) -> Result<(), MemTypeError> {
        Ok(())
    }

    fn ev_sanitycheck(
        &self,
        _frames: &PageFrames,
        _slot: PageSlot,
    ) -> Result<(), MemTypeError> {
        Ok(())
    }

    fn writable(&self, _frames: &PageFrames, _slot: PageSlot) -> bool {
        false
    }

    fn ev_copy(
        &self,
        _src: &crate::region::VirRegion,
        _dst: &mut crate::region::VirRegion,
    ) -> Result<(), MemTypeError> {
        Ok(())
    }

    fn region_id(&self, _region: &crate::region::VirRegion) -> u32 {
        0
    }

    fn ref_count(&self, _region: &crate::region::VirRegion) -> i32 {
        0
    }

    fn pt_flags(&self, _region: &crate::region::VirRegion) -> i32 {
        0
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

    fn writable(&self, frames: &PageFrames, slot: PageSlot) -> bool {
        if !slot.is_mapped() {
            return false;
        }
        frames.get(slot.pfn)
            .map(|s| s.refcount == 1)
            .unwrap_or(false)
    }

    fn ev_unreference(&self, _frames: &mut PageFrames, _pfn: u32) {
        // Physical page freeing is the caller's responsibility via PfnAllocator::free_pfn().
        // Minix3's mem_anon.c ev_unreference frees the page here, but in 方案三
        // the separation of concerns means PageFrames only tracks refcount/flags,
        // and PfnAllocator handles allocation/deallocation.
    }

    fn ev_pagefault(
        &self,
        _proc: &ActiveProc<'_>,
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
        region.id as u32
    }

    fn ref_count(&self, region: &crate::region::VirRegion) -> i32 {
        1 + region.remaps
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

    fn writable(&self, _frames: &PageFrames, slot: PageSlot) -> bool {
        slot.is_mapped()
    }

    fn ev_pagefault(
        &self,
        _proc: &ActiveProc<'_>,
        region: &mut crate::region::VirRegion,
        _frames: &mut PageFrames,
        offset: VirBytes,
        _write: bool,
    ) -> Result<PagefaultResult, MemTypeError> {
        if let crate::region::VrParam::Direct { phys: base_phys } = &region.param {
            if base_phys.0 == 0 {
                return Err(MemTypeError::InvalidParam);
            }
            let slot = region.get_slot(offset);
            match slot {
                None => {
                    return Ok(PagefaultResult::NeedNewPage);
                }
                Some(s) if !s.is_mapped() => {
                    return Ok(PagefaultResult::NeedNewPage);
                }
                _ => {
                    return Ok(PagefaultResult::Handled);
                }
            }
        }
        Err(MemTypeError::InvalidParam)
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

    fn writable(&self, _frames: &PageFrames, slot: PageSlot) -> bool {
        slot.is_mapped()
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

    fn writable(&self, _frames: &PageFrames, slot: PageSlot) -> bool {
        slot.is_mapped()
    }

    fn ev_new(&self, region: &mut crate::region::VirRegion) -> Result<(), MemTypeError> {
        let pages = region.physblocks.len();
        if pages == 0 {
            return Ok(());
        }
        Ok(())
    }

    fn ev_pagefault(
        &self,
        _proc: &ActiveProc<'_>,
        _region: &mut crate::region::VirRegion,
        _frames: &mut PageFrames,
        offset: VirBytes,
        _write: bool,
    ) -> Result<PagefaultResult, MemTypeError> {
        Ok(PagefaultResult::NeedNewPage)
    }

    fn ev_unreference(&self, _frames: &mut PageFrames, _pfn: u32) {
        // Physical page freeing is the caller's responsibility via PfnAllocator::free_pfn().
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

    fn writable(&self, frames: &PageFrames, slot: PageSlot) -> bool {
        if !slot.is_mapped() {
            return false;
        }
        frames.get(slot.pfn)
            .map(|s| s.refcount == 1)
            .unwrap_or(false)
    }

    fn ev_pagefault(
        &self,
        _proc: &ActiveProc<'_>,
        _region: &mut crate::region::VirRegion,
        _frames: &mut PageFrames,
        offset: VirBytes,
        write: bool,
    ) -> Result<PagefaultResult, MemTypeError> {
        Ok(PagefaultResult::NeedNewPage)
    }

    fn ev_unreference(&self, _frames: &mut PageFrames, _pfn: u32) {}

    fn ev_delete(&self, region: &mut crate::region::VirRegion) {
        if let crate::region::VrParam::PbCache { pfn } = &mut region.param {
            *pfn = 0;
        }
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

    fn writable(&self, frames: &PageFrames, slot: PageSlot) -> bool {
        if !slot.is_mapped() {
            return false;
        }
        frames.get(slot.pfn)
            .map(|s| s.refcount == 1)
            .unwrap_or(false)
    }

    fn ev_pagefault(
        &self,
        _proc: &ActiveProc<'_>,
        _region: &mut crate::region::VirRegion,
        _frames: &mut PageFrames,
        offset: VirBytes,
        write: bool,
    ) -> Result<PagefaultResult, MemTypeError> {
        Ok(PagefaultResult::NeedNewPage)
    }

    fn ev_unreference(&self, _frames: &mut PageFrames, _pfn: u32) {}

    fn ev_copy(
        &self,
        _src: &crate::region::VirRegion,
        _dst: &mut crate::region::VirRegion,
    ) -> Result<(), MemTypeError> {
        Err(MemTypeError::NotSupported)
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

        assert!(MEM_TYPE_ANON.writable(&frames, *slot));

        frames.get_mut(pfn).unwrap().refcount = 2;
        assert!(!MEM_TYPE_ANON.writable(&frames, *slot));
    }

    #[test]
    fn test_mapped_file_copy_not_supported() {
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
        assert_eq!(mf.ev_copy(&src, &mut dst), Err(MemTypeError::NotSupported));
    }
}
