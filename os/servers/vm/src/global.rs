//! VM global state module.
//!
//! Provides simple global variables like Minix3's glo.h:
//! - `TOTAL_PAGES`: Total physical memory pages
//! - `VM_INSTANCE_COUNT`: Number of active VM instances (for RS restart)
//!
//! Boot images (`kernel_boot_info.boot_procs[]`) are NOT global here —
//! `VmServer.boot_procs` (from `BootParams`) is the single source of truth
//! (V9-P3-1, todo: BOOT_INFO/find_boot_image/set_boot_image were dead code
//! with no production callers).

use minix_types::{KernelLayout, AssumeSyncCell};

/// Total physical memory pages.
/// Corresponds to Minix3's `total_pages`.
static TOTAL_PAGES: AssumeSyncCell<usize> = AssumeSyncCell::new(0);

/// Number of active VM instances.
/// Corresponds to Minix3's `num_vm_instances`.
/// Uses AssumeSyncCell (not AtomicU32) because VM is single-threaded.
static VM_INSTANCE_COUNT: AssumeSyncCell<u32> = AssumeSyncCell::new(0);

/// Kernel memory layout — populated once during VM server initialization.
///
/// Replaces the hardcoded constants previously guarded by the
/// `hardcoded_kernel_layout` feature (hardcoded kernel layout fix). The kernel mapping is
/// identical in every user process page table, so a single global
/// suffices. Initialized via `set_kernel_layout()` before any
/// `init_page_table()` call.
///
/// `Option<KernelLayout>` is used instead of `KernelLayout::default()`
/// because there is no meaningful default for kernel layout — a zero
/// layout would produce incorrect mappings. `init_page_table()`
/// panics if the layout has not been set, failing fast rather than
/// silently mapping memory incorrectly.
static KERNEL_LAYOUT: AssumeSyncCell<Option<KernelLayout>> = AssumeSyncCell::new(None);

/// Initializes global state with total pages.
/// 
/// # Safety
/// Must be called exactly once during VM server initialization.
pub(crate) unsafe fn init(total_pages: usize) {
    // SAFETY: Must be called exactly once during VM startup (documented in
    // # Safety section above). Single-threaded VM ensures no concurrent reads
    // or writes to TOTAL_PAGES at this point.
    unsafe {
        *TOTAL_PAGES.get() = total_pages;
    }
}

// V10-P2-1: `total_pages()` has no production callers (the server reads
// `page_alloc.total_pages()` directly); it exists for tests that assert
// the boot accounting after `global::init()`.
#[cfg(test)]
pub(crate) fn total_pages() -> usize {
    // SAFETY: Single-threaded VM; TOTAL_PAGES is initialized before first read.
    unsafe { *TOTAL_PAGES.get() }
}

/// Adds pages to the global physical page total.
///
/// C: `mem_add_total_pages()` (alloc.c:281-284) — called from
/// `init_vm()` for boot modules and the kernel's own allocations
/// (main.c:485-495). The kernel's free list does not include boot-time
/// modules, so the total must be calibrated after `mem_init()`.
///
/// # Safety
/// Must be called during VM startup (before any concurrent reader of
/// `TOTAL_PAGES` exists). Single-threaded VM guarantees this.
pub(crate) unsafe fn add_total_pages(pages: usize) {
    // SAFETY: Single-threaded VM; called during startup, before any
    // concurrent read of TOTAL_PAGES (documented above).
    unsafe {
        *TOTAL_PAGES.get() += pages;
    }
}

/// Sets the kernel memory layout.
///
/// Must be called exactly once during VM server initialization, after
/// the boot image / multiboot2 / stivale2 headers have been parsed and
/// before any `init_page_table()` call. Subsequent calls overwrite the
/// previous value — this is intentional to support re-initialization
/// during testing, but production code must call this exactly once.
///
/// # Safety
/// Must be called before any concurrent reader of `KERNEL_LAYOUT` exists.
/// In practice this means before the first `init_page_table()` call,
/// which happens during process table setup — well after `VmServer::init()`.
pub(crate) unsafe fn set_kernel_layout(layout: KernelLayout) {
    // SAFETY: Single-threaded VM; no concurrent access to KERNEL_LAYOUT at
    // the point this is called (during VmServer::init, before any process
    // page table is created).
    unsafe {
        *KERNEL_LAYOUT.get() = Some(layout);
    }
}

