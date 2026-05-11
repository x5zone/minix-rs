use minix_types::VirBytes;

use crate::direct_map::vm_phys_to_virt;
use crate::phys_mem::{PhysAlloc, PhysAllocator, PageAllocFlags, AllocError, PhysBytes};

pub(crate) struct VmPageAllocator {
    phys_alloc: PhysAlloc,
}

impl VmPageAllocator {
    pub(crate) fn new(phys_alloc: PhysAlloc) -> Self {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::phys_mem::{BitmapAllocator, BootMemRegion, PhysAllocType, bytes_to_clicks, CLICK_SIZE};
    use crate::direct_map::vm_phys_to_virt;

    fn make_test_phys_alloc(available_pages: usize) -> PhysAlloc {
        let mock_phys_size = available_pages * CLICK_SIZE + CLICK_SIZE;
        let mock_phys: alloc::vec::Vec<u8> = alloc::vec![0u8; mock_phys_size];
        let mock_phys_leaked = alloc::boxed::Box::leak(mock_phys.into_boxed_slice());

        let raw_base = mock_phys_leaked.as_ptr() as usize;
        let aligned_base = (raw_base + CLICK_SIZE - 1) & !(CLICK_SIZE - 1);
        crate::direct_map::set_mock_phys_base(aligned_base as u64);

        let base = 0usize;
        let size = available_pages * CLICK_SIZE;
        let total_pages = available_pages;

        let meta_size = PhysAllocType::Bitmap.metadata_size(total_pages);
        let meta_pages = bytes_to_clicks(meta_size);

        let meta_phys_base = base;
        let meta_va = vm_phys_to_virt(PhysBytes::new(meta_phys_base as u64));
        let metadata = unsafe {
            core::slice::from_raw_parts_mut(meta_va.0 as *mut u8, meta_size)
        };

        let adjusted_base = meta_phys_base + meta_pages * CLICK_SIZE;
        let adjusted_size = size.saturating_sub(meta_pages * CLICK_SIZE);
        let adjusted_regions = [BootMemRegion { base: adjusted_base, size: adjusted_size }];

        PhysAlloc::Bitmap(BitmapAllocator::init(metadata, total_pages, &adjusted_regions))
    }

    #[test]
    fn test_alloc_page() {
        let phys_alloc = make_test_phys_alloc(256);
        let mut alloc = VmPageAllocator::new(phys_alloc);

        let (v1, p1) = alloc.alloc_page().unwrap();
        assert_eq!(v1.0 - p1.as_u64(), crate::direct_map::mock_map::offset());

        let (v2, p2) = alloc.alloc_page().unwrap();
        assert_ne!(p1.as_u64(), p2.as_u64());
        assert_eq!(v2.0 - p2.as_u64(), crate::direct_map::mock_map::offset());
    }

    #[test]
    fn test_alloc_phys_and_free() {
        let phys_alloc = make_test_phys_alloc(256);
        let mut alloc = VmPageAllocator::new(phys_alloc);

        let p1 = alloc.alloc_phys(1, PageAllocFlags::empty()).unwrap();
        let p2 = alloc.alloc_phys(1, PageAllocFlags::empty()).unwrap();
        assert_ne!(p1, p2);

        alloc.free_page(p1);
        let p3 = alloc.alloc_phys(1, PageAllocFlags::empty()).unwrap();
        assert_ne!(p3, p2);
    }

    #[test]
    fn test_total_pages() {
        let phys_alloc = make_test_phys_alloc(10);
        let alloc = VmPageAllocator::new(phys_alloc);
        assert_eq!(alloc.total_pages(), 10);
    }
}
