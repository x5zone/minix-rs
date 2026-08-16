//! VM global state module.
//!
//! Provides simple global variables like Minix3's glo.h:
//! - `BOOT_INFO`: Boot image array (kernel-provided process info)
//! - `TOTAL_PAGES`: Total physical memory pages
//! - `VM_INSTANCE_COUNT`: Number of active VM instances (for RS restart)

use minix_types::{BootImage, Endpoint, KernelLayout, NR_BOOT_PROCS, AssumeSyncCell};

/// Boot image array - populated by kernel at startup.
/// Corresponds to Minix3's `kernel_boot_info`.
static BOOT_INFO: AssumeSyncCell<[BootImage; NR_BOOT_PROCS]> = 
    AssumeSyncCell::new([BootImage::empty(); NR_BOOT_PROCS]);

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

/// Returns total physical memory pages.
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

/// Finds boot image by endpoint.
pub(crate) fn find_boot_image(endpoint: Endpoint) -> Option<BootImage> {
    // SAFETY: Single-threaded VM; BOOT_INFO is initialized before first read.
    unsafe {
        let boot_info = &*BOOT_INFO.get();
        boot_info.iter().find(|b| b.endpoint == endpoint).copied()
    }
}

/// Sets boot image at index.
/// 
/// # Safety
/// Must only be called during initialization.
pub(crate) unsafe fn set_boot_image(index: usize, image: BootImage) {
    // SAFETY: Must only be called during initialization (documented in # Safety
    // above). Single-threaded VM ensures no concurrent reads of BOOT_INFO.
    // Index bounds check prevents out-of-bounds write.
    unsafe {
        if index < NR_BOOT_PROCS {
            let boot_info = &mut *BOOT_INFO.get();
            boot_info[index] = image;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_boot_image_empty() {
        let img = BootImage::empty();
        assert_eq!(img.name(), "");
        assert_eq!(img.endpoint, Endpoint::NONE);
    }

    #[test]
    fn test_boot_image_name() {
        let mut img = BootImage::empty();
        img.proc_name = *b"kernel\0\0\0\0\0\0\0\0\0\0";
        assert_eq!(img.name(), "kernel");
    }

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
        crate::pagetable::vm_self_map::init_vm_self_pt();
        let mut page_alloc = make_page_alloc(256);
        register_page_alloc(&mut page_alloc);

        let allocator = VmAllocator {
            arena_base: AssumeSyncCell::new(core::ptr::null_mut()),
            cursor: AssumeSyncCell::new(0),
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

use core::alloc::{GlobalAlloc, Layout};
use core::sync::atomic::{AtomicPtr, Ordering};
use crate::alloc_page::VmPageAllocator;
use crate::heap_arena::HeapArena;
use crate::phys_mem::CLICK_SIZE;

/// Global allocator — bump allocator that splits pages in the HeapArena region.
///
/// Preallocates 16 pages (64KB) as an arena, with internal cursor-based splitting.
/// Automatically allocates new arenas when exhausted (old arenas are not immediately reclaimed).
/// Dealloc is a no-op — individual objects are not reclaimed;
/// arena pages remain mapped in HeapArena for the VM process lifetime.
///
/// # Architecture
///
/// ```text
/// Box::new → GlobalAlloc::alloc → VmAllocator::alloc
///     → bump within current arena
///     → arena exhausted? → refill_arena()
///         → HeapArena::grow(ARENA_PAGES, page_alloc)
///             → alloc_phys(1) × N  (physical pages, can be fragmented)
///             → vm_self_mappages()  (map into contiguous VA in HeapArena)
///         → new arena_base = HeapArena VA
/// ```
///
/// The arena VA comes from HeapArena (contiguous), NOT from Direct Map (has holes).
/// Direct Map is used only for physical page management (page table ops, metadata, CoW).
pub(crate) struct VmAllocator {
    arena_base: AssumeSyncCell<*mut u8>,
    cursor: AssumeSyncCell<usize>,
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

impl VmAllocator {
    const ARENA_PAGES: usize = 16;
    const ARENA_BYTES: usize = Self::ARENA_PAGES * CLICK_SIZE;

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
        // SAFETY: Single-threaded VM; no concurrent access to arena_base/cursor.
        unsafe {
            let size = layout.size();
            let align = layout.align();

            // Oversize guard: a request larger than one arena cannot fit after
            // any refill. Without this, `cursor + total > ARENA_BYTES` would
            // refill repeatedly until the whole HeapArena (64MB) is consumed,
            // then return null. Fail fast instead of exhausting the heap.
            if size > Self::ARENA_BYTES {
                return core::ptr::null_mut();
            }

            if !self.ensure_arena() {
                return core::ptr::null_mut();
            }

            // SAFETY: arena_base and cursor are only accessed here (single-threaded).
            let base = *self.arena_base.get();
            let cursor = *self.cursor.get();
            // SAFETY: base is a valid pointer into the VM direct-mapped region;
            // cursor is within ARENA_BYTES bounds (checked below).
            let ptr = base.add(cursor);
            let offset = ptr.align_offset(align);
            // SAFETY: offset is within the arena bounds; total checked below.
            let alloc_start = ptr.add(offset);
            let total = offset + size;

            if cursor + total > Self::ARENA_BYTES {
                if !self.refill_arena() {
                    return core::ptr::null_mut();
                }
                return self.alloc(layout);
            }

            // SAFETY: cursor write is exclusive (single-threaded).
            *self.cursor.get() = cursor + total;
            alloc_start
        }
    }

    unsafe fn dealloc(&self, _ptr: *mut u8, _layout: Layout) {
        // SAFETY: No-op deallocator. Bump allocator does not reclaim individual
        // objects; arena pages remain mapped for the VM process lifetime. This
        // is intentional for a long-lived system service with stable allocations.
    }
}

#[cfg_attr(not(test), global_allocator)]
static GLOBAL: VmAllocator = VmAllocator {
    arena_base: AssumeSyncCell::new(core::ptr::null_mut()),
    cursor: AssumeSyncCell::new(0),
};
