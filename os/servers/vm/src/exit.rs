//! VM exit handling.
//!
//! Handles VM_EXIT and VM_WILLEXIT requests from PM.
//! Releases all process resources: memory regions, physical pages, page tables.
//!
//! Corresponds to Minix3's `do_exit()` and `do_willexit()` in `exit.c`.

use minix_types::{Endpoint, UserSlot, VirBytes, EINVAL, ESRCH, EPERM, EIO};
use crate::vmproc::{VmProcTable, ActiveProc, ExitingProc, VmFlags};
use crate::region::{VirRegion, VrFlags, RegionAvl};
use crate::alloc_page::VmPageAllocator;
use crate::phys_mem::PhysBytes;
use crate::pagetable::{PageTable, Paging};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum VmExitError {
    ProcessNotFound,
    AlreadyExiting,
    InvalidState,
    InternalError,
}

impl VmExitError {
    pub(crate) fn to_errno(&self) -> i32 {
        match self {
            Self::ProcessNotFound => ESRCH,
            Self::AlreadyExiting => EINVAL,
            Self::InvalidState => EPERM,
            Self::InternalError => EIO,
        }
    }
}

pub(crate) fn handle_vm_exit(
    table: &VmProcTable,
    page_alloc: &mut VmPageAllocator,
    endpoint: Endpoint,
) -> Result<(), VmExitError> {
    let slot = table.vm_isokendpt(endpoint)
        .map_err(|_| VmExitError::ProcessNotFound)?;

    let mut active = table.get_active(slot)
        .ok_or(VmExitError::ProcessNotFound)?;

    if active.flags().contains(VmFlags::EXITING) {
        return Err(VmExitError::AlreadyExiting);
    }

    let region_top = active.region_top();
    let total = active.total();

    let regions: alloc::vec::Vec<VirRegion> = active.regions_mut().iter()
        .map(|r| {
            VirRegion::new(r.vaddr, r.length, r.flags)
        })
        .collect();

    let mut page_table = active.page_table_mut();

    for region in &regions {
        let page_count = (region.length.0 / 4096) as usize;
        for i in 0..page_count {
            let vaddr = VirBytes(region.vaddr.0 + (i as u64) * 4096);
            if let Ok(_) = page_table.unmap(vaddr) {}
        }
    }

    drop(page_table);

    let exiting = active.mark_exiting();

    free_process_regions(exiting, page_alloc, region_top, total);

    Ok(())
}

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

fn free_process_regions(
    mut exiting: ExitingProc<'_>,
    page_alloc: &mut VmPageAllocator,
    _region_top: VirBytes,
    _total: VirBytes,
) {
    unsafe {
        exiting.reap();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vmproc::VmProcTable;
    use minix_types::Endpoint;
    use crate::phys_mem::{BitmapAllocator, PhysAlloc};

    fn init_test_process(slot: UserSlot) -> Endpoint {
        let table = VmProcTable::get_global();
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

    #[test]
    fn test_exit_error_to_errno() {
        assert_eq!(VmExitError::ProcessNotFound.to_errno(), ESRCH);
        assert_eq!(VmExitError::AlreadyExiting.to_errno(), EINVAL);
        assert_eq!(VmExitError::InvalidState.to_errno(), EPERM);
    }

    #[test]
    fn test_exit_process_not_found() {
        let table = VmProcTable::get_global();
        let mut page_alloc = VmPageAllocator::new(PhysAlloc::Bitmap(BitmapAllocator::new_for_test(256)));

        let result = handle_vm_exit(table, &mut page_alloc, Endpoint::NONE);
        assert!(matches!(result, Err(VmExitError::ProcessNotFound)));
    }
}
