//! VM process structure.

use super::VmFlags;
use crate::acl::AclState;
use crate::pagetable::PageTable;
use crate::region::RegionMap;
use core::mem::MaybeUninit;
use minix_arch::paging::Paging;
use minix_types::{BootImage, Endpoint, UserSlot, VirBytes};

/// VM process structure.
///
/// Design principles:
/// - All fields always exist (no Option)
/// - State expressed via flags
/// - Allows temporary inconsistency (e.g., during fork)
/// - `vm_pt` and `vm_regions` are MaybeUninit - only valid when IN_USE
///
/// Corresponds to Minix3's `struct vmproc`.
///
/// # Visibility Design
/// `VmProc` is NOT exported from the `vmproc` module. External code must use
/// typestate views (`EmptySlot`, `ActiveProc`, `ExitingProc`) to access
/// process data. Fields are `pub(crate)` for internal use within the vmproc
/// module tree.
#[derive(Debug)]
pub(crate) struct VmProc {
    pub(crate) vm_slot: UserSlot,
    pub(crate) vm_endpoint: Endpoint,
    pub(crate) vm_flags: VmFlags,
    pub(crate) vm_acl: AclState,
    /// Boot image info (only valid for boot-time processes).
    pub(crate) vm_boot: Option<BootImage>,

    /// Page table - uninitialized until `init_page_table()` is called.
    ///
    /// `MaybeUninit + bool` is the standard Rust pattern for delayed initialization
    /// (used by Vec, Box, ArrayVec, etc.). Use `vm_pt_initialized` as guard before
    /// calling `assume_init_mut()` / `assume_init_ref()`.
    pub(crate) vm_pt: MaybeUninit<PageTable>,
    /// Virtual memory regions map - uninitialized until `init_regions()` is called.
    ///
    /// Same `MaybeUninit + bool` pattern as `vm_pt`. Use `vm_regions_initialized`
    /// as guard before calling `assume_init_mut()` / `assume_init_ref()`.
    pub(crate) vm_regions: MaybeUninit<RegionMap>,
    /// Whether vm_pt has been initialized (must check before assume_init).
    pub(crate) vm_pt_initialized: bool,
    /// Whether vm_regions has been initialized (must check before assume_init).
    pub(crate) vm_regions_initialized: bool,
    pub(crate) vm_region_top: VirBytes,

    pub(crate) vm_total: VirBytes,
    pub(crate) vm_total_max: VirBytes,

    pub(crate) vm_minor_page_fault: u64,
    pub(crate) vm_major_page_fault: u64,

    /// Byte copy count (only when vmstats feature is enabled).
    #[cfg(feature = "vmstats")]
    pub(crate) vm_bytecopies: u64,
}

impl VmProc {
    /// Creates a vacant (unoccupied) process slot.
    ///
    /// Corresponds to Minix3's `memset(vmproc, 0, sizeof(vmproc))`.
    /// `vm_pt` and `vm_regions` are uninitialized - only access when IN_USE.
    pub(crate) const fn vacant() -> Self {
        Self {
            vm_slot: UserSlot(0),
            vm_endpoint: Endpoint::NONE,
            vm_flags: VmFlags::empty(),
            vm_acl: AclState::Uninitialized,
            vm_boot: None,
            vm_pt: MaybeUninit::uninit(),
            vm_regions: MaybeUninit::uninit(),
            vm_pt_initialized: false,
            vm_regions_initialized: false,
            vm_region_top: VirBytes::new(0),
            vm_total: VirBytes::new(0),
            vm_total_max: VirBytes::new(0),
            vm_minor_page_fault: 0,
            vm_major_page_fault: 0,
            #[cfg(feature = "vmstats")]
            vm_bytecopies: 0,
        }
    }

    /// Creates a vacant slot with the given slot number.
    ///
    /// This creates a vacant slot and sets the vm_slot field.
    /// The vm_pt and vm_regions remain uninitialized.
    pub(crate) const fn vacant_with_slot(vm_slot: UserSlot) -> Self {
        let mut proc = Self::vacant();
        proc.vm_slot = vm_slot;
        proc
    }

    /// Checks if the process is in use.
    #[inline]
    pub(crate) fn is_in_use(&self) -> bool {
        self.vm_flags.contains(VmFlags::IN_USE)
    }

    /// Checks if the process is exiting.
    #[inline]
    pub(crate) fn is_exiting(&self) -> bool {
        self.vm_flags.contains(VmFlags::EXITING)
    }

    /// Checks if this is a VM instance.
    #[inline]
    pub(crate) fn is_vm_instance(&self) -> bool {
        self.vm_flags.contains(VmFlags::VM_INSTANCE)
    }

    /// Resets usage statistics.
    ///
    /// Corresponds to Minix3's `reset_vm_rusage()` (exit.c:25-31), called by
    /// both `free_proc()` and `clear_proc()`. Shared by `VmProc::clear()` and
    /// the VMPPARAM_CLEAR path (`ActiveProc::reset_rusage`).
    pub(crate) fn reset_rusage(&mut self) {
        self.vm_total = VirBytes::new(0);
        self.vm_total_max = VirBytes::new(0);
        self.vm_minor_page_fault = 0;
        self.vm_major_page_fault = 0;
    }

