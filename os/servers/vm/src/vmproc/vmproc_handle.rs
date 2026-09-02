//! VM process typestate views.
//!
//! Compile-time enforced state machine:
//! ```text
//! EmptySlot ──[activate]──> ActiveProc ──[mark_exiting]──> ExitingProc ──[reap]──> EmptySlot
//!                           ActiveProc ──[force_clear]──────────────────────────> EmptySlot
//! ```
//!
//! Each view borrows `&mut VmProc`, granting exclusive access
//! and preventing unsound concurrent mutations.

use minix_types::{BootImage, Endpoint, UserSlot, VirBytes};
use minix_arch::paging::Paging;
use minix_arch::paging::bind_to_process;
#[cfg(not(test))]
use minix_arch::paging::map_kernel;
use super::{VmFlags, vmproc::VmProc};
use crate::pagetable::PageTable;
use crate::region::RegionMap;
use minix_types::VmError;

/// Empty (free) slot typestate view.
///
/// Represents a slot that is not in use. The only meaningful operation
/// is `activate()` to transition it into an `ActiveProc`.
///
/// This view cannot be obtained for slots that are in use.
pub(crate) struct EmptySlot<'a> {
    inner: &'a mut VmProc,
}

impl<'a> EmptySlot<'a> {
    pub(crate) fn new(inner: &'a mut VmProc) -> Self {
        debug_assert!(!inner.vm_flags.contains(VmFlags::IN_USE));
        Self { inner }
    }

    #[inline]
    pub(crate) fn slot(&self) -> UserSlot {
        self.inner.vm_slot
    }

    /// Activates this slot with the given endpoint (strict mode).
    ///
    /// Verifies that `endpoint.slot()` matches `self.slot()` to ensure consistency.
    /// Sets IN_USE flag and assigns the endpoint. Does NOT initialize page_table
    /// or regions - caller must call `ActiveProc::init_page_table()` and
    /// `ActiveProc::init_regions()` separately for exec/new processes,
    /// or `ActiveProc::init_page_table()` + CoW setup for fork.
    ///
    /// # Panics
    /// Panics in debug mode if endpoint's slot doesn't match this slot.
    pub(crate) fn activate(self, endpoint: Endpoint) -> ActiveProc<'a> {
        debug_assert_eq!(
            endpoint.slot() as usize,
            self.slot().get(),
            "endpoint slot mismatch: endpoint.slot()={}, self.slot()={}",
            endpoint.slot(),
            self.slot().get()
        );
        self.activate_relaxed(endpoint)
    }

    /// Activates this slot with the given endpoint (relaxed mode).
    ///
    /// Does NOT enforce the strict endpoint-slot pairing that `activate()`
    /// does. Use ONLY for these specific cases (see `vmproc/mod.rs` API
    /// safety contract for the full discussion):
    ///
    /// - **Fork**: child endpoint is `Endpoint::NONE` initially, set by
    ///   the kernel after `sys_fork()` succeeds. See `fork.rs::do_fork`.
    /// - **Exec temporary slot**: the temporary slot at index
    ///   `VM_EXEC_TMP_SLOT` (`NR_PROCS`) may temporarily hold an endpoint
    ///   that does not match the slot index, because exec is the only
    ///   legitimate way to overwrite endpoint metadata.
    /// - **Tests**: arbitrary endpoints without matching slot numbers are
    ///   acceptable because test code does not exercise the IPC routing
    ///   path.
    ///
    /// In normal cases, use `activate()` which enforces consistency.
    ///
    /// # Debug-build sanity checks (2026-06-13)
    ///
    /// Even in relaxed mode, the function performs **release-build-cheap
    /// debug-only validation** to catch obvious misuses that would
    /// otherwise silently corrupt downstream state:
    ///
    /// 1. The endpoint must be either `Endpoint::NONE` (fork's child) or
    ///    have a slot index strictly less than `VM_PROC_COUNT`. This
    ///    bounds the endpoint to the addressable process table — an
    ///    endpoint with slot `>= VM_PROC_COUNT` is a bug because the
    ///    process table cannot store it.
    /// 2. The endpoint's slot, if non-NONE, must be either equal to
    ///    `self.slot()` (the strict path's invariant) or equal to
    ///    `VM_EXEC_TMP_SLOT` (the exec-rewrite path's exception). Any
    ///    other mismatch is a programming error.
    ///
    /// Both checks are `debug_assert!` only and are compiled out of
    /// release builds. The `// BKL protected`-equivalent single-threaded
    /// VM event loop model (see `vmproc/mod.rs` module header) keeps the
    /// race-free invariant; this is a typestate-API hardening, not a
    /// concurrency guard.
    ///
    /// Does NOT initialize vm_pt or vm_regions - caller must initialize
    /// separately based on the specific operation (fork/exec/new).
    pub(crate) fn activate_relaxed(self, endpoint: Endpoint) -> ActiveProc<'a> {
        use super::table::{VM_EXEC_TMP_SLOT, VM_PROC_COUNT};

        // (2026-06-13): defence-in-depth debug-only validation.
        // Catches: (a) endpoint with slot >= VM_PROC_COUNT (impossible
        // process index), (b) non-NONE endpoint whose slot does not
        // match self.slot() when self.slot() is not the exec-rewrite
        // temporary slot. Both are programming errors that previously
        // would have propagated silently to the fork/exec handlers.
        //
        // Use `is_none()` rather than `== Endpoint::NONE` so the check
        // follows the existing convention in the codebase and is robust
        // against any future change to Endpoint's internal representation.
        if !endpoint.is_none() {
            let ep_slot = endpoint.slot();
            debug_assert!(
                (ep_slot as usize) < VM_PROC_COUNT,
                "activate_relaxed: endpoint slot {} >= VM_PROC_COUNT {} \
                 (out-of-range endpoint)",
                ep_slot,
                VM_PROC_COUNT
            );
            // The exec-rewrite exception is keyed on the slot being
            // activated (self.slot()), not on the endpoint's slot. When
            // we are activating the exec temporary slot, we may
            // legitimately install an endpoint whose slot differs from
            // self.slot() — exec is the only legitimate way to overwrite
            // endpoint metadata through a different slot index.
            let is_exec_tmp = self.slot().get() == VM_EXEC_TMP_SLOT.get();
            debug_assert!(
                ep_slot as usize == self.slot().get() || is_exec_tmp,
                "activate_relaxed: non-NONE endpoint slot {} does not match \
                 self.slot()={} (and self.slot() is not VM_EXEC_TMP_SLOT={}, \
                 so exec-rewrite exception does not apply; use activate() \
                 for strict pairing)",
                ep_slot,
                self.slot().get(),
                VM_EXEC_TMP_SLOT.get()
            );
        }

        self.inner.vm_flags = VmFlags::IN_USE;
        self.inner.vm_endpoint = endpoint;
        ActiveProc::new(self.inner)
    }
}

