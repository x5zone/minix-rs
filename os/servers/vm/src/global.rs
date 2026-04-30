//! VM global state module.
//!
//! Provides simple global variables like Minix3's glo.h:
//! - `BOOT_INFO`: Boot image array (kernel-provided process info)
//! - `TOTAL_PAGES`: Total physical memory pages
//! - `VM_INSTANCE_COUNT`: Number of active VM instances (for RS restart)

use core::sync::atomic::{AtomicU32, Ordering};

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
static VM_INSTANCE_COUNT: AtomicU32 = AtomicU32::new(0);

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
    VM_INSTANCE_COUNT.fetch_add(1, Ordering::SeqCst);
}

/// Decrements VM instance count.
pub(crate) fn dec_vm_instance() {
    VM_INSTANCE_COUNT.fetch_sub(1, Ordering::SeqCst);
}

/// Returns current VM instance count.
pub(crate) fn vm_instance_count() -> u32 {
    VM_INSTANCE_COUNT.load(Ordering::SeqCst)
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
        // Reset counter for test
        VM_INSTANCE_COUNT.store(0, Ordering::SeqCst);
        
        assert_eq!(vm_instance_count(), 0);
        
        inc_vm_instance();
        assert_eq!(vm_instance_count(), 1);

        inc_vm_instance();
        assert_eq!(vm_instance_count(), 2);

        dec_vm_instance();
        assert_eq!(vm_instance_count(), 1);
        
        // Reset
        VM_INSTANCE_COUNT.store(0, Ordering::SeqCst);
    }
}
