//! VM exit handling.
//!
//! Handles VM_EXIT and VM_WILLEXIT requests from PM.
//! Releases all process resources: memory regions, physical pages, page tables.
//!
//! Corresponds to Minix3's `do_exit()` and `do_willexit()` in `exit.c`.

use minix_types::{Endpoint, EINVAL};
use crate::vmproc::VmProcTable;
use crate::region::{RegionMap, PageFrames, PageFlags, PFN_NONE, PfnAllocator};
use crate::alloc_page::VmPageAllocator;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum VmExitError {
    ProcessNotFound,
    NotExiting,
}

impl VmExitError {
    pub(crate) fn to_errno(&self) -> i32 {
        match self {
            Self::ProcessNotFound => EINVAL,
            Self::NotExiting => EINVAL,
        }
    }
}

/// Handle VM_EXIT — release process resources.
///
/// Corresponds to Minix3's `do_exit()` (exit.c:60).
/// Prerequisite: VM_WILLEXIT must have been called first (enforced by typestate).
pub(crate) fn handle_vm_exit(
    table: &VmProcTable,
    page_alloc: &mut VmPageAllocator,
    frames: &mut PageFrames,
    endpoint: Endpoint,
) -> Result<(), VmExitError> {
    let slot = table.vm_isokendpt(endpoint)
        .map_err(|_| VmExitError::ProcessNotFound)?;

    let exiting = table.get_exiting(slot)
        .ok_or(VmExitError::NotExiting)?;

    free_process_phys(exiting.regions(), frames, page_alloc);

    // SAFETY: Single-threaded VM ensures no concurrent access to this slot.
    // reap() restores the VmProc slot to vacant state (empty typestate).
    unsafe { exiting.reap(); }

    Ok(())
}

/// Handle VM_WILLEXIT — pre-notification that process will exit.
///
/// Corresponds to Minix3's `do_willexit()` (exit.c:100).
/// Sets EXITING flag so subsequent memory allocation requests are rejected.
pub(crate) fn handle_vm_willexit(
    table: &VmProcTable,
    endpoint: Endpoint,
) -> Result<(), VmExitError> {
    let slot = table.vm_isokendpt(endpoint)
        .map_err(|_| VmExitError::ProcessNotFound)?;

    let active = table.get_active(slot)
        .ok_or(VmExitError::ProcessNotFound)?;

    let _exiting = active.mark_exiting();

    Ok(())
}

/// Release physical pages for all regions in the exiting process.
///
/// Corresponds to Minix3's map_free_proc() → map_free() → map_subfree() →
/// pb_unreferenced() → ev_unreference() → free_mem() chain (region.c:589-602,
/// region.c:568-585, region.c:527-563, pb.c:96-133, mem_anon.c:56-62).
///
/// For each mapped PageSlot: decrements PageFrames refcount (equivalent to
/// Minix3's pb.refcount--), calls the MemType ev_unreference callback,
/// and conditionally frees the physical page when refcount reaches 0.
/// RegionMap::clear() (inside reap()) handles releasing the data structures.
///
/// NOTE: ev_unreference in the PFN model is a no-op for anonymous/direct
/// memory — the caller is responsible for both refcount decrement and
/// physical page freeing. This separation of concerns is documented in
/// §3.2 of 20-vm-exit.md.
fn free_process_phys(
    regions: &RegionMap,
    frames: &mut PageFrames,
    page_alloc: &mut VmPageAllocator,
) {
    for region in regions.iter() {
        for slot_opt in &region.physblocks {
            if let Some(slot) = slot_opt {
                if slot.pfn == PFN_NONE {
                    continue;
                }
                if let Some(mt) = slot.memtype {
                    mt.ev_unreference(frames, slot.pfn);
                }
                let should_free = if let Some(state) = frames.get_mut(slot.pfn) {
                    if state.refcount > 0 {
                        state.refcount -= 1;
                    }
                    state.refcount == 0 && !state.flags.contains(PageFlags::IN_CACHE)
                } else {
                    false
                };
                if should_free {
                    page_alloc.free_pfn(slot.pfn);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vmproc::VmProcTable;
    use minix_types::{Endpoint, UserSlot};
    use crate::phys_mem::{BitmapAllocator, PhysAlloc};
    use crate::region::PageFrames;
    use minix_types::PhysBytes;
    use crate::region::PAGE_SIZE as REGION_PAGE_SIZE;

    fn init_test_process(slot: UserSlot) -> Endpoint {
        let table = VmProcTable::get_global();
        // SAFETY: test-only cleanup. Single-threaded, no concurrent access.
        unsafe {
            table.reset_slot(slot);
        }
        let empty = table.get_empty(slot).unwrap();
        let ep = Endpoint::from_generation_slot(1, slot.get() as i32);
        let mut active = empty.activate(ep);
        active.init_page_table().unwrap();
        active.init_regions();
        ep
    }

    fn make_frames() -> PageFrames {
        PageFrames::new(PhysBytes(256 * REGION_PAGE_SIZE as u64))
    }

    fn make_page_alloc() -> VmPageAllocator {
        VmPageAllocator::new(PhysAlloc::Bitmap(BitmapAllocator::new_for_test(256)))
    }

    #[test]
    fn test_exit_error_to_errno() {
        assert_eq!(VmExitError::ProcessNotFound.to_errno(), EINVAL);
        assert_eq!(VmExitError::NotExiting.to_errno(), EINVAL);
    }

    #[test]
    fn test_exit_process_not_found() {
        let table = VmProcTable::get_global();
        let mut page_alloc = make_page_alloc();
        let mut frames = make_frames();

        let result = handle_vm_exit(table, &mut page_alloc, &mut frames, Endpoint::NONE);
        assert!(matches!(result, Err(VmExitError::ProcessNotFound)));
    }

    #[test]
    fn test_exit_without_willexit_fails() {
        let table = VmProcTable::get_global();
        let mut page_alloc = make_page_alloc();
        let mut frames = make_frames();
        let slot = UserSlot::new(50);
        let ep = init_test_process(slot);

        let result = handle_vm_exit(table, &mut page_alloc, &mut frames, ep);
        assert!(matches!(result, Err(VmExitError::NotExiting)));
    }

    #[test]
    fn test_willexit_then_exit_succeeds() {
        let table = VmProcTable::get_global();
        let mut page_alloc = make_page_alloc();
        let mut frames = make_frames();
        let slot = UserSlot::new(51);
        let ep = init_test_process(slot);

        handle_vm_willexit(table, ep).unwrap();
        let result = handle_vm_exit(table, &mut page_alloc, &mut frames, ep);
        assert!(result.is_ok());
    }

    #[test]
    fn test_exit_slot_reusable() {
        let table = VmProcTable::get_global();
        let mut page_alloc = make_page_alloc();
        let mut frames = make_frames();
        let slot = UserSlot::new(52);
        let ep = init_test_process(slot);

        handle_vm_willexit(table, ep).unwrap();
        handle_vm_exit(table, &mut page_alloc, &mut frames, ep).unwrap();

        let empty = table.get_empty(slot);
        assert!(empty.is_some(), "slot should be empty after exit");
    }
}