/// Active process typestate view.
///
/// Process is in use and not exiting. Can perform normal operations
/// or transition to `ExitingProc` via `mark_exiting()`.
pub(crate) struct ActiveProc<'a> {
    inner: &'a mut VmProc,
}

impl<'a> ActiveProc<'a> {
    pub(crate) fn new(inner: &'a mut VmProc) -> Self {
        debug_assert!(inner.vm_flags.contains(VmFlags::IN_USE));
        debug_assert!(!inner.vm_flags.contains(VmFlags::EXITING));
        Self { inner }
    }

    /// Marks process as exiting.
    ///
    /// In-place state migration: sets EXITING flag, returns ExitingProc view.
    pub(crate) fn mark_exiting(self) -> ExitingProc<'a> {
        self.inner.vm_flags.insert(VmFlags::EXITING);
        ExitingProc::new(self.inner)
    }

    /// Force-clears an active process back to empty.
    ///
    /// Used for abnormal termination (e.g., force kill).
    /// Calls `VmProc::clear()` to release resources.
    ///
    /// # Safety
    /// Caller must ensure this process's page table is no longer in use by hardware.
    #[cfg_attr(not(test), allow(dead_code))] // V10-P2-1 (DEFERRED): swap/exit paths use it in tests only
    pub(crate) unsafe fn force_clear(self) -> EmptySlot<'a> {
        // SAFETY: Caller guarantees this process's page table is no longer in use
        // by hardware. Single-threaded VM ensures no concurrent access.
        unsafe { self.inner.clear(); }
        EmptySlot::new(self.inner)
    }

    #[inline]
    pub(crate) fn slot(&self) -> UserSlot {
        self.inner.vm_slot
    }

    #[inline]
    pub(crate) fn endpoint(&self) -> Endpoint {
        self.inner.vm_endpoint
    }

    #[inline]
    #[cfg_attr(not(test), allow(dead_code))] // V10-P2-1: test-only accessor
    pub(crate) fn flags(&self) -> VmFlags {
        self.inner.vm_flags
    }

    #[inline]
    #[allow(dead_code)] // V10-P2-1: no callers (VmProc::is_vm_instance is test-only too)
    pub(crate) fn is_vm_instance(&self) -> bool {
        self.inner.is_vm_instance()
    }

    #[inline]
    pub(crate) fn total(&self) -> VirBytes {
        self.inner.vm_total
    }

    #[inline]
    pub(crate) fn total_max(&self) -> VirBytes {
        self.inner.vm_total_max
    }

    #[inline]
    pub(crate) fn region_top(&self) -> VirBytes {
        self.inner.vm_region_top
    }

    #[inline]
    pub(crate) fn minor_fault(&self) -> u64 {
        self.inner.vm_minor_page_fault
    }

    #[inline]
    pub(crate) fn major_fault(&self) -> u64 {
        self.inner.vm_major_page_fault
    }

    #[inline]
    #[cfg_attr(not(test), allow(dead_code))] // V10-P2-1: test-only accessor
    pub(crate) fn acl(&self) -> crate::acl::AclState {
        self.inner.vm_acl
    }

    #[inline]
    pub(crate) fn set_acl(&mut self, acl: crate::acl::AclState) {
        self.inner.vm_acl = acl;
    }

    #[inline]
    pub(crate) fn acl_check(&self, call: u32) -> Result<(), minix_types::VmError> {
        self.inner.vm_acl.acl_check(self.inner.vm_endpoint, call)
    }

    #[inline]
    pub(crate) fn set_endpoint(&mut self, endpoint: Endpoint) {
        self.inner.vm_endpoint = endpoint;
    }

    #[inline]
    #[cfg_attr(not(test), allow(dead_code))] // V10-P2-1: test-only (query/table tests)
    pub(crate) fn set_total_max(&mut self, value: VirBytes) {
        self.inner.vm_total_max = value;
    }

    #[inline]
    pub(crate) fn set_boot(&mut self, boot: BootImage) {
        self.inner.vm_boot = Some(boot);
    }

    /// Marks this process as a VM instance.
    ///
    /// C: `init_vm()` — `vmproc[VM_PROC_NR].vm_flags |= VMF_VM_INSTANCE`
    /// (main.c:578) and `num_vm_instances = 1` (main.c:574).
    ///
    /// The global VM-instance count is incremented here so the count
    /// stays in sync with the flag; `VmProc::clear()` decrements it
    /// when the slot is released.
    pub(crate) fn mark_vm_instance(&mut self) {
        self.inner.vm_flags |= VmFlags::VM_INSTANCE;
        crate::global::inc_vm_instance();
    }

    #[inline]
    pub(crate) fn set_region_top(&mut self, value: VirBytes) {
        self.inner.vm_region_top = value;
    }

    pub(crate) fn add_total(&mut self, value: VirBytes) {
        self.inner.vm_total.0 += value.0;
        if self.inner.vm_total > self.inner.vm_total_max {
            self.inner.vm_total_max = self.inner.vm_total;
        }
    }

    pub(crate) fn sub_total(&mut self, value: VirBytes) {
        self.inner.vm_total.0 = self.inner.vm_total.0.saturating_sub(value.0);
    }

    #[inline]
    #[allow(dead_code)] // V10-P2-1: no callers yet (brk keeps totals via add/sub)
    pub(crate) fn set_total(&mut self, value: VirBytes) {
        self.inner.vm_total = value;
    }

    #[inline]
    // V10-P2-1 (DEFERRED): pagefault accounting counters have no producer
    // yet — the fault path does not bump them.
    #[allow(dead_code)]
    pub(crate) fn inc_minor_fault(&mut self) {
        self.inner.vm_minor_page_fault += 1;
    }

    #[inline]
    #[allow(dead_code)]
    pub(crate) fn inc_major_fault(&mut self) {
        self.inner.vm_major_page_fault += 1;
    }

    /// Initializes child process from fork parent data.
    ///
    /// Sets endpoint, memory stats, region_top, and clears all flags except IN_USE.
    /// `region_top` is inherited from parent (Minix3's `*vmc = *vmp` copies all fields).
    ///
    /// Note: ACL is NOT set here. Caller must call `copy_acl_from()` separately
    /// to set the child's ACL state (Minix3's `*vmc = *vmp` copies all fields
    /// including `vm_acl`, then `acl_fork()` corrects it; Rust uses explicit
    /// per-field initialization instead of struct copy).
    pub(crate) fn init_from_fork(&mut self, endpoint: Endpoint, total: VirBytes, total_max: VirBytes, region_top: VirBytes) {
        self.inner.vm_flags = VmFlags::IN_USE;
        self.inner.vm_endpoint = endpoint;
        self.inner.vm_total = total;
        self.inner.vm_total_max = total_max;
        self.inner.vm_region_top = region_top;
    }

    /// Copies ACL from parent to child.
    ///
    /// Corresponds to Minix3's `acl_fork()`:
    /// - If parent has Default, child gets Default
    /// - Otherwise, child gets Uninitialized
    pub(crate) fn copy_acl_from(&mut self, parent: &ActiveProc<'_>) {
        self.inner.vm_acl = parent.inner.vm_acl.acl_fork();
    }

    /// Initializes page table for exec or new processes.
    ///
    /// Creates a new empty page table with kernel mappings. Must be called before accessing vm_pt.
    /// For fork, also use this method to create a new page table, then call
    /// `setup_cow_for_all_regions()` + `write_page_table_mappings()` to copy parent's mappings.
    ///
    /// Corresponds to Minix3's `pt_new()` which calls `pt_mapkernel()`.
    pub(crate) fn init_page_table(&mut self) -> Result<(), minix_arch::paging::PageTableError> {
        // In test builds, write a zero-initialized page table stub.
        // X86_64Paging::new() is todo!() and new_from_page(0) would dereference
        // null. MaybeUninit::zeroed() gives a valid bit-pattern (root_paddr=0)
        // that is safe to reference but must not be used for real paging ops.
        // free_region_pages skips unmap in tests via Option<&mut PageTable>.
        #[cfg(test)]
        {
            self.inner.vm_pt = core::mem::MaybeUninit::zeroed();
            self.inner.vm_pt_initialized = true;
            return Ok(());
        }

        #[cfg(not(test))]
        {
            let mut pt = <PageTable as Paging>::new()?;

            // Map kernel address space into this user process's page table.
            //
            // The kernel layout is determined once at boot (from multiboot2 /
            // stivale2 headers or linker symbols) and stored in the global
            // `KERNEL_LAYOUT`. Every user process page table gets the same
            // kernel mapping — this is a fundamental x86-64 invariant.
            //
            // Previously (hardcoded kernel layout), these values were hardcoded constants guarded
            // by a `hardcoded_kernel_layout` feature flag with a `compile_error!`
            // guard. That approach acknowledged the values were wrong for real
            // hardware but provided no mechanism to supply correct values. The
            // current approach uses a runtime-configurable `KernelLayout` set
            // once during `VmServer::init()`, which:
            //   (1) removes the `compile_error!` guard (no feature gate needed),
            //   (2) makes the code correct by construction — the layout is
            //       explicitly supplied rather than assumed,
            //   (3) fails fast via `kernel_layout()`'s panic if `VmServer::init()`
            //       was not run, rather than silently mapping memory wrong.
            let layout = crate::global::kernel_layout();

            map_kernel(
                &mut pt,
                layout.kernel_text_vbase,
                layout.kernel_text_pbase,
                layout.kernel_text_pages,
                layout.kernel_data_pages,
                layout.dm_vbase,
                layout.dm_pages,
            )?;

            self.inner.vm_pt.write(pt);
            self.inner.vm_pt_initialized = true;
            Ok(())
        }
    }

    /// Binds the page table to the kernel for this process.
    ///
    /// Corresponds to Minix3's `pt_bind()`.
    /// Must be called after `init_page_table()` and before the process runs.
    pub(crate) fn bind_page_table(&self) -> Result<(), minix_arch::paging::PageTableError> {
        let pt = self.page_table();
        bind_to_process(pt.root_paddr(), self.endpoint())
    }

    /// Frees the page table resources and resets initialization flag.
    ///
    /// Used for rollback when fork fails after `init_page_table()` succeeds
    /// but before `sys_fork()`. Corresponds to Minix3's `pt_free(&vmc->vm_pt)`.
    ///
    /// # Safety
    /// Caller must ensure this process's page table is not currently active on any CPU.
    pub(crate) unsafe fn free_page_table(&mut self) {
        if self.inner.vm_pt_initialized {
            // SAFETY: vm_pt_initialized is true, so vm_pt was initialized by
            // init_page_table(). Caller guarantees page table is not active on any CPU.
            unsafe { self.inner.vm_pt.assume_init_mut().destroy(); }
            self.inner.vm_pt_initialized = false;
        }
    }

    /// Initializes memory regions map for exec or new processes.
    ///
    /// Creates a new empty region map. Must be called before accessing vm_regions.
    pub(crate) fn init_regions(&mut self) {
        self.inner.vm_regions.write(RegionMap::new());
        self.inner.vm_regions_initialized = true;
    }

    /// Returns reference to the page table.
    ///
    /// # Panics
    /// Panics in debug mode if vm_pt has not been initialized.
    #[inline]
    pub(crate) fn page_table(&self) -> &PageTable {
        debug_assert!(self.inner.vm_pt_initialized, "vm_pt accessed before init_page_table()");
        // SAFETY: vm_pt_initialized is true (checked by debug_assert above).
        // Single-threaded VM ensures no concurrent mutation.
        unsafe { self.inner.vm_pt.assume_init_ref() }
    }

    /// Returns mutable reference to the page table.
    ///
    /// # Panics
    /// Panics in debug mode if vm_pt has not been initialized.
    #[inline]
    pub(crate) fn page_table_mut(&mut self) -> &mut PageTable {
        debug_assert!(self.inner.vm_pt_initialized, "vm_pt accessed before init_page_table()");
        // SAFETY: vm_pt_initialized is true (checked by debug_assert above).
        // &mut self ensures exclusive access.
        unsafe { self.inner.vm_pt.assume_init_mut() }
    }

    /// Returns reference to the memory regions.
    ///
    /// # Panics
    /// Panics in debug mode if vm_regions has not been initialized.
    #[inline]
    pub(crate) fn regions(&self) -> &RegionMap {
        debug_assert!(self.inner.vm_regions_initialized, "vm_regions accessed before init_regions()");
        // SAFETY: vm_regions_initialized is true (checked by debug_assert above).
        // Single-threaded VM ensures no concurrent mutation.
        unsafe { self.inner.vm_regions.assume_init_ref() }
    }

    /// Reset usage statistics (C `reset_vm_rusage`, exit.c:25-31).
    ///
    /// Called on the VMPPARAM_CLEAR path — C `free_proc()` resets
    /// `vm_total`/`vm_total_max`/fault counters before `pt_new`.
    #[inline]
    pub(crate) fn reset_rusage(&mut self) {
        self.inner.reset_rusage();
    }

    /// Returns mutable reference to the memory regions.
    ///
    /// # Panics
    /// Panics in debug mode if vm_regions has not been initialized.
    #[inline]
    pub(crate) fn regions_mut(&mut self) -> &mut RegionMap {
        debug_assert!(self.inner.vm_regions_initialized, "vm_regions accessed before init_regions()");
        // SAFETY: vm_regions_initialized is true (checked by debug_assert above).
        // &mut self ensures exclusive access.
        unsafe { self.inner.vm_regions.assume_init_mut() }
    }

    #[allow(dead_code)] // V10-P2-1: no callers (region counts go via regions().len())
    pub(crate) fn region_count(&self) -> usize {
        self.regions().len()
    }

    /// Sets up CoW for all memory regions (PFN index model).
    ///
    /// Marks all regions as writable and prepares CoW (sets pages read-only
    /// in the page table when refcount > 1). Does NOT increment refcount —
    /// that was already done by `fork_region()` during region copying.
    ///
    /// # Safety
    /// Caller must ensure PageFrames is initialized and valid.
    pub(crate) unsafe fn setup_cow_for_all_regions(&mut self, frames: &mut crate::region::PageFrames) {
        use crate::region::VrFlags;

        for region in self.regions_mut().iter_mut() {
            region.flags.insert(VrFlags::WRITABLE);
            region.prepare_cow(frames);
        }
    }

    /// Writes all physical mappings into the page table (PFN index model).
    ///
    /// Uses PageFrames + PageSlot instead of PhysBlock.
    /// Iterates all regions and their PageSlots, writing
    /// virtual-to-physical mappings into the hardware page table.
    ///
    /// # Safety
    /// Caller must ensure page table is initialized and valid.
    pub(crate) unsafe fn write_page_table_mappings(
        &mut self,
        frames: &crate::region::PageFrames,
    ) -> Result<(), minix_arch::paging::PageTableError> {
        use minix_arch::paging::PageFlags;
        use minix_types::{PhysBytes, VirBytes};

        const PAGE_SIZE: u64 = <PageTable as Paging>::PAGE_SIZE as u64;

        let mut mappings: alloc::vec::Vec<(VirBytes, PhysBytes, PageFlags)> = alloc::vec::Vec::new();

        for region in self.regions_mut().iter_mut() {
            for (i, slot) in region.physblocks.iter().enumerate() {
                if let Some(pfn) = slot.pfn() {
                    let vaddr = VirBytes(region.vaddr.0 + i as u64 * PAGE_SIZE);
                    let paddr = frames.pfn_to_phys(pfn);

                    let writable = region.is_writable()
                        && frames.get(pfn)
                            .map(|s| s.refcount == 1)
                            .unwrap_or(false);
                    let flags = if writable {
                        PageFlags::read_write()
                    } else {
                        PageFlags::read_only()
                    };

                    mappings.push((vaddr, paddr, flags));
                }
            }
        }

        let pt = self.page_table_mut();
        for (vaddr, paddr, flags) in mappings {
            pt.map(vaddr, paddr, flags)?;
        }
        Ok(())
    }

    /// Adds a memory region to the process's region map.
    ///
    /// Creates an empty region (no physical pages mapped). Physical pages
    /// are added later via `write_page_table_mappings()` or `map_page_at()`.
    /// The page table is NOT modified here — this matches Minix3's behavior
    /// where region insertion and page table mapping are separate steps.
    ///
    /// Returns `Err(VmError::InvalidParam)` if the region overlaps an existing region.
    /// Corresponds to Minix3's region insertion in `vm_fork()` / `do_mmap()`.
    #[allow(dead_code)] // V10-P2-1: dispatcher inserts via regions_mut() instead
    pub(crate) fn add_region(&mut self, start: VirBytes, len: VirBytes) -> Result<(), VmError> {
        use crate::region::{VirRegion, VrFlags};
        let end = VirBytes(start.0 + len.0);
        if self.regions().find_overlap(start, end).is_some() {
            return Err(VmError::InvalidParam);
        }
        let region = VirRegion::new(start, len, VrFlags::empty());
        match self.regions_mut().insert(region) {
            Ok(None) => Ok(()),
            Ok(Some(_)) => Err(VmError::InvalidParam),
            Err(_) => Err(VmError::InvalidParam),
        }
    }

    /// Removes a memory region starting at the given address.
    ///
    /// Unmaps all pages in the region from the page table before removing
    /// from the region map. This prevents stale page table entries that
    /// would allow access to freed physical memory.
    ///
    /// Corresponds to Minix3's region removal in `do_munmap()` / `vm_exit()`,
    /// where `pt_writemap(vaddr, MAP_NONE, ...)` is called before removing
    /// the region from the AVL tree.
    #[allow(dead_code)] // V10-P2-1: munmap/exit remove via regions_mut() instead
    pub(crate) fn remove_region(&mut self, start: VirBytes) -> Result<(), VmError> {
        // Find the region first to get its length for page table unmap.
        let region_len = self.regions()
            .find(start)
            .map(|r| r.length)
            .ok_or(VmError::NotFound)?;

        // Unmap all pages in the region from the page table.
        // C: pt_writemap(vaddr, MAP_NONE, ...) — pagetable.c
        const PAGE_SIZE: u64 = <PageTable as Paging>::PAGE_SIZE as u64;
        let num_pages = (region_len.0 / PAGE_SIZE) as usize;
        if num_pages > 0 {
            // Ignore unmap errors — the page may not be mapped (e.g., never
            // had physical backing), which is safe. C code also silently
            // ignores unmapped pages in pt_writemap with MAP_NONE.
            let _ = self.page_table_mut().unmap_range(start, num_pages);
        }

        // Remove from region map.
        if self.regions_mut().remove(start).is_none() {
            // Should not happen — we found it above.
            return Err(VmError::NotFound);
        }
        Ok(())
    }

    /// Swaps the content of two processes, preserving their endpoint and slot.
    ///
    /// Used for live update: old service and new service swap vmproc content,
    /// old service keeps its endpoint (clients still access via that endpoint),
    /// but gains new service's memory state (new code, new data).
    ///
    /// Corresponds to Minix3's `swap_proc_slot()` in `lib/libmisc/utility.c`.
    /// The C implementation uses a per-field `memcpy` of `struct vmproc`,
    /// which is bitwise-equivalent to a `core::ptr::swap` because `vmproc`
    /// has no self-referential pointers or heap-owned members.
    ///
    /// # Example
    /// ```ignore
    /// let mut old_service = table.get_active(old_slot)?;
    /// let mut new_service = table.get_active(new_slot)?;
    /// old_service.swap_proc_slot(&mut new_service);
    /// // old_service now has new_service's memory, but keeps original endpoint
    /// ```
    #[allow(dead_code)] // V10-P2-1 (DEFERRED): live-update path not wired
    pub(crate) fn swap_proc_slot(&mut self, other: &mut ActiveProc<'_>) {
        // The Rust borrow checker guarantees `&mut self` and `&mut other`
        // cannot alias a single `VmProc`. The typestate system further
        // requires that two ActiveProc views cannot reference the same slot
        // (the table's split-borrow API only yields one view per slot).
        // Belt-and-suspenders debug assertion catches misuse early.
        debug_assert_ne!(
            self.slot(), other.slot(),
            "swap_proc_slot: self and other must reference distinct slots"
        );

        // Snapshot the four fields whose identity must be preserved across
        // the swap (endpoint + slot for each side). All other fields will
        // be exchanged as part of the bitwise swap.
        let self_endpoint = self.inner.vm_endpoint;
        let self_slot = self.inner.vm_slot;
        let other_endpoint = other.inner.vm_endpoint;
        let other_slot = other.inner.vm_slot;

        // SAFETY: `core::ptr::swap` exchanges the bitwise contents of two
        // distinct `VmProc` slots in the global process table. Safety
        // rests on four invariants:
        //
        // 1. **Distinct raw pointers** — `self.inner` and `other.inner`
        //    point to distinct slots in the static `VM_PROC_TABLE` array.
        //    The borrow checker (`&mut self` and `&mut other`) plus the
        //    debug_assert above forbid aliasing.
        //
        // 2. **Bitwise swap safety** — every field of `VmProc` is safe
        //    to exchange by raw bit copy:
        //      - `vm_slot: UserSlot`, `vm_endpoint: Endpoint`,
        //        `vm_flags: VmFlags`, `vm_acl: AclState`,
        //        `vm_region_top / vm_total / vm_total_max: VirBytes`,
        //        `vm_*_page_fault: u64` — all are `Copy` types.
        //      - `vm_boot: Option<BootImage>` — `BootImage` is `Copy`
        //        per minix-types definition.
        //      - `vm_pt: MaybeUninit<PageTable>` — `PageTable` is a
        //        stack-allocated value with no CR3 binding at this
        //        point (the kernel has not yet loaded it; the previous
        //        owner has been unbound via typestate transitions). No
        //        heap pointers are invalidated by a bitwise move.
        //      - `vm_regions: MaybeUninit<RegionMap>` — `RegionMap` is
        //        a `BTreeMap<VirBytes, VirRegion>`; BTreeMap nodes are
        //        heap-owned but their internal pointers are relative
        //        to the `BTreeMap` value itself, so they move with
        //        the containing struct.
        //      - `vm_pt_initialized` / `vm_regions_initialized: bool` —
        //        trivially `Copy`.
        //
        // 3. **No concurrent access** — VM is single-threaded
        //    (documented in `lib.rs` module-level header).
        //
        // 4. **No hardware in-flight** — neither process's page table
        //    is loaded in CR3 at this point. Live update requires the
        //    caller to ensure both processes are quiescent before
        //    invoking swap.
        //
        // Minix3 reference: `swap_proc_slot()` in `lib/libmisc/utility.c`,
        // which uses `memcpy` to exchange fields; semantically equivalent
        // because `struct vmproc` has the same ownership profile as
        // our `VmProc` (no self-referential pointers).
        // SAFETY: See reasoning above — both pointers are valid, properly aligned,
        // no aliasing, no active CR3, single-threaded VM, no self-referential pointers.
        unsafe {
            core::ptr::swap(self.inner as *mut VmProc, other.inner as *mut VmProc);
        }

        // Restore endpoint/slot so each typestate view's identity matches
        // its original table position. This is the *purpose* of
        // `swap_proc_slot`: the old service keeps its endpoint (clients
        // still route to it) but gains the new service's memory state.
        self.inner.vm_endpoint = self_endpoint;
        self.inner.vm_slot = self_slot;
        other.inner.vm_endpoint = other_endpoint;
        other.inner.vm_slot = other_slot;
    }
}

