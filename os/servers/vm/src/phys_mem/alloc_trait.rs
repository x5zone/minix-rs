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
    fn reserve_pages(&mut self, base_page: usize, count: usize);
}

pub trait PhysAllocatorStats {
    fn memstats(&self) -> PhysMemStats;
}
