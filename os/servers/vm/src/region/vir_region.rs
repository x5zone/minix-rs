//! Virtual region (vir_region) implementation.
//!
//! Manages process virtual address space layout using a BTreeMap.
//! Corresponds to Minix3's `vir_region` struct in `region.h`.
//!
//! PFN index model: uses `Vec<Option<PageSlot>>` instead of `Vec<Option<Box<PhysRegion>>>`.

use super::page_state::{PageFrames, PageSlot, PageFlags, PFN_NONE, PAGE_SIZE, PfnAllocator, PfnAllocError};
use minix_types::{PhysBytes, VirBytes, UserSlot};
use alloc::vec::Vec;
use crate::memtype::MemType;

bitflags::bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
    pub(crate) struct VrFlags: u16 {
        const WRITABLE = 0x001;
        const PHYS64K = 0x004;
        const LOWER16MB = 0x008;
        const LOWER1MB = 0x010;
        const SHARED = 0x040;
        const UNINITIALIZED = 0x080;
        const ANON = 0x100;
        const DIRECT = 0x200;
        const PREALLOC_MAP = 0x400;
    }
}

bitflags::bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
    pub(crate) struct PageAllocFlags: u32 {
        const CLEAR = 0x01;
        const CONTIG = 0x02;
        const ALIGN64K = 0x04;
        const LOWER16MB = 0x08;
        const LOWER1MB = 0x10;
        const ALIGN16K = 0x40;
    }
}

impl VrFlags {
    pub(crate) fn to_alloc_flags(&self) -> PageAllocFlags {
        let mut af = PageAllocFlags::empty();
        if self.contains(Self::PHYS64K) { af |= PageAllocFlags::ALIGN64K; }
        if self.contains(Self::LOWER16MB) { af |= PageAllocFlags::LOWER16MB; }
        if self.contains(Self::LOWER1MB) { af |= PageAllocFlags::LOWER1MB; }
        if !self.contains(Self::UNINITIALIZED) { af |= PageAllocFlags::CLEAR; }
        af
    }
}

#[derive(Debug, Clone)]
pub(crate) enum VrParam {
    Direct { phys: PhysBytes },
    Shared { ep: i32, vaddr: VirBytes, id: i32 },
    PbCache { pfn: u32 },
    // TODO: File variant is missing `fdref` field. Minix3's `param.file.fdref` tracks
    // the file descriptor reference count for mmap'd files. Rust rewrite needs a
    // corresponding mechanism (e.g., `Arc<FdRef>`) when implementing `mem_type_mappedfile`.
    File { inited: bool, offset: u64, clearend: u16 },
}

impl Default for VrParam {
    fn default() -> Self {
        Self::Direct { phys: PhysBytes(0) }
    }
}

pub(crate) struct VirRegion {
    pub vaddr: VirBytes,
    pub length: VirBytes,
    pub physblocks: Vec<Option<PageSlot>>,
    pub flags: VrFlags,
    pub parent_slot: Option<UserSlot>,
    pub def_memtype: Option<&'static dyn MemType>,
    pub remaps: i32,
    pub id: i32,
    pub param: VrParam,
}

impl core::fmt::Debug for VirRegion {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("VirRegion")
            .field("vaddr", &self.vaddr)
            .field("length", &self.length)
            .field("physblocks_len", &self.physblocks.len())
            .field("flags", &self.flags)
            .field("parent_slot", &self.parent_slot)
            .field("def_memtype", &self.def_memtype.map(|m| m.name()))
            .field("remaps", &self.remaps)
            .field("id", &self.id)
            .field("param", &self.param)
            .finish()
    }
}

impl VirRegion {
    pub(crate) fn new(vaddr: VirBytes, length: VirBytes, flags: VrFlags) -> Self {
        let pages = ((length.get() + PAGE_SIZE - 1) / PAGE_SIZE) as usize;
        let physblocks = (0..pages).map(|_| None).collect();
        Self {
            vaddr,
            length,
            physblocks,
            flags,
            parent_slot: None,
            def_memtype: None,
            remaps: 0,
            id: 0,
            param: VrParam::default(),
        }
    }

    pub(crate) fn with_memtype(
        vaddr: VirBytes,
        length: VirBytes,
        flags: VrFlags,
        memtype: &'static dyn MemType,
    ) -> Self {
        let mut region = Self::new(vaddr, length, flags);
        region.def_memtype = Some(memtype);
        region
    }

