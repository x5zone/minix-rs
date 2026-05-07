//! VM process table.
//!
//! Each slot is wrapped in `AssumeSyncCell<VmProc>` for independent access.
//! Memory is always initialized (like Minix3's `memset(vmproc, 0, ...)`).
//!
//! State transitions via typestate views:
//! ```text
//! EmptySlot ──[activate]──> ActiveProc ──[mark_exiting]──> ExitingProc ──[reap]──> EmptySlot
//!                           ActiveProc ──[force_clear]──────────────────────────> EmptySlot
//! ```

use minix_types::{AssumeSyncCell, Endpoint, NR_PROCS, UserSlot};
use super::{VmFlags, vmproc::VmProc, ActiveProc, ExitingProc, EmptySlot};

/// Error type for endpoint validation.
///
/// Corresponds to Minix3's `vm_isokendpt()` error distinction:
/// - `EINVAL`: slot index out of range (bad endpoint encoding)
/// - `EDEADEPT`: slot valid but endpoint mismatch or process not active
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum EndpointError {
    InvalidSlot,
    DeadEndpoint,
}

/// Number of VM process slots: all user processes + 1 exec temporary slot.
pub(crate) const VM_PROC_COUNT: usize = NR_PROCS + 1;

/// Exec temporary slot (index = NR_PROCS), used during exec to store old process state.
pub(crate) const VM_EXEC_TMP_SLOT: UserSlot = UserSlot(NR_PROCS);

/// VM process table.
///
/// Each slot is independently accessible via `AssumeSyncCell`, allowing
/// concurrent access to different slots. All slots are initialized with
/// zeroed `VmProc` objects at compile time. A slot is considered "free"
/// when its `flags` field is empty (no IN_USE flag set).
pub(crate) struct VmProcTable {
    slots: [AssumeSyncCell<VmProc>; VM_PROC_COUNT],
}

/// Global VM process table.
/// Slots are initialized at compile time with `vacant()` (like Minix3's `memset(vmproc, 0, ...)`).
/// Slot numbers are set when accessing via `get_empty()` or `alloc_empty_slot()` (like Minix3's `vm_slot = i`).
static VM_PROC_TABLE: VmProcTable = VmProcTable {
    slots: [const { AssumeSyncCell::new(VmProc::vacant()) }; VM_PROC_COUNT],
};

