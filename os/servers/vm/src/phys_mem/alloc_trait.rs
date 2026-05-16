use super::types::{AllocError, PageAllocFlags, AlignedPhysBytes};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PhysMemStats {
    pub free_nodes: usize,
    pub free_pages: usize,
    pub largest_free: usize,
}

pub trait PhysAllocator {
    fn alloc_mem(&mut self, clicks: usize, flags: PageAllocFlags) -> Result<AlignedPhysBytes, AllocError>;
    fn free_mem(&mut self, base: AlignedPhysBytes, clicks: usize);
    fn total_count(&self) -> usize;
    fn reserve_pages(&mut self, base_page: usize, count: usize);
}