/// Returns the kernel memory layout.
///
/// # Panics
/// Panics if `set_kernel_layout()` has not been called. This is intentional:
/// a missing kernel layout would produce incorrect page table mappings, so
/// we fail fast rather than silently mapping memory wrong.
pub(crate) fn kernel_layout() -> KernelLayout {
    // SAFETY: Single-threaded VM; KERNEL_LAYOUT is initialized via
    // set_kernel_layout() before the first init_page_table() call.
    // The panic on None is a deliberate fail-fast guard.
    unsafe {
        (*KERNEL_LAYOUT.get())
            .expect("kernel_layout() called before set_kernel_layout() — VmServer::init() was not run")
    }
}

/// Increments VM instance count.
pub(crate) fn inc_vm_instance() {
    // SAFETY: Single-threaded VM; no concurrent access to VM_INSTANCE_COUNT.
    unsafe {
        *VM_INSTANCE_COUNT.get() += 1;
    }
}

/// Decrements VM instance count.
pub(crate) fn dec_vm_instance() {
    // SAFETY: Single-threaded VM; no concurrent access to VM_INSTANCE_COUNT.
    unsafe {
        *VM_INSTANCE_COUNT.get() -= 1;
    }
}

/// Returns current VM instance count.
pub(crate) fn vm_instance_count() -> u32 {
    // SAFETY: Single-threaded VM; VM_INSTANCE_COUNT is always valid to read.
    unsafe { *VM_INSTANCE_COUNT.get() }
}

#[cfg(test)]
mod tests {
    use super::*;

    // V9-P3-1 (todo): BOOT_INFO/find_boot_image/set_boot_image removed —
    // boot images live in `VmServer.boot_procs` (BootParams), the single
    // source of truth. BootImage::empty()/name() are covered by
    // minix-types tests instead.

    #[test]
    fn test_vm_instance_count() {
        unsafe {
            *VM_INSTANCE_COUNT.get() = 0;
        }
        
        assert_eq!(vm_instance_count(), 0);
        
        inc_vm_instance();
        assert_eq!(vm_instance_count(), 1);

        inc_vm_instance();
        assert_eq!(vm_instance_count(), 2);

        dec_vm_instance();
        assert_eq!(vm_instance_count(), 1);
        
        unsafe {
            *VM_INSTANCE_COUNT.get() = 0;
        }
    }

    #[test]
    fn test_kernel_layout_set_and_get() {
        // Reset to None to ensure a clean state.
        unsafe {
            *KERNEL_LAYOUT.get() = None;
        }

        // Before set: kernel_layout() should panic.
        let result = std::panic::catch_unwind(|| kernel_layout());
        assert!(result.is_err(), "kernel_layout() must panic before set_kernel_layout()");

        // Set a layout.
        let layout = KernelLayout::new(
            0xFFFF_FFFF_8000_0000,
            0x100_0000,
            8,
            8,
            0xFFFF_8000_0000_0000,
            4,
        );
        unsafe {
            set_kernel_layout(layout);
        }

        // After set: kernel_layout() returns the stored value.
        let got = kernel_layout();
        assert_eq!(got, layout);

        // Cleanup: reset to None so other tests are not affected.
        unsafe {
            *KERNEL_LAYOUT.get() = None;
        }
    }

    #[test]
    fn test_kernel_layout_overwrite() {
        unsafe {
            *KERNEL_LAYOUT.get() = None;
        }

        let a = KernelLayout::new(1, 2, 3, 4, 5, 6);
        let b = KernelLayout::new(10, 20, 30, 40, 50, 60);

        unsafe {
            set_kernel_layout(a);
        }
        assert_eq!(kernel_layout(), a);

        // Overwriting is allowed (for test re-initialization).
        unsafe {
            set_kernel_layout(b);
        }
        assert_eq!(kernel_layout(), b);

        unsafe {
            *KERNEL_LAYOUT.get() = None;
        }
    }

    /// Build a page allocator over a fresh bitmap with `available_pages`
    /// free pages (physical addresses start at 0).
    fn make_page_alloc(available_pages: usize) -> VmPageAllocator {
        let bitmap = crate::phys_mem::BitmapAllocator::new_for_test(available_pages);
        VmPageAllocator::new(crate::phys_mem::PhysAlloc::Bitmap(bitmap))
    }