    /// Debug invariant check (runtime assertion).
    #[cfg(debug_assertions)]
    pub(crate) fn check(&self) {
        if self.vm_flags.contains(VmFlags::IN_USE) {
            debug_assert!(
                !self.vm_endpoint.is_none(),
                "IN_USE but vm_endpoint is NONE"
            );
        }
    }

    /// Explicitly clears process resources.
    ///
    /// Core of explicit resource management - called from typestate transitions
    /// (`ExitingProc::reap()`, `ActiveProc::force_clear()`).
    /// Does not rely on Drop.
    ///
    /// Only clears `vm_pt` and `vm_regions` if they were previously initialized
    /// (tracked by `vm_pt_initialized` / `vm_regions_initialized` flags). This makes it
    /// safe to call on slots that were activated but never had vm_pt/vm_regions
    /// initialized (e.g., fork intermediate state).
    ///
    /// Corresponds to Minix3's `acl_clear()` + `free_proc()` + `clear_proc()`, combined:
    /// - Resets ACL state to `Uninitialized` (Minix3's `do_exit()` calls `acl_clear()` before
    ///   `clear_proc()`, we combine both into one operation)
    /// - Resets `vm_endpoint` to `NONE` and `vm_boot` to `None` (Minix3's `clear_proc()`
    ///   does not reset these, but Rust is more thorough)
    /// - Handles `VM_INSTANCE` flag counter: if `VM_INSTANCE` is set, decrements
    ///   the global counter (corresponds to Minix3's `do_exit()` handling before
    ///   calling `free_proc()` + `clear_proc()`)
    ///
    /// # Safety
    /// Caller must ensure:
    /// - This process's page table is not currently active on any CPU
    /// - The page table has been unbound from any process (typestate guarantees this
    ///   via `force_clear()` / `reap()` transitions)
    /// - All mappings have been properly unmapped, or caller accepts memory leak
    ///   (in `force_clear()` / `reap()`, regions are cleared first, so this is satisfied)
    pub(crate) unsafe fn clear(&mut self) {
        // SAFETY: Caller guarantees this process's page table is not active on any CPU
        // and has been unbound from any process. All mappings have been properly
        // unmapped by the caller (via typestate transitions: force_clear/reap).
        // vm_pt_initialized / vm_regions_initialized guards ensure we only call
        // assume_init_mut() on fields that were actually initialized.
        if self.vm_regions_initialized {
            // SAFETY: vm_regions_initialized is true, so vm_regions was previously
            // initialized by init_regions(). No concurrent access (single-threaded VM).
            unsafe {
                self.vm_regions.assume_init_mut().clear();
            }
        }
        if self.vm_pt_initialized {
            // SAFETY: vm_pt_initialized is true, so vm_pt was previously initialized
            // by init_page_table(). No concurrent access (single-threaded VM).
            // In test builds, skip destroy() since X86_64Paging::destroy() is todo!().
            #[cfg(not(test))]
            unsafe {
                self.vm_pt.assume_init_mut().destroy();
            }
        }

        if self.vm_flags.contains(VmFlags::VM_INSTANCE) {
            crate::global::dec_vm_instance();
        }

        self.vm_flags = VmFlags::empty();
        self.vm_endpoint = Endpoint::NONE;
        self.vm_boot = None;
        self.vm_acl = AclState::Uninitialized;
        self.vm_pt_initialized = false;
        self.vm_regions_initialized = false;

        self.vm_region_top = VirBytes::new(0);
        self.reset_rusage();

        #[cfg(feature = "vmstats")]
        {
            self.vm_bytecopies = 0;
        }
    }
}

impl Default for VmProc {
    fn default() -> Self {
        Self::vacant()
    }
}

