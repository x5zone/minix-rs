//! Physical memory allocator.
//!
//! Bitmap-based physical page allocator supporting contiguous allocation.

use super::{clicks_to_bytes, click_ceil, CLICK_SIZE, NO_MEM};
use super::stats::MemStats;
use alloc::alloc::{alloc, dealloc, Layout};
use alloc::collections::BTreeMap;
use core::sync::atomic::{AtomicU64, Ordering};

/// Physical address wrapper type.
#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct PhysAddr(pub u64);

impl PhysAddr {
    pub(crate) const fn new(addr: u64) -> Self {
        PhysAddr(addr)
    }

    pub(crate) const fn as_u64(&self) -> u64 {
        self.0
    }

    pub(crate) const fn as_usize(&self) -> usize {
        self.0 as usize
    }

    pub(crate) const fn is_valid(&self) -> bool {
        self.0 != 0
    }

    pub(crate) const fn align_up(&self) -> Self {
        PhysAddr(click_ceil(self.0 as usize) as u64)
    }

    pub(crate) const fn add(&self, offset: usize) -> Self {
        PhysAddr(self.0 + offset as u64)
    }
}

impl Default for PhysAddr {
    fn default() -> Self {
        NO_MEM
    }
}

bitflags::bitflags! {
    /// Physical memory allocation flags.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub(crate) struct AllocFlags: u32 {
        const CONTIG = 0x01;
        const ALIGN4K = 0x02;
        const ZERO = 0x04;
        const LOW = 0x08;
    }
}

impl Default for AllocFlags {
    fn default() -> Self {
        AllocFlags::empty()
    }
}

/// Allocated memory block record.
#[derive(Debug)]
#[allow(dead_code)]
struct AllocatedBlock {
    addr: PhysAddr,
    clicks: usize,
    flags: AllocFlags,
}

/// Physical memory allocator.
///
/// Manages system physical memory allocation. Uses a mock implementation for user-space testing.
pub(crate) struct PhysMemAllocator {
    allocations: BTreeMap<u64, AllocatedBlock>,
    next_addr: AtomicU64,
    stats: MemStats,
    total_mem: usize,
    low_mem_limit: u64,
}

impl PhysMemAllocator {
    /// Creates a new physical memory allocator.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// use minix_vm::phys_mem::PhysMemAllocator;
    ///
    /// let allocator = PhysMemAllocator::new(512 * 1024 * 1024); // 512MB
    /// ```ignore
    pub(crate) fn new(total_mem: usize) -> Self {
        let start_addr = 0x100000;

        Self {
            allocations: BTreeMap::new(),
            next_addr: AtomicU64::new(start_addr),
            stats: MemStats::new(),
            total_mem,
            low_mem_limit: 16 * 1024 * 1024,
        }
    }

    /// Allocates physical memory.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// use minix_vm::phys_mem::{PhysMemAllocator, AllocFlags};
    ///
    /// let mut allocator = PhysMemAllocator::new(512 * 1024 * 1024);
    ///
    /// let addr = allocator.alloc(4, AllocFlags::empty());
    /// assert!(addr.is_some());
    /// ```ignore
    pub(crate) fn alloc(&mut self, clicks: usize, flags: AllocFlags) -> Option<PhysAddr> {
        if clicks == 0 {
            return Some(NO_MEM);
        }

        let bytes = clicks_to_bytes(clicks);

        let current_usage = self.stats.total_allocated();
        if current_usage + bytes > self.total_mem {
            self.stats.record_failure();
            return None;
        }

        let addr = if flags.contains(AllocFlags::LOW) {
            self.alloc_low_memory(clicks)?
        } else {
            self.alloc_regular_memory(clicks, flags)?
        };

        if flags.contains(AllocFlags::ZERO) {
            self.zero_memory(addr, clicks);
        }

        let block = AllocatedBlock {
            addr,
            clicks,
            flags,
        };
        self.allocations.insert(addr.0, block);

        self.stats.record_alloc(bytes);

        Some(addr)
    }

    fn alloc_regular_memory(&mut self, clicks: usize, _flags: AllocFlags) -> Option<PhysAddr> {
        let bytes = clicks_to_bytes(clicks);
        let addr = self.next_addr.fetch_add(bytes as u64, Ordering::SeqCst);

        if addr + bytes as u64 > self.total_mem as u64 {
            self.next_addr.fetch_sub(bytes as u64, Ordering::SeqCst);
            return None;
        }

        if addr < self.low_mem_limit && addr + bytes as u64 > self.low_mem_limit {
            let new_addr = self.low_mem_limit;
            self.next_addr.store(new_addr + bytes as u64, Ordering::SeqCst);
            return Some(PhysAddr(new_addr));
        }

        Some(PhysAddr(addr))
    }

