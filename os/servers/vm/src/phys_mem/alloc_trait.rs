//! Physical memory allocator trait definitions.

use super::{AllocFlags, PhysAddr, PhysMemAllocator};
use super::stats::MemStats;
use alloc::vec::Vec;
use core::cell::RefCell;
use core::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AllocError {
    OutOfMemory,
    AlignmentFailed,
    ContiguityFailed,
    LowMemoryExhausted,
}

impl fmt::Display for AllocError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AllocError::OutOfMemory => write!(f, "out of memory"),
            AllocError::AlignmentFailed => write!(f, "alignment requirement failed"),
            AllocError::ContiguityFailed => write!(f, "contiguity requirement failed"),
            AllocError::LowMemoryExhausted => write!(f, "low memory exhausted"),
        }
    }
}

pub(crate) trait PhysMemAlloc {
    fn alloc(&mut self, clicks: usize, flags: AllocFlags) -> Result<PhysAddr, AllocError>;
    fn free(&mut self, addr: PhysAddr, clicks: usize);
    fn stats(&self) -> &MemStats;
}

impl PhysMemAlloc for PhysMemAllocator {
    fn alloc(&mut self, clicks: usize, flags: AllocFlags) -> Result<PhysAddr, AllocError> {
        PhysMemAllocator::alloc(self, clicks, flags).ok_or(AllocError::OutOfMemory)
    }

    fn free(&mut self, addr: PhysAddr, clicks: usize) {
        PhysMemAllocator::free(self, addr, clicks);
    }

    fn stats(&self) -> &MemStats {
        PhysMemAllocator::stats(self)
    }
}

#[derive(Debug)]
pub(crate) struct MockPhysMemAlloc {
    allocations: Vec<(PhysAddr, usize)>,
    next_addr: u64,
    fail_after: Option<usize>,
    stats: MemStats,
}

impl MockPhysMemAlloc {
    pub(crate) fn new() -> Self {
        Self {
            allocations: Vec::new(),
            next_addr: 0x100000,
            fail_after: None,
            stats: MemStats::new(),
        }
    }

    pub(crate) fn fail_after(&mut self, n: usize) {
        self.fail_after = Some(n);
    }

    pub(crate) fn allocation_count(&self) -> usize {
        self.allocations.len()
    }

    pub(crate) fn is_allocated(&self, addr: PhysAddr) -> bool {
        self.allocations.iter().any(|(a, _)| *a == addr)
    }
}

impl Default for MockPhysMemAlloc {
    fn default() -> Self {
        Self::new()
    }
}

impl PhysMemAlloc for MockPhysMemAlloc {
    fn alloc(&mut self, clicks: usize, flags: AllocFlags) -> Result<PhysAddr, AllocError> {
        if let Some(fail_after) = self.fail_after {
            if self.allocations.len() >= fail_after {
                self.stats.record_failure();
                return Err(AllocError::OutOfMemory);
            }
        }

        if flags.contains(AllocFlags::LOW) && self.next_addr >= 0x1000000 {
            self.stats.record_failure();
            return Err(AllocError::LowMemoryExhausted);
        }

        let addr = PhysAddr::new(self.next_addr);
        let bytes = clicks * super::CLICK_SIZE;
        self.next_addr += bytes as u64;
        self.allocations.push((addr, clicks));
        self.stats.record_alloc(bytes);

        Ok(addr)
    }

    fn free(&mut self, addr: PhysAddr, clicks: usize) {
        let initial_len = self.allocations.len();
        self.allocations.retain(|(a, c)| *a != addr || *c != clicks);

        if self.allocations.len() < initial_len {
            let bytes = clicks * super::CLICK_SIZE;
            self.stats.record_free(bytes);
        }
    }

    fn stats(&self) -> &MemStats {
        &self.stats
    }
}

pub(crate) struct PhysMemGlobalAlloc {
    #[allow(dead_code)]
    inner: RefCell<PhysMemAllocator>,
}

impl PhysMemGlobalAlloc {
    pub(crate) fn new(total_mem: usize) -> Self {
        Self { inner: RefCell::new(PhysMemAllocator::new(total_mem)) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::string::ToString;

    #[test]
    fn test_mock_alloc_free() {
        let mut mock = MockPhysMemAlloc::new();

        let addr = mock.alloc(1, AllocFlags::empty()).expect("alloc failed");
        assert!(addr.is_valid());
        assert_eq!(mock.allocation_count(), 1);

        mock.free(addr, 1);
        assert_eq!(mock.allocation_count(), 0);
    }

    #[test]
    fn test_mock_fail_after() {
        let mut mock = MockPhysMemAlloc::new();
        mock.fail_after(2);

        assert!(mock.alloc(1, AllocFlags::empty()).is_ok());
        assert!(mock.alloc(1, AllocFlags::empty()).is_ok());

        let result = mock.alloc(1, AllocFlags::empty());
        assert!(matches!(result, Err(AllocError::OutOfMemory)));
    }

    #[test]
    fn test_mock_low_memory() {
        let mut mock = MockPhysMemAlloc::new();
        mock.next_addr = 0xFF0000;

        let result = mock.alloc(1, AllocFlags::LOW);
        assert!(result.is_ok());

        mock.next_addr = 0x1000000;

        let result = mock.alloc(1, AllocFlags::LOW);
        assert!(matches!(result, Err(AllocError::LowMemoryExhausted)));
    }

    #[test]
    fn test_trait_object() {
        fn allocate_through_trait(alloc: &mut dyn PhysMemAlloc) -> Result<PhysAddr, AllocError> {
            alloc.alloc(4, AllocFlags::CONTIG)
        }

        let mut mock = MockPhysMemAlloc::new();
        let addr = allocate_through_trait(&mut mock).expect("alloc failed");
        assert!(addr.is_valid());
    }

    #[test]
    fn test_mock_stats() {
        let mut mock = MockPhysMemAlloc::new();
        let initial_allocated = mock.stats().total_allocated();

        let addr = mock.alloc(2, AllocFlags::empty()).expect("alloc failed");
        let after_alloc = mock.stats().total_allocated();
        assert!(after_alloc > initial_allocated);

        mock.free(addr, 2);
        let after_free = mock.stats().total_allocated();
        assert_eq!(after_free, initial_allocated);
    }

    #[test]
    fn test_alloc_error_display() {
        assert_eq!(AllocError::OutOfMemory.to_string(), "out of memory");
        assert_eq!(AllocError::AlignmentFailed.to_string(), "alignment requirement failed");
        assert_eq!(AllocError::ContiguityFailed.to_string(), "contiguity requirement failed");
        assert_eq!(AllocError::LowMemoryExhausted.to_string(), "low memory exhausted");
    }

    #[test]
    fn test_real_allocator_through_trait() {
        let mut allocator = PhysMemAllocator::new(512 * 1024 * 1024);
        let trait_alloc: &mut dyn PhysMemAlloc = &mut allocator;

        let addr = trait_alloc.alloc(1, AllocFlags::empty()).expect("alloc failed");
        assert!(addr.is_valid());

        trait_alloc.free(addr, 1);
    }
}
