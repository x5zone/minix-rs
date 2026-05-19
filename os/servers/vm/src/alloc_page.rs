//! VM page-level allocator wrapper.
//!
//! Provides `VmPageAllocator` which wraps a physical memory allocator
//! (`PhysAlloc`) and handles the Direct Map VA↔PA translation for
//! single-page and multi-page allocations.
//!
//! Implements `PfnAllocator` trait for integration with PageFrames
//! (方案三：PFN 索引模型).

use minix_types::VirBytes;

use crate::alloc_stats::VmAllocStats;
use crate::direct_map::vm_phys_to_virt;
use crate::phys_mem::{PhysAlloc, PhysAllocator, PageAllocFlags, AllocError, AlignedPhysBytes};
use crate::region::{PfnAllocator, PfnAllocError, PAGE_SIZE};

pub(crate) struct VmPageAllocator {
    phys_alloc: PhysAlloc,
    stats: VmAllocStats,
}

impl VmPageAllocator {
    pub(crate) fn new(phys_alloc: PhysAlloc) -> Self {
        Self {
            phys_alloc,
            stats: VmAllocStats::new(),
        }
    }

    pub(crate) fn alloc_phys(
        &mut self, clicks: usize, flags: PageAllocFlags,
    ) -> Result<AlignedPhysBytes, AllocError> {
        let result = self.phys_alloc.alloc_mem(clicks, flags);
        match &result {
            Ok(_) => self.stats.record_alloc(clicks),
            Err(_) => self.stats.record_failure(),
        }
        result
    }

    pub(crate) fn alloc_page(&mut self, flags: PageAllocFlags) -> Option<(VirBytes, AlignedPhysBytes)> {
        let phys = self.alloc_phys(1, flags).ok()?;
        let virt = vm_phys_to_virt(phys);
        Some((virt, phys))
    }

    pub(crate) fn alloc_pages(
        &mut self, clicks: usize, flags: PageAllocFlags,
    ) -> Option<(VirBytes, AlignedPhysBytes)> {
        let phys = self.alloc_phys(clicks, flags).ok()?;
        let virt = vm_phys_to_virt(phys);
        Some((virt, phys))
    }

    pub(crate) fn free_page(&mut self, phys: AlignedPhysBytes) {
        self.free_pages(phys, 1);
    }

    pub(crate) fn free_pages(&mut self, phys: AlignedPhysBytes, clicks: usize) {
        self.phys_alloc.free_mem(phys, clicks);
        self.stats.record_dealloc(clicks);
    }

    pub(crate) fn total_pages(&self) -> usize {
        self.phys_alloc.total_count()
    }

    pub(crate) fn self_alloc_count(&self) -> usize {
        self.stats.active_allocations()
    }

    pub(crate) fn self_page_count(&self) -> usize {
        self.stats.active_pages()
    }

    pub(crate) fn stats(&self) -> &VmAllocStats {
        &self.stats
    }

    pub(crate) fn phys_alloc(&self) -> &PhysAlloc {
        &self.phys_alloc
    }

    pub(crate) fn phys_alloc_mut(&mut self) -> &mut PhysAlloc {
        &mut self.phys_alloc
    }
}

impl PfnAllocator for VmPageAllocator {
    fn alloc_pfn(&mut self) -> Result<u32, PfnAllocError> {
        self.alloc_phys(1, PageAllocFlags::empty())
            .map(|phys| (phys.as_u64() / PAGE_SIZE) as u32)
            .map_err(|_| PfnAllocError::OutOfMemory)
    }

