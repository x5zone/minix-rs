//! Physical region implementation.

#[cfg(test)]
use alloc::boxed::Box;
use core::ptr::NonNull;
use minix_types::{PhysBytes, VirBytes};
use crate::memtype::MemType;
use super::vir_region::VirRegion;

#[derive(Debug)]
pub(crate) struct PhysBlock {
    phys: PhysBytes,
    refcount: u16,
    flags: PhysBlockFlags,
    first_region: Option<NonNull<PhysRegion>>,
}

impl PhysBlock {
    pub(crate) const MAP_NONE: PhysBytes = PhysBytes(0xFFFF_FFFF_FFFF_FFFE);

    pub(crate) fn new(phys: PhysBytes) -> Self {
        debug_assert!(phys.0 != 0, "PhysBlock::new(0) is likely a bug; use PhysBlock::new(MAP_NONE) for unmapped blocks");
        Self {
            phys,
            refcount: 0,
            flags: PhysBlockFlags::empty(),
            first_region: None,
        }
    }

    pub(crate) fn phys(&self) -> PhysBytes {
        self.phys
    }

    pub(crate) fn set_phys(&mut self, phys: PhysBytes) {
        self.phys = phys;
    }

    pub(crate) fn is_mapped(&self) -> bool {
        self.phys != Self::MAP_NONE
    }

    pub(crate) fn refcount(&self) -> u16 {
        self.refcount
    }

    pub(crate) fn add_ref(&mut self) {
        self.refcount = self.refcount.saturating_add(1);
    }

    pub(crate) fn release_ref(&mut self) -> bool {
        if self.refcount > 0 {
            self.refcount -= 1;
        }
        self.refcount > 0
    }

    pub(crate) fn is_in_cache(&self) -> bool {
        self.flags.contains(PhysBlockFlags::IN_CACHE)
    }
}

bitflags::bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
    pub(crate) struct PhysBlockFlags: u8 {
        const IN_CACHE = 0x01;
    }
}

pub(crate) struct PhysRegion {
    pub ph: Option<NonNull<PhysBlock>>,
    pub parent: Option<NonNull<VirRegion>>,
    pub offset: VirBytes,
    pub memtype: Option<&'static dyn MemType>,
    pub next_ph_list: Option<NonNull<PhysRegion>>,
}

impl core::fmt::Debug for PhysRegion {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("PhysRegion")
            .field("ph", &self.ph)
            .field("parent", &self.parent)
            .field("offset", &self.offset)
            .field("memtype", &self.memtype.map(|m| m.name()))
            .field("next_ph_list", &self.next_ph_list)
            .finish()
    }
}

impl PhysRegion {
    pub(crate) fn new(offset: VirBytes) -> Self {
        Self {
            ph: None,
            parent: None,
            offset,
            memtype: None,
            next_ph_list: None,
        }
    }

    pub(crate) fn with_memtype(offset: VirBytes, memtype: &'static dyn MemType) -> Self {
        Self {
            ph: None,
            parent: None,
            offset,
            memtype: Some(memtype),
            next_ph_list: None,
        }
    }

    /// Binds this PhysRegion to a PhysBlock (simple refcount only, no list management).
    ///
    /// For full list management, use `link_to_block()` instead.
    /// Corresponds to a simplified version of Minix3's `pb_link()`.
    ///
    /// # Safety
    /// Caller must ensure:
    /// - `block` points to a valid PhysBlock
    /// - `block`'s lifetime exceeds that of `self`
    /// - No concurrent modifications to `block` (VM is single-threaded)
    /// - After this call, `self.ph` is set and subsequent operations depend on `block` remaining valid
    pub(crate) unsafe fn bind_block(&mut self, block: NonNull<PhysBlock>) {
        self.ph = Some(block);
        unsafe {
            (*block.as_ptr()).add_ref();
        }
    }

