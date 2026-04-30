//! Physical memory allocation module.
//!
//! Provides system-level physical memory allocation, a core VM server function.
//!
//! # Click Unit System
//!
//! Minix3 uses "click" as the basic memory allocation unit:
//! - 1 click = 4096 bytes (4KB)
//! - CLICK_SHIFT = 12 (for bit operations)
//!
//! # Main Functions
//!
//! - `alloc_mem`: Allocate physical memory by clicks
//! - `free_mem`: Free physical memory
//! - `memstats`: Query memory statistics

pub(crate) mod alloc_trait;
pub(crate) mod allocator;
pub(crate) mod frame;
pub(crate) mod stats;
pub(crate) mod reserved;
#[cfg(test)]
pub(crate) mod tests;

pub(crate) use alloc_trait::{AllocError, MockPhysMemAlloc, PhysMemAlloc, PhysMemGlobalAlloc};
pub(crate) use allocator::{PhysMemAllocator, AllocFlags, PhysAddr};
pub(crate) use frame::PhysFrame;
pub(crate) use stats::{MemStats, MemStatsReporter};
pub(crate) use reserved::{ReservedQueueManager, QueueInfo};

/// Click size: 4096 bytes (4KB).
pub(crate) const CLICK_SIZE: usize = 4096;

/// Click shift: log2(CLICK_SIZE) = 12.
pub(crate) const CLICK_SHIFT: usize = 12;

/// Invalid physical address marker.
pub(crate) const NO_MEM: PhysAddr = PhysAddr(0);

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