    /// VmAllocator with `arena_base` pointing into a leaked, page-aligned
    /// buffer (bypasses refill for pure bump-within-arena tests).
    fn bump_allocator_with_fake_arena(cursor: usize) -> (VmAllocator, usize) {
        let buf: alloc::boxed::Box<[u8]> = alloc::vec![0u8; CLICK_SIZE].into_boxed_slice();
        let leaked = alloc::boxed::Box::leak(buf);
        let base = (leaked.as_ptr() as usize + CLICK_SIZE - 1) & !(CLICK_SIZE - 1);
        let allocator = VmAllocator {
            arena_base: AssumeSyncCell::new(base as *mut u8),
            cursor: AssumeSyncCell::new(cursor),
            free_head: AssumeSyncCell::new(core::ptr::null_mut()),
        };
        (allocator, base)
    }

    #[test]
    fn test_bump_alignment_and_no_overlap() {
        let (allocator, base) = bump_allocator_with_fake_arena(0);
        unsafe {
            let p1 = allocator.alloc(Layout::from_size_align(1, 1).unwrap());
            assert!(!p1.is_null());
            let p2 = allocator.alloc(Layout::from_size_align(16, 8).unwrap());
            assert!(!p2.is_null());
            let p3 = allocator.alloc(Layout::from_size_align(32, 32).unwrap());
            assert!(!p3.is_null());

            assert_eq!(p2 as usize % 8, 0, "8-byte alignment");
            assert_eq!(p3 as usize % 32, 0, "32-byte alignment");
            assert!(p2 > p1 && p3 > p2, "bump must not overlap");
            // All pointers stay within the fake arena buffer.
            assert!(p1 as usize >= base && (p3 as usize) + 32 <= base + CLICK_SIZE);
        }
    }

    #[test]
    fn test_bump_oversize_returns_null() {
        // size > ARENA_BYTES must fail fast without touching PAGE_ALLOC_PTR
        // or growing the HeapArena (2026-08-15 guard).
        let (allocator, _) = bump_allocator_with_fake_arena(0);
        unsafe {
            let p = allocator.alloc(Layout::from_size_align(VmAllocator::ARENA_BYTES + 1, 1).unwrap());
            assert!(p.is_null());
        }
    }

    #[test]
    fn test_bump_refill_failure_returns_null() {
        // Current arena has 8 bytes left; a 16-byte request cannot fit and
        // refill fails because no page allocator is registered.
        unregister_page_alloc();
        let (allocator, _) = bump_allocator_with_fake_arena(VmAllocator::ARENA_BYTES - 8);
        unsafe {
            let p = allocator.alloc(Layout::from_size_align(16, 1).unwrap());
            assert!(p.is_null());
        }
    }

    #[test]
    fn test_bump_refill_via_heap_arena() {
        // Full chain: GLOBAL-style allocator with null arena_base →
        // ensure_arena → refill_arena → HEAP_ARENA.grow(16) → bump.
        crate::pagetable::vm_self_map::reset_vm_self_pt_for_test();
        crate::pagetable::vm_self_map::init_vm_self_pt(minix_types::PhysBytes(0x900_000));
        let mut page_alloc = make_page_alloc(256);
        register_page_alloc(&mut page_alloc);

        let allocator = VmAllocator {
            arena_base: AssumeSyncCell::new(core::ptr::null_mut()),
            cursor: AssumeSyncCell::new(0),
            free_head: AssumeSyncCell::new(core::ptr::null_mut()),
        };
        unsafe {
            let p = allocator.alloc(Layout::from_size_align(64, 8).unwrap());
            assert!(!p.is_null(), "refill via HeapArena must succeed");
            let va = p as u64;
            assert!(va >= crate::direct_map::VM_HEAP_BASE);
            assert!(va < crate::direct_map::VM_HEAP_BASE + crate::direct_map::VM_HEAP_SIZE);
        }

        unregister_page_alloc();
        crate::pagetable::vm_self_map::reset_vm_self_pt_for_test();
    }

    #[test]
    fn test_free_list_reuses_freed_block() {
        // P1-1: dealloc must return the block to the free list so a later
        // alloc of the same size reuses it (bump-only reused nothing).
        let (allocator, base) = bump_allocator_with_fake_arena(0);
        unsafe {
            let p1 = allocator.alloc(Layout::from_size_align(64, 8).unwrap());
            assert!(!p1.is_null());
            allocator.dealloc(p1, Layout::from_size_align(64, 8).unwrap());
            let p2 = allocator.alloc(Layout::from_size_align(64, 8).unwrap());
            assert_eq!(p2, p1, "freed block must be reused, not re-bumped");
            assert!(p2 as usize >= base && (p2 as usize) < base + CLICK_SIZE);
            allocator.dealloc(p2, Layout::from_size_align(64, 8).unwrap());
        }
    }