    pub(crate) fn unbind_block(&mut self) -> bool {
        if let Some(block) = self.ph {
            // SAFETY: block.as_ptr() is valid because self.ph is Some(NonNull),
            // meaning the pointer is non-null and was obtained from a valid reference.
            // The PhysBlock is alive because we hold a reference to it via self.ph.
            unsafe {
                let has_more_refs = (*block.as_ptr()).release_ref();
                self.ph = None;
                has_more_refs
            }
        } else {
            false
        }
    }

    pub(crate) fn get_phys_addr(&self) -> Option<PhysBytes> {
        // SAFETY: block.as_ptr() is valid because NonNull guarantees non-null,
        // and the PhysBlock is alive as long as self.ph is Some.
        self.ph.map(|block| unsafe { (*block.as_ptr()).phys })
    }

    pub(crate) fn has_phys_block(&self) -> bool {
        self.ph.is_some()
    }

    pub(crate) fn get_refcount(&self) -> Option<u16> {
        // SAFETY: same as get_phys_addr.
        self.ph.map(|block| unsafe { (*block.as_ptr()).refcount })
    }

    pub(crate) fn is_writable(&self) -> bool {
        if let Some(memtype) = self.memtype {
            memtype.is_writable(self)
        } else {
            match self.get_refcount() {
                Some(1) => true,
                _ => false,
            }
        }
    }

    pub(crate) fn needs_cow(&self) -> bool {
        match self.get_refcount() {
            Some(count) if count > 1 => true,
            _ => false,
        }
    }

    pub(crate) fn get_block_flags(&self) -> Option<PhysBlockFlags> {
        // SAFETY: same as get_phys_addr.
        self.ph.map(|block| unsafe { (*block.as_ptr()).flags })
    }

    pub(crate) fn is_in_cache(&self) -> bool {
        self.get_block_flags()
            .map(|flags| flags.contains(PhysBlockFlags::IN_CACHE))
            .unwrap_or(false)
    }

    pub(crate) fn get_virtual_addr(&self) -> Option<VirBytes> {
        // SAFETY: parent.as_ptr() is valid because self.parent is Some(NonNull),
        // and the VirRegion is alive as long as this PhysRegion exists.
        self.parent.map(|parent: NonNull<VirRegion>| unsafe {
            VirBytes((*parent.as_ptr()).vaddr.0 + self.offset.0)
        })
    }

    pub(crate) fn get_page_index(&self) -> usize {
        const PAGE_SIZE: u64 = 4096;
        (self.offset.0 / PAGE_SIZE) as usize
    }

    pub(crate) fn validate_offset(&self, region_length: VirBytes) -> bool {
        const PAGE_SIZE: u64 = 4096;

        let offset = self.offset.0;

        if offset % PAGE_SIZE != 0 {
            return false;
        }

        if offset >= region_length.0 {
            return false;
        }

        true
    }

    pub(crate) fn offset_from_vaddr(virtual_addr: VirBytes, region_vaddr: VirBytes) -> VirBytes {
        VirBytes(virtual_addr.0 - region_vaddr.0)
    }

    pub(crate) fn offset_from_page_index(page_index: usize) -> VirBytes {
        const PAGE_SIZE: u64 = 4096;
        VirBytes((page_index as u64) * PAGE_SIZE)
    }

    /// Links this PhysRegion to a PhysBlock and parent VirRegion.
    ///
    /// Corresponds to Minix3's `pb_link()`.
    ///
    /// # Safety
    /// Caller must ensure:
    /// - `block` points to a valid PhysBlock
    /// - `parent` points to a valid VirRegion
    /// - This PhysRegion is not already linked to another block
    /// - `block`'s lifetime exceeds that of `self`
    /// - No concurrent modifications to `block` (VM is single-threaded)
    pub(crate) unsafe fn link_to_block(&mut self, block: NonNull<PhysBlock>, parent: NonNull<VirRegion>, offset: VirBytes) {
        debug_assert!(self.ph.is_none(), "PhysRegion must not already be in a list");

        self.offset = offset;
        self.ph = Some(block);
        self.parent = Some(parent);

        let block_ptr = block.as_ptr();
        self.next_ph_list = unsafe { (*block_ptr).first_region };
        unsafe {
            (*block_ptr).first_region = NonNull::new(self as *mut PhysRegion);
            (*block_ptr).refcount = (*block_ptr).refcount.saturating_add(1);
        }

        debug_assert!(unsafe { (*block_ptr).refcount } > 0, "refcount must be positive after link");
        debug_assert!(self.ph.is_some(), "ph must be set after link");
    }

