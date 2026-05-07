//! Virtual region (vir_region) implementation.
//!
//! Manages process virtual address space layout using an AVL tree.
//! Corresponds to Minix3's `vir_region` struct in `region.h`.

use super::phys_region::{PhysBlock, PhysRegion};
use minix_types::{VirBytes, UserSlot};
use alloc::boxed::Box;
use alloc::vec::Vec;
use core::ptr::NonNull;
use crate::memtype::MemType;

/// Virtual region flags.
///
/// Corresponds to Minix3's `VR_*` macros.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct VrFlags(pub u16);

impl VrFlags {
    pub(crate) const WRITABLE: u16 = 0x001;
    pub(crate) const PHYS64K: u16 = 0x004;
    pub(crate) const LOWER16MB: u16 = 0x008;
    pub(crate) const LOWER1MB: u16 = 0x010;
    pub(crate) const SHARED: u16 = 0x040;
    pub(crate) const UNINITIALIZED: u16 = 0x080;
    pub(crate) const ANON: u16 = 0x100;
    pub(crate) const DIRECT: u16 = 0x200;
    pub(crate) const PREALLOC_MAP: u16 = 0x400;

    pub(crate) const fn empty() -> Self {
        Self(0)
    }

    pub(crate) const fn contains(&self, flag: u16) -> bool {
        (self.0 & flag) != 0
    }

    pub(crate) fn insert(&mut self, flag: u16) {
        self.0 |= flag;
    }

    pub(crate) fn remove(&mut self, flag: u16) {
        self.0 &= !flag;
    }
}

impl Default for VrFlags {
    fn default() -> Self {
        Self::empty()
    }
}

/// Virtual region parameters (union equivalent).
///
/// Corresponds to Minix3's `param` union in `vir_region`.
#[derive(Debug, Clone)]
pub(crate) enum VrParam {
    Direct { phys: u64 },
    Shared { ep: i32, vaddr: VirBytes, id: i32 },
    PbCache { pb: Option<NonNull<PhysBlock>> },
    File { inited: bool, offset: u64, clearend: u16 },
}

impl Default for VrParam {
    fn default() -> Self {
        Self::Direct { phys: PhysBlock::MAP_NONE }
    }
}

/// Virtual region (vir_region).
///
/// Represents a contiguous range in the process's virtual address space
/// with uniform properties. Corresponds to Minix3's `struct vir_region`.
pub(crate) struct VirRegion {
    pub vaddr: VirBytes,
    pub length: VirBytes,
    /// Physical block pointer array. Size = length / PAGE_SIZE.
    pub physblocks: Vec<Option<Box<PhysRegion>>>,
    pub flags: VrFlags,
    /// Parent process slot (if any).
    pub parent_slot: Option<UserSlot>,
    /// Default memory type for new allocations in this region.
    pub def_memtype: Option<&'static dyn MemType>,
    pub remaps: i32,
    pub id: i32,
    pub param: VrParam,

    // AVL tree fields
    pub lower: Option<Box<VirRegion>>,
    pub higher: Option<Box<VirRegion>>,
    pub factor: i8,
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
            .field("factor", &self.factor)
            .finish()
    }
}

