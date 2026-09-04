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
#[cfg(not(test))]
use crate::direct_map::{VM_HEAP_BASE, VM_HEAP_LIMIT};
#[cfg(test)]
use crate::direct_map::{VM_HEAP_BASE, VM_HEAP_LIMIT, VM_HEAP_SIZE};
use crate::pagetable::{PageFlags, PageTableError, vm_self_mappages, vm_self_unmap};
use crate::phys_mem::{PageAllocFlags, AlignedPhysBytes, CLICK_SIZE};

const PAGE_SIZE: u64 = CLICK_SIZE as u64;

pub(crate) struct HeapArena {
    // V10-P2-1: `base` is read only by the test-only accessors and the
    // DEFERRED `shrink` path; production uses `limit`/`top`.
    #[cfg_attr(not(test), allow(dead_code))]
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

    // V10-P2-1: layout accessors are test-only today.
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn base(&self) -> u64 {
        self.base
    }

    pub fn limit(&self) -> u64 {
        // SAFETY: Single-threaded VM; limit is only accessed here and in set_limit.
        unsafe { *self.limit.get() }
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn top(&self) -> u64 {
        self.top
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn available_va(&self) -> u64 {
        self.top - self.limit()
    }

    #[cfg_attr(not(test), allow(dead_code))]
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
    ///
    /// V10-P2-1 (DEFERRED): no production caller yet — the heap-shrink
    /// path (process exit / heap release) is not wired. Revisit with the
    /// 24-page-cache reclaim work.
    #[allow(dead_code)]
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
            if let Ok(phys) = vm_self_unmap(va) {
                page_alloc.free_page(AlignedPhysBytes::new(phys.0));
            }
        }

        // SAFETY: Single-threaded VM; limit write is exclusive.
        unsafe { *self.limit.get() = new_limit; }
        Ok(())
    }
}