    fn is_in_list(&self, block: NonNull<PhysBlock>) -> bool {
        // SAFETY: block.as_ptr() is valid because NonNull guarantees non-null,
        // and the caller ensures the PhysBlock is alive. The linked list
        // traversal follows next_ph_list pointers set by link_to_block().
        unsafe {
            let block_ptr = block.as_ptr();
            let mut current = (*block_ptr).first_region;
            while let Some(ptr) = current {
                if ptr.as_ptr() == self as *const PhysRegion as *mut PhysRegion {
                    return true;
                }
                current = (*ptr.as_ptr()).next_ph_list;
            }
            false
        }
    }

    pub(crate) fn unlink_from_block(&mut self) -> bool {
        if let Some(block) = self.ph {
            // SAFETY: block.as_ptr() is valid because self.ph is Some(NonNull).
            // All pointer dereferences are within the PhysBlock's linked list,
            // which is only modified through link_to_block/unlink_from_block
            // under &mut self, ensuring no aliasing violations.
            unsafe {
                let block_ptr = block.as_ptr();
                debug_assert!((*block_ptr).refcount > 0, "refcount must be positive before unlink");
                debug_assert!(self.is_in_list(block), "PhysRegion must be in the list");

                (*block_ptr).refcount = (*block_ptr).refcount.saturating_sub(1);

                let self_ptr = NonNull::new(self as *mut PhysRegion);
                if (*block_ptr).first_region == self_ptr {
                    (*block_ptr).first_region = self.next_ph_list;
                } else if let Some(first) = (*block_ptr).first_region {
                    let mut current = first;
                    loop {
                        let next = (*current.as_ptr()).next_ph_list;
                        if next == self_ptr {
                            (*current.as_ptr()).next_ph_list = self.next_ph_list;
                            break;
                        }
                        match next {
                            Some(n) => current = n,
                            None => {
                                debug_assert!(false, "PhysRegion not found in list");
                                break;
                            }
                        }
                    }
                }

                self.ph = None;
                self.next_ph_list = None;

                debug_assert!(self.ph.is_none(), "ph must be None after unlink");
                debug_assert!(self.next_ph_list.is_none(), "next_ph_list must be None after unlink");

                (*block_ptr).refcount == 0
            }
        } else {
            false
        }
    }

