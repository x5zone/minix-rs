use super::types::{AllocError, PageAllocFlags, PhysBytes};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PhysMemStats {
    pub free_nodes: usize,
    pub free_pages: usize,
    pub largest_free: usize,
}

pub trait PhysAllocator {
    fn alloc_mem(&mut self, clicks: usize, flags: PageAllocFlags) -> Result<PhysBytes, AllocError>;
    fn free_mem(&mut self, base: PhysBytes, clicks: usize);
    fn total_count(&self) -> usize;

    fn reloc_array_count(&self) -> usize { 0 }
    fn reloc_array_info(&self, _index: usize) -> (*const u8, usize, usize) {
        (core::ptr::null(), 0, 0)
    }
    fn update_relocated_arrays(&mut self, _new_ptrs: &[*mut u8]) {}
}

pub trait PhysAllocatorStats {
    fn memstats(&self) -> PhysMemStats;
}