    #[test]
    fn test_free_list_alignment_variants() {
        let (allocator, base) = bump_allocator_with_fake_arena(0);
        unsafe {
            for align in [1usize, 8, 16, 32, 64, 512, 4096] {
                let layout = Layout::from_size_align(48, align).unwrap();
                let p = allocator.alloc(layout);
                assert!(!p.is_null(), "alloc failed for align {align}");
                assert_eq!(
                    p as usize % align,
                    0,
                    "payload must honor align {align}"
                );
                assert!(
                    p as usize >= base && (p as usize) < base + CLICK_SIZE,
                    "payload must stay inside the fake arena"
                );
                allocator.dealloc(p, layout);
            }
        }
    }

    #[test]
    fn test_free_list_split_and_coalesce() {
        // Split: a freed 2048-byte block reused by a smaller request leaves a
        // remainder block on the free list; coalesce: freeing two adjacent
        // 2048-byte blocks merges them into one block that serves a 4096-byte
        // request from the merged range (not from a fresh bump).
        let (allocator, _) = bump_allocator_with_fake_arena(0);
        let l2048 = Layout::from_size_align(2048, 8).unwrap();
        let l4096 = Layout::from_size_align(4096, 8).unwrap();
        unsafe {
            let a = allocator.alloc(l2048);
            let b = allocator.alloc(l2048);
            assert!(!a.is_null() && !b.is_null());

            // Free both; adjacent blocks must coalesce into a single block.
            allocator.dealloc(a, l2048);
            allocator.dealloc(b, l2048);
            assert_eq!(allocator.free_block_count(), 1, "adjacent frees must coalesce");

            // The merged block (a + b, 4096 bytes) must serve the 4096 request.
            let p = allocator.alloc(l4096);
            assert!(!p.is_null(), "coalesced block must serve the 4096 request");
            assert_eq!(
                p as usize,
                a as usize,
                "alloc from the merged block starts at its head (not a fresh bump)"
            );

            // The merged block is consumed exactly, so the free list is empty
            // again.
            assert_eq!(allocator.free_block_count(), 0);
            allocator.dealloc(p, l4096);
        }
    }

    #[test]
    fn test_free_list_reuse_cycles_without_refill() {
        // 50 alloc/free cycles of a 200-byte object must never exhaust the
        // fake arena: bump-only would run out after ~17 allocations
        // (4096 / 232), free-list recycling keeps the cursor parked at the
        // first allocation.
        let (allocator, base) = bump_allocator_with_fake_arena(0);
        let layout = Layout::from_size_align(200, 8).unwrap();
        unsafe {
            for _ in 0..50 {
                let p = allocator.alloc(layout);
                assert!(!p.is_null(), "cycle alloc must succeed");
                assert!(
                    p as usize >= base && (p as usize) < base + CLICK_SIZE,
                    "recycling must stay within the first arena"
                );
                allocator.dealloc(p, layout);
            }
        }
    }

    #[test]
    fn test_register_page_alloc_idempotent() {
        let mut page_alloc = make_page_alloc(8);
        register_page_alloc(&mut page_alloc);
        // Same pointer re-registration must not panic (BSS→heap defensive path).
        register_page_alloc(&mut page_alloc);
        unregister_page_alloc();
    }

    #[test]
    fn test_register_page_alloc_overwrite_panics() {
        let mut a = make_page_alloc(8);
        let mut b = make_page_alloc(8);
        register_page_alloc(&mut a);
        let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            register_page_alloc(&mut b);
        }));
        assert!(r.is_err(), "overwriting a different allocator must panic");
        unregister_page_alloc();
    }
}


/// Global pointer to the page allocator, set during VmServer initialization.
///
/// # Safety invariant
///
/// The `VmPageAllocator` pointed to by this static **must outlive** the
/// `VmAllocator` (the `GLOBAL` static below). This is guaranteed by the
/// VM lifecycle: `register_page_alloc()` is called in `VmServer::new()`
/// and `unregister_page_alloc()` in `VmServer::drop()`. Since VmServer
/// owns the VmPageAllocator, the allocator lives as long as VmServer,
/// and VmServer lives as long as the VM process — which is the entire
/// lifetime of the `GLOBAL` allocator.
static PAGE_ALLOC_PTR: AtomicPtr<VmPageAllocator> = AtomicPtr::new(core::ptr::null_mut());

static HEAP_ARENA: HeapArena = HeapArena::new();