/// Exiting process typestate view.
///
/// Process is in use and exiting. Can only be reaped (cleaned up)
/// to return the slot to empty state.
pub(crate) struct ExitingProc<'a> {
    inner: &'a mut VmProc,
}

impl<'a> ExitingProc<'a> {
    pub(crate) fn new(inner: &'a mut VmProc) -> Self {
        debug_assert!(inner.vm_flags.contains(VmFlags::IN_USE));
        debug_assert!(inner.vm_flags.contains(VmFlags::EXITING));
        Self { inner }
    }

    #[inline]
    #[allow(dead_code)] // V10-P2-1: exit path reads via reap() internals; accessors unused
    pub(crate) fn slot(&self) -> UserSlot {
        self.inner.vm_slot
    }

    #[inline]
    #[allow(dead_code)]
    pub(crate) fn endpoint(&self) -> Endpoint {
        self.inner.vm_endpoint
    }

    #[inline]
    #[allow(dead_code)]
    pub(crate) fn flags(&self) -> VmFlags {
        self.inner.vm_flags
    }

    #[inline]
    #[allow(dead_code)]
    pub(crate) fn regions(&self) -> &RegionMap {
        debug_assert!(self.inner.vm_regions_initialized, "vm_regions accessed after clear()");
        // SAFETY: vm_regions_initialized is true (checked by debug_assert above).
        // Single-threaded VM ensures no concurrent mutation.
        unsafe { self.inner.vm_regions.assume_init_ref() }
    }