impl VmProcTable {
    /// Gets reference to the global process table.
    pub(crate) fn get_global() -> &'static VmProcTable {
        &VM_PROC_TABLE
    }

    /// Resets a slot to vacant state for testing.
    ///
    /// # Safety
    /// Caller must ensure no other references to this slot are active.
    /// This is intended for test cleanup only.
    #[cfg(test)]
    pub(crate) unsafe fn reset_slot(&self, slot: UserSlot) {
        if let Some(proc) = self.get_slot_mut(slot) {
            // Clear the slot first to avoid Drop panic on IN_USE processes
            if proc.vm_flags.contains(VmFlags::IN_USE) {
                proc.clear();
            }
            // Use ptr::write to avoid triggering Drop on the old value
            core::ptr::write(proc, VmProc::vacant_with_slot(slot));
        }
    }

    // ---- Internal Helpers ----

    /// Validates slot index and returns the usize index if valid.
    #[inline]
    const fn check_slot(slot: UserSlot) -> Option<usize> {
        let index = slot.get();
        if index >= VM_PROC_COUNT {
            return None;
        }
        Some(index)
    }

    /// Gets immutable reference to slot if index is valid.
    /// Returns `None` if slot index is out of bounds.
    ///
    /// # Safety
    /// Caller must ensure no mutable references to the same slot are active.
    /// Even in single-threaded contexts, aliasing a mutable reference is UB.
    unsafe fn get_slot(&self, slot: UserSlot) -> Option<&VmProc> {
        let index = Self::check_slot(slot)?;
        Some(unsafe { &*self.slots[index].get() })
    }

    /// Gets mutable reference to slot if index is valid.
    /// Returns `None` if slot index is out of bounds.
    ///
    /// # Safety
    /// Caller must ensure no other references (mutable or immutable) to the same slot are active.
    /// Even in single-threaded contexts, holding both `&T` and `&mut T` to the same data is UB.
    ///
    /// # Visibility
    /// `pub(super)` — only the vmproc module tree should access raw `&mut VmProc`.
    /// All other code must use typestate views (`EmptySlot`, `ActiveProc`, `ExitingProc`).
    #[allow(clippy::mut_from_ref)]
    pub(super) unsafe fn get_slot_mut(&self, slot: UserSlot) -> Option<&mut VmProc> {
        let index = Self::check_slot(slot)?;
        Some(unsafe { &mut *self.slots[index].get() })
    }

    // ---- Typestate View API ----

    /// Returns an `EmptySlot` view for the given slot if it's free.
    ///
    /// Use this to initialize a new process in the slot.
    /// Sets the vm_slot field to the given value (Minix3's `vm_slot = i`).
    pub(crate) fn get_empty(&self, slot: UserSlot) -> Option<EmptySlot<'_>> {
        let proc = unsafe { self.get_slot_mut(slot)? };
        if !proc.vm_flags.contains(VmFlags::IN_USE) {
            proc.vm_slot = slot;
            Some(EmptySlot::new(proc))
        } else {
            None
        }
    }

    /// Finds and returns the first free slot as an `EmptySlot`.
    ///
    /// The returned `EmptySlot` holds an exclusive mutable reference to the slot.
    /// Sets the vm_slot field to the found index (Minix3's `vm_slot = i`).
    ///
    /// This method is not atomic and assumes single-threaded execution.
    pub(crate) fn alloc_empty_slot(&self) -> Option<EmptySlot<'_>> {
        for i in 0..VM_PROC_COUNT {
            // SAFETY: We only access slot i, and the returned EmptySlot
            // holds an exclusive mutable reference preventing concurrent access.
            let proc = unsafe { self.get_slot_mut(UserSlot::new(i))? };
            if !proc.vm_flags.contains(VmFlags::IN_USE) {
                proc.vm_slot = UserSlot::new(i);
                return Some(EmptySlot::new(proc));
            }
        }
        None
    }

    /// Returns an `ActiveProc` view for the given slot.
    ///
    /// Returns `Some` only if the process is active (IN_USE and not EXITING).
    pub(crate) fn get_active(&self, slot: UserSlot) -> Option<ActiveProc<'_>> {
        let proc = unsafe { self.get_slot_mut(slot)? };
        let vm_flags = proc.vm_flags;
        if vm_flags.contains(VmFlags::IN_USE) && !vm_flags.contains(VmFlags::EXITING) {
            Some(ActiveProc::new(proc))
        } else {
            None
        }
    }

    /// Returns an `ExitingProc` view for the given slot.
    ///
    /// Returns `Some` only if the process is exiting (IN_USE and EXITING).
    pub(crate) fn get_exiting(&self, slot: UserSlot) -> Option<ExitingProc<'_>> {
        let proc = unsafe { self.get_slot_mut(slot)? };
        let vm_flags = proc.vm_flags;
        if vm_flags.contains(VmFlags::IN_USE) && vm_flags.contains(VmFlags::EXITING) {
            Some(ExitingProc::new(proc))
        } else {
            None
        }
    }

    // ---- Query API ----

    /// Checks if the slot is in use.
    #[inline]
    pub(crate) fn is_slot_in_use(&self, slot: UserSlot) -> bool {
        unsafe { self.get_slot(slot) }
            .map(|proc| proc.vm_flags.contains(VmFlags::IN_USE))
            .unwrap_or(false)
    }

    /// Finds a free slot index.
    ///
    /// Unlike `alloc_empty_slot()`, this returns only the slot index
    /// without an `EmptySlot` handle, so the slot might be taken
    /// by another operation before you use it.
    pub(crate) fn find_free_slot(&self) -> Option<UserSlot> {
        for i in 0..VM_PROC_COUNT {
            let proc = unsafe { &*self.slots[i].get() };
            if !proc.vm_flags.contains(VmFlags::IN_USE) {
                return Some(UserSlot::new(i));
            }
        }
        None
    }

    /// Returns the number of used slots.
    pub(crate) fn used_count(&self) -> usize {
        (0..VM_PROC_COUNT)
            .filter(|&i| {
                let proc = unsafe { &*self.slots[i].get() };
                proc.vm_flags.contains(VmFlags::IN_USE)
            })
            .count()
    }

    /// Returns the number of free slots.
    pub(crate) fn free_count(&self) -> usize {
        VM_PROC_COUNT - self.used_count()
    }

    /// Checks if the table is empty.
    pub(crate) fn is_empty(&self) -> bool {
        self.used_count() == 0
    }

    /// Checks if the table is full.
    pub(crate) fn is_full(&self) -> bool {
        self.find_free_slot().is_none()
    }

    /// Validates the endpoint and returns the corresponding slot index.
    ///
    /// Corresponds to Minix3's `vm_isokendpt()`.
    /// Returns `Ok(UserSlot)` if the endpoint is valid and the process is in use.
    ///
    /// Distinguishes `InvalidSlot` (Minix3's EINVAL: slot out of range)
    /// from `DeadEndpoint` (Minix3's EDEADEPT: endpoint mismatch or not active).
    /// This distinction is important for debugging and for callers that
    /// need to differentiate between "bad endpoint encoding" and "stale endpoint".
    pub(crate) fn vm_isokendpt(&self, endpoint: Endpoint) -> Result<UserSlot, EndpointError> {
        let vm_slot = endpoint.slot();
        if vm_slot < 0 || vm_slot as usize >= VM_PROC_COUNT {
            return Err(EndpointError::InvalidSlot);
        }
        let slot_idx = UserSlot(vm_slot as usize);
        let proc = unsafe { &*self.slots[slot_idx.get()].get() };

        if proc.vm_endpoint != endpoint {
            return Err(EndpointError::DeadEndpoint);
        }

        if !proc.vm_flags.contains(VmFlags::IN_USE) {
            return Err(EndpointError::DeadEndpoint);
        }

        Ok(slot_idx)
    }

    /// Iterates over all used processes immutably.
    ///
    /// Returns an iterator that yields `&VmProc` for each in-use slot.
    ///
    /// # Safety Warning
    ///
    /// This iterator returns raw `&VmProc` references, bypassing the typestate
    /// system. The returned `&VmProc` may contain uninitialized `MaybeUninit`
    /// fields (`vm_pt`, `vm_regions_avl`) if `vm_pt_initialized` /
    /// `vm_regions_avl_initialized` are false. Callers must NOT access these
    /// fields directly — use `ActiveProc::page_table()` / `regions()` which
    /// check initialization flags.
    ///
    /// # Limitations
    ///
    /// **Cannot modify during iteration**: The iterator holds `&VmProcTable`,
    /// blocking all typestate view operations (`get_empty()`, `get_active()`,
    /// etc.) for its entire lifetime. Collect what you need first, drop the
    /// iterator, then modify.
    ///
    /// # Visibility
    ///
    /// `pub(super)` — this iterator bypasses typestate and should only be
    /// used within the vmproc module tree. Prefer typestate views for all
    /// stateful operations.
    pub(super) fn iter(&self) -> VmProcIter<'_> {
        VmProcIter {
            table: self,
            index: 0,
        }
    }
}