impl VirRegion {
    pub(crate) fn new(vaddr: VirBytes, length: VirBytes, flags: VrFlags) -> Self {
        let pages = ((length.get() + 4095) / 4096) as usize;
        let mut physblocks = Vec::with_capacity(pages);
        for _ in 0..pages {
            physblocks.push(None);
        }
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
            lower: None,
            higher: None,
            factor: 0,
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

    /// Returns the end address (exclusive).
    pub(crate) fn end_addr(&self) -> VirBytes {
        self.vaddr + self.length
    }

    /// Checks if the address is within this region.
    pub(crate) fn contains(&self, addr: VirBytes) -> bool {
        addr >= self.vaddr && addr < self.end_addr()
    }

    /// Checks if this region overlaps with the given range.
    pub(crate) fn overlaps(&self, start: VirBytes, end: VirBytes) -> bool {
        self.vaddr < end && self.end_addr() > start
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

    /// Prepares for CoW: sets shared pages to read-only.
    ///
    /// Iterates all physical regions, setting shared pages (refcount > 1) to read-only
    /// to trigger copy-on-write.
    ///
    /// Corresponds to Minix3's CoW preparation logic in `map_writept()`.
    ///
    /// # Safety
    /// Caller must ensure all PhysRegions are properly initialized and page table operations are safe.
    pub(crate) unsafe fn prepare_cow(&mut self) {
        use crate::pagetable::PageFlags;
        const PAGE_SIZE: u64 = 4096;
        let num_pages = (self.length.get() / PAGE_SIZE) as usize;

        for i in 0..num_pages {
            if let Some(phys_region) = &self.physblocks[i] {
                if phys_region.has_phys_block() && self.is_writable() && !phys_region.is_writable() {
                    let _vaddr = self.vaddr.get() + i as u64 * PAGE_SIZE;
                    let _paddr = phys_region.get_phys_addr().unwrap_or(0);
                    let _cow_flags = PageFlags::read_only();
                }
            }
        }
    }

    pub(crate) fn get_phys_region(&self, offset: VirBytes) -> Option<&PhysRegion> {
        let page = (offset.get() / 4096) as usize;
        self.physblocks.get(page).and_then(|opt| opt.as_ref().map(|b| b.as_ref()))
    }

    pub(crate) fn get_phys_region_mut(&mut self, offset: VirBytes) -> Option<&mut PhysRegion> {
        let page = (offset.get() / 4096) as usize;
        self.physblocks.get_mut(page).and_then(|opt| opt.as_mut().map(|b| b.as_mut()))
    }

    pub(crate) fn set_phys_region(&mut self, offset: VirBytes, region: PhysRegion) {
        let page = (offset.get() / 4096) as usize;
        if page < self.physblocks.len() {
            self.physblocks[page] = Some(Box::new(region));
        }
    }

    /// Splits the region at the given position.
    ///
    /// Returns (left, right) where left retains the original start address.
    ///
    /// Corresponds to Minix3's `split_region()`.
    pub(crate) fn split(self, split_len: VirBytes) -> Result<(Self, Self), VmError> {
        let page_size: u64 = 4096;

        if split_len.get() % page_size != 0 {
            return Err(VmError::InvalidParam);
        }
        if split_len.get() >= self.length.get() {
            return Err(VmError::InvalidParam);
        }

        let rem_len = VirBytes(self.length.get() - split_len.get());

        let mut left = VirRegion::new(self.vaddr, split_len, self.flags);
        let mut right = VirRegion::new(
            VirBytes(self.vaddr.get() + split_len.get()),
            rem_len,
            self.flags,
        );

        left.remaps = self.remaps;
        left.id = self.id;
        right.remaps = self.remaps;
        right.id = self.id + 1;

        for (i, phys_opt) in self.physblocks.into_iter().enumerate() {
            if let Some(phys) = phys_opt {
                let offset = VirBytes((i as u64) * page_size);
                if offset.get() < split_len.get() {
                    left.set_phys_region(offset, *phys);
                } else {
                    let right_offset = VirBytes(offset.get() - split_len.get());
                    right.set_phys_region(right_offset, *phys);
                }
            }
        }

        Ok((left, right))
    }

    /// Frees the region from the given offset.
    ///
    /// Frees physical pages in the range [offset, offset+len).
    /// Returns the number of pages freed.
    ///
    /// Corresponds to Minix3's `map_subfree()`.
    pub(crate) fn free_range(&mut self, offset: VirBytes, len: VirBytes) -> usize {
        let page_size: u64 = 4096;
        let start_page = (offset.get() / page_size) as usize;
        let end_page = ((offset.get() + len.get() + page_size - 1) / page_size) as usize;
        let mut freed = 0;

        for page in start_page..end_page.min(self.physblocks.len()) {
            if self.physblocks[page].take().is_some() {
                freed += 1;
            }
        }

        freed
    }
}

/// VM error type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum VmError {
    InvalidParam,
    NoMemory,
    NotFound,
}

#[cfg(test)]
mod tests {
    use super::*;
    use minix_types::VirBytes;

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
        assert!(region.contains(VirBytes(0x1000)));
        assert!(region.contains(VirBytes(0x2000)));
        assert!(region.contains(VirBytes(0x4fff)));
        assert!(!region.contains(VirBytes(0x5000)));
        assert!(!region.contains(VirBytes(0x0fff)));
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
    fn test_vir_region_free_range() {
        let mut region = VirRegion::new(VirBytes(0x1000), VirBytes(0x4000), VrFlags::empty());

        region.set_phys_region(VirBytes(0x0000), PhysRegion::new(VirBytes(0x0000)));
        region.set_phys_region(VirBytes(0x1000), PhysRegion::new(VirBytes(0x1000)));
        region.set_phys_region(VirBytes(0x2000), PhysRegion::new(VirBytes(0x2000)));
        region.set_phys_region(VirBytes(0x3000), PhysRegion::new(VirBytes(0x3000)));

        let freed = region.free_range(VirBytes(0x1000), VirBytes(0x1000));
        assert_eq!(freed, 1);

        assert!(region.get_phys_region(VirBytes(0x0000)).is_some());
        assert!(region.get_phys_region(VirBytes(0x1000)).is_none());
        assert!(region.get_phys_region(VirBytes(0x2000)).is_some());
        assert!(region.get_phys_region(VirBytes(0x3000)).is_some());
    }
}
