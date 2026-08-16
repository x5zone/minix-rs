//! Virtual region (vir_region) implementation.
//!
//! Manages process virtual address space layout using a BTreeMap.
//! Corresponds to Minix3's `vir_region` struct in `region.h`.
//!
//! PFN index model: uses `Vec<PageSlot>` with `PageSlot::EMPTY` sentinel
//! (pfn=PFN_NONE) instead of `Vec<Option<PageSlot>>`. This eliminates the
//! Option discriminant overhead (4+ bytes per slot) while preserving the
//! same semantics: `slot.is_mapped()` replaces `slot.is_some()`.

use super::page_state::{PageFrames, PageSlot, PageFlags, PFN_NONE, PAGE_SIZE, PfnAllocator, PfnAllocError};
use minix_types::{PhysBytes, VirBytes, UserSlot};
use alloc::vec::Vec;
use crate::memtype::MemType;
use crate::phys_mem::PageAllocFlags;

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
    File { inited: bool, fdref_id: Option<u32>, offset: u64, clearend: u16 },
}

impl Default for VrParam {
    fn default() -> Self {
        Self::Direct { phys: PhysBytes(0) }
    }
}

pub(crate) struct VirRegion {
    pub vaddr: VirBytes,
    pub length: VirBytes,
    /// Per-page mapping slots. Uses `PageSlot::EMPTY` (pfn=PFN_NONE) as
    /// sentinel for unmapped pages instead of `Option<PageSlot>`, saving
    /// 4+ bytes of Option discriminant per slot.
    pub physblocks: Vec<PageSlot>,
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
        let physblocks = alloc::vec![PageSlot::EMPTY; pages];
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

    /// Extend a region by `extra` bytes (page-aligned): push EMPTY page slots
    /// and grow `length`.
    ///
    /// Memtype-agnostic growth. C dispatches to the memtype `ev_resize`
    /// callback when present, else appends via `map_page_region`
    /// (region.c:1037-1045); minix-rs folds the resize semantics into this
    /// single operation ([ARCH: A-12], 19-vm-brk.md §3.2).
    pub(crate) fn extend(&mut self, extra: VirBytes) -> Result<(), VmError> {
        if extra.0 == 0 || extra.0 % PAGE_SIZE != 0 {
            return Err(VmError::InvalidParam);
        }
        let old_pages = self.physblocks.len();
        let added_pages = (extra.0 / PAGE_SIZE) as usize;
        self.physblocks.reserve(added_pages);
        for _ in 0..added_pages {
            self.physblocks.push(PageSlot::EMPTY);
        }
        self.length = VirBytes(self.length.0 + extra.0);
        debug_assert_eq!(self.physblocks.len(), old_pages + added_pages);
        Ok(())
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
        self.physblocks[page_idx] = slot;
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
        let slot = core::mem::replace(&mut self.physblocks[page_idx], PageSlot::EMPTY);
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
            self.physblocks[page_idx] = PageSlot::new(PFN_NONE, offset, self.def_memtype);
        }
    }

    pub(crate) fn get_slot(&self, offset: VirBytes) -> Option<&PageSlot> {
        let page_idx = (offset.0 / PAGE_SIZE) as usize;
        self.physblocks.get(page_idx).filter(|s| s.is_mapped())
    }

    pub(crate) fn get_slot_mut(&mut self, offset: VirBytes) -> Option<&mut PageSlot> {
        let page_idx = (offset.0 / PAGE_SIZE) as usize;
        self.physblocks.get_mut(page_idx).filter(|s| s.is_mapped())
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

    pub(crate) fn prepare_cow(&mut self, frames: &mut PageFrames) {
        // Mark all mapped pages with refcount > 1 as COW.
        // This sets the COW flag on the PageState so that
        // write_page_table_mappings() can map them read-only.
        // Equivalent to Minix3's pt_writemap() with ~PT_W flag in map_copy_region().
        for slot in self.physblocks.iter() {
            if slot.is_mapped() {
                if let Some(state) = frames.get_mut(slot.pfn) {
                    if state.refcount > 1 {
                        state.flags.insert(PageFlags::COW);
                    }
                }
            }
        }
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

        match &self.param {
            VrParam::File { inited: true, fdref_id, offset, clearend } => {
                let fdref_id = *fdref_id;
                let orig_offset = *offset;
                let orig_clearend = *clearend;
                left.param = VrParam::File {
                    inited: true,
                    fdref_id,
                    offset: orig_offset,
                    clearend: 0,
                };
                right.param = VrParam::File {
                    inited: true,
                    fdref_id,
                    offset: orig_offset + split_len.get(),
                    clearend: orig_clearend,
                };
            }
            _ => {
                left.param = self.param.clone();
                right.param = self.param.clone();
            }
        }

        // fdref balance: one reference per live region. The original region
        // held one reference; split consumes it without an explicit deref,
        // so creating two regions needs exactly ONE additional reference
        // (1 region → 2 regions, net +1). C's split_region nets the same:
        // mappedfile_split refs both r1 and r2 (+2) while map_free(original)
        // derefs once (−1).
        //
        // Previously this added two references, which left one orphaned
        // reference per split — a head/tail cut (free one half immediately)
        // never reached refcount 0, so VFS_FDCLOSE was never sent (21-P1-2).
        if let VrParam::File { fdref_id: Some(id), .. } = left.param {
            crate::fdref::FdRefTable::get_global().ref_entry(id);
        }

        let left_pages = (split_len.0 / PAGE_SIZE) as usize;

        for (i, slot) in self.physblocks.into_iter().enumerate() {
            if i < left_pages {
                left.physblocks[i] = slot;
            } else {
                let right_idx = i - left_pages;
                if right_idx < right.physblocks.len() {
                    right.physblocks[right_idx] = slot;
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

    #[test]
    fn test_extend() {
        let mut region = VirRegion::new(VirBytes(0x1000), VirBytes(0x2000), VrFlags::empty());
        assert_eq!(region.physblocks.len(), 2);
        assert_eq!(region.end_addr(), VirBytes(0x3000));

        region.extend(VirBytes(0x1000)).unwrap();
        assert_eq!(region.length, VirBytes(0x3000));
        assert_eq!(region.physblocks.len(), 3);
        assert_eq!(region.end_addr(), VirBytes(0x4000));
        assert!(region.get_slot(VirBytes(0x2000)).is_none());
    }

    #[test]
    fn test_extend_invalid() {
        let mut region = VirRegion::new(VirBytes(0x1000), VirBytes(0x2000), VrFlags::empty());

        assert!(matches!(region.extend(VirBytes(0)), Err(VmError::InvalidParam)));
        assert!(matches!(region.extend(VirBytes(0x1001)), Err(VmError::InvalidParam)));
    }
}
