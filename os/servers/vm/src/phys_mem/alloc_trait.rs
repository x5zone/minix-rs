//! Physical memory allocator trait — shared interface for all backends.
//!
//! Defines the contract every concrete allocator must satisfy:
//!
//! - `alloc_mem(clicks, flags) -> Result<AlignedPhysBytes, AllocError>`
//! - `free_mem(base, clicks)`
//! - `total_count() -> usize`  (number of total pages the allocator manages)
//! - `reserve_pages(base_page, count)`  (mark a range as in-use, for boot-time reservation)
//! - `available_regions(callback)`  (walk free regions; used by VFS for fd-table setup)
//!
//! Plus a default-implemented `memstats()` that returns a `PhysMemStats`
//! snapshot for the dispatcher to forward to `VM_INFO` / `VM_GETRUSAGE`
//! callers.
//!
//! # Why a trait (and not `dyn`)?
//!
//! The three backends are selected at boot time via Cargo features
//! (only one is compiled in). The `PhysAlloc` enum in `mod.rs` is the
//! single static-dispatch point, so we don't need `dyn PhysAllocator`.
//! This avoids vtable indirection on the hot allocation path.
//!
//! [ARCH: A-5] — single C bitmap → three-backend strategy pattern
//! (`PhysAllocator` trait), see plan.md §4 A-5 and 05-physical-memory.md §3.1.
//!
use super::types::{AllocError, PageAllocFlags, AlignedPhysBytes};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PhysMemStats {
    pub(crate) free_nodes: usize,
    pub(crate) free_pages: usize,
    pub(crate) largest_free: usize,
}

pub(crate) trait PhysAllocator {
    fn alloc_mem(&mut self, clicks: usize, flags: PageAllocFlags) -> Result<AlignedPhysBytes, AllocError>;
    fn free_mem(&mut self, base: AlignedPhysBytes, clicks: usize);
    fn total_count(&self) -> usize;
    fn reserve_pages(&mut self, base_page: usize, count: usize);

    fn available_regions(&self, callback: &mut dyn FnMut(usize, usize));
}