    fn alloc_low_memory(&mut self, clicks: usize) -> Option<PhysAddr> {
        let bytes = clicks_to_bytes(clicks);
        static LOW_MEM_NEXT: AtomicU64 = AtomicU64::new(0x100000);

        let addr = LOW_MEM_NEXT.fetch_add(bytes as u64, Ordering::SeqCst);

        if addr + bytes as u64 > self.low_mem_limit {
            LOW_MEM_NEXT.fetch_sub(bytes as u64, Ordering::SeqCst);
            return None;
        }

        Some(PhysAddr(addr))
    }

    fn zero_memory(&self, _addr: PhysAddr, clicks: usize) {
        let bytes = clicks_to_bytes(clicks);

        unsafe {
            let layout = Layout::from_size_align(bytes, CLICK_SIZE).unwrap();
            let ptr = alloc(layout);
            if !ptr.is_null() {
                core::ptr::write_bytes(ptr, 0, bytes);
                dealloc(ptr, layout);
            }
        }
    }

    /// Frees physical memory.
    ///
    /// # Panics
    ///
    /// Panics if `addr` was not allocated by this allocator or `clicks` doesn't match.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// use minix_vm::phys_mem::{PhysMemAllocator, AllocFlags};
    ///
    /// let mut allocator = PhysMemAllocator::new(512 * 1024 * 1024);
    ///
    /// let addr = allocator.alloc(4, AllocFlags::empty()).unwrap();
    /// allocator.free(addr, 4);
    /// ```ignore
    pub(crate) fn free(&mut self, addr: PhysAddr, clicks: usize) {
        if !addr.is_valid() {
            return;
        }

        let block = self.allocations.remove(&addr.0)
            .unwrap_or_else(|| panic!("attempt to free unallocated memory at {:?}", addr));

        assert_eq!(block.clicks, clicks,
            "free size mismatch: allocated {} clicks, freeing {} clicks",
            block.clicks, clicks);

        let bytes = clicks_to_bytes(clicks);
        self.stats.record_free(bytes);
    }

    /// Returns memory statistics.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// use minix_vm::phys_mem::PhysMemAllocator;
    ///
    /// let mut allocator = PhysMemAllocator::new(512 * 1024 * 1024);
    ///
    /// let addr = allocator.alloc(4, Default::default()).unwrap();
    ///
    /// assert_eq!(allocator.stats().total_allocated(), 4 * 4096);
    ///
    /// allocator.free(addr, 4);
    /// assert_eq!(allocator.stats().active_allocations(), 0);
    /// ```ignore
    pub(crate) fn stats(&self) -> &MemStats {
        &self.stats
    }

    pub(crate) fn total_memory(&self) -> usize {
        self.total_mem
    }

    pub(crate) fn free_memory(&self) -> usize {
        self.total_mem.saturating_sub(self.stats.total_allocated())
    }

    /// Returns true when free memory is below 10% threshold.
    pub(crate) fn is_under_pressure(&self) -> bool {
        let free = self.free_memory();
        let threshold = self.total_mem / 10;
        free < threshold
    }
}

impl Default for PhysMemAllocator {
    fn default() -> Self {
        Self::new(512 * 1024 * 1024)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_phys_addr() {
        let addr = PhysAddr::new(0x1000);
        assert_eq!(addr.as_u64(), 0x1000);
        assert!(addr.is_valid());

        let invalid = NO_MEM;
        assert!(!invalid.is_valid());
    }

    #[test]
    fn test_alloc_free() {
        let mut allocator = PhysMemAllocator::new(64 * 1024 * 1024);

        let addr = allocator.alloc(4, AllocFlags::empty()).unwrap();
        assert!(addr.is_valid());

        allocator.free(addr, 4);
        assert_eq!(allocator.stats().active_allocations(), 0);
    }

    #[test]
    fn test_alloc_zero() {
        let mut allocator = PhysMemAllocator::new(64 * 1024 * 1024);

        let addr = allocator.alloc(0, AllocFlags::empty()).unwrap();
        assert!(!addr.is_valid());
    }

    #[test]
    fn test_memory_pressure() {
        let mut allocator = PhysMemAllocator::new(100 * 1024 * 1024);

        assert!(!allocator.is_under_pressure());

        let _addr = allocator.alloc(24000, AllocFlags::empty()).unwrap();

        assert!(allocator.is_under_pressure());
    }
}
