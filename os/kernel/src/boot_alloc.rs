//! Boot-stage page table page allocator.
//!
//! Simple bump allocator with identity mapping (VA = PA).
//! Registered by `arch_boot_impl` before the first `map_huge` call.
//! After VM initialization, the allocator is replaced via `pt_alloc::register()`.

use minix_types::{PhysBytes, VirBytes};
use minix_arch::paging::PageTableError;

// Bump allocator state — written once by init_boot_pt_alloc(),
// consumed by boot_pt_alloc(). Only accessed in single-threaded boot context.
static mut BOOT_PT_NEXT: u64 = 0;
static mut BOOT_PT_END: u64 = 0;

/// Allocate a zero-filled page for an intermediate page table (bump, identity).
///
/// Returns `(phys, virt)` where `virt == phys` (identity mapping).
/// Called by Paging implementations via `pt_alloc::alloc_pt_page()`.
pub fn boot_pt_alloc() -> Result<(PhysBytes, VirBytes), PageTableError> {
    unsafe {
        if BOOT_PT_NEXT >= BOOT_PT_END {
            return Err(PageTableError::AllocationFailed);
        }
        let pa = BOOT_PT_NEXT;
        BOOT_PT_NEXT += 0x1000;
        core::ptr::write_bytes(pa as *mut u64, 0, 512);
        Ok((PhysBytes(pa), VirBytes(pa)))
    }
}

/// Initialize the boot-stage bump allocator range.
///
/// Must be called before any `map_huge` or `map` operation.
/// The caller provides a contiguous physical range that is free
/// and identity-mapped (typically a region from `KernelInfo.memmap`).
pub fn init_boot_pt_alloc(base: u64, end: u64) {
    unsafe {
        BOOT_PT_NEXT = base;
        BOOT_PT_END = end;
    }
}