    pub(crate) fn iterate_block_refs<F>(block: &PhysBlock, mut f: F)
    where
        F: FnMut(&PhysRegion),
    {
        let mut current = block.first_region;
        while let Some(ptr) = current {
            // SAFETY: ptr.as_ptr() is valid because it was obtained from
            // block.first_region chain, which is only populated by
            // link_to_block() with valid NonNull pointers.
            unsafe {
                let region = &*ptr.as_ptr();
                f(region);
                current = region.next_ph_list;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;
    use crate::region::{VirRegion, VrFlags};

    #[test]
    fn test_phys_block_refcount() {
        let mut block = PhysBlock::new(PhysBytes(0x1000));
        assert_eq!(block.refcount, 0);

        block.add_ref();
        assert_eq!(block.refcount, 1);

        block.add_ref();
        assert_eq!(block.refcount, 2);

        assert!(block.release_ref());
        assert_eq!(block.refcount, 1);

        assert!(!block.release_ref());
        assert_eq!(block.refcount, 0);
    }

    #[test]
    fn test_phys_region_bind() {
        let mut region = PhysRegion::new(VirBytes(0));
        let mut block = PhysBlock::new(PhysBytes(0x1000));

        unsafe { region.bind_block(NonNull::from(&mut block)); }
        assert_eq!(block.refcount, 1);

        assert!(!region.unbind_block());
        assert_eq!(block.refcount, 0);
    }

    #[test]
    fn test_link_to_block() {
        let mut block = PhysBlock::new(PhysBytes(0x8000));
        let mut region1 = Box::new(PhysRegion::new(VirBytes(0x1000)));
        let mut region2 = Box::new(PhysRegion::new(VirBytes(0x2000)));
        let mut vir_region = Box::new(VirRegion::new(VirBytes(0x400000), VirBytes(0x3000), VrFlags(0)));

        let block_ptr = NonNull::from(&mut block);
        let parent_ptr = NonNull::from(&mut *vir_region);

        unsafe {
            region1.link_to_block(block_ptr, parent_ptr, VirBytes(0));
            assert_eq!(block.refcount, 1);

            region2.link_to_block(block_ptr, parent_ptr, VirBytes(0));
            assert_eq!(block.refcount, 2);
            assert_eq!(block.first_region, NonNull::new(&mut *region2 as *mut PhysRegion));
            assert_eq!(region2.next_ph_list, NonNull::new(&mut *region1 as *mut PhysRegion));
        }
    }

    #[test]
    fn test_unlink_from_block() {
        let mut block = PhysBlock::new(PhysBytes(0x8000));
        let mut region1 = Box::new(PhysRegion::new(VirBytes(0x1000)));
        let mut region2 = Box::new(PhysRegion::new(VirBytes(0x2000)));
        let mut vir_region = Box::new(VirRegion::new(VirBytes(0x400000), VirBytes(0x3000), VrFlags(0)));

        let block_ptr = NonNull::from(&mut block);
        let parent_ptr = NonNull::from(&mut *vir_region);

        unsafe {
            region1.link_to_block(block_ptr, parent_ptr, VirBytes(0));
            region2.link_to_block(block_ptr, parent_ptr, VirBytes(0));

            assert_eq!(block.refcount, 2);

            let should_free = region2.unlink_from_block();
            assert!(!should_free);
            assert_eq!(block.refcount, 1);
            assert_eq!(block.first_region, NonNull::new(&mut *region1 as *mut PhysRegion));

            let should_free = region1.unlink_from_block();
            assert!(should_free);
            assert_eq!(block.refcount, 0);
            assert_eq!(block.first_region, None);
        }
    }

    #[test]
    fn test_iterate_block_refs() {
        let mut block = PhysBlock::new(PhysBytes(0x8000));
        let mut region1 = Box::new(PhysRegion::new(VirBytes(0x1000)));
        let mut region2 = Box::new(PhysRegion::new(VirBytes(0x2000)));
        let mut region3 = Box::new(PhysRegion::new(VirBytes(0x3000)));
        let mut vir_region = Box::new(VirRegion::new(VirBytes(0x400000), VirBytes(0x3000), VrFlags(0)));

        let block_ptr = NonNull::from(&mut block);
        let parent_ptr = NonNull::from(&mut *vir_region);

        unsafe {
            region1.link_to_block(block_ptr, parent_ptr, VirBytes(0x1000));
            region2.link_to_block(block_ptr, parent_ptr, VirBytes(0x2000));
            region3.link_to_block(block_ptr, parent_ptr, VirBytes(0x3000));
        }

        let mut count = 0;
        let mut offsets = Vec::new();
        PhysRegion::iterate_block_refs(&block, |region| {
            count += 1;
            offsets.push(region.offset.0);
        });

        assert_eq!(count, 3);
        assert_eq!(offsets, vec![0x3000, 0x2000, 0x1000]);
    }

    #[test]
    fn test_unlink_middle_node() {
        let mut block = PhysBlock::new(PhysBytes(0x8000));
        let mut region1 = Box::new(PhysRegion::new(VirBytes(0x1000)));
        let mut region2 = Box::new(PhysRegion::new(VirBytes(0x2000)));
        let mut region3 = Box::new(PhysRegion::new(VirBytes(0x3000)));
        let mut vir_region = Box::new(VirRegion::new(VirBytes(0x400000), VirBytes(0x3000), VrFlags(0)));

        let block_ptr = NonNull::from(&mut block);
        let parent_ptr = NonNull::from(&mut *vir_region);

        unsafe {
            region1.link_to_block(block_ptr, parent_ptr, VirBytes(0x1000));
            region2.link_to_block(block_ptr, parent_ptr, VirBytes(0x2000));
            region3.link_to_block(block_ptr, parent_ptr, VirBytes(0x3000));

            let should_free = region2.unlink_from_block();
            assert!(!should_free);
            assert_eq!(block.refcount, 2);

            let mut count = 0;
            PhysRegion::iterate_block_refs(&block, |_| count += 1);
            assert_eq!(count, 2);
        }
    }

    #[test]
    fn test_offset_calculations() {
        let region = PhysRegion::new(VirBytes(0x2000));

        assert_eq!(region.get_page_index(), 2);

        let vaddr = PhysRegion::offset_from_vaddr(VirBytes(0x401500), VirBytes(0x400000));
        assert_eq!(vaddr, VirBytes(0x1500));

        let offset = PhysRegion::offset_from_page_index(5);
        assert_eq!(offset, VirBytes(0x5000));
    }

    #[test]
    fn test_validate_offset() {
        let region1 = PhysRegion::new(VirBytes(0x1000));
        assert!(region1.validate_offset(VirBytes(0x3000)));

        let region2 = PhysRegion::new(VirBytes(0x1001));
        assert!(!region2.validate_offset(VirBytes(0x3000)));

        let region3 = PhysRegion::new(VirBytes(0x4000));
        assert!(!region3.validate_offset(VirBytes(0x3000)));
    }

    #[test]
    fn test_virtual_addr_calculation() {
        let mut vir_region = Box::new(VirRegion::new(
            VirBytes(0x400000),
            VirBytes(0x3000),
            crate::region::VrFlags(0),
        ));

        let mut phys_region = Box::new(PhysRegion::new(VirBytes(0x1000)));
        phys_region.parent = Some(NonNull::from(&mut *vir_region));

        let vaddr = phys_region.get_virtual_addr();
        assert_eq!(vaddr, Some(VirBytes(0x401000)));
    }

    #[test]
    fn test_has_phys_block() {
        let mut region = PhysRegion::new(VirBytes(0x1000));
        assert!(!region.has_phys_block());

        let mut block = PhysBlock::new(PhysBytes(0x8000));
        unsafe { region.bind_block(NonNull::from(&mut block)); }
        assert!(region.has_phys_block());
    }

    #[test]
    fn test_refcount_methods() {
        let mut block = PhysBlock::new(PhysBytes(0x8000));
        let mut region1 = PhysRegion::new(VirBytes(0x1000));
        let mut region2 = PhysRegion::new(VirBytes(0x2000));

        unsafe { region1.bind_block(NonNull::from(&mut block)); }
        assert_eq!(region1.get_refcount(), Some(1));
        assert!(region1.is_writable());
        assert!(!region1.needs_cow());

        unsafe { region2.bind_block(NonNull::from(&mut block)); }
        assert_eq!(region1.get_refcount(), Some(2));
        assert!(!region1.is_writable());
        assert!(region1.needs_cow());
    }

    #[test]
    fn test_block_flags() {
        let mut block = PhysBlock::new(PhysBytes(0x8000));
        block.flags = PhysBlockFlags::IN_CACHE;

        let mut region = PhysRegion::new(VirBytes(0x1000));
        unsafe { region.bind_block(NonNull::from(&mut block)); }

        assert!(region.is_in_cache());
        assert_eq!(region.get_block_flags(), Some(PhysBlockFlags::IN_CACHE));
    }

    #[test]
    fn test_page_flags() {
        use crate::pagetable::PageFlags;
        let flags = PageFlags::read_only();
        assert!(flags.contains(PageFlags::PRESENT));
        assert!(!flags.contains(PageFlags::WRITABLE));
        assert!(flags.contains(PageFlags::USER_ACCESSIBLE));

        let flags = PageFlags::read_write();
        assert!(flags.contains(PageFlags::WRITABLE));
    }
}