pub(crate) fn register_page_alloc(alloc: &mut VmPageAllocator) {
    let new_ptr = alloc as *mut VmPageAllocator;
    // Use compare_exchange to detect unintended overwrites:
    // - null → new_ptr: first registration (normal)
    // - old_ptr == new_ptr: idempotent re-registration (safe, no-op)
    // - old_ptr != new_ptr: different allocator being registered — likely a bug
    //
    // The BSS → heap transition re-registers the same allocator object
    // (VmServer moves, but the allocator field address may change).
    // If the pointer genuinely changes, that's a real double-init bug
    // we want to catch rather than silently mask.
    //
    // SAFETY: Single-threaded VM model ensures no TOCTOU between
    // compare_exchange and any concurrent access.
    match PAGE_ALLOC_PTR.compare_exchange(
        core::ptr::null_mut(),
        new_ptr,
        Ordering::SeqCst,
        Ordering::SeqCst,
    ) {
        Ok(_) => {} // First registration
        Err(old_ptr) if old_ptr == new_ptr => {
            // Idempotent re-registration with the same pointer — safe, no-op.
        }
        Err(old_ptr) => {
            panic!(
                "register_page_alloc: overwriting different allocator (old={:?}, new={:?}). \
                 If this is a legitimate BSS→heap transition, the pointer should match.",
                old_ptr, new_ptr
            );
        }
    }
}

/// Unregister the global page allocator pointer.
/// Called when VmServer is dropped to prevent dangling pointer.
pub(crate) fn unregister_page_alloc() {
    PAGE_ALLOC_PTR.store(core::ptr::null_mut(), Ordering::SeqCst);
}

/// Mutable access to the registered VM page allocator.
///
/// Used by allocator hooks that cannot receive `&mut VmPageAllocator` through
/// their signature — notably `alloc_page::vm_pt_alloc()`, which is registered
/// as a `fn()` with `minix_arch::pt_alloc::register()`.
///
/// # Panics
///
/// Panics if `register_page_alloc()` has not been called yet.
pub(crate) fn page_alloc_mut() -> &'static mut VmPageAllocator {
    let ptr = PAGE_ALLOC_PTR.load(Ordering::SeqCst);
    assert!(
        !ptr.is_null(),
        "page_alloc_mut: page allocator not registered — call register_page_alloc() first"
    );
    // SAFETY: 1. Single-threaded VM event loop: no concurrent access to
    // PAGE_ALLOC_PTR or the allocator it points to. 2. Outlive constraint:
    // the pointer is set during VmServer::new() and cleared during
    // VmServer::drop(); callers never dereference after it is nulled
    // (checked above). 3. No aliasing `&mut` exists: callers of this
    // accessor must not simultaneously hold a borrow of the same allocator
    // (e.g. via a `VmServer` field); the allocator hooks use it at points
    // where the event loop holds no such borrow.
    unsafe { &mut *ptr }
}

pub(crate) fn heap_arena_grow(
    pages: usize,
    page_alloc: &mut VmPageAllocator,
) -> Result<u64, crate::heap_arena::HeapArenaError> {
    HEAP_ARENA.grow(pages, page_alloc)
}

use core::alloc::{GlobalAlloc, Layout};
use core::sync::atomic::{AtomicPtr, Ordering};
use crate::alloc_page::VmPageAllocator;
use crate::heap_arena::HeapArena;
use crate::phys_mem::CLICK_SIZE;

/// Block alignment and header layout.
///
/// Every block (free or allocated) is a multiple of [`BLOCK_ALIGN`] bytes and
/// starts at a [`BLOCK_ALIGN`]-aligned address, so block headers are always
/// 16-byte aligned. While free, a block stores a [`FreeBlock`] header at its
/// start; while allocated, an [`AllocHeader`] is stored immediately before
/// the payload (`payload − 16`), so `dealloc` recovers the block start and
/// size from the payload pointer alone.
const BLOCK_ALIGN: usize = 16;
/// Minimum allocation size / free-block header size (two `usize` fields).
const HEADER_SIZE: usize = 16;
/// Minimum payload of a free block worth keeping: header + one payload slot.
const MIN_FREE_PAYLOAD: usize = HEADER_SIZE;

/// Free-block header, stored inside the block while it is on the free list.
///
/// An allocated block carries no header: the returned payload pointer IS the
/// block start, and `dealloc` reconstructs the header from the (contractually
/// identical) `Layout` — the same trick as `linked_list_allocator`.
#[repr(C)]
struct FreeBlock {
    /// Total block size in bytes, multiple of [`BLOCK_ALIGN`].
    size: usize,
    /// Next free block by ascending address (coalescing-friendly order).
    next: *mut FreeBlock,
}

