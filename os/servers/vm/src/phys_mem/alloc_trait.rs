use super::types::{AllocError, PageAllocFlags, PhysAddr};

pub(crate) trait PhysMemAlloc {
    fn alloc_mem(&mut self, clicks: usize, flags: PageAllocFlags) -> Result<PhysAddr, AllocError>;
    fn free_mem(&mut self, base: PhysAddr, clicks: usize);
}
