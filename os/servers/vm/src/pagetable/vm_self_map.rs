//! VM self page table mapping interface.
//!
//! Provides `vm_self_mappages()` and `vm_self_unmappages()` for mapping
//! physical pages into the VM process's own page table. This is used by
//! HeapArena to establish contiguous VA mappings for the Rust heap.
//!
//! # Why not just use Paging::map() directly?
//!
//! The Paging trait requires `&mut self`, but the global allocator (VmAllocator)
//! only has `&self` (GlobalAlloc trait constraint). This module stores a raw
//! pointer to VM's page table (registered during init) and provides free
//! functions that unsafely convert it to `&mut`. This is safe because VM is
//! single-threaded.
//!
//! # Storage stability
//!
//! The page table is stored in a module-level static (`VM_PAGE_TABLE_STORAGE`),
//! not inside VmServer. This ensures the page table has a stable address
//! regardless of VmServer being moved. The pointer registered via
//! `init_vm_self_pt()` points into this static storage and remains valid
//! for the entire VM process lifetime.
//!
//! # No recursion risk
//!
//! When VM writes PTEs for HeapArena, the page table pages themselves are
//! accessed via Direct Map (`vm_phys_to_virt(page_table_page_phys)`), not
//! through HeapArena. Therefore there is no recursive dependency:
//! HeapArena → vm_self_mappages → Paging::map → Direct Map (stable VA).

use core::mem::MaybeUninit;
use core::sync::atomic::{AtomicBool, AtomicPtr, Ordering};
use minix_types::{AssumeSyncCell, PhysBytes, VirBytes};
use crate::pagetable::{PageTable, PageFlags, PageTableError, Paging};

static VM_SELF_PT: AtomicPtr<PageTable> = AtomicPtr::new(core::ptr::null_mut());

static VM_PAGE_TABLE_STORAGE: AssumeSyncCell<MaybeUninit<PageTable>> =
    AssumeSyncCell::new(MaybeUninit::uninit());

static VM_PAGE_TABLE_INITIALIZED: AtomicBool = AtomicBool::new(false);

/// Initialize VM's own page table in static storage.
///
/// Creates a new PageTable, stores it in the module-level static,
/// and registers the pointer. Must be called once during VmServer init,
/// before any heap allocation that triggers HeapArena::grow().
///
/// # Panics
///
/// Panics if called more than once or if PageTable::new() fails.
pub(crate) fn init_vm_self_pt() {
    if VM_PAGE_TABLE_INITIALIZED.load(Ordering::SeqCst) {
        return;
    }

    let pt = PageTable::new().expect("init_vm_self_pt: failed to create page table");
    unsafe {
        core::ptr::write(VM_PAGE_TABLE_STORAGE.get(), MaybeUninit::new(pt));
    }
    let pt_ref = unsafe { (*VM_PAGE_TABLE_STORAGE.get()).assume_init_mut() };
    VM_SELF_PT.store(pt_ref as *mut PageTable, Ordering::SeqCst);
    VM_PAGE_TABLE_INITIALIZED.store(true, Ordering::SeqCst);
}

/// Map a single page into VM's own page table.
///
/// Used by HeapArena to map physical pages into the contiguous VA region.
/// Page table pages are accessed via Direct Map — no recursive mapping needed.
///
/// # Panics
///
/// Panics if `init_vm_self_pt()` has not been called yet.
pub(crate) fn vm_self_mappages(
    va: VirBytes,
    phys: PhysBytes,
    flags: PageFlags,
) -> Result<(), PageTableError> {
    let pt = VM_SELF_PT.load(Ordering::SeqCst);
    assert!(!pt.is_null(), "vm_self_mappages: VM self page table not initialized");
    unsafe { (*pt).map(va, phys, flags) }
}

/// Unmap a single page from VM's own page table, returning the physical address.
///
/// Unlike `vm_self_unmappages()` which unmaps a range, this returns the
/// physical address of the unmapped page, allowing the caller to free it.
///
/// # Panics
///
/// Panics if `init_vm_self_pt()` has not been called yet.
pub(crate) fn vm_self_unmap(va: VirBytes) -> Result<PhysBytes, PageTableError> {
    let pt = VM_SELF_PT.load(Ordering::SeqCst);
    assert!(!pt.is_null(), "vm_self_unmap: VM self page table not initialized");
    unsafe { (*pt).unmap(va) }
}

/// Query a mapping in VM's own page table.
///
/// Returns the physical address and flags if the VA is mapped, or `None`.
///
/// # Panics
///
/// Panics if `init_vm_self_pt()` has not been called yet.
pub(crate) fn vm_self_query(va: VirBytes) -> Option<(PhysBytes, PageFlags)> {
    let pt = VM_SELF_PT.load(Ordering::SeqCst);
    assert!(!pt.is_null(), "vm_self_query: VM self page table not initialized");
    unsafe { (*pt).query(va) }
}

/// Unmap a range of pages from VM's own page table.
///
/// Used by HeapArena to shrink the mapped region.
///
/// # Panics
///
/// Panics if `init_vm_self_pt()` has not been called yet.
pub(crate) fn vm_self_unmappages(
    va_start: VirBytes,
    pages: usize,
) -> Result<(), PageTableError> {
    let pt = VM_SELF_PT.load(Ordering::SeqCst);
    assert!(!pt.is_null(), "vm_self_unmappages: VM self page table not initialized");
    unsafe { (*pt).unmap_range(va_start, pages) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use minix_types::PhysBytes;

    #[test]
    fn test_vm_self_mappages_not_initialized() {
        assert!(VM_SELF_PT.load(Ordering::SeqCst).is_null());
    }
}
