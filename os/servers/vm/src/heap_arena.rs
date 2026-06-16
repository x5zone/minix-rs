//! HeapArena: contiguous VA region for the Rust heap.
//!
//! # Problem
//!
//! Direct Map provides stable VA for any physical page (`VA = PA + BASE`),
//! but does NOT provide VA continuity — physical holes become VA holes.
//! A bump allocator needs a contiguous VA arena; it cannot skip holes.
//!
//! # Solution
//!
//! HeapArena reserves a contiguous VA range (`VM_HEAP_BASE .. VM_HEAP_BASE + VM_HEAP_SIZE`)
//! and maps non-contiguous physical pages into it one-by-one via `vm_self_mappages()`.
//! The physical pages can be fragmented, but the VA is always contiguous.
//!
//! # Three-VA model
//!
//! A physical page used as heap has up to three VAs:
//! 1. `KERNEL_DIRECT_MAP_BASE + phys` (kernel Direct Map, Ring 0)
//! 2. `VM_DIRECT_MAP_BASE + phys` (VM Direct Map, Ring 3)
//! 3. HeapArena VA (only for heap pages, Ring 3)
//!
//! # No recursion
//!
//! When HeapArena::grow() writes PTEs via vm_self_mappages(), the page table
//! pages are accessed via Direct Map (stable VA), not through HeapArena.
//! Therefore there is no recursive dependency.
//!
//! # No GlobalAlloc dependency
//!
//! HeapArena contains only three `u64` fields wrapped in `AssumeSyncCell`,
//! constructible in BSS/static context. It does not allocate through the
//! global allocator.

use minix_types::{AssumeSyncCell, PhysBytes};
use minix_types::VirBytes as VB;
use crate::alloc_page::VmPageAllocator;
use crate::direct_map::{VM_HEAP_BASE, VM_HEAP_SIZE, VM_HEAP_LIMIT};
use crate::pagetable::{PageFlags, PageTableError, vm_self_mappages, vm_self_unmap};
use crate::phys_mem::{PageAllocFlags, AlignedPhysBytes, CLICK_SIZE};

const PAGE_SIZE: u64 = CLICK_SIZE as u64;

pub(crate) struct HeapArena {
    base: u64,
    limit: AssumeSyncCell<u64>,
    top: u64,
}

impl HeapArena {
    pub const fn new() -> Self {
        Self {
            base: VM_HEAP_BASE,
            limit: AssumeSyncCell::new(VM_HEAP_BASE),
            top: VM_HEAP_LIMIT,
        }
    }

    pub fn base(&self) -> u64 {
        self.base
    }

    pub fn limit(&self) -> u64 {
        // SAFETY: Single-threaded VM; limit is only accessed here and in set_limit.
        unsafe { *self.limit.get() }
    }

    pub fn top(&self) -> u64 {
        self.top
    }

    pub fn available_va(&self) -> u64 {
        self.top - self.limit()
    }

    pub fn mapped_bytes(&self) -> u64 {
        self.limit() - self.base
    }

    /// Grow the arena by mapping `pages` physical pages into the contiguous VA range.
    ///
    /// Physical pages are allocated one-by-one (can be fragmented), but their VAs
    /// are contiguous within the HeapArena region. Each page is mapped via
    /// `vm_self_mappages()` with user-space read-write permissions.
    ///
    /// On failure, any partially mapped pages are rolled back (unmapped and freed).
    ///
    /// Returns the VA of the first newly mapped page on success.
    pub fn grow(
        &self,
        pages: usize,
        page_alloc: &mut VmPageAllocator,
    ) -> Result<u64, HeapArenaError> {
        if pages == 0 {
            return Err(HeapArenaError::ZeroPages);
        }

        let old_limit = self.limit();
        let new_limit = old_limit.checked_add(pages as u64 * PAGE_SIZE)
            .ok_or(HeapArenaError::ArithmeticOverflow)?;

        if new_limit > self.top {
            return Err(HeapArenaError::Exhausted {
                requested: pages as u64 * PAGE_SIZE,
                available: self.top - old_limit,
            });
        }

        for i in 0..pages {
            let va = VB(old_limit + i as u64 * PAGE_SIZE);
            let phys = page_alloc.alloc_phys(1, PageAllocFlags::empty())
                .map_err(|_| HeapArenaError::PhysicalAllocFailed)?;

            match vm_self_mappages(va, PhysBytes(phys.as_u64()), PageFlags::read_write()) {
                Ok(()) => {}
                Err(e) => {
                    for j in 0..i {
                        let undo_va = VB(old_limit + j as u64 * PAGE_SIZE);
                        if let Ok(undo_phys) = vm_self_unmap(undo_va) {
                            page_alloc.free_page(AlignedPhysBytes::new(undo_phys.0));
                        }
                    }
                    page_alloc.free_page(phys);
                    return Err(HeapArenaError::MapFailed(e));
                }
            }
        }

        // SAFETY: Single-threaded VM; limit write is exclusive.
        unsafe { *self.limit.get() = new_limit; }
        Ok(old_limit)
    }

    /// Shrink the arena by unmapping `pages` pages from the end.
    ///
    /// Unmaps the pages from VM's page table and frees the physical pages
    /// back to the page allocator. Pages are unmapped from highest VA
    /// downward to maintain contiguity.
    pub fn shrink(
        &self,
        pages: usize,
        page_alloc: &mut VmPageAllocator,
    ) -> Result<(), HeapArenaError> {
        if pages == 0 {
            return Err(HeapArenaError::ZeroPages);
        }

        let current_limit = self.limit();
        let mapped_pages = (current_limit - self.base) / PAGE_SIZE;

        if pages > mapped_pages as usize {
            return Err(HeapArenaError::Underflow {
                requested: pages,
                mapped: mapped_pages as usize,
            });
        }

        let new_limit = current_limit - pages as u64 * PAGE_SIZE;

        for i in 0..pages {
            let va = VB(new_limit + i as u64 * PAGE_SIZE);
            match vm_self_unmap(va) {
                Ok(phys) => {
                    page_alloc.free_page(AlignedPhysBytes::new(phys.0));
                }
                Err(_) => {}
            }
        }

        // SAFETY: Single-threaded VM; limit write is exclusive.
        unsafe { *self.limit.get() = new_limit; }
        Ok(())
    }
}

#[derive(Debug)]
pub(crate) enum HeapArenaError {
    ZeroPages,
    Exhausted { requested: u64, available: u64 },
    Underflow { requested: usize, mapped: usize },
    PhysicalAllocFailed,
    MapFailed(PageTableError),
    ArithmeticOverflow,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_heap_arena_constants() {
        let arena = HeapArena::new();
        assert_eq!(arena.base(), VM_HEAP_BASE);
        assert_eq!(arena.top(), VM_HEAP_BASE + VM_HEAP_SIZE);
        assert_eq!(arena.limit(), VM_HEAP_BASE);
        assert_eq!(arena.mapped_bytes(), 0);
        assert_eq!(arena.available_va(), VM_HEAP_SIZE);
    }
}
