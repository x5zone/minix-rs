//! Physical region implementation.

#[cfg(test)]
use alloc::boxed::Box;
use alloc::vec::Vec;
use super::vir_region::VirRegion;
use minix_types::VirBytes;
use crate::memtype::MemType;
use crate::pagetable::PageFlags;

#[derive(Debug)]
pub(crate) struct PhysBlock {
    pub phys: u64,
    pub refcount: u8,
    pub flags: PhysBlockFlags,
    pub first_region: Option<*mut PhysRegion>,
}

impl PhysBlock {
    pub(crate) fn new(phys: u64) -> Self {
        Self {
            phys,
            refcount: 0,
            flags: PhysBlockFlags::empty(),
            first_region: None,
        }
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
    pub ph: Option<*mut PhysBlock>,
    pub parent: Option<*mut VirRegion>,
    pub offset: VirBytes,
    pub memtype: Option<&'static dyn MemType>,
    pub next_ph_list: Option<*mut PhysRegion>,
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

    /// Binds this PhysRegion to a PhysBlock.
    ///
    /// # Safety
    /// Caller must ensure `block` is a valid, non-null pointer to a PhysBlock.
    pub(crate) unsafe fn bind_block(&mut self, block: *mut PhysBlock) {
        self.ph = Some(block);
        unsafe {
            (*block).add_ref();
        }
    }

    pub(crate) fn unbind_block(&mut self) -> bool {
        if let Some(block) = self.ph {
            unsafe {
                let has_more_refs = (*block).release_ref();
                self.ph = None;
                has_more_refs
            }
        } else {
            false
        }
    }

    pub(crate) fn get_phys_addr(&self) -> Option<u64> {
        self.ph.map(|block| unsafe { (*block).phys })
    }

    pub(crate) fn has_phys_block(&self) -> bool {
        self.ph.is_some()
    }

    pub(crate) fn get_refcount(&self) -> Option<u8> {
        self.ph.map(|block| unsafe { (*block).refcount })
    }

    pub(crate) fn is_writable(&self) -> bool {
        match self.get_refcount() {
            Some(1) => true,
            _ => false,
        }
    }

    pub(crate) fn needs_cow(&self) -> bool {
        match self.get_refcount() {
            Some(count) if count > 1 => true,
            _ => false,
        }
    }

    pub(crate) fn get_block_flags(&self) -> Option<PhysBlockFlags> {
        self.ph.map(|block| unsafe { (*block).flags })
    }

    pub(crate) fn is_in_cache(&self) -> bool {
        self.get_block_flags()
            .map(|flags| flags.contains(PhysBlockFlags::IN_CACHE))
            .unwrap_or(false)
    }

    pub(crate) fn get_virtual_addr(&self) -> Option<VirBytes> {
        self.parent.map(|parent| unsafe {
            VirBytes((*parent).vaddr.0 + self.offset.0)
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
    /// # Safety
    /// Caller must ensure:
    /// - `block` is a valid, non-null pointer to a PhysBlock
    /// - `parent` is a valid pointer to the parent VirRegion
    /// - This PhysRegion is not already linked to another block
    pub(crate) unsafe fn link_to_block(&mut self, block: *mut PhysBlock, parent: *mut VirRegion) {
        debug_assert!(!block.is_null(), "block pointer must not be null");
        debug_assert!(self.ph.is_none(), "PhysRegion must not already be in a list");

        self.ph = Some(block);
        self.parent = Some(parent);

        self.next_ph_list = unsafe { (*block).first_region };
        unsafe {
            (*block).first_region = Some(self as *mut PhysRegion);
            (*block).refcount = (*block).refcount.saturating_add(1);
        }

        debug_assert!(unsafe { (*block).refcount } > 0, "refcount must be positive after link");
        debug_assert!(self.ph.is_some(), "ph must be set after link");
    }

    fn is_in_list(&self, block: *mut PhysBlock) -> bool {
        unsafe {
            let mut current = (*block).first_region;
            while let Some(ptr) = current {
                if ptr == self as *const PhysRegion as *mut PhysRegion {
                    return true;
                }
                current = (*ptr).next_ph_list;
            }
            false
        }
    }

    pub(crate) fn unlink_from_block(&mut self) -> bool {
        if let Some(block) = self.ph {
            unsafe {
                debug_assert!((*block).refcount > 0, "refcount must be positive before unlink");
                debug_assert!(self.is_in_list(block), "PhysRegion must be in the list");

                (*block).refcount = (*block).refcount.saturating_sub(1);

                if (*block).first_region == Some(self as *mut PhysRegion) {
                    (*block).first_region = self.next_ph_list;
                } else if let Some(first) = (*block).first_region {
                    let mut current = first;
                    loop {
                        let next = (*current).next_ph_list;
                        if next == Some(self as *mut PhysRegion) {
                            (*current).next_ph_list = self.next_ph_list;
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

                (*block).refcount == 0
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
            unsafe {
                let region = &*ptr;
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

    #[test]
    fn test_phys_block_refcount() {
        let mut block = PhysBlock::new(0x1000);
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
        use minix_types::VirBytes;
        let mut region = PhysRegion::new(VirBytes(0));
        let mut block = PhysBlock::new(0x1000);

        unsafe { region.bind_block(&mut block); }
        assert_eq!(block.refcount, 1);

        assert!(!region.unbind_block());
        assert_eq!(block.refcount, 0);
    }

    #[test]
    fn test_link_to_block() {
        use minix_types::VirBytes;
        
        let mut block = PhysBlock::new(0x8000);
        let mut region1 = Box::new(PhysRegion::new(VirBytes(0x1000)));
        let mut region2 = Box::new(PhysRegion::new(VirBytes(0x2000)));

        let block_ptr = &mut block as *mut PhysBlock;

        unsafe {
            region1.link_to_block(block_ptr, core::ptr::null_mut());
            assert_eq!(block.refcount, 1);
            assert_eq!(block.first_region, Some(&mut *region1 as *mut PhysRegion));

            region2.link_to_block(block_ptr, core::ptr::null_mut());
            assert_eq!(block.refcount, 2);
            assert_eq!(block.first_region, Some(&mut *region2 as *mut PhysRegion));
            assert_eq!(region2.next_ph_list, Some(&mut *region1 as *mut PhysRegion));
        }
    }

    #[test]
    fn test_unlink_from_block() {
        use minix_types::VirBytes;
        
        let mut block = PhysBlock::new(0x8000);
        let mut region1 = Box::new(PhysRegion::new(VirBytes(0x1000)));
        let mut region2 = Box::new(PhysRegion::new(VirBytes(0x2000)));

        let block_ptr = &mut block as *mut PhysBlock;

        unsafe {
            region1.link_to_block(block_ptr, core::ptr::null_mut());
            region2.link_to_block(block_ptr, core::ptr::null_mut());

            assert_eq!(block.refcount, 2);

            let should_free = region2.unlink_from_block();
            assert!(!should_free);
            assert_eq!(block.refcount, 1);
            assert_eq!(block.first_region, Some(&mut *region1 as *mut PhysRegion));

            let should_free = region1.unlink_from_block();
            assert!(should_free);
            assert_eq!(block.refcount, 0);
            assert_eq!(block.first_region, None);
        }
    }

    #[test]
    fn test_iterate_block_refs() {
        use minix_types::VirBytes;
        
        let mut block = PhysBlock::new(0x8000);
        let mut region1 = Box::new(PhysRegion::new(VirBytes(0x1000)));
        let mut region2 = Box::new(PhysRegion::new(VirBytes(0x2000)));
        let mut region3 = Box::new(PhysRegion::new(VirBytes(0x3000)));

        let block_ptr = &mut block as *mut PhysBlock;

        unsafe {
            region1.link_to_block(block_ptr, core::ptr::null_mut());
            region2.link_to_block(block_ptr, core::ptr::null_mut());
            region3.link_to_block(block_ptr, core::ptr::null_mut());
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
        use minix_types::VirBytes;
        
        let mut block = PhysBlock::new(0x8000);
        let mut region1 = Box::new(PhysRegion::new(VirBytes(0x1000)));
        let mut region2 = Box::new(PhysRegion::new(VirBytes(0x2000)));
        let mut region3 = Box::new(PhysRegion::new(VirBytes(0x3000)));

        let block_ptr = &mut block as *mut PhysBlock;

        unsafe {
            region1.link_to_block(block_ptr, core::ptr::null_mut());
            region2.link_to_block(block_ptr, core::ptr::null_mut());
            region3.link_to_block(block_ptr, core::ptr::null_mut());

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
        use minix_types::VirBytes;
        
        let region = PhysRegion::new(VirBytes(0x2000));
        
        assert_eq!(region.get_page_index(), 2);
        
        let vaddr = PhysRegion::offset_from_vaddr(VirBytes(0x401500), VirBytes(0x400000));
        assert_eq!(vaddr, VirBytes(0x1500));
        
        let offset = PhysRegion::offset_from_page_index(5);
        assert_eq!(offset, VirBytes(0x5000));
    }

    #[test]
    fn test_validate_offset() {
        use minix_types::VirBytes;
        
        let region1 = PhysRegion::new(VirBytes(0x1000));
        assert!(region1.validate_offset(VirBytes(0x3000)));
        
        let region2 = PhysRegion::new(VirBytes(0x1001));
        assert!(!region2.validate_offset(VirBytes(0x3000)));
        
        let region3 = PhysRegion::new(VirBytes(0x4000));
        assert!(!region3.validate_offset(VirBytes(0x3000)));
    }

    #[test]
    fn test_virtual_addr_calculation() {
        use minix_types::VirBytes;
        use crate::region::VirRegion;
        
        let mut vir_region = Box::new(VirRegion::new(
            VirBytes(0x400000),
            VirBytes(0x3000),
            crate::region::VrFlags(0),
        ));
        
        let mut phys_region = Box::new(PhysRegion::new(VirBytes(0x1000)));
        phys_region.parent = Some(&mut *vir_region as *mut VirRegion);
        
        let vaddr = phys_region.get_virtual_addr();
        assert_eq!(vaddr, Some(VirBytes(0x401000)));
    }

    #[test]
    fn test_has_phys_block() {
        use minix_types::VirBytes;
        
        let mut region = PhysRegion::new(VirBytes(0x1000));
        assert!(!region.has_phys_block());
        
        let mut block = PhysBlock::new(0x8000);
        unsafe { region.bind_block(&mut block); }
        assert!(region.has_phys_block());
    }

    #[test]
    fn test_refcount_methods() {
        use minix_types::VirBytes;
        
        let mut block = PhysBlock::new(0x8000);
        let mut region1 = PhysRegion::new(VirBytes(0x1000));
        let mut region2 = PhysRegion::new(VirBytes(0x2000));
        
        unsafe { region1.bind_block(&mut block); }
        assert_eq!(region1.get_refcount(), Some(1));
        assert!(region1.is_writable());
        assert!(!region1.needs_cow());
        
        unsafe { region2.bind_block(&mut block); }
        assert_eq!(region1.get_refcount(), Some(2));
        assert!(!region1.is_writable());
        assert!(region1.needs_cow());
    }

    #[test]
    fn test_block_flags() {
        use minix_types::VirBytes;
        
        let mut block = PhysBlock::new(0x8000);
        block.flags = PhysBlockFlags::IN_CACHE;
        
        let mut region = PhysRegion::new(VirBytes(0x1000));
        unsafe { region.bind_block(&mut block); }
        
        assert!(region.is_in_cache());
        assert_eq!(region.get_block_flags(), Some(PhysBlockFlags::IN_CACHE));
    }

    #[test]
    fn test_page_flags() {
        let flags = PageFlags::read_only();
        assert!(flags.present());
        assert!(!flags.writable());
        assert!(flags.user_accessible());

        let flags = PageFlags::read_write();
        assert!(flags.writable());
    }
}