    pub(crate) fn end_addr(&self) -> VirBytes {
        VirBytes(self.vaddr.0 + self.length.0)
    }

    pub(crate) fn contains_addr(&self, addr: VirBytes) -> bool {
        addr.0 >= self.vaddr.0 && addr.0 < self.end_addr().0
    }

    pub(crate) fn overlaps(&self, start: VirBytes, end: VirBytes) -> bool {
        self.vaddr.0 < end.0 && self.end_addr().0 > start.0
    }

    pub(crate) fn is_writable(&self) -> bool {
        self.flags.contains(VrFlags::WRITABLE)
    }

    pub(crate) fn is_anon(&self) -> bool {
        self.flags.contains(VrFlags::ANON)
    }

    pub(crate) fn is_direct(&self) -> bool {
        self.flags.contains(VrFlags::DIRECT)
    }

    pub(crate) fn set_writable(&mut self, writable: bool) {
        if writable {
            self.flags.insert(VrFlags::WRITABLE);
        } else {
            self.flags.remove(VrFlags::WRITABLE);
        }
    }

    pub(crate) fn map_page(
        &mut self,
        frames: &mut PageFrames,
        offset: VirBytes,
        pfn: u32,
        memtype: &'static dyn MemType,
    ) {
        let page_idx = (offset.0 / PAGE_SIZE) as usize;
        let slot = PageSlot::new(pfn, offset, Some(memtype));
        self.physblocks[page_idx] = Some(slot);
        if let Some(state) = frames.get_mut(pfn) {
            state.refcount = state.refcount.saturating_add(1);
        }
    }

    /// Unmap a page from this region, decrementing its refcount in PageFrames.
    ///
    /// Returns `Some((pfn, memtype))` if the page's refcount dropped to 0 AND
    /// it is not marked as cached (`!IN_CACHE`). In that case, the caller is
    /// responsible for calling `memtype.ev_unreference(frames, pfn)` and then
    /// freeing the physical page via `PfnAllocator::free_pfn(pfn)`.
    ///
    /// Returns `None` if:
    /// - The slot was not mapped, or
    /// - The refcount is still > 0 (other regions share this page), or
    /// - The refcount is 0 but the page is cached (`IN_CACHE` flag set)
    pub(crate) fn unmap_page(
        &mut self,
        frames: &mut PageFrames,
        offset: VirBytes,
    ) -> Option<(u32, &'static dyn MemType)> {
        let page_idx = (offset.0 / PAGE_SIZE) as usize;
        let slot = self.physblocks[page_idx].take()?;
        if slot.is_mapped() {
            if let Some(state) = frames.get_mut(slot.pfn) {
                if state.refcount > 0 {
                    state.refcount -= 1;
                }
                if state.refcount == 0
                    && !state.flags.contains(PageFlags::IN_CACHE)
                {
                    if let Some(mt) = slot.memtype {
                        return Some((slot.pfn, mt));
                    }
                }
            }
        }
        None
    }

    pub(crate) fn map_lazy(&mut self, offset: VirBytes) {
        let page_idx = (offset.0 / PAGE_SIZE) as usize;
        if page_idx < self.physblocks.len() {
            self.physblocks[page_idx] = Some(PageSlot::new(PFN_NONE, offset, self.def_memtype));
        }
    }

    pub(crate) fn get_slot(&self, offset: VirBytes) -> Option<&PageSlot> {
        let page_idx = (offset.0 / PAGE_SIZE) as usize;
        self.physblocks.get(page_idx).and_then(|opt| opt.as_ref())
    }

    pub(crate) fn get_slot_mut(&mut self, offset: VirBytes) -> Option<&mut PageSlot> {
        let page_idx = (offset.0 / PAGE_SIZE) as usize;
        self.physblocks.get_mut(page_idx).and_then(|opt| opt.as_mut())
    }

    pub(crate) fn needs_cow(&self, frames: &PageFrames, offset: VirBytes) -> bool {
        match self.get_slot(offset) {
            Some(slot) if slot.is_mapped() => {
                frames.get(slot.pfn)
                    .map(|s| s.refcount > 1)
                    .unwrap_or(false)
            }
            _ => false,
        }
    }

    pub(crate) fn is_page_writable(&self, frames: &PageFrames, offset: VirBytes) -> bool {
        match self.get_slot(offset) {
            Some(slot) if slot.is_mapped() => {
                if let Some(mt) = slot.memtype {
                    mt.writable(frames, *slot, self)
                } else {
                    frames.get(slot.pfn)
                        .map(|s| s.refcount == 1)
                        .unwrap_or(false)
                }
            }
            _ => false,
        }
    }

