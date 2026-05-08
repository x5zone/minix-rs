use alloc::boxed::Box;

use minix_types::VirBytes;

use crate::direct_map::vm_phys_to_virt;
use crate::phys_mem::{PhysAllocator, PageAllocFlags, AllocError, PhysBytes};

pub(crate) struct VmPageAllocator {
    phys_alloc: Box<dyn PhysAllocator>,
}

impl VmPageAllocator {
    pub(crate) fn new(phys_alloc: Box<dyn PhysAllocator>) -> Self {
        Self { phys_alloc }
    }

    pub(crate) fn alloc_phys(
        &mut self, clicks: usize, flags: PageAllocFlags,
    ) -> Result<PhysBytes, AllocError> {
        self.phys_alloc.alloc_mem(clicks, flags)
    }

    pub(crate) fn alloc_page(&mut self) -> Option<(VirBytes, PhysBytes)> {
        let phys = self.alloc_phys(1, PageAllocFlags::empty()).ok()?;
        let virt = vm_phys_to_virt(phys);
        Some((virt, phys))
    }

    pub(crate) fn free_page(&mut self, phys: PhysBytes) {
        self.phys_alloc.free_mem(phys, 1);
    }

    pub(crate) fn total_pages(&self) -> usize {
        self.phys_alloc.total_count()
    }
}

pub(crate) struct ReservedRegion {
    phys_start: PhysBytes,
    total_pages: usize,
    allocated_pages: usize,
    bitmap: u64,
}

impl ReservedRegion {
    pub(crate) fn new(phys_start: PhysBytes, total_pages: usize) -> Self {
        assert!(total_pages <= 64, "reserved region too large for u64 bitmap");
        Self {
            phys_start,
            total_pages,
            allocated_pages: 0,
            bitmap: 0,
        }
    }

    pub(crate) fn alloc_page(&mut self) -> Option<(VirBytes, PhysBytes)> {
        let mask = !self.bitmap;
        let free_bit = mask.trailing_zeros() as usize;

        if free_bit >= self.total_pages {
            return None;
        }

        self.bitmap |= 1 << free_bit;
        self.allocated_pages += 1;

        let offset = free_bit * 4096;
        let phys = self.phys_start.add(offset);
        let virt = vm_phys_to_virt(phys);

        Some((virt, phys))
    }

    pub(crate) fn allocated_pages(&self) -> usize {
        self.allocated_pages
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::phys_mem::{PhysAllocator, PageAllocFlags, AllocError, PhysBytes};

    struct MockPhysAllocator {
        next: u64,
        total: usize,
    }

    impl MockPhysAllocator {
        fn new(total: usize) -> Self {
            Self { next: 0x1000, total }
        }
    }

    impl PhysAllocator for MockPhysAllocator {
        fn alloc_mem(&mut self, _clicks: usize, _flags: PageAllocFlags) -> Result<PhysBytes, AllocError> {
            if self.total == 0 {
                return Err(AllocError::OutOfMemory);
            }
            let addr = PhysBytes::new(self.next);
            self.next += 0x1000;
            self.total -= 1;
            Ok(addr)
        }

        fn free_mem(&mut self, _base: PhysBytes, _clicks: usize) {
            self.total += 1;
        }

        fn total_count(&self) -> usize {
            self.total
        }
    }

    #[test]
    fn test_alloc_page() {
        let phys_alloc = Box::new(MockPhysAllocator::new(256));
        let mut alloc = VmPageAllocator::new(phys_alloc);

        let (v1, p1) = alloc.alloc_page().unwrap();
        assert_eq!(v1.0, crate::direct_map::DIRECT_MAP_BASE + p1.as_u64());

        let (v2, p2) = alloc.alloc_page().unwrap();
        assert_ne!(p1.as_u64(), p2.as_u64());
        assert_eq!(v2.0, crate::direct_map::DIRECT_MAP_BASE + p2.as_u64());
    }

    #[test]
    fn test_alloc_phys_and_free() {
        let phys_alloc = Box::new(MockPhysAllocator::new(4));
        let mut alloc = VmPageAllocator::new(phys_alloc);

        let p1 = alloc.alloc_phys(1, PageAllocFlags::empty()).unwrap();
        let p2 = alloc.alloc_phys(1, PageAllocFlags::empty()).unwrap();
        assert_ne!(p1, p2);

        alloc.free_page(p1);
        let p3 = alloc.alloc_phys(1, PageAllocFlags::empty()).unwrap();
        assert_ne!(p3, p2);
    }

    #[test]
    fn test_reserved_alloc_and_exhaustion() {
        let mut reserved = ReservedRegion::new(PhysBytes::new(0x1000), 2);

        let (v1, p1) = reserved.alloc_page().unwrap();
        assert_eq!(p1.as_u64(), 0x1000);
        assert_eq!(v1.0, crate::direct_map::DIRECT_MAP_BASE + 0x1000);

        let (v2, p2) = reserved.alloc_page().unwrap();
        assert_eq!(p2.as_u64(), 0x2000);
        assert_eq!(v2.0, crate::direct_map::DIRECT_MAP_BASE + 0x2000);

        assert!(reserved.alloc_page().is_none());
    }

    #[test]
    fn test_total_pages() {
        let phys_alloc = Box::new(MockPhysAllocator::new(10));
        let mut alloc = VmPageAllocator::new(phys_alloc);
        assert_eq!(alloc.total_pages(), 10);

        alloc.alloc_page().unwrap();
        assert_eq!(alloc.total_pages(), 9);
    }
}