/// VmProc is always in-place in the process table and must never be dropped.
///
/// # Design Note
/// In production, the process table is a static variable that never gets dropped
/// (program exits without calling destructors). Any Drop call indicates a bug
/// in process table management (e.g., moving a VmProc out of its slot).
///
/// In tests, vacant slots may be dropped during cleanup, which is acceptable.
/// Only dropping an IN_USE slot is a bug.
impl Drop for VmProc {
    fn drop(&mut self) {
        // Use debug_assert so release builds don't panic on drop.
        // In production, the static process table is never dropped.
        // If this fires in debug/test, it indicates a process table management bug.
        debug_assert!(
            !self.vm_flags.contains(VmFlags::IN_USE),
            "VmProc dropped while IN_USE — process table management bug. \
             Use in-place cleanup via VmProc::clear() or typestate transitions."
        );
        // Defensive cleanup: if an IN_USE slot somehow reaches drop in release mode,
        // clear it to prevent resource leaks. This is a safety net — the typestate
        // system should prevent this from ever happening.
        if self.vm_flags.contains(VmFlags::IN_USE) {
            // SAFETY: Defensive cleanup in release mode — the typestate system
            // should prevent this from ever happening. If an IN_USE slot reaches
            // drop, it is a bug, but we clear it to prevent resource leaks.
            // Single-threaded VM ensures no concurrent access.
            unsafe {
                self.clear();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vmproc::VmProcTable;

    /// Gets a mutable reference to a VmProc from the global table.
    /// The slot is reset to vacant state before returning.
    fn get_vmproc(vm_slot: UserSlot) -> &'static mut VmProc {
        let table = VmProcTable::get_global();
        unsafe {
            table.reset_slot(vm_slot);
            table.get_slot_mut(vm_slot).unwrap()
        }
    }

    #[test]
    fn test_vmproc_empty() {
        let slot = UserSlot::new(5);
        let proc = get_vmproc(slot);
        // Note: global table slots are initialized with vm_slot = 0,
        // actual slot number is set when activating via get_empty()
        assert!(proc.vm_endpoint.is_none());
        assert!(!proc.is_in_use());
        assert!(!proc.is_exiting());
    }

    #[test]
    fn test_vmproc_flags() {
        let proc = get_vmproc(UserSlot::new(1));
        assert!(!proc.is_in_use());

        proc.vm_flags |= VmFlags::IN_USE;
        assert!(proc.is_in_use());

        proc.vm_flags |= VmFlags::EXITING;
        assert!(proc.is_exiting());
    }

    #[test]
    fn test_vmproc_endpoint() {
        let proc = get_vmproc(UserSlot::new(2));
        proc.vm_endpoint = Endpoint::from_generation_slot(1, 2);
        proc.vm_flags |= VmFlags::IN_USE;

        assert!(proc.vm_endpoint.is_valid());
        assert!(proc.is_in_use());
    }

    #[test]
    fn test_slot_endpoint_consistency() {
        let proc = get_vmproc(UserSlot::new(5));
        proc.vm_endpoint = Endpoint::from_generation_slot(1, 5);
        proc.vm_flags |= VmFlags::IN_USE;

        assert!(proc.vm_slot.matches(proc.vm_endpoint));
    }

    #[test]
    fn test_slot_endpoint_inconsistency() {
        let proc = get_vmproc(UserSlot::new(6));
        proc.vm_endpoint = Endpoint::from_generation_slot(1, 3);
        proc.vm_flags |= VmFlags::IN_USE;

        assert!(!proc.vm_slot.matches(proc.vm_endpoint));
    }

    #[test]
    fn test_vmproc_memory_limit() {
        let proc = get_vmproc(UserSlot::new(7));
        proc.vm_total_max = VirBytes(1024 * 1024);
        proc.vm_total = VirBytes(512 * 1024);

        assert!(proc.vm_total.0 <= proc.vm_total_max.0);
    }

    #[test]
    fn test_vmproc_stats() {
        let proc = get_vmproc(UserSlot::new(8));
        assert_eq!(proc.vm_minor_page_fault, 0);
        assert_eq!(proc.vm_major_page_fault, 0);

        proc.vm_minor_page_fault += 1;
        proc.vm_major_page_fault += 1;

        assert_eq!(proc.vm_minor_page_fault, 1);
        assert_eq!(proc.vm_major_page_fault, 1);
    }

    #[cfg(feature = "vmstats")]
    #[test]
    fn test_vmproc_byte_copies() {
        let proc = get_vmproc(UserSlot::new(9));
        proc.vm_bytecopies = 1000;

        assert_eq!(proc.vm_bytecopies, 1000);
    }

    #[test]
    fn test_vacant_with_slot_preserves_slot() {
        // C: main.c:461 `vmproc[i].vm_slot = i` — the slot number is part
        // of the vacant state, not deferred until activation.
        let slot = UserSlot::new(42);
        let proc = VmProc::vacant_with_slot(slot);

        assert_eq!(proc.vm_slot, slot);
        assert!(proc.vm_endpoint.is_none());
        assert!(proc.vm_flags.is_empty());
        assert!(!proc.vm_pt_initialized);
        assert!(!proc.vm_regions_initialized);
    }

    #[test]
    fn test_clear_decrements_vm_instance_count() {
        // C: main.c:574-579 sets `num_vm_instances = 1` together with
        // `VMF_VM_INSTANCE`; exit.c:77-79 clears both on exit. `clear()`
        // owns the decrement so flag and counter stay in sync.
        let proc = get_vmproc(UserSlot::new(10));
        proc.vm_flags |= VmFlags::VM_INSTANCE;
        crate::global::inc_vm_instance();
        assert_eq!(crate::global::vm_instance_count(), 1);

        // SAFETY: Test-only slot, no page table bound to hardware,
        // no concurrent access (single-threaded test).
        unsafe {
            proc.clear();
        }

        assert!(!proc.vm_flags.contains(VmFlags::VM_INSTANCE));
        assert!(proc.vm_flags.is_empty());
        assert_eq!(crate::global::vm_instance_count(), 0);
    }
}
