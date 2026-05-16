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
use crate::direct_map::vm_phys_to_virt;
use crate::phys_mem::{PageAllocFlags, CLICK_SIZE};

/// Global allocator - bump allocator that splits physical pages in the Direct Map region
///
/// Preallocates 16 pages (64KB) as an arena, with internal cursor-based splitting.
/// Automatically allocates new arenas when exhausted (old arenas are not immediately reclaimed).
/// Dealloc is a no-op - individual objects are not reclaimed;
/// arena pages are released with Direct Map when the VM process exits.
pub(crate) struct VmAllocator {
    arena_base: AssumeSyncCell<*mut u8>,
    cursor: AssumeSyncCell<usize>,
}

// Global pointer to the VmPageAllocator instance held by VmServer.
// Set during VM initialization via register_page_alloc(); null before that (alloc returns null).
// Uses AtomicPtr instead of AssumeSyncCell because GlobalAlloc::alloc takes &self (immutable),
// while VmPageAllocator methods require &mut self. AtomicPtr allows us to obtain a *mut pointer
// inside &self and unsafely convert it to &mut - VM is single-threaded, so no data race.
static PAGE_ALLOC_PTR: AtomicPtr<VmPageAllocator> = AtomicPtr::new(core::ptr::null_mut());

/// Called during VmServer initialization to register the VmPageAllocator pointer with the global allocator.
/// After this, heap allocations like Box::new() and Vec::push() will work.
pub(crate) fn register_page_alloc(alloc: &mut VmPageAllocator) {
    PAGE_ALLOC_PTR.store(alloc as *mut VmPageAllocator, Ordering::SeqCst);
}

impl VmAllocator {
    /// Number of pages to preallocate. First arena requested by bump allocator after VM startup.
    /// 64KB can hold ~570 PhysBlocks (24B) or ~1300 PhysRegions (48B).
    const ARENA_PAGES: usize = 16;
    const ARENA_BYTES: usize = Self::ARENA_PAGES * CLICK_SIZE;

    fn refill_arena(&self) {
        let alloc = unsafe { &mut *PAGE_ALLOC_PTR.load(Ordering::SeqCst) };
        let phys = alloc.alloc_phys(Self::ARENA_PAGES, PageAllocFlags::empty())
            .expect("VmAllocator: out of physical memory");
        let va = vm_phys_to_virt(phys).0 as *mut u8;
        unsafe { *self.arena_base.get() = va; }
        unsafe { *self.cursor.get() = 0; }
    }

    fn ensure_arena(&self) {
        if unsafe { (*self.arena_base.get()).is_null() } {
            self.refill_arena();
        }
    }
}

unsafe impl GlobalAlloc for VmAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        self.ensure_arena();

        let size = layout.size();
        let align = layout.align();

        // bump: align cursor, carve out size bytes
        let base = unsafe { *self.arena_base.get() };
        let cursor = unsafe { *self.cursor.get() };
        let ptr = unsafe { base.add(cursor) };
        let offset = ptr.align_offset(align);
        let alloc_start = unsafe { ptr.add(offset) };
        let total = offset + size;

        if cursor + total > Self::ARENA_BYTES {
            self.refill_arena();
            return self.alloc(layout);
        }

        unsafe { *self.cursor.get() = cursor + total; }
        alloc_start
    }

    unsafe fn dealloc(&self, _ptr: *mut u8, _layout: Layout) {
        // no-op: bump allocator does not reclaim individual objects.
        // arena pages are released with Direct Map when the VM process exits.
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
