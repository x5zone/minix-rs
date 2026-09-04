//! VM self page table mapping interface ([ARCH: A-9]: Minix3 BSS spare
//! resources `static_sparepages`/`static_sparepagedirs` evolve into a
//! module-level `Option<VmSelfPageTable>` + free-function interface).
//!
//! Provides `vm_self_mappages()` and `vm_self_unmappages()` for mapping
//! physical pages into the VM process's own page table. This is used by
//! HeapArena to establish contiguous VA mappings for the Rust heap.
//!
//! # Address-space identity: adoption, not creation
//!
//! VM's initial page table IS the bootstrap root the kernel built and
//! enabled before scheduling VM (A1 adoption, see
//! `07-paging_init_design` §4). The kernel hands over the root's physical
//! address via the boot handoff page; [`VmSelfPageTable::adopt`] wraps
//! that root instead of creating a fresh one. The newtype has no
//! `Clone`/`Copy` and no fresh-root constructor: a second root would
//! split VM's address-space identity (two "self" page tables with
//! divergent contents), and a duplicated handle would allow two handles
//! to evolve independently of what must remain a single mutable
//! authority. Construction is therefore a controlled one-time operation
//! — exactly one `VmSelfPageTable` exists, created by
//! [`init_vm_self_pt`] from the handoff root.
//!
//! # Why not just use Paging::map() directly?
//!
//! The Paging trait requires `&mut self`, but the global allocator (VmAllocator)
//! only has `&self` (GlobalAlloc trait constraint). This module stores the page
//! table in a module-level `Option<VmSelfPageTable>` static and provides free
//! functions that obtain `&mut VmSelfPageTable` from it. This is safe because VM is
//! single-threaded — only one mutable reference can exist at any time.
//!
//! # Storage stability
//!
//! The page table is stored in a module-level static (`VM_SELF_PT_STORAGE`),
//! not inside VmServer. This ensures the page table has a stable address
//! regardless of VmServer being moved. The `Option<VmSelfPageTable>` is set once
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

/// VM's own page-table identity: a handle to the bootstrap root the kernel
/// built and enabled before scheduling VM (A1 adoption).
///
/// Construction is deliberately restricted:
///
/// - The only constructor is [`adopt`](Self::adopt), which binds an
///   existing root by physical address — there is no fresh-root
///   constructor, so this type cannot create a second address space.
/// - No `Clone`/`Copy`: a duplicated handle would allow two paths to
///   mutate the same root independently of the single-authority rule
///   enforced by [`init_vm_self_pt`].
///
/// The wrapped handle uses the VM Direct Map PTE access channel (D2-⑥):
/// the handle is exercised while VM — a user-space process — runs, where
/// the kernel Direct Map window (supervisor-only) is unreachable, so
/// page-table pages must be reached through the VM Direct Map.
pub(crate) struct VmSelfPageTable {
    inner: PageTable,
}

impl VmSelfPageTable {
    /// Adopt `root_paddr` as VM's own page-table root.
    ///
    /// # Panics
    ///
    /// Panics (debug builds) if `root_paddr` is not page-aligned — the
    /// boot handoff already validates this; the assertion guards
    /// test-constructed roots.
    pub(crate) fn adopt(root_paddr: PhysBytes) -> Self {
        debug_assert!(
            root_paddr.0 & 0xFFF == 0,
            "VmSelfPageTable::adopt: root_paddr 0x{:x} must be page-aligned",
            root_paddr.0
        );
        Self {
            inner: PageTable::adopt_active_root(root_paddr),
        }
    }

    /// Physical address of the adopted root (adopt round-trip check).
    #[cfg(test)]
    pub(crate) fn root_paddr(&self) -> PhysBytes {
        self.inner.root_paddr()
    }

    pub(crate) fn map(
        &mut self,
        vaddr: VirBytes,
        paddr: PhysBytes,
        flags: PageFlags,
    ) -> Result<(), PageTableError> {
        self.inner.map(vaddr, paddr, flags)
    }

    pub(crate) fn unmap(&mut self, vaddr: VirBytes) -> Result<PhysBytes, PageTableError> {
        self.inner.unmap(vaddr)
    }

    #[cfg_attr(not(test), allow(dead_code))] // V10-P2-1 (DEFERRED): test-only today
    pub(crate) fn query(&self, vaddr: VirBytes) -> Option<(PhysBytes, PageFlags)> {
        self.inner.query(vaddr)
    }

    pub(crate) fn unmap_range(
        &mut self,
        va_start: VirBytes,
        pages: usize,
    ) -> Result<(), PageTableError> {
        self.inner.unmap_range(va_start, pages)
    }
}

