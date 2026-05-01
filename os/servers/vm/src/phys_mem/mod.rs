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
//! All three allocators implement the `PhysMemAlloc` trait:
//!
//! | Allocator | Time | Precise | Anti-fragmentation | Complexity |
//! |-----------|------|---------|---------------------|------------|
//! | BitmapAllocator | O(n) | Yes | No | Low |
//! | SegmentTreeAllocator | O(log n) | Yes | No | High |
//! | BuddyAllocator | O(log n) | No (2^n) | Yes | Medium |
//!
//! Default: `BitmapAllocator` (matches Minix3 behavior).

pub(crate) mod types;
pub(crate) mod alloc_trait;
pub(crate) mod bitmap_alloc;
pub(crate) mod segment_tree_alloc;
pub(crate) mod buddy_alloc;
pub(crate) mod stats;
#[cfg(test)]
pub(crate) mod allocator_tests;

pub(crate) use alloc_trait::PhysMemAlloc;
pub(crate) use bitmap_alloc::BitmapAllocator;
pub(crate) use buddy_alloc::BuddyAllocator;
pub(crate) use segment_tree_alloc::SegmentTreeAllocator;
pub(crate) use stats::MemStats;
pub(crate) use types::{AllocError, AllocParams, PageAllocFlags, PhysAddr};

#[cfg(feature = "bitmap-alloc")]
pub(crate) type DefaultAllocator = BitmapAllocator;

#[cfg(feature = "segment-tree-alloc")]
pub(crate) type DefaultAllocator = SegmentTreeAllocator;

#[cfg(feature = "buddy-alloc")]
pub(crate) type DefaultAllocator = BuddyAllocator;

#[cfg(not(any(feature = "bitmap-alloc", feature = "segment-tree-alloc", feature = "buddy-alloc")))]
pub(crate) type DefaultAllocator = BitmapAllocator;

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
        debug_assert!(
            self.base % CLICK_SIZE == 0,
            "BootMemRegion base must be page-aligned, got {:#x}",
            self.base
        );
        debug_assert!(
            self.size % CLICK_SIZE == 0,
            "BootMemRegion size must be page-aligned, got {:#x}",
            self.size
        );
    }
}