impl FreeBlock {
    const fn new(size: usize, next: *mut FreeBlock) -> Self {
        Self { size, next }
    }

    fn start(&self) -> usize {
        self as *const FreeBlock as usize
    }

    fn end(&self) -> usize {
        self.start() + self.size
    }
}

/// Global allocator — first-fit free-list allocator over HeapArena arenas.
///
/// `[ARCH: A-3 v2]` (2026-08-16): v1 (bump-only, `dealloc` no-op) let heap
/// memory grow monotonically until server shutdown — long-running VM heap only
/// grew (todo P1-1). v2 adds free-list reclamation: released heap objects
/// (region `Vec`s, page-cache entries, transient messages) are reused instead
/// of leaked, so the heap stays bounded by peak live usage plus fragmentation.
/// Minix3's slab allocator (slaballoc.c) already recycled objects via its
/// free-list + empty-slab return (slaballoc.c:449 `vm_freepages`) — v2 closes
/// the recycling gap that the v1 design had accepted (09-slab-allocator.md
/// §1.8/§3.1).
///
/// # Architecture
///
/// ```text
/// Box::new → GlobalAlloc::alloc → VmAllocator::alloc
///     → free-list first-fit (address-sorted; split + coalesce)
///         → found → split remainder, return aligned payload
///     → else bump within current arena
///         → arena exhausted? → free_tail() + refill_arena()
///             → HeapArena::grow(ARENA_PAGES, page_alloc)
///                 → alloc_phys(1) × N  (physical pages, can be fragmented)
///                 → vm_self_mappages()  (map into contiguous VA in HeapArena)
///             → new arena_base = HeapArena VA
///         → retry bump (the oversize guard guarantees a fresh arena fits)
///
/// GlobalAlloc::dealloc → rebuild FreeBlock from the (identical) Layout
///     → insert into free list (address-sorted, coalesce adjacent blocks)
/// ```
///
/// The arena VA comes from HeapArena (contiguous), NOT from Direct Map (has holes).
/// Direct Map is used only for physical page management (page table ops, metadata, CoW).
///
/// # Fragmentation note
///
/// Coalescing merges adjacent free blocks, but an arena whose free blocks are
/// interleaved with live allocations cannot shrink — arena pages stay mapped
/// until server shutdown (same as v1). Arena shrink via `HeapArena::shrink`
/// remains a documented future step (09-slab-allocator.md §5.3).
pub(crate) struct VmAllocator {
    arena_base: AssumeSyncCell<*mut u8>,
    cursor: AssumeSyncCell<usize>,
    free_head: AssumeSyncCell<*mut FreeBlock>,
}

impl VmAllocator {
    const ARENA_PAGES: usize = 16;
    const ARENA_BYTES: usize = Self::ARENA_PAGES * CLICK_SIZE;

    /// Rounds `size` up to a [`BLOCK_ALIGN`] multiple.
    const fn round_up(size: usize) -> usize {
        (size + BLOCK_ALIGN - 1) & !(BLOCK_ALIGN - 1)
    }

    /// Rounds `x` up to an `align` multiple (`align` must be a power of two).
    const fn align_up(x: usize, align: usize) -> usize {
        debug_assert!(align.is_power_of_two());
        (x + align - 1) & !(align - 1)
    }

    /// Inserts `block` into the address-sorted free list, coalescing with any
    /// adjacent free block so freed neighbours merge back into one block.
    ///
    /// # Safety
    /// `block` must be a valid, 16-aligned free block not already in the list.
    unsafe fn free_list_insert(&self, block: *mut FreeBlock) {
        // SAFETY: callers pass a valid free block; the list is only touched
        // here and in try_alloc_from_free_list, and the VM event loop is
        // single-threaded.
        unsafe {
            let mut prev: *mut FreeBlock = core::ptr::null_mut();
            let mut cur = *self.free_head.get();
            while !cur.is_null() && (*cur).start() < (*block).start() {
                prev = cur;
                cur = (*cur).next;
            }

            // Merge with the successor when the block ends exactly where the
            // successor begins.
            if !cur.is_null() && (*block).end() == (*cur).start() {
                (*block).size += (*cur).size;
                (*block).next = (*cur).next;
            } else {
                (*block).next = cur;
            }

            // Merge into the predecessor when it ends exactly where the block
            // begins; otherwise link after it (or make it the new head).
            if !prev.is_null() && (*prev).end() == (*block).start() {
                (*prev).size += (*block).size;
                (*prev).next = (*block).next;
            } else if prev.is_null() {
                *self.free_head.get() = block;
            } else {
                (*prev).next = block;
            }
        }
    }