/// Module-level storage for VM's own page table.
///
/// `None` before `init_vm_self_pt()` is called; `Some(pt)` afterwards.
/// `AssumeSyncCell` is safe because VM is single-threaded — no concurrent
/// access is possible.
static VM_SELF_PT_STORAGE: AssumeSyncCell<Option<VmSelfPageTable>> =
    AssumeSyncCell::new(None);

/// Obtain a mutable reference to VM's page table.
///
/// # Panics
///
/// Panics if `init_vm_self_pt()` has not been called yet.
fn get_pt_mut() -> &'static mut VmSelfPageTable {
    // SAFETY: VM is single-threaded (event loop model). No concurrent &mut
    // references can exist. The AssumeSyncCell wrapper only exists to satisfy
    // the `Sync` requirement for static items; the actual safety guarantee
    // comes from the single-threaded execution model.
    let opt = unsafe { &mut *VM_SELF_PT_STORAGE.get() };
    opt.as_mut()
        .expect("vm_self_pt: page table not initialized — call init_vm_self_pt() first")
}

/// Initialize VM's own page table by adopting the bootstrap root.
///
/// `root_paddr` is the address-space identity handed over by the kernel
/// via the boot handoff page (`BootParams::root_paddr`, read from
/// `minix_types::VM_BOOT_HANDOFF_VA` in `main`): VM's initial page table
/// IS the bootstrap root the kernel built and enabled, so VM wraps it
/// instead of creating a fresh one. Must be called once during VmServer
/// init, before any heap allocation that triggers HeapArena::grow().
///
/// # Panics
///
/// Panics if called more than once — re-adoption would create a second
/// authority for the same address space.
pub(crate) fn init_vm_self_pt(root_paddr: PhysBytes) {
    // SAFETY: VM is single-threaded. No concurrent access to VM_SELF_PT_STORAGE.
    let opt = unsafe { &mut *VM_SELF_PT_STORAGE.get() };
    if opt.is_some() {
        panic!("init_vm_self_pt: called more than once");
    }
    *opt = Some(VmSelfPageTable::adopt(root_paddr));
}

/// Test-only: reset the module-level storage so `init_vm_self_pt()` can be
/// re-run by the next test. Tests execute single-threaded (RUST_TEST_THREADS=1,
/// .cargo/config.toml), so this cannot race with a live page table.
#[cfg(test)]
pub(crate) fn reset_vm_self_pt_for_test() {
    // SAFETY: Single-threaded test execution; no concurrent access.
    unsafe {
        *VM_SELF_PT_STORAGE.get() = None;
    }
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
#[cfg_attr(not(test), allow(dead_code))] // V10-P2-1 (DEFERRED): test-only today
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

    /// Fake-but-valid bootstrap root for tests (page-aligned, non-zero).
    /// The mock `Paging` records it verbatim; real arches never see it.
    const TEST_ROOT: PhysBytes = PhysBytes(0x900_000);

    #[test]
    fn test_vm_self_pt_not_initialized_by_default() {
        // Reset first so this test is order-independent (HeapArena tests
        // may have initialized the storage earlier in the same process).
        reset_vm_self_pt_for_test();
        let opt = unsafe { &*VM_SELF_PT_STORAGE.get() };
        assert!(opt.is_none());
    }

    #[test]
    fn test_init_vm_self_pt_then_reset() {
        reset_vm_self_pt_for_test();
        init_vm_self_pt(TEST_ROOT);
        assert!(unsafe { &*VM_SELF_PT_STORAGE.get() }.is_some());
        // init is a one-shot: second call must panic (re-adoption would
        // create a second authority for the same address space).
        assert!(std::panic::catch_unwind(|| init_vm_self_pt(TEST_ROOT)).is_err());
        reset_vm_self_pt_for_test();
        assert!(unsafe { &*VM_SELF_PT_STORAGE.get() }.is_none());
    }

    #[test]
    fn test_adopt_round_trips_root_paddr() {
        // A1 adoption contract: the handle must wrap the root the kernel
        // handed over — not a freshly created one. If `adopt` ever
        // regressed to `new()`, the root PA would no longer round-trip.
        reset_vm_self_pt_for_test();
        init_vm_self_pt(TEST_ROOT);
        let pt = get_pt_mut();
        assert_eq!(pt.root_paddr(), TEST_ROOT,
            "adopted handle must wrap the handoff root, not a fresh root");
        reset_vm_self_pt_for_test();
    }
}