    /// Returns mutable reference to the memory regions of an exiting process.
    ///
    /// Used by `exit::free_process_phys` to unreference pages and run
    /// per-region `ev_delete` before `reap()` clears the map.
    ///
    /// # Panics
    /// Panics in debug mode if vm_regions has not been initialized.
    #[inline]
    pub(crate) fn regions_mut(&mut self) -> &mut RegionMap {
        debug_assert!(self.inner.vm_regions_initialized, "vm_regions accessed before init_regions()");
        // SAFETY: vm_regions_initialized is true (checked by debug_assert above).
        // &mut self ensures exclusive access.
        unsafe { self.inner.vm_regions.assume_init_mut() }
    }

    /// Reaps the exiting process, releasing resources and returning the slot to empty.
    ///
    /// Calls `VmProc::clear()` to release resources, then returns `EmptySlot`.
    /// Corresponds to Minix3's `clear_proc()` followed by slot becoming free.
    ///
    /// # Safety
    /// Caller must ensure no other references to this process exist
    /// and the page table is no longer in use by hardware.
    pub(crate) unsafe fn reap(self) -> EmptySlot<'a> {
        // SAFETY: Caller guarantees no other references to this process exist
        // and the page table is no longer in use by hardware.
        unsafe { self.inner.clear(); }
        EmptySlot::new(self.inner)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vmproc::VmProcTable;