    /// First-fit search of the free list for a block that can serve `payload`
    /// aligned bytes. On success removes the block from the list, splits
    /// padding/remainder off as new free blocks, and returns the payload
    /// pointer (the block start).
    ///
    /// # Safety
    /// `align` must be a power of two; `payload` must be non-zero and within
    /// one arena (caller's oversize guard).
    unsafe fn try_alloc_from_free_list(&self, payload: usize, align: usize) -> Option<*mut u8> {
        // SAFETY: free-list blocks are valid 16-aligned free blocks; the list
        // is only touched here and in free_list_insert, and the VM event loop
        // is single-threaded.
        unsafe {
            let mut prev: *mut FreeBlock = core::ptr::null_mut();
            let mut cur = *self.free_head.get();
            while !cur.is_null() {
                let block_start = (*cur).start();
                let block_end = block_start + (*cur).size;
                let payload_addr = Self::align_up(block_start, align);
                let payload_end = payload_addr + payload;
                if payload_end <= block_end {
                    // Detach the block from the list.
                    let next = (*cur).next;
                    if prev.is_null() {
                        *self.free_head.get() = next;
                    } else {
                        (*prev).next = next;
                    }

                    // Padding before the payload may form a new free block.
                    let pad = payload_addr - block_start;
                    if pad >= HEADER_SIZE + MIN_FREE_PAYLOAD {
                        let pad_block = block_start as *mut FreeBlock;
                        (*pad_block) = FreeBlock::new(pad, core::ptr::null_mut());
                        self.free_list_insert(pad_block);
                    }

                    // Remainder after the payload may form a new free block.
                    let rest = block_end - payload_end;
                    if rest >= HEADER_SIZE + MIN_FREE_PAYLOAD {
                        let rest_block = payload_end as *mut FreeBlock;
                        (*rest_block) = FreeBlock::new(rest, core::ptr::null_mut());
                        self.free_list_insert(rest_block);
                    }

                    return Some(payload_addr as *mut u8);
                }
                prev = cur;
                cur = (*cur).next;
            }
            None
        }
    }

    /// Bump-cuts `payload` aligned bytes from the current arena. Alignment
    /// padding ahead of the payload is returned to the free list instead of
    /// being wasted. No memory is written — bump blocks carry no header
    /// (`dealloc` rebuilds it from the `Layout`).
    ///
    /// Returns `None` if the current arena cannot fit the request (caller
    /// returns the arena tail to the free list, refills, and retries; the
    /// oversize guard guarantees a fresh arena fits).
    ///
    /// # Safety
    /// `align` must be a power of two and the request must fit in one arena.
    unsafe fn alloc_bump(&self, payload: usize, align: usize) -> Option<*mut u8> {
        // SAFETY: arena_base/cursor are only mutated here and in refill_arena
        // (single-threaded VM event loop).
        unsafe {
            let base = *self.arena_base.get() as usize;
            let cursor = *self.cursor.get();
            let raw = base + cursor;
            let payload_addr = Self::align_up(raw, align);
            let payload_end = payload_addr + payload;
            // Bounds check before any state change: the fake test arenas are
            // smaller than ARENA_BYTES and must never be touched out of bounds.
            if payload_end > base + Self::ARENA_BYTES {
                return None;
            }

            *self.cursor.get() = payload_end - base;

            // Alignment padding ahead of the payload forms a reusable block.
            let gap = payload_addr - raw;
            if gap >= HEADER_SIZE + MIN_FREE_PAYLOAD {
                let gap_block = raw as *mut FreeBlock;
                (*gap_block) = FreeBlock::new(gap, core::ptr::null_mut());
                self.free_list_insert(gap_block);
            }
            Some(payload_addr as *mut u8)
        }
    }

    /// Returns the unallocated tail of the current arena to the free list.
    ///
    /// Called when bump can no longer fit a request: the tail
    /// `[arena_base + cursor, arena_base + ARENA_BYTES)` is still mapped and
    /// must not be abandoned when the allocator switches to a fresh arena.
    ///
    /// # Safety
    /// Must be called before `refill_arena()` moves `arena_base`.
    unsafe fn free_tail(&self) {
        // SAFETY: single-threaded VM event loop; no other free-list access.
        unsafe {
            let base = *self.arena_base.get() as usize;
            let cursor = *self.cursor.get();
            let tail = Self::ARENA_BYTES - cursor;
            if tail >= HEADER_SIZE + MIN_FREE_PAYLOAD {
                let tail_block = (base + cursor) as *mut FreeBlock;
                (*tail_block) = FreeBlock::new(tail, core::ptr::null_mut());
                self.free_list_insert(tail_block);
            }
        }
    }

