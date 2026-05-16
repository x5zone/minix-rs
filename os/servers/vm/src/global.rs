//! VM global state module.
//!
//! Provides simple global variables like Minix3's glo.h:
//! - `BOOT_INFO`: Boot image array (kernel-provided process info)
//! - `TOTAL_PAGES`: Total physical memory pages
//! - `VM_INSTANCE_COUNT`: Number of active VM instances (for RS restart)

use minix_types::{BootImage, Endpoint, NR_BOOT_PROCS, AssumeSyncCell};

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

/// Initializes global state with total pages.
/// 
/// # Safety
/// Must be called exactly once during VM server initialization.
pub(crate) unsafe fn init(total_pages: usize) {
    unsafe {
        *TOTAL_PAGES.get() = total_pages;
    }
}

/// Returns total physical memory pages.
pub(crate) fn total_pages() -> usize {
    unsafe { *TOTAL_PAGES.get() }
}

/// Increments VM instance count.
pub(crate) fn inc_vm_instance() {
    unsafe {
        *VM_INSTANCE_COUNT.get() += 1;
    }
}

/// Decrements VM instance count.
pub(crate) fn dec_vm_instance() {
    unsafe {
        *VM_INSTANCE_COUNT.get() -= 1;
    }
}

/// Returns current VM instance count.
pub(crate) fn vm_instance_count() -> u32 {
    unsafe { *VM_INSTANCE_COUNT.get() }
}

/// Finds boot image by endpoint.
pub(crate) fn find_boot_image(endpoint: Endpoint) -> Option<BootImage> {
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
    if index < NR_BOOT_PROCS {
        let boot_info = &mut *BOOT_INFO.get();
        boot_info[index] = image;
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

static PAGE_ALLOC_PTR: AtomicPtr<VmPageAllocator> = AtomicPtr::new(core::ptr::null_mut());

static HEAP_ARENA: HeapArena = HeapArena::new();

pub(crate) fn register_page_alloc(alloc: &mut VmPageAllocator) {
    PAGE_ALLOC_PTR.store(alloc as *mut VmPageAllocator, Ordering::SeqCst);
}

impl VmAllocator {
    const ARENA_PAGES: usize = 16;
    const ARENA_BYTES: usize = Self::ARENA_PAGES * CLICK_SIZE;

    fn refill_arena(&self) -> bool {
        let alloc_ptr = PAGE_ALLOC_PTR.load(Ordering::SeqCst);
        if alloc_ptr.is_null() {
            return false;
        }
        let alloc = unsafe { &mut *alloc_ptr };
        match HEAP_ARENA.grow(Self::ARENA_PAGES, alloc) {
            Ok(va) => {
                unsafe { *self.arena_base.get() = va as *mut u8; }
                unsafe { *self.cursor.get() = 0; }
                true
            }
            Err(_) => false,
        }
    }

    fn ensure_arena(&self) -> bool {
        if unsafe { (*self.arena_base.get()).is_null() } {
            return self.refill_arena();
        }
        true
    }
}

unsafe impl GlobalAlloc for VmAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if !self.ensure_arena() {
            return core::ptr::null_mut();
        }

        let size = layout.size();
        let align = layout.align();

        let base = unsafe { *self.arena_base.get() };
        let cursor = unsafe { *self.cursor.get() };
        let ptr = unsafe { base.add(cursor) };
        let offset = ptr.align_offset(align);
        let alloc_start = unsafe { ptr.add(offset) };
        let total = offset + size;

        if cursor + total > Self::ARENA_BYTES {
            if !self.refill_arena() {
                return core::ptr::null_mut();
            }
            return self.alloc(layout);
        }

        unsafe { *self.cursor.get() = cursor + total; }
        alloc_start
    }

    unsafe fn dealloc(&self, _ptr: *mut u8, _layout: Layout) {
        // no-op: bump allocator does not reclaim individual objects.
        // arena pages remain mapped in HeapArena for the VM process lifetime.
        //
        // This is intentional: VM server is a long-lived system service,
        // and most dynamically allocated structures (VirRegion, PhysRegion, PhysBlock...)
        // have lifetimes bound to the VM process. There is no "high-frequency alloc-immediate-free"
        // temporary object pattern.
    }
}

#[cfg_attr(not(test), global_allocator)]
static GLOBAL: VmAllocator = VmAllocator {
    arena_base: AssumeSyncCell::new(core::ptr::null_mut()),
    cursor: AssumeSyncCell::new(0),
};
