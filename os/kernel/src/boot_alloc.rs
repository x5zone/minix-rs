//! Boot-stage page table page allocator.
//!
//! Simple bump allocator with identity mapping (VA = PA).
//! Registered by `arch_boot_impl` before the first `map_huge` call.
//! After VM initialization, the allocator is replaced via `pt_alloc::register()`.
//!
//! # Concurrency safety
//!
//! Boot stage is single-threaded (only one CPU runs, no SMP yet).
//! `AtomicU64` is used instead of `static mut` to avoid Rust 2024 edition
//! UB and to be safe if the allocator is ever used under SMP in the future.
//! `Ordering::Relaxed` is sufficient since there is no concurrent access.

use core::sync::atomic::{AtomicU64, Ordering};
use minix_types::{PhysBytes, VirBytes};
use minix_arch::paging::PageTableError;

static BOOT_PT_NEXT: AtomicU64 = AtomicU64::new(0);
static BOOT_PT_END: AtomicU64 = AtomicU64::new(0);

/// Allocate a zero-filled page for an intermediate page table (bump, identity).
///
/// Returns `(phys, virt)` where `virt == phys` (identity mapping).
/// Called by Paging implementations via `pt_alloc::alloc_pt_page()`.
pub fn boot_pt_alloc() -> Result<(PhysBytes, VirBytes), PageTableError> {
    let pa = BOOT_PT_NEXT.fetch_add(0x1000, Ordering::Relaxed);
    if pa >= BOOT_PT_END.load(Ordering::Relaxed) {
        // Roll back the increment on failure
        BOOT_PT_NEXT.fetch_sub(0x1000, Ordering::Relaxed);
        return Err(PageTableError::AllocationFailed);
    }
    // Zero-fill the page. In test mode, skip the write (no real memory).
    #[cfg(not(test))]
    // SAFETY: pa is a valid, 4KB-aligned physical address returned by the
    // bump allocator. Under identity mapping (boot stage), VA=PA, so the
    // pointer is valid. The page is not aliased by any other live reference.
    unsafe { core::ptr::write_bytes(pa as *mut u64, 0, 512) };
    Ok((PhysBytes(pa), VirBytes(pa)))
}

/// Initialize the boot-stage bump allocator range.
///
/// Must be called before any `map_huge` or `map` operation.
/// The caller provides a contiguous physical range that is free
/// and identity-mapped (typically a region from `KernelInfo.memmap`).
pub fn init_boot_pt_alloc(base: u64, end: u64) {
    BOOT_PT_NEXT.store(base, Ordering::Relaxed);
    BOOT_PT_END.store(end, Ordering::Relaxed);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verify that sequential allocations return contiguous pages
    /// with identity mapping (virt == phys).
    #[test]
    fn test_sequential_allocation() {
        init_boot_pt_alloc(0x1000_0000, 0x1000_0000 + 0x3000); // 3 pages

        let (pa0, va0) = boot_pt_alloc().unwrap();
        assert_eq!(pa0.0, 0x1000_0000, "first page at base");
        assert_eq!(va0.0, 0x1000_0000, "identity: virt == phys");

        let (pa1, va1) = boot_pt_alloc().unwrap();
        assert_eq!(pa1.0, 0x1000_1000, "second page at base + 4KB");
        assert_eq!(va1.0, 0x1000_1000, "identity: virt == phys");

        let (pa2, va2) = boot_pt_alloc().unwrap();
        assert_eq!(pa2.0, 0x1000_2000, "third page at base + 8KB");
        assert_eq!(va2.0, 0x1000_2000, "identity: virt == phys");
    }

    /// Verify that allocation fails when the bump region is exhausted.
    #[test]
    fn test_allocation_exhaustion() {
        init_boot_pt_alloc(0x2000_0000, 0x2000_0000 + 0x2000); // 2 pages

        let _ = boot_pt_alloc().unwrap();
        let _ = boot_pt_alloc().unwrap();
        let result = boot_pt_alloc();
        assert!(result.is_err(), "should fail when bump region exhausted");
        match result {
            Err(PageTableError::AllocationFailed) => {}
            other => panic!("expected AllocationFailed, got {:?}", other),
        }
    }

    /// Verify that allocation fails immediately when the region has zero size.
    #[test]
    fn test_zero_size_region() {
        init_boot_pt_alloc(0x3000_0000, 0x3000_0000); // zero size

        let result = boot_pt_alloc();
        assert!(result.is_err(), "should fail with zero-size region");
    }

    /// Verify that re-initialization resets the allocator state.
    #[test]
    fn test_reinitialization() {
        init_boot_pt_alloc(0x4000_0000, 0x4000_0000 + 0x1000); // 1 page
        let _ = boot_pt_alloc().unwrap();

        // Re-init with a new range
        init_boot_pt_alloc(0x5000_0000, 0x5000_0000 + 0x1000); // 1 page
        let (pa, va) = boot_pt_alloc().unwrap();
        assert_eq!(pa.0, 0x5000_0000, "after reinit, allocation starts at new base");
        assert_eq!(va.0, 0x5000_0000, "identity mapping preserved after reinit");
    }

    /// Verify that each allocation advances by exactly one page (4KB).
    #[test]
    fn test_page_size_alignment() {
        init_boot_pt_alloc(0x6000_0000, 0x6000_0000 + 0x5000); // 5 pages

        let mut prev = 0u64;
        for i in 0..5 {
            let (pa, _) = boot_pt_alloc().unwrap();
            if i > 0 {
                assert_eq!(pa.0 - prev, 0x1000,
                    "each allocation advances by exactly 4KB");
            }
            prev = pa.0;
        }
    }
}