    fn get_active_vmproc(slot: UserSlot) -> ActiveProc<'static> {
        super::super::test_utils::get_active_vmproc(slot)
    }

    #[test]
    fn test_empty_slot_activate() {
        let slot = UserSlot::new(10);
        let active = get_active_vmproc(slot);

        assert!(active.flags().contains(VmFlags::IN_USE));
        assert_eq!(active.endpoint(), Endpoint::from_generation_slot(1, 10));
    }

    // ── (2026-06-13) — activate_relaxed debug_assert coverage ──
    //
    // These tests exercise the new debug-only validation in
    // `activate_relaxed`. The validation guards three misuse modes:
    //
    // 1. None endpoint: must be accepted silently (fork's child case).
    // 2. Matching endpoint: must be accepted silently (the strict path's
    //    invariant also holds in relaxed mode for backwards compatibility).
    // 3. VM_EXEC_TMP_SLOT endpoint: must be accepted silently (exec's
    //    legitimate exception).
    //
    // The negative case (endpoint with slot >= VM_PROC_COUNT, or
    // mismatched slot that is not VM_EXEC_TMP_SLOT) intentionally uses
    // `#[should_panic]` so the test harness catches the debug_assert
    // in debug builds; in release builds the panic won't fire and
    // the test will fail loudly, alerting anyone running tests with
    // release profile that the guard is disabled.

    use crate::vmproc::table::{VM_EXEC_TMP_SLOT, VM_PROC_COUNT};

    /// Wraps an EmptySlot for testing. Mirrors `fork.rs::do_fork` which
    /// obtains an empty slot and calls `activate_relaxed(Endpoint::NONE)`.
    fn get_empty_slot_for_test(slot: UserSlot) -> EmptySlot<'static> {
        use crate::vmproc::VmProcTable;
        let table = VmProcTable::get_global();
        unsafe { table.reset_slot(slot); }
        table.get_empty(slot).expect("slot must be empty after reset")
    }

    #[test]
    fn test_activate_relaxed_none_endpoint() {
        // Fork's child case: child endpoint is NONE initially.
        let slot = UserSlot::new(10);
        let empty = get_empty_slot_for_test(slot);
        // Must not panic in either debug or release builds.
        let active = empty.activate_relaxed(Endpoint::NONE);
        assert_eq!(active.endpoint(), Endpoint::NONE);
    }

    #[test]
    fn test_activate_relaxed_matching_endpoint() {
        // Strict path's invariant also holds in relaxed mode.
        let slot = UserSlot::new(11);
        let empty = get_empty_slot_for_test(slot);
        let ep = Endpoint::from_generation_slot(1, slot.get() as i32);
        // Must not panic — matching slot/endpoint is the canonical use.
        let active = empty.activate_relaxed(ep);
        assert_eq!(active.endpoint(), ep);
    }

    #[test]
    fn test_activate_relaxed_exec_tmp_slot() {
        // Exec-rewrite case: VM_EXEC_TMP_SLOT may hold a different endpoint.
        let tmp_slot = VM_EXEC_TMP_SLOT;
        let empty = get_empty_slot_for_test(tmp_slot);
        // Endpoint whose slot is something else (e.g., slot 5) but
        // passes because we are using VM_EXEC_TMP_SLOT — the exec
        // path's exception.
        let ep = Endpoint::from_generation_slot(1, 5);
        // Must not panic — exec-rewrite exception.
        let active = empty.activate_relaxed(ep);
        assert_eq!(active.endpoint(), ep);
    }

    #[test]
    #[should_panic(expected = "out-of-range endpoint")]
    fn test_activate_relaxed_panics_on_oor_slot() {
        // Endpoint with slot >= VM_PROC_COUNT must be rejected.
        let slot = UserSlot::new(12);
        let empty = get_empty_slot_for_test(slot);
        // Build an endpoint whose slot equals VM_PROC_COUNT (just past the
        // last valid index). This is the canonical "out of range" misuse.
        let oor_ep = Endpoint::from_generation_slot(1, VM_PROC_COUNT as i32);
        // This must panic in debug builds (release: the guard is
        // compiled out and the call proceeds, which is acceptable
        // because the existing release-build behavior was unguarded).
        let _ = empty.activate_relaxed(oor_ep);
    }

    #[test]
    #[should_panic(expected = "use activate() for strict pairing")]
    fn test_activate_relaxed_panics_on_mismatched_non_exec() {
        // Endpoint whose slot does NOT match self.slot() and is NOT
        // VM_EXEC_TMP_SLOT must be rejected. The classic "I called
        // activate_relaxed by mistake for a normal activation".
        let slot = UserSlot::new(13);
        let empty = get_empty_slot_for_test(slot);
        // Use a different in-range slot to trigger the mismatch.
        let wrong_ep = Endpoint::from_generation_slot(1, 14);
        // slot is 13, ep.slot() is 14, neither matches nor is
        // VM_EXEC_TMP_SLOT — must panic in debug.
        let _ = empty.activate_relaxed(wrong_ep);
    }

    #[test]
    fn test_active_proc_readonly() {
        let active = get_active_vmproc(UserSlot::new(11));

        assert_eq!(active.slot().get(), 11);
        assert_eq!(active.endpoint(), Endpoint::from_generation_slot(1, 11));
        assert!(active.flags().contains(VmFlags::IN_USE));
    }

    #[test]
    fn test_active_proc_write() {
        let mut active = get_active_vmproc(UserSlot::new(12));
        active.set_endpoint(Endpoint::RS);

        assert_eq!(active.endpoint(), Endpoint::RS);
    }

    #[test]
    fn test_active_proc_memory_tracking() {
        let mut active = get_active_vmproc(UserSlot::new(13));
        active.add_total(VirBytes(1024));
        assert_eq!(active.total().0, 1024);
        assert_eq!(active.total_max().0, 1024);

        active.add_total(VirBytes(512));
        assert_eq!(active.total().0, 1536);
        assert_eq!(active.total_max().0, 1536);

        active.sub_total(VirBytes(500));
        assert_eq!(active.total().0, 1036);
    }

    #[test]
    fn test_mark_exiting() {
        let active = get_active_vmproc(UserSlot::new(14));
        let exiting = active.mark_exiting();

        assert!(exiting.flags().contains(VmFlags::EXITING));
        assert!(exiting.flags().contains(VmFlags::IN_USE));
    }

    #[test]
    fn test_exiting_proc_reap() {
        let active = get_active_vmproc(UserSlot::new(15));
        let exiting = active.mark_exiting();

        let empty = unsafe { exiting.reap() };
        assert_eq!(empty.slot(), UserSlot::new(15));

        let inner = &mut *empty.inner;
        assert!(!inner.vm_flags.contains(VmFlags::IN_USE));
        assert!(!inner.vm_flags.contains(VmFlags::EXITING));
        assert!(inner.vm_endpoint.is_none());
    }

    #[test]
    fn test_force_clear() {
        let active = get_active_vmproc(UserSlot::new(16));

        let empty = unsafe { active.force_clear() };
        assert_eq!(empty.slot(), UserSlot::new(16));

        let inner = &mut *empty.inner;
        assert!(!inner.vm_flags.contains(VmFlags::IN_USE));
    }

    #[test]
    fn test_page_fault_counters() {
        let mut active = get_active_vmproc(UserSlot::new(17));

        assert_eq!(active.minor_fault(), 0);
        assert_eq!(active.major_fault(), 0);

        active.inc_minor_fault();
        active.inc_minor_fault();
        active.inc_major_fault();

        assert_eq!(active.minor_fault(), 2);
        assert_eq!(active.major_fault(), 1);
    }

    #[test]
    fn test_full_lifecycle() {
        let slot = UserSlot::new(18);
        let table = VmProcTable::get_global();
        unsafe { table.reset_slot(slot); }
        let empty = table.get_empty(slot).unwrap();
        let ep1 = Endpoint::from_generation_slot(1, 18);
        let active = empty.activate(ep1);
        let exiting = active.mark_exiting();

        let empty = unsafe { exiting.reap() };

        let ep2 = Endpoint::from_generation_slot(2, 18);
        let active = empty.activate(ep2);
        assert_eq!(active.endpoint(), ep2);

        let empty = unsafe { active.force_clear() };
        assert_eq!(empty.slot(), slot);
    }

    // ── (2026-06-14) — swap_proc_slot SAFETY coverage ──
    //
    // `swap_proc_slot` is the load-bearing primitive for live update:
    // it exchanges the contents of two VmProc slots while preserving
    // their slot/endpoint identity. This test verifies:
    // 1. Endpoint/slot identity is preserved across the swap.
    // 2. Non-identity fields (vm_total) actually flow across the swap.
    //
    // The `debug_assert_ne!(self.slot(), other.slot())` guard inside
    // `swap_proc_slot` is defensive — the public typestate API
    // (`VmProcTable::get_active`) returns at most one `&mut VmProc`
    // per slot, so the Rust borrow checker already prevents aliasing
    // at compile time. The debug_assert catches misuse only if a
    // future caller manually constructs aliased `&mut ActiveProc`
    // (e.g., via `&mut *ptr` casts). Such a misuse is fundamentally
    // UB in Rust, so we cannot write a test for it without invoking
    // UB itself; the guard is therefore not testable from safe code.

    #[test]
    fn test_swap_proc_slot_preserves_identities() {
        let mut a = get_active_vmproc(UserSlot::new(20));
        let mut b = get_active_vmproc(UserSlot::new(21));

        // Snapshot the identities that must be preserved.
        let ep_a = a.endpoint();
        let ep_b = b.endpoint();
        let slot_a = a.slot();
        let slot_b = b.slot();

        // Make b's non-identity state distinguishable so we can verify
        // it actually flows to a during the swap.
        b.add_total(VirBytes(4096));
        let b_total_before_swap = b.total().0;

        // Swap
        a.swap_proc_slot(&mut b);

        // Identities preserved
        assert_eq!(a.endpoint(), ep_a, "A's endpoint must be preserved");
        assert_eq!(a.slot(), slot_a, "A's slot must be preserved");
        assert_eq!(b.endpoint(), ep_b, "B's endpoint must be preserved");
        assert_eq!(b.slot(), slot_b, "B's slot must be preserved");

        // Non-identity state swapped: A now has B's total.
        assert_eq!(
            a.total().0, b_total_before_swap,
            "A now has B's memory accounting state"
        );
    }
}
