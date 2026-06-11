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
use minix_arch::paging::{bind_to_process, map_kernel};
use super::{VmFlags, vmproc::VmProc};
use crate::pagetable::PageTable;
use crate::region::RegionMap;

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
    /// Does NOT verify endpoint/slot consistency. Use for special cases:
    /// - Fork: child endpoint is NONE initially, set by kernel after sys_fork()
    /// - Exec temporary slot: old endpoint may differ from temporary slot
    /// - Tests: using arbitrary endpoints without matching slot numbers
    ///
    /// Does NOT initialize vm_pt or vm_regions - caller must initialize
    /// separately based on the specific operation (fork/exec/new).
    ///
    /// In normal cases, use `activate()` which enforces consistency.
    pub(crate) fn activate_relaxed(self, endpoint: Endpoint) -> ActiveProc<'a> {
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
    pub(crate) fn flags(&self) -> VmFlags {
        self.inner.vm_flags
    }

    #[inline]
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
    pub(crate) fn acl(&self) -> crate::acl::AclState {
        self.inner.vm_acl
    }

    #[inline]
    pub(crate) fn set_acl(&mut self, acl: crate::acl::AclState) {
        self.inner.vm_acl = acl;
    }

    #[inline]
    pub(crate) fn acl_check(&self, call: u32) -> Result<(), minix_types::VmError> {
        self.inner.vm_acl.acl_check(self, call)
    }

    #[inline]
    pub(crate) fn set_endpoint(&mut self, endpoint: Endpoint) {
        self.inner.vm_endpoint = endpoint;
    }

    #[inline]
    pub(crate) fn set_total_max(&mut self, value: VirBytes) {
        self.inner.vm_total_max = value;
    }

    #[inline]
    pub(crate) fn set_boot(&mut self, boot: BootImage) {
        self.inner.vm_boot = Some(boot);
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
    pub(crate) fn set_total(&mut self, value: VirBytes) {
        self.inner.vm_total = value;
    }

    #[inline]
    pub(crate) fn inc_minor_fault(&mut self) {
        self.inner.vm_minor_page_fault += 1;
    }

    #[inline]
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
        let mut pt = <PageTable as Paging>::new()?;

        // Map kernel address space into this user process's page table.
        // Kernel layout constants — in real implementation, these come from
        // boot_info / linker symbols. For now, use mock layout.
        const KERNEL_TEXT_VBASE: u64 = 0xFFFF_FFFF_8000_0000;
        const KERNEL_TEXT_PBASE: u64 = 0x100_0000;
        const KERNEL_TEXT_PAGES: usize = 8;
        const KERNEL_DATA_PAGES: usize = 8;
        const DM_VBASE: u64 = 0xFFFF_8000_0000_0000;
        const DM_SENTINEL_PAGES: usize = 4;

        map_kernel(
            &mut pt,
            KERNEL_TEXT_VBASE,
            KERNEL_TEXT_PBASE,
            KERNEL_TEXT_PAGES,
            KERNEL_DATA_PAGES,
            DM_VBASE,
            DM_SENTINEL_PAGES,
        )?;

        self.inner.vm_pt.write(pt);
        self.inner.vm_pt_initialized = true;
        Ok(())
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

    /// Returns mutable reference to the memory regions.
    ///
    /// # Panics
    /// Panics in debug mode if vm_regions has not been initialized.
    #[inline]
    pub(crate) fn regions_mut(&mut self) -> &mut RegionMap {
        debug_assert!(self.inner.vm_regions_initialized, "vm_regions accessed before init_regions()");
        unsafe { self.inner.vm_regions.assume_init_mut() }
    }

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
    pub(crate) unsafe fn write_page_table_mappings(&mut self, frames: &crate::region::PageFrames) {
        use minix_arch::paging::PageFlags;
        use minix_types::{PhysBytes, VirBytes};

        const PAGE_SIZE: u64 = <PageTable as Paging>::PAGE_SIZE as u64;

        let mut mappings: alloc::vec::Vec<(VirBytes, PhysBytes, PageFlags)> = alloc::vec::Vec::new();

        for region in self.regions_mut().iter_mut() {
            for (i, slot_opt) in region.physblocks.iter().enumerate() {
                if let Some(slot) = slot_opt {
                    if slot.is_mapped() {
                        let vaddr = VirBytes(region.vaddr.0 + i as u64 * PAGE_SIZE);
                        let paddr = frames.pfn_to_phys(slot.pfn);

                        let writable = region.is_writable()
                            && frames.get(slot.pfn)
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
        }

        let pt = self.page_table_mut();
        for (vaddr, paddr, flags) in mappings {
            let _ = pt.map(vaddr, paddr, flags);
        }
    }

    /// Adds a memory region.
    ///
    /// TODO: Implement region insertion into the AVL tree.
    pub(crate) fn add_region(&mut self, _start: u64, _len: u64) -> Result<(), &'static str> {
        Ok(())
    }

    /// Removes a memory region.
    ///
    /// TODO: Implement region removal from the AVL tree.
    pub(crate) fn remove_region(&mut self, _start: u64) -> Result<(), &'static str> {
        Ok(())
    }

    /// Swaps the content of two processes, preserving their endpoint and slot.
    ///
    /// Used for live update: old service and new service swap vmproc content,
    /// old service keeps its endpoint (clients still access via that endpoint),
    /// but gains new service's memory state (new code, new data).
    ///
    /// Corresponds to Minix3's `swap_proc_slot()` (utility.c).
    ///
    /// # Example
    /// ```ignore
    /// let mut old_service = table.get_active(old_slot)?;
    /// let mut new_service = table.get_active(new_slot)?;
    /// old_service.swap_proc_slot(&mut new_service);
    /// // old_service now has new_service's memory, but keeps original endpoint
    /// ```
    pub(crate) fn swap_proc_slot(&mut self, other: &mut ActiveProc<'_>) {
        let self_endpoint = self.inner.vm_endpoint;
        let self_slot = self.inner.vm_slot;
        let other_endpoint = other.inner.vm_endpoint;
        let other_slot = other.inner.vm_slot;

        unsafe {
            core::ptr::swap(self.inner as *mut VmProc, other.inner as *mut VmProc);
        }

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
    pub(crate) fn slot(&self) -> UserSlot {
        self.inner.vm_slot
    }

    #[inline]
    pub(crate) fn endpoint(&self) -> Endpoint {
        self.inner.vm_endpoint
    }

    #[inline]
    pub(crate) fn flags(&self) -> VmFlags {
        self.inner.vm_flags
    }

    #[inline]
    pub(crate) fn regions(&self) -> &RegionMap {
        debug_assert!(self.inner.vm_regions_initialized, "vm_regions accessed after clear()");
        unsafe { self.inner.vm_regions.assume_init_ref() }
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
}
