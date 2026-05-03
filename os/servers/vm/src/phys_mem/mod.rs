//! Physical memory allocation module.
//!
//! Provides system-level physical memory allocation, a core VM server function.
//!
//! # Single-threaded Assumption
//!
//! This module assumes single-threaded execution. All data structures are
//! `!Sync` by default. The VM server runs as a single-threaded process.
//!
//! # Click Unit System
//!
//! Minix3 uses "click" as the basic memory allocation unit:
//! - 1 click = 4096 bytes (4KB) = 1 page
//! - CLICK_SHIFT = 12 (for bit operations)
//! - CLICK_SIZE == VM_PAGE_SIZE (asserted in Minix3 alloc.c)
//!
//! # Three Implementations
//!
//! All three allocators implement the `PhysAllocator` trait:
//!
//! | Allocator | Time | Precise | Anti-fragmentation | Complexity |
//! |-----------|------|---------|---------------------|------------|
//! | BitmapAllocator | O(n) | Yes | No | Low |
//! | SegmentTreeAllocator | O(log n) | Yes | No | High |
//! | BuddyAllocator | O(log n) | No (2^n) | Yes | Medium |
//!
//! Default: `BitmapAllocator` (matches Minix3 behavior).
//!
//! # TODO: Reserved Page Queue
//!
//! Minix3 maintains a reserved page queue for critical kernel allocations
//! (e.g., page tables during fork). This is not yet implemented.
//! See Minix3 `alloc.c:alloc_mem()` — the `reserved_queue` mechanism.
//!
//! # Early Heap
//!
//! For 64-bit systems, we cannot use static BSS arrays like Minix3.
//! Instead, we use an early heap (bump allocator) to allocate metadata
//! before the permanent heap is available. See `early_heap.rs` and
//! `heap-bootstrap.md` for details.

pub(crate) mod early_heap;
pub(crate) mod types;
pub(crate) mod alloc_trait;
pub(crate) mod bitmap_alloc;
pub(crate) mod segment_tree_alloc;
pub(crate) mod buddy_alloc;
pub(crate) mod stats;
#[cfg(test)]
pub(crate) mod allocator_tests;

pub(crate) use early_heap::EarlyHeap;
pub(crate) use alloc_trait::{PhysAllocator, PhysAllocatorStats, PhysMemStats};
pub(crate) use bitmap_alloc::BitmapAllocator;
pub(crate) use buddy_alloc::BuddyAllocator;
pub(crate) use segment_tree_alloc::SegmentTreeAllocator;
pub(crate) use stats::MemStats;
pub(crate) use types::{AllocError, PageAllocFlags, PhysBytes};

#[cfg(feature = "buddy_alloc")]
pub(crate) type DefaultAllocator = BuddyAllocator;

#[cfg(all(feature = "segment_tree_alloc", not(feature = "buddy_alloc")))]
pub(crate) type DefaultAllocator = SegmentTreeAllocator;

#[cfg(all(feature = "bitmap_alloc", not(any(feature = "segment_tree_alloc", feature = "buddy_alloc"))))]
pub(crate) type DefaultAllocator = BitmapAllocator;

#[cfg(not(any(feature = "bitmap_alloc", feature = "segment_tree_alloc", feature = "buddy_alloc")))]
pub(crate) type DefaultAllocator = BitmapAllocator;

/// Physical allocator type selector.
/// Used to compute metadata size before initializing the early heap.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PhysAllocType {
    Bitmap,
    Buddy,
    SegmentTree,
}

impl PhysAllocType {
    pub fn metadata_size(&self, total_pages: usize) -> usize {
        let exact = self.metadata_size_exact(total_pages);
        (exact + CLICK_SIZE - 1) & !(CLICK_SIZE - 1)
    }

    pub fn metadata_size_exact(&self, total_pages: usize) -> usize {
        match self {
            PhysAllocType::Bitmap => {
                let bitmap_chunks = (total_pages + 63) / 64;
                bitmap_chunks * core::mem::size_of::<u64>()
                    + 10000 * core::mem::size_of::<usize>()
            }
            PhysAllocType::Buddy => {
                let max_order = if total_pages == 0 {
                    0
                } else {
                    total_pages.next_power_of_two().trailing_zeros() as usize
                };
                (max_order + 1) * core::mem::size_of::<u32>()
                    + total_pages * core::mem::size_of::<u32>()
                    + total_pages * core::mem::size_of::<u8>()
            }
            PhysAllocType::SegmentTree => {
                let offset = if total_pages == 0 {
                    1
                } else {
                    total_pages.next_power_of_two()
                };
                let tree_size = if total_pages > 0 { 2 * offset } else { 2 };
                tree_size * core::mem::size_of::<(usize, usize, usize, usize)>()
            }
        }
    }
}

