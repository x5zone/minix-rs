//! VM self page table mapping interface.
//!
//! Provides `vm_self_mappages()` and `vm_self_unmappages()` for mapping
//! physical pages into the VM process's own page table. This is used by
//! HeapArena to establish contiguous VA mappings for the Rust heap.
//!
//! # Why not just use Paging::map() directly?
//!
//! The Paging trait requires `&mut self`, but the global allocator (VmAllocator)
//! only has `&self` (GlobalAlloc trait constraint). This module stores the page
//! table in a module-level `Option<PageTable>` static and provides free
//! functions that obtain `&mut PageTable` from it. This is safe because VM is
//! single-threaded — only one mutable reference can exist at any time.
//!
//! # Storage stability
//!
//! The page table is stored in a module-level static (`VM_SELF_PT_STORAGE`),
//! not inside VmServer. This ensures the page table has a stable address
//! regardless of VmServer being moved. The `Option<PageTable>` is set once
//! during init and remains `Some` for the entire VM process lifetime.
//!
//! # No recursion risk
//!
//! When VM writes PTEs for HeapArena, the page table pages themselves are
//! accessed via Direct Map (`vm_phys_to_virt(page_table_page_phys)`), not
//! through HeapArena. Therefore there is no recursive dependency:
//! HeapArena → vm_self_mappages → Paging::map → Direct Map (stable VA).
//!
//! # Design: Option<PageTable> vs AtomicPtr
//!
//! Previous implementation used `AtomicPtr<PageTable>` + `AtomicBool` +
//! `AssumeSyncCell<MaybeUninit<PageTable>>` — a three-piece pattern with
//! null-pointer sentinel (C-style) and `MaybeUninit` (unnecessary here
//! since `Option<PageTable>` provides the same "uninitialized" state via
//! Rust's type system). The current design:
//!
//! - Uses `AssumeSyncCell<Option<PageTable>>` — single static, no raw pointers,
//!   no `MaybeUninit`, no sentinel values. `None` = not initialized, `Some(pt)`
//!   = initialized. This is the Rust-idiomatic way to express "value may or may
//!   not exist" without unsafe pointer manipulation.
//! - All access goes through `get_pt_mut()` which returns `&mut PageTable`.
//!   The `AssumeSyncCell::get_mut()` is safe because VM is single-threaded
//!   (no concurrent `&mut` references can exist). No `unsafe` blocks needed
//!   in the access functions — only in `init_vm_self_pt()` which writes the
//!   `Some` value (unsafe due to `AssumeSyncCell::get()` returning a raw pointer).

use minix_types::{AssumeSyncCell, PhysBytes, VirBytes};
use crate::pagetable::{PageTable, PageFlags, PageTableError, Paging};

/// Module-level storage for VM's own page table.
///
/// `None` before `init_vm_self_pt()` is called; `Some(pt)` afterwards.
/// `AssumeSyncCell` is safe because VM is single-threaded — no concurrent
/// access is possible.
static VM_SELF_PT_STORAGE: AssumeSyncCell<Option<PageTable>> =
    AssumeSyncCell::new(None);

/// Obtain a mutable reference to VM's page table.
///
/// # Panics
///
/// Panics if `init_vm_self_pt()` has not been called yet.
fn get_pt_mut() -> &'static mut PageTable {
    // SAFETY: VM is single-threaded (event loop model). No concurrent &mut
    // references can exist. The AssumeSyncCell wrapper only exists to satisfy
    // the `Sync` requirement for static items; the actual safety guarantee
    // comes from the single-threaded execution model.
    let opt = unsafe { &mut *VM_SELF_PT_STORAGE.get() };
    opt.as_mut()
        .expect("vm_self_pt: page table not initialized — call init_vm_self_pt() first")
}

/// Initialize VM's own page table in static storage.
///
/// Creates a new PageTable, stores it in the module-level static.
/// Must be called once during VmServer init, before any heap allocation
/// that triggers HeapArena::grow().
///
/// # Panics
///
/// Panics if called more than once or if PageTable::new() fails.
pub(crate) fn init_vm_self_pt() {
    // SAFETY: VM is single-threaded. No concurrent access to VM_SELF_PT_STORAGE.
    let opt = unsafe { &mut *VM_SELF_PT_STORAGE.get() };
    if opt.is_some() {
        panic!("init_vm_self_pt: called more than once");
    }
    let pt = PageTable::new().expect("init_vm_self_pt: failed to create page table");
    *opt = Some(pt);
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
    get_pt_mut().map(va, phys, flags)
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
    get_pt_mut().unmap(va)
}

/// Query a mapping in VM's own page table.
///
/// Returns the physical address and flags if the VA is mapped, or `None`.
///
/// # Panics
///
/// Panics if `init_vm_self_pt()` has not been called yet.
pub(crate) fn vm_self_query(va: VirBytes) -> Option<(PhysBytes, PageFlags)> {
    get_pt_mut().query(va)
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
    get_pt_mut().unmap_range(va_start, pages)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vm_self_pt_not_initialized_by_default() {
        // In test context, VM_SELF_PT_STORAGE starts as None.
        // We cannot call get_pt_mut() without init — that would panic.
        // Instead verify the storage is accessible.
        let opt = unsafe { &*VM_SELF_PT_STORAGE.get() };
        assert!(opt.is_none());
    }
}