    fn free_pfn(&mut self, pfn: u32) {
        let phys = AlignedPhysBytes::new(pfn as u64 * PAGE_SIZE);
        self.free_page(phys);
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
        minix_arch::direct_map::set_mock_vm_base(aligned_base as u64);

        let base = 0usize;
        let size = available_pages * CLICK_SIZE;
        let total_pages = available_pages;

        let meta_size = PhysAllocType::Bitmap.metadata_size(total_pages);
        let meta_pages = bytes_to_clicks(meta_size);

        let meta_phys_base = base;
        let meta_va = vm_phys_to_virt(AlignedPhysBytes::new(meta_phys_base as u64));
        let metadata = unsafe {
            core::slice::from_raw_parts_mut(meta_va.0 as *mut u8, meta_size)
        };

        let adjusted_base = meta_phys_base + meta_pages * CLICK_SIZE;
        let adjusted_size = size.saturating_sub(meta_pages * CLICK_SIZE);
        let adjusted_regions = [BootMemRegion { base: adjusted_base, size: adjusted_size }];

        PhysAlloc::Bitmap(BitmapAllocator::init(metadata, total_pages, &adjusted_regions, meta_phys_base as u64, meta_pages))
    }

    #[test]
    fn test_alloc_page() {
        let phys_alloc = make_test_phys_alloc(256);
        let mut alloc = VmPageAllocator::new(phys_alloc);

        let (v1, p1) = alloc.alloc_page(PageAllocFlags::empty()).unwrap();
        assert_eq!(v1.0 - p1.as_u64(), minix_arch::direct_map::mock_vm_base());

        let (v2, p2) = alloc.alloc_page(PageAllocFlags::empty()).unwrap();
        assert_ne!(p1.as_u64(), p2.as_u64());
        assert_eq!(v2.0 - p2.as_u64(), minix_arch::direct_map::mock_vm_base());
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
        assert_eq!(p3, p1);
        assert_ne!(p3, p2);
    }

    #[test]
    fn test_total_pages() {
        let phys_alloc = make_test_phys_alloc(10);
        let alloc = VmPageAllocator::new(phys_alloc);
        assert_eq!(alloc.total_pages(), 10);
    }

    #[test]
    fn test_alloc_pages_multi() {
        let phys_alloc = make_test_phys_alloc(256);
        let mut alloc = VmPageAllocator::new(phys_alloc);

        let (v1, p1) = alloc.alloc_pages(4, PageAllocFlags::empty()).unwrap();
        assert_eq!(v1.0 - p1.as_u64(), minix_arch::direct_map::mock_vm_base());
        assert_eq!(crate::direct_map::virt_to_phys(v1), p1);
        assert_eq!(crate::direct_map::virt_to_phys(VirBytes(v1.0 + 3 * CLICK_SIZE as u64)), AlignedPhysBytes::from_page_index(p1.page_index() + 3));

        let (v2, p2) = alloc.alloc_pages(2, PageAllocFlags::empty()).unwrap();
        assert_ne!(p1, p2);
        assert_eq!(crate::direct_map::virt_to_phys(v2), p2);
    }

    #[test]
    fn test_free_pages_multi() {
        let phys_alloc = make_test_phys_alloc(256);
        let mut alloc = VmPageAllocator::new(phys_alloc);

        let (_, p1) = alloc.alloc_pages(4, PageAllocFlags::empty()).unwrap();
        assert_eq!(alloc.self_alloc_count(), 1);
        assert_eq!(alloc.self_page_count(), 4);

        alloc.free_pages(p1, 4);
        assert_eq!(alloc.self_alloc_count(), 0);
        assert_eq!(alloc.self_page_count(), 0);
    }

    #[test]
    fn test_self_pages_tracking() {
        let phys_alloc = make_test_phys_alloc(256);
        let mut alloc = VmPageAllocator::new(phys_alloc);

        assert_eq!(alloc.self_alloc_count(), 0);
        assert_eq!(alloc.self_page_count(), 0);

        let (_, p1) = alloc.alloc_page(PageAllocFlags::empty()).unwrap();
        assert_eq!(alloc.self_alloc_count(), 1);
        assert_eq!(alloc.self_page_count(), 1);

        let (_, p2) = alloc.alloc_page(PageAllocFlags::empty()).unwrap();
        assert_eq!(alloc.self_alloc_count(), 2);
        assert_eq!(alloc.self_page_count(), 2);

        alloc.free_page(p1);
        assert_eq!(alloc.self_alloc_count(), 1);
        assert_eq!(alloc.self_page_count(), 1);

        alloc.free_page(p2);
        assert_eq!(alloc.self_alloc_count(), 0);
        assert_eq!(alloc.self_page_count(), 0);
    }
}