/// Click size: 4096 bytes (4KB).
/// In Minix3, CLICK_SIZE == VM_PAGE_SIZE is asserted (alloc.c:298).
pub(crate) const CLICK_SIZE: usize = 4096;

/// Click shift: log2(CLICK_SIZE) = 12.
pub(crate) const CLICK_SHIFT: usize = 12;

/// Converts bytes to clicks (round up).
#[inline]
pub(crate) const fn bytes_to_clicks(bytes: usize) -> usize {
    (bytes + CLICK_SIZE - 1) >> CLICK_SHIFT
}

/// Converts clicks to bytes.
#[inline]
pub(crate) const fn clicks_to_bytes(clicks: usize) -> usize {
    clicks << CLICK_SHIFT
}

/// Rounds down to click boundary.
#[inline]
pub(crate) const fn click_floor(addr: usize) -> usize {
    (addr >> CLICK_SHIFT) << CLICK_SHIFT
}

/// Rounds up to click boundary.
#[inline]
pub(crate) const fn click_ceil(addr: usize) -> usize {
    ((addr + CLICK_SIZE - 1) >> CLICK_SHIFT) << CLICK_SHIFT
}

/// Boot memory region descriptor, received from kernel via `sys_getkinfo()`.
/// Corresponds to Minix3 `struct memory` (memlist.h).
#[derive(Debug, Clone, Copy)]
pub(crate) struct BootMemRegion {
    pub base: usize,
    pub size: usize,
}

impl BootMemRegion {
    pub(crate) fn validate(&self) {
        assert!(
            self.base % CLICK_SIZE == 0,
            "BootMemRegion base must be page-aligned, got {:#x}",
            self.base
        );
        assert!(
            self.size % CLICK_SIZE == 0,
            "BootMemRegion size must be page-aligned, got {:#x}",
            self.size
        );
    }
}

pub(super) fn compute_memory_bounds(regions: &[BootMemRegion]) -> (usize, usize, usize) {
    let mut mem_low = usize::MAX;
    let mut mem_high = 0;

    for region in regions {
        if region.size == 0 {
            continue;
        }
        let from = region.base;
        let to = region.base + region.size - 1;
        if from < mem_low {
            mem_low = from;
        }
        if to > mem_high {
            mem_high = to;
        }
    }

    if mem_low == usize::MAX {
        mem_low = 0;
    }

    let total_pages = if mem_high > 0 { mem_high / CLICK_SIZE + 1 } else { 0 };
    (total_pages, mem_low, mem_high)
}

pub(super) fn is_low_mem_flag(flags: PageAllocFlags) -> bool {
    flags.intersects(PageAllocFlags::LOWER1MB | PageAllocFlags::LOWER16MB)
}

pub(super) fn oom_error(flags: PageAllocFlags) -> AllocError {
    if is_low_mem_flag(flags) {
        AllocError::LowMemoryExhausted
    } else {
        AllocError::OutOfMemory
    }
}

#[cfg(test)]
mod metadata_size_tests {
    use super::*;

    #[test]
    fn test_bitmap_metadata_size() {
        let total_pages = 1024;
        let size = PhysAllocType::Bitmap.metadata_size_exact(total_pages);
        let bitmap_chunks = (total_pages + 63) / 64;
        let expected = bitmap_chunks * 8 + 10000 * core::mem::size_of::<usize>();
        assert_eq!(size, expected);
    }

    #[test]
    fn test_buddy_metadata_size() {
        let total_pages = 1024;
        let size = PhysAllocType::Buddy.metadata_size_exact(total_pages);
        let max_order = total_pages.next_power_of_two().trailing_zeros() as usize;
        let expected = (max_order + 1) * 4 + total_pages * 4 + total_pages * 1;
        assert_eq!(size, expected);
    }

    #[test]
    fn test_metadata_size_page_aligned() {
        let total_pages = 1024;
        for alloc_type in [PhysAllocType::Bitmap, PhysAllocType::Buddy, PhysAllocType::SegmentTree] {
            let aligned = alloc_type.metadata_size(total_pages);
            assert_eq!(aligned % CLICK_SIZE, 0, "{:?}: metadata_size not page-aligned", alloc_type);
            assert!(aligned >= alloc_type.metadata_size_exact(total_pages));
        }
    }

    #[test]
    fn test_metadata_size_zero_pages() {
        assert_eq!(PhysAllocType::Buddy.metadata_size_exact(0), 4);
        assert_eq!(PhysAllocType::SegmentTree.metadata_size_exact(0), 2 * core::mem::size_of::<(usize, usize, usize, usize)>());
        let bitmap_zero = PhysAllocType::Bitmap.metadata_size_exact(0);
        assert_eq!(bitmap_zero, 10000 * core::mem::size_of::<usize>());
    }
}