    pub(crate) fn prepare_cow(&mut self, frames: &PageFrames) {
        // TODO: Set page table entries to read-only for all mapped writable pages.
        // This triggers page faults on write, enabling CoW resolution.
        // Equivalent to Minix3's pt_writemap() with ~PT_W flag in map_copy_region().
        // Requires access to the page table (PageTable) to modify PTE flags.
        let _ = (frames, &self.physblocks);
    }

    pub(crate) fn split(self, split_len: VirBytes) -> Result<(Self, Self), VmError> {
        if split_len.0 == 0 || split_len.0 % PAGE_SIZE != 0 || split_len.0 >= self.length.0 {
            return Err(VmError::InvalidParam);
        }

        let rem_len = VirBytes(self.length.0 - split_len.0);

        let mut left = VirRegion::new(self.vaddr, split_len, self.flags);
        let mut right = VirRegion::new(
            VirBytes(self.vaddr.0 + split_len.0),
            rem_len,
            self.flags,
        );

        left.remaps = self.remaps;
        left.id = self.id;
        right.remaps = self.remaps;
        right.id = self.id + 1;

        let left_pages = (split_len.0 / PAGE_SIZE) as usize;

        for (i, slot_opt) in self.physblocks.into_iter().enumerate() {
            if i < left_pages {
                left.physblocks[i] = slot_opt;
            } else {
                let right_idx = i - left_pages;
                if right_idx < right.physblocks.len() {
                    right.physblocks[right_idx] = slot_opt;
                }
            }
        }

        Ok((left, right))
    }

