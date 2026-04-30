//! VM process structure.

use core::mem::MaybeUninit;
use minix_types::{BootImage, Endpoint, UserSlot, VirBytes};
use minix_arch::paging::Paging;
use super::VmFlags;
use crate::acl::AclState;
use crate::region::RegionAvl;
use crate::pagetable::PageTable;

/// VM process structure.
///
/// Design principles:
/// - All fields always exist (no Option)
/// - State expressed via flags
/// - Allows temporary inconsistency (e.g., during fork)
/// - `vm_pt` and `vm_regions_avl` are MaybeUninit - only valid when IN_USE
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
    /// TODO: Evaluate replacing MaybeUninit+bool with a custom InPlaceOption<T>
    /// that provides safe in-place initialization/cleanup without move-out.
    pub(crate) vm_pt: MaybeUninit<PageTable>,
    /// Virtual memory regions AVL tree - uninitialized until `init_regions()` is called.
    /// TODO: Evaluate replacing MaybeUninit+bool with a custom InPlaceOption<T>
    /// that provides safe in-place initialization/cleanup without move-out.
    pub(crate) vm_regions_avl: MaybeUninit<RegionAvl>,
    /// Whether vm_pt has been initialized (must check before assume_init).
    pub(crate) vm_pt_initialized: bool,
    /// Whether vm_regions_avl has been initialized (must check before assume_init).
    pub(crate) vm_regions_avl_initialized: bool,
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
    /// `vm_pt` and `vm_regions_avl` are uninitialized - only access when IN_USE.
    pub(crate) const fn vacant() -> Self {
        Self {
            vm_slot: UserSlot(0),
            vm_endpoint: Endpoint::NONE,
            vm_flags: VmFlags::empty(),
            vm_acl: AclState::Uninitialized,
            vm_boot: None,
            vm_pt: MaybeUninit::uninit(),
            vm_regions_avl: MaybeUninit::uninit(),
            vm_pt_initialized: false,
            vm_regions_avl_initialized: false,
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
    /// The vm_pt and vm_regions_avl remain uninitialized.
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

    /// Debug invariant check (runtime assertion).
    #[cfg(debug_assertions)]
    pub(crate) fn check(&self) {
        if self.vm_flags.contains(VmFlags::IN_USE) {
            debug_assert!(!self.vm_endpoint.is_none(), "IN_USE but vm_endpoint is NONE");
        }
    }

    /// Explicitly clears process resources.
    ///
    /// Core of explicit resource management - called from typestate transitions
    /// (`ExitingProc::reap()`, `ActiveProc::force_clear()`).
    /// Does not rely on Drop.
    ///
    /// Only clears `vm_pt` and `vm_regions_avl` if they were previously initialized
    /// (tracked by `vm_pt_initialized` / `vm_regions_avl_initialized` flags). This makes it
    /// safe to call on slots that were activated but never had vm_pt/vm_regions_avl
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
    /// Caller must ensure this process's page table is no longer in use by hardware.
    pub(crate) unsafe fn clear(&mut self) {
        if self.vm_regions_avl_initialized {
            unsafe { self.vm_regions_avl.assume_init_mut().clear(); }
        }
        if self.vm_pt_initialized {
            unsafe { self.vm_pt.assume_init_mut().destroy(); }
        }

        if self.vm_flags.contains(VmFlags::VM_INSTANCE) {
            crate::global::dec_vm_instance();
        }

        self.vm_flags = VmFlags::empty();
        self.vm_endpoint = Endpoint::NONE;
        self.vm_boot = None;
        self.vm_acl = AclState::Uninitialized;
        self.vm_pt_initialized = false;
        self.vm_regions_avl_initialized = false;

        self.vm_region_top = VirBytes::new(0);
        self.vm_total = VirBytes::default();
        self.vm_total_max = VirBytes::default();
        self.vm_minor_page_fault = 0;
        self.vm_major_page_fault = 0;

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
        #[cfg(not(test))]
        {
            panic!(
                "VmProc should never be dropped in production — use in-place cleanup via clear()"
            );
        }

        #[cfg(test)]
        if self.vm_flags.contains(VmFlags::IN_USE) {
            panic!(
                "VmProc dropped while IN_USE — process table management bug. \
                 Use in-place cleanup via VmProc::clear() or typestate transitions."
            );
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
}