/// Iterator over the process table.
///
/// Yields `&VmProc` for each slot that has the `IN_USE` flag set.
/// Holds a shared reference to the table, preventing mutable access
/// for the duration of the iterator's lifetime.
///
/// # Typestate Bypass
///
/// This iterator returns `&VmProc` directly, bypassing the typestate
/// system. It is restricted to `pub(super)` to prevent external code
/// from accessing process data without typestate enforcement.
pub(super) struct VmProcIter<'a> {
    table: &'a VmProcTable,
    index: usize,
}

impl<'a> Iterator for VmProcIter<'a> {
    type Item = &'a VmProc;

    fn next(&mut self) -> Option<Self::Item> {
        while self.index < VM_PROC_COUNT {
            let i = self.index;
            self.index += 1;
            let proc = unsafe { &*self.table.slots[i].get() };
            if proc.vm_flags.contains(VmFlags::IN_USE) {
                return Some(proc);
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_table_empty() {
        let table = VmProcTable::get_global();
        unsafe { table.reset_slot(UserSlot::new(0)); }
        // After reset, slot 0 should be empty
        assert!(!table.is_slot_in_use(UserSlot::new(0)));
    }

    #[test]
    fn test_typestate_activate() {
        let table = VmProcTable::get_global();
        unsafe { table.reset_slot(UserSlot::new(0)); }

        let empty = table.get_empty(UserSlot::new(0)).unwrap();
        assert_eq!(empty.slot(), UserSlot::new(0));

        let ep = Endpoint::from_generation_slot(1, 0);
        let mut active = empty.activate(ep);
        assert!(active.flags().contains(VmFlags::IN_USE));
        assert_eq!(active.endpoint(), ep);

        active.set_total_max(minix_types::VirBytes(1024));
        assert_eq!(active.total_max().0, 1024);
    }

    #[test]
    fn test_typestate_lifecycle() {
        let table = VmProcTable::get_global();
        unsafe { table.reset_slot(UserSlot::new(0)); }

        let empty = table.get_empty(UserSlot::new(0)).unwrap();
        let ep = Endpoint::from_generation_slot(1, 0);
        let active = empty.activate(ep);

        let exiting = active.mark_exiting();
        assert!(exiting.flags().contains(VmFlags::EXITING));

        let empty = unsafe { exiting.reap() };
        assert_eq!(empty.slot(), UserSlot::new(0));

        assert!(!table.is_slot_in_use(UserSlot::new(0)));
    }

    #[test]
    fn test_as_active_returns_none_for_empty() {
        let table = VmProcTable::get_global();
        unsafe { table.reset_slot(UserSlot::new(1)); }
        assert!(table.get_active(UserSlot::new(1)).is_none());
    }

    #[test]
    fn test_as_active_returns_none_for_exiting() {
        let table = VmProcTable::get_global();
        unsafe { table.reset_slot(UserSlot::new(2)); }

        let empty = table.get_empty(UserSlot::new(2)).unwrap();
        let ep = Endpoint::from_generation_slot(1, 2);
        let active = empty.activate(ep);
        let _exiting = active.mark_exiting();

        assert!(table.get_active(UserSlot::new(2)).is_none());
        assert!(table.get_exiting(UserSlot::new(2)).is_some());
    }

    #[test]
    fn test_as_empty_returns_none_for_active() {
        let table = VmProcTable::get_global();
        unsafe { table.reset_slot(UserSlot::new(3)); }

        let empty = table.get_empty(UserSlot::new(3)).unwrap();
        let ep = Endpoint::from_generation_slot(1, 3);
        let _active = empty.activate(ep);
        // Explicitly drop to release the mutable borrow before re-accessing table.
        // This demonstrates the borrow checker restriction in typestate pattern.
        drop(_active);

        assert!(table.get_empty(UserSlot::new(3)).is_none());
    }

    #[test]
    fn test_force_clear() {
        let table = VmProcTable::get_global();
        unsafe { table.reset_slot(UserSlot::new(4)); }

        let empty = table.get_empty(UserSlot::new(4)).unwrap();
        let ep = Endpoint::from_generation_slot(1, 4);
        let active = empty.activate(ep);

        let _empty = unsafe { active.force_clear() };
        drop(_empty);
        assert!(!table.is_slot_in_use(UserSlot::new(4)));
    }

    #[test]
    fn test_alloc_empty_slot() {
        let table = VmProcTable::get_global();
        unsafe {
            table.reset_slot(UserSlot::new(5));
            table.reset_slot(UserSlot::new(6));
        }

        let empty = table.alloc_empty_slot().unwrap();
        let slot = empty.slot();
        let ep = Endpoint::from_generation_slot(1, slot.get() as i32);
        let _active = empty.activate(ep);
        drop(_active);
        assert!(table.is_slot_in_use(slot));
    }

    #[test]
    fn test_vm_isokendpt_valid() {
        let table = VmProcTable::get_global();
        unsafe { table.reset_slot(UserSlot::new(6)); }

        let ep = Endpoint::from_generation_slot(1, 6);
        let empty = table.get_empty(UserSlot::new(6)).unwrap();
        let _active = empty.activate(ep);

        let result = table.vm_isokendpt(ep);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), UserSlot::new(6));
    }

    #[test]
    fn test_vm_isokendpt_mismatch() {
        let table = VmProcTable::get_global();
        unsafe { table.reset_slot(UserSlot::new(7)); }

        let ep = Endpoint::from_generation_slot(1, 7);
        let empty = table.get_empty(UserSlot::new(7)).unwrap();
        let _active = empty.activate(ep);

        let old_ep = Endpoint::from_generation_slot(0, 7);
        assert_eq!(table.vm_isokendpt(old_ep), Err(EndpointError::DeadEndpoint));
    }

    #[test]
    fn test_reap_and_reactivate() {
        let table = VmProcTable::get_global();
        unsafe { table.reset_slot(UserSlot::new(8)); }

        let empty = table.get_empty(UserSlot::new(8)).unwrap();
        let ep = Endpoint::from_generation_slot(1, 8);
        let active = empty.activate(ep);
        let exiting = active.mark_exiting();

        let empty = unsafe { exiting.reap() };
        drop(empty);
        assert!(!table.is_slot_in_use(UserSlot::new(8)));

        let empty = table.get_empty(UserSlot::new(8)).unwrap();
        let reactivate_ep = Endpoint::from_generation_slot(2, 8);
        let active = empty.activate(reactivate_ep);
        assert_eq!(active.endpoint(), reactivate_ep);
    }

    #[test]
    fn test_table_iter() {
        // Use high slot numbers to avoid conflicts with other tests
        // that use slots 0-28. We verify iteration works by counting
        // processes we create in a specific range.
        const BASE_SLOT: usize = 200;
        let table = VmProcTable::get_global();

        // Clean up our test slots first
        for i in 0..3 {
            unsafe {
                table.reset_slot(UserSlot::new(BASE_SLOT + i));
            }
        }

        // Create exactly 3 processes in high slot range
        for i in 0..3 {
            let empty = table.get_empty(UserSlot::new(BASE_SLOT + i)).unwrap();
            let _active = empty
                .activate(Endpoint::from_generation_slot(1, (BASE_SLOT + i) as i32));
        }

        // Count processes in our specific range
        let count = table
            .iter()
            .filter(|p| {
                let slot = p.vm_slot.get();
                slot >= BASE_SLOT && slot < BASE_SLOT + 3
            })
            .count();
        assert_eq!(count, 3);

        // Clean up
        for i in 0..3 {
            unsafe {
                table.reset_slot(UserSlot::new(BASE_SLOT + i));
            }
        }
    }

    #[test]
    fn test_get_global() {
        let table = VmProcTable::get_global();
        // The global table may not be empty if other tests ran before this one.
        // We just verify that we can get the global instance.
        assert_eq!(table as *const _, VmProcTable::get_global() as *const _);
    }
}