#[derive(Debug)]
// V10-P2-1: error payloads are diagnostic-only (callers propagate or
// `.expect()` without inspecting them), so dead fields/variants are
// allowed until a caller inspects them.
#[allow(dead_code)]
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
    use crate::pagetable::vm_self_map::{init_vm_self_pt, reset_vm_self_pt_for_test, vm_self_query};
    use crate::phys_mem::{BitmapAllocator, PhysAlloc};

    /// Initialize VM's own page table (MockPaging in test builds) and run
    /// the closure, then reset the storage so the next test can re-init.
    /// Tests execute single-threaded (RUST_TEST_THREADS=1, .cargo/config.toml).
    fn with_vm_self_pt<F: FnOnce()>(f: F) {
        reset_vm_self_pt_for_test();
        init_vm_self_pt(minix_types::PhysBytes(0x900_000));
        f();
        reset_vm_self_pt_for_test();
    }

    /// Build a page allocator over a fresh bitmap with `available_pages`
    /// free pages. Physical addresses start at 0.
    fn make_page_alloc(available_pages: usize) -> VmPageAllocator {
        let bitmap = BitmapAllocator::new_for_test(available_pages);
        VmPageAllocator::new(PhysAlloc::Bitmap(bitmap))
    }

    #[test]
    fn test_heap_arena_constants() {
        let arena = HeapArena::new();
        assert_eq!(arena.base(), VM_HEAP_BASE);
        assert_eq!(arena.top(), VM_HEAP_BASE + VM_HEAP_SIZE);
        assert_eq!(arena.limit(), VM_HEAP_BASE);
        assert_eq!(arena.mapped_bytes(), 0);
        assert_eq!(arena.available_va(), VM_HEAP_SIZE);
    }

    #[test]
    fn test_grow_advances_limit_and_maps() {
        with_vm_self_pt(|| {
            let mut alloc = make_page_alloc(64);
            let arena = HeapArena::new();

            let va = arena.grow(2, &mut alloc).expect("grow 2 pages");
            assert_eq!(va, VM_HEAP_BASE);
            assert_eq!(arena.limit(), VM_HEAP_BASE + 2 * PAGE_SIZE);
            assert_eq!(arena.mapped_bytes(), 2 * PAGE_SIZE);
            assert_eq!(arena.available_va(), VM_HEAP_SIZE - 2 * PAGE_SIZE);

            // Both VAs are mapped with user read-write permissions.
            for i in 0..2u64 {
                let (phys, flags) = vm_self_query(VB(VM_HEAP_BASE + i * PAGE_SIZE))
                    .expect("grow must map the page");
                assert!(flags.contains(crate::pagetable::PageFlags::WRITABLE));
                assert!(phys.0 % CLICK_SIZE as u64 == 0, "phys must be page-aligned");
            }

            // Grow again: limit advances from the previous position.
            let va2 = arena.grow(1, &mut alloc).expect("grow 1 more page");
            assert_eq!(va2, VM_HEAP_BASE + 2 * PAGE_SIZE);
            assert_eq!(arena.mapped_bytes(), 3 * PAGE_SIZE);
        });
    }

    #[test]
    fn test_grow_zero_pages_is_error() {
        with_vm_self_pt(|| {
            let mut alloc = make_page_alloc(8);
            let arena = HeapArena::new();
            assert!(matches!(
                arena.grow(0, &mut alloc),
                Err(HeapArenaError::ZeroPages)
            ));
            assert_eq!(arena.limit(), VM_HEAP_BASE);
        });
    }

    #[test]
    fn test_grow_exhausted_reports_remaining() {
        with_vm_self_pt(|| {
            let mut alloc = make_page_alloc(8);
            let arena = HeapArena::new();
            let pages = (VM_HEAP_SIZE / PAGE_SIZE) as usize + 1;
            match arena.grow(pages, &mut alloc) {
                Err(HeapArenaError::Exhausted { requested, available }) => {
                    assert_eq!(requested, pages as u64 * PAGE_SIZE);
                    assert_eq!(available, VM_HEAP_SIZE);
                }
                other => panic!("expected Exhausted, got {other:?}"),
            }
            assert_eq!(arena.limit(), VM_HEAP_BASE);
        });
    }

    #[test]
    fn test_grow_rolls_back_on_map_failure() {
        with_vm_self_pt(|| {
            let mut alloc = make_page_alloc(64);
            let arena = HeapArena::new();

            // Pre-map the second target VA so grow(2) fails on page 1
            // (MockPaging returns AlreadyMapped). grow must roll back page 0
            // (unmap + free) and leave the limit unchanged.
            let va1 = VB(VM_HEAP_BASE + PAGE_SIZE);
            let pre_phys = alloc.alloc_phys(1, PageAllocFlags::empty()).unwrap();
            crate::pagetable::vm_self_mappages(va1, PhysBytes(pre_phys.as_u64()), PageFlags::read_write())
                .expect("pre-map second page");

            match arena.grow(2, &mut alloc) {
                Err(HeapArenaError::MapFailed(_)) => {}
                other => panic!("expected MapFailed, got {other:?}"),
            }
            assert_eq!(arena.limit(), VM_HEAP_BASE, "limit must not advance on failure");
            assert!(
                vm_self_query(VB(VM_HEAP_BASE)).is_none(),
                "rolled-back page must be unmapped"
            );

            // Cleanup: unmap the pre-mapped page.
            let _ = crate::pagetable::vm_self_unmap(va1);
        });
    }

    #[test]
    fn test_shrink_unmaps_and_frees_pages() {
        with_vm_self_pt(|| {
            let mut alloc = make_page_alloc(64);
            let arena = HeapArena::new();

            arena.grow(3, &mut alloc).expect("grow 3 pages");
            let phys_before = arena.mapped_bytes();

            arena.shrink(2, &mut alloc).expect("shrink 2 pages");
            assert_eq!(arena.limit(), VM_HEAP_BASE + PAGE_SIZE);
            assert_eq!(arena.mapped_bytes(), phys_before - 2 * PAGE_SIZE);
            // Top two VAs are unmapped.
            assert!(vm_self_query(VB(VM_HEAP_BASE + PAGE_SIZE)).is_none());
            assert!(vm_self_query(VB(VM_HEAP_BASE + 2 * PAGE_SIZE)).is_none());
            // Bottom page still mapped.
            assert!(vm_self_query(VB(VM_HEAP_BASE)).is_some());
        });
    }

    #[test]
    fn test_shrink_underflow_is_error() {
        with_vm_self_pt(|| {
            let mut alloc = make_page_alloc(8);
            let arena = HeapArena::new();
            assert!(matches!(
                arena.shrink(1, &mut alloc),
                Err(HeapArenaError::Underflow { requested: 1, mapped: 0 })
            ));
            assert!(matches!(
                arena.shrink(0, &mut alloc),
                Err(HeapArenaError::ZeroPages)
            ));
        });
    }
}