    fn refill_arena(&self) -> bool {
        let alloc_ptr = PAGE_ALLOC_PTR.load(Ordering::SeqCst);
        if alloc_ptr.is_null() {
            return false;
        }
        // SAFETY:
        // 1. Single-threaded VM event loop: no concurrent access to PAGE_ALLOC_PTR.
        // 2. Outlive constraint: PAGE_ALLOC_PTR is set by `register_page_alloc()`
        //    during VmServer::new() and only cleared by `unregister_page_alloc()`
        //    during VmServer::drop. Since VmServer owns the VmPageAllocator field,
        //    the allocator is guaranteed to outlive all arena refills — arena
        //    refills only happen during GlobalAlloc::alloc calls, which only
        //    occur while VmServer is alive (the VM event loop drives all allocation).
        // 3. The pointer is never dereferenced after unregister_page_alloc()
        //    sets it to null (checked above).
        let alloc = unsafe { &mut *alloc_ptr };
        match HEAP_ARENA.grow(Self::ARENA_PAGES, alloc) {
            Ok(va) => {
                // SAFETY: arena_base and cursor are UnsafeCell fields of VmAllocator.
                // VmAllocator is a static (GLOBAL), and the single-threaded VM event
                // loop ensures no concurrent access.
                unsafe { *self.arena_base.get() = va as *mut u8; }
                unsafe { *self.cursor.get() = 0; }
                true
            }
            Err(_) => false,
        }
    }

    fn ensure_arena(&self) -> bool {
        // SAFETY: arena_base is an UnsafeCell in a static; single-threaded VM
        // ensures no concurrent access. Only reading to check for null.
        if unsafe { (*self.arena_base.get()).is_null() } {
            return self.refill_arena();
        }
        true
    }
}

unsafe impl GlobalAlloc for VmAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: all writes go through the free-list / bump helpers above,
        // which maintain the block invariants (single-threaded VM event loop).
        unsafe {
            let payload = Self::round_up(layout.size()).max(BLOCK_ALIGN);
            let align = layout.align();

            // Oversize guard: a request larger than one arena cannot be served
            // by any single block. Without this, bump would refill repeatedly
            // until the whole HeapArena (64MB) is consumed, then return null.
            // Alignment padding is included so a fresh arena always fits and
            // the bump retry loop below cannot spin forever.
            if payload + align - 1 > Self::ARENA_BYTES {
                return core::ptr::null_mut();
            }

            if let Some(p) = self.try_alloc_from_free_list(payload, align) {
                return p;
            }
            if !self.ensure_arena() {
                return core::ptr::null_mut();
            }
            loop {
                if let Some(p) = self.try_alloc_from_free_list(payload, align) {
                    return p;
                }
                if let Some(p) = self.alloc_bump(payload, align) {
                    return p;
                }
                // The current arena's tail is still mapped — keep it usable
                // before switching to a fresh arena.
                self.free_tail();
                if !self.refill_arena() {
                    return core::ptr::null_mut();
                }
            }
        }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: `ptr` came from a prior `alloc` of this allocator, and
        // `layout` is the same layout used for that `alloc` (GlobalAlloc
        // contract). The block start IS the payload pointer, so the free
        // header is rebuilt in place. No other free-list access is concurrent
        // (single-threaded VM event loop).
        unsafe {
            debug_assert!(!ptr.is_null());
            let payload = Self::round_up(layout.size()).max(BLOCK_ALIGN);
            let block = ptr as *mut FreeBlock;
            (*block) = FreeBlock::new(payload, core::ptr::null_mut());
            self.free_list_insert(block);
        }
    }
}

#[cfg(test)]
impl VmAllocator {
    /// Number of free blocks currently on the free list (test observability).
    fn free_block_count(&self) -> usize {
        // SAFETY: single-threaded tests; the list is only mutated by the
        // allocator methods under test.
        unsafe {
            let mut n = 0;
            let mut cur = *self.free_head.get();
            while !cur.is_null() {
                n += 1;
                cur = (*cur).next;
            }
            n
        }
    }
}

#[cfg_attr(not(test), global_allocator)]
static GLOBAL: VmAllocator = VmAllocator {
    arena_base: AssumeSyncCell::new(core::ptr::null_mut()),
    cursor: AssumeSyncCell::new(0),
    free_head: AssumeSyncCell::new(core::ptr::null_mut()),
};