    pub(crate) fn free_range(&mut self, frames: &mut PageFrames, offset: VirBytes, len: VirBytes) -> Vec<(u32, &'static dyn MemType)> {
        let start_page = (offset.0 / PAGE_SIZE) as usize;
        let end_page = ((offset.0 + len.0 + PAGE_SIZE - 1) / PAGE_SIZE) as usize;
        let mut pending_unrefs = Vec::new();

        for page in start_page..end_page.min(self.physblocks.len()) {
            let page_offset = VirBytes((page as u64) * PAGE_SIZE);
            if let Some((pfn, mt)) = self.unmap_page(frames, page_offset) {
                pending_unrefs.push((pfn, mt));
            }
        }

        pending_unrefs
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum VmError {
    InvalidParam,
    NoMemory,
    NotFound,
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TestAlloc { next: u32 }
    impl PfnAllocator for TestAlloc {
        fn alloc_pfn(&mut self) -> Result<u32, PfnAllocError> {
            let pfn = self.next;
            self.next += 1;
            Ok(pfn)
        }
        fn free_pfn(&mut self, _pfn: u32) {}
    }

    fn make_frames(pages: u32) -> PageFrames {
        PageFrames::new(PhysBytes(pages as u64 * PAGE_SIZE))
    }

    #[test]
    fn test_vir_region_creation() {
        let region = VirRegion::new(VirBytes(0x1000), VirBytes(0x4000), VrFlags::empty());
        assert_eq!(region.vaddr, VirBytes(0x1000));
        assert_eq!(region.length, VirBytes(0x4000));
        assert_eq!(region.end_addr(), VirBytes(0x5000));
    }

    #[test]
    fn test_vir_region_contains() {
        let region = VirRegion::new(VirBytes(0x1000), VirBytes(0x4000), VrFlags::empty());
        assert!(region.contains_addr(VirBytes(0x1000)));
        assert!(region.contains_addr(VirBytes(0x2000)));
        assert!(region.contains_addr(VirBytes(0x4fff)));
        assert!(!region.contains_addr(VirBytes(0x5000)));
        assert!(!region.contains_addr(VirBytes(0x0fff)));
    }

    #[test]
    fn test_vir_region_flags() {
        let mut flags = VrFlags::empty();
        assert!(!flags.contains(VrFlags::WRITABLE));

        flags.insert(VrFlags::WRITABLE);
        assert!(flags.contains(VrFlags::WRITABLE));

        flags.insert(VrFlags::ANON);
        assert!(flags.contains(VrFlags::ANON));

        flags.remove(VrFlags::WRITABLE);
        assert!(!flags.contains(VrFlags::WRITABLE));
    }

    #[test]
    fn test_map_unmap_page() {
        let mut frames = make_frames(4);
        let mut alloc = TestAlloc { next: 0 };
        let mut region = VirRegion::new(VirBytes(0x1000), VirBytes(0x4000), VrFlags::empty());

        let pfn = alloc.alloc_pfn().unwrap();
        region.map_page(&mut frames, VirBytes(0), pfn, &crate::memtype::MEM_TYPE_ANON);

        assert!(region.get_slot(VirBytes(0)).is_some());
        assert_eq!(frames.get(pfn).unwrap().refcount, 1);

        let result = region.unmap_page(&mut frames, VirBytes(0));
        assert!(result.is_some());
        assert_eq!(frames.get(pfn).unwrap().refcount, 0);
    }

    #[test]
    fn test_needs_cow() {
        let mut frames = make_frames(4);
        let mut alloc = TestAlloc { next: 0 };
        let mut region = VirRegion::new(VirBytes(0x1000), VirBytes(0x4000), VrFlags::empty());

        let pfn = alloc.alloc_pfn().unwrap();
        region.map_page(&mut frames, VirBytes(0), pfn, &crate::memtype::MEM_TYPE_ANON);

        assert!(!region.needs_cow(&frames, VirBytes(0)));

        frames.get_mut(pfn).unwrap().refcount = 2;
        assert!(region.needs_cow(&frames, VirBytes(0)));
    }

    #[test]
    fn test_vir_region_split() {
        let region = VirRegion::new(VirBytes(0x1000), VirBytes(0x4000), VrFlags::empty());

        let (left, right) = region.split(VirBytes(0x2000)).unwrap();

        assert_eq!(left.vaddr, VirBytes(0x1000));
        assert_eq!(left.length, VirBytes(0x2000));
        assert_eq!(right.vaddr, VirBytes(0x3000));
        assert_eq!(right.length, VirBytes(0x2000));
    }

    #[test]
    fn test_vir_region_split_invalid() {
        let region1 = VirRegion::new(VirBytes(0x1000), VirBytes(0x4000), VrFlags::empty());
        let result1 = region1.split(VirBytes(0x1001));
        assert!(matches!(result1, Err(VmError::InvalidParam)));

        let region2 = VirRegion::new(VirBytes(0x1000), VirBytes(0x4000), VrFlags::empty());
        let result2 = region2.split(VirBytes(0x4000));
        assert!(matches!(result2, Err(VmError::InvalidParam)));
    }

    #[test]
    fn test_free_range() {
        let mut frames = make_frames(8);
        let mut alloc = TestAlloc { next: 0 };
        let mut region = VirRegion::new(VirBytes(0x1000), VirBytes(0x4000), VrFlags::empty());

        let pfn0 = alloc.alloc_pfn().unwrap();
        let pfn1 = alloc.alloc_pfn().unwrap();
        let pfn2 = alloc.alloc_pfn().unwrap();
        let pfn3 = alloc.alloc_pfn().unwrap();

        region.map_page(&mut frames, VirBytes(0x0000), pfn0, &crate::memtype::MEM_TYPE_ANON);
        region.map_page(&mut frames, VirBytes(0x1000), pfn1, &crate::memtype::MEM_TYPE_ANON);
        region.map_page(&mut frames, VirBytes(0x2000), pfn2, &crate::memtype::MEM_TYPE_ANON);
        region.map_page(&mut frames, VirBytes(0x3000), pfn3, &crate::memtype::MEM_TYPE_ANON);

        let pending = region.free_range(&mut frames, VirBytes(0x1000), VirBytes(0x1000));
        assert_eq!(pending.len(), 1);

        assert!(region.get_slot(VirBytes(0x0000)).is_some());
        assert!(region.get_slot(VirBytes(0x1000)).is_none());
        assert!(region.get_slot(VirBytes(0x2000)).is_some());
        assert!(region.get_slot(VirBytes(0x3000)).is_some());
    }

    #[test]
    fn test_map_lazy() {
        let mut region = VirRegion::new(VirBytes(0x1000), VirBytes(0x4000), VrFlags::empty());
        region.def_memtype = Some(&crate::memtype::MEM_TYPE_ANON);

        region.map_lazy(VirBytes(0x1000));

        let slot = region.get_slot(VirBytes(0x1000)).unwrap();
        assert!(!slot.is_mapped());
        assert_eq!(slot.pfn, PFN_NONE);
    }
}
