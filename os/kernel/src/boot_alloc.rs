//! Boot-stage page table page allocator.
//!
//! Simple bump allocator with identity mapping (VA = PA).
//! Registered by `arch_boot_impl` before the first `map_huge` call.
//! After VM initialization, the allocator is replaced via `pt_alloc::register()`.
//!
//! # Design
//!
//! The allocator state lives in [`BootAlloc`] so tests can instantiate
//! isolated instances without interfering with each other. The production
//! path uses a single global `static` accessed via [`boot_pt_alloc`] /
//! [`init_boot_pt_alloc`].
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

/// Boot-stage bump allocator state.
///
/// Encapsulates the `[base, end)` range and a `next` cursor so that tests
/// can create independent instances without sharing global state. The
/// production kernel uses a single `static` instance via
/// [`boot_pt_alloc`] / [`init_boot_pt_alloc`].
pub struct BootAlloc {
    next: AtomicU64,
    end: AtomicU64,
}

impl BootAlloc {
    /// Create an empty allocator (zero-length range → all allocations fail).
    pub const fn new() -> Self {
        Self {
            next: AtomicU64::new(0),
            end: AtomicU64::new(0),
        }
    }

    /// Initialize (or reset) the bump allocator range `[base, end)`.
    ///
    /// Must be called before the first [`Self::alloc`].
    pub fn init(&self, base: u64, end: u64) {
        self.next.store(base, Ordering::Relaxed);
        self.end.store(end, Ordering::Relaxed);
    }

    /// Allocate a zero-filled 4 KiB page with identity mapping (VA = PA).
    ///
    /// Returns `(phys, virt)` where `virt == phys`.
    pub fn alloc(&self) -> Result<(PhysBytes, VirBytes), PageTableError> {
        let pa = self.next.fetch_add(0x1000, Ordering::Relaxed);
        if pa >= self.end.load(Ordering::Relaxed) {
            // Roll back the increment on failure
            self.next.fetch_sub(0x1000, Ordering::Relaxed);
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
}

impl Default for BootAlloc {
    fn default() -> Self {
        Self::new()
    }
}

// ── Global instance (production path) ──

static BOOT_ALLOC: BootAlloc = BootAlloc::new();

/// Allocate a zero-filled page for an intermediate page table (bump, identity).
///
/// Returns `(phys, virt)` where `virt == phys` (identity mapping).
/// Called by Paging implementations via `pt_alloc::alloc_pt_page()`.
///
/// Delegates to the global [`BootAlloc`] instance.
pub fn boot_pt_alloc() -> Result<(PhysBytes, VirBytes), PageTableError> {
    BOOT_ALLOC.alloc()
}

/// Initialize the boot-stage bump allocator range.
///
/// Must be called before any `map_huge` or `map` operation.
/// The caller provides a contiguous physical range that is free
/// and identity-mapped (typically a region from `KernelInfo.memmap`).
pub fn init_boot_pt_alloc(base: u64, end: u64) {
    BOOT_ALLOC.init(base, end);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verify that sequential allocations return contiguous pages
    /// with identity mapping (virt == phys).
    #[test]
    fn test_sequential_allocation() {
        let alloc = BootAlloc::new();
        alloc.init(0x1000_0000, 0x1000_0000 + 0x3000); // 3 pages

        let (pa0, va0) = alloc.alloc().unwrap();
        assert_eq!(pa0.0, 0x1000_0000, "first page at base");
        assert_eq!(va0.0, 0x1000_0000, "identity: virt == phys");

        let (pa1, va1) = alloc.alloc().unwrap();
        assert_eq!(pa1.0, 0x1000_1000, "second page at base + 4KB");
        assert_eq!(va1.0, 0x1000_1000, "identity: virt == phys");

        let (pa2, va2) = alloc.alloc().unwrap();
        assert_eq!(pa2.0, 0x1000_2000, "third page at base + 8KB");
        assert_eq!(va2.0, 0x1000_2000, "identity: virt == phys");
    }

    /// Verify that allocation fails when the bump region is exhausted.
    #[test]
    fn test_allocation_exhaustion() {
        let alloc = BootAlloc::new();
        alloc.init(0x2000_0000, 0x2000_0000 + 0x2000); // 2 pages

        let _ = alloc.alloc().unwrap();
        let _ = alloc.alloc().unwrap();
        let result = alloc.alloc();
        assert!(result.is_err(), "should fail when bump region exhausted");
        match result {
            Err(PageTableError::AllocationFailed) => {}
            other => panic!("expected AllocationFailed, got {:?}", other),
        }
    }

    /// Verify that allocation fails immediately when the region has zero size.
    #[test]
    fn test_zero_size_region() {
        let alloc = BootAlloc::new();
        alloc.init(0x3000_0000, 0x3000_0000); // zero size

        let result = alloc.alloc();
        assert!(result.is_err(), "should fail with zero-size region");
    }

    /// Verify that re-initialization resets the allocator state.
    #[test]
    fn test_reinitialization() {
        let alloc = BootAlloc::new();
        alloc.init(0x4000_0000, 0x4000_0000 + 0x1000); // 1 page
        let _ = alloc.alloc().unwrap();

        // Re-init with a new range
        alloc.init(0x5000_0000, 0x5000_0000 + 0x1000); // 1 page
        let (pa, va) = alloc.alloc().unwrap();
        assert_eq!(pa.0, 0x5000_0000, "after reinit, allocation starts at new base");
        assert_eq!(va.0, 0x5000_0000, "identity mapping preserved after reinit");
    }

    /// Verify that each allocation advances by exactly one page (4KB).
    #[test]
    fn test_page_size_alignment() {
        let alloc = BootAlloc::new();
        alloc.init(0x6000_0000, 0x6000_0000 + 0x5000); // 5 pages

        let mut prev = 0u64;
        for i in 0..5 {
            let (pa, _) = alloc.alloc().unwrap();
            if i > 0 {
                assert_eq!(pa.0 - prev, 0x1000,
                    "each allocation advances by exactly 4KB");
            }
            prev = pa.0;
        }
    }
}
