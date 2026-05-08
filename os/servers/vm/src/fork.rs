//! VM Fork handling.
//!
//! Handles VM_FORK requests from PM to duplicate process address space.
//!
//! Corresponds to Minix3's `do_fork()` in `fork.c`.

use minix_types::{Endpoint, UserSlot, VirBytes};
use crate::vmproc::{VmProcTable, VmFlags};
use crate::region::{VirRegion, VrFlags, PhysRegion, PhysBlock};
use alloc::boxed::Box;
use alloc::vec::Vec;

/// VM Fork request message from PM.
#[derive(Debug, Clone, Copy)]
pub(crate) struct VmForkRequest {
    pub(crate) parent_endpoint: Endpoint,
    pub(crate) child_slot: UserSlot,
}

/// VM Fork response message.
#[derive(Debug, Clone, Copy)]
pub(crate) struct VmForkResponse {
    pub(crate) child_endpoint: Endpoint,
    pub(crate) success: bool,
}

/// Fork error type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum VmForkError {
    ParentNotFound,
    InvalidChildSlot,
    ChildSlotNotEmpty,
    OutOfMemory,
    InternalError,
}

impl VmForkError {
    pub(crate) fn to_errno(&self) -> i32 {
        match self {
            Self::ParentNotFound => 3,      // ESRCH
            Self::InvalidChildSlot => 22,   // EINVAL
            Self::ChildSlotNotEmpty => 22,  // EINVAL
            Self::OutOfMemory => 12,        // ENOMEM
            Self::InternalError => 5,       // EIO
        }
    }
}

/// Fork context containing all info needed for the operation.
pub(crate) struct ForkContext<'a> {
    pub(crate) table: &'a VmProcTable,
    pub(crate) parent_index: usize,
    pub(crate) child_index: usize,
    pub(crate) child_endpoint: Endpoint,
}

impl<'a> ForkContext<'a> {
    pub(crate) fn new(
        table: &'a VmProcTable,
        parent_index: usize,
        child_index: usize,
        child_endpoint: Endpoint,
    ) -> Self {
        Self {
            table,
            parent_index,
            child_index,
            child_endpoint,
        }
    }

    /// Performs the fork operation.
    ///
    /// Corresponds to Minix3's `do_fork()` main logic.
    pub(crate) fn do_fork(&mut self) -> Result<Endpoint, VmForkError> {
        let parent_total;
        let parent_total_max;
        let parent_region_top;
        let parent_region_count;

        {
            let parent = self.table.get_active(UserSlot::new(self.parent_index))
                .ok_or(VmForkError::ParentNotFound)?;

            parent_total = parent.total();
            parent_total_max = parent.total_max();
            parent_region_top = parent.region_top();
            parent_region_count = parent.region_count();
        }

        let child_endpoint = self.child_endpoint;

        {
            let empty = self.table.get_empty(UserSlot::new(self.child_index))
                .ok_or(VmForkError::ChildSlotNotEmpty)?;

            let mut child = empty.activate(child_endpoint);
            child.init_page_table().map_err(|_| VmForkError::OutOfMemory)?;
            child.init_regions();
            child.init_from_fork(child_endpoint, parent_total, parent_total_max, parent_region_top);

            // Copy ACL from parent (corresponds to Minix3's acl_fork())
            let parent = self.table.get_active(UserSlot::new(self.parent_index))
                .ok_or(VmForkError::ParentNotFound)?;
            child.copy_acl_from(&parent);
        }

        self.copy_regions_with_cow(parent_region_count)?;

        // Bind child's page table to kernel (pt_bind equivalent)
        {
            let child = self.table.get_active(UserSlot::new(self.child_index))
                .ok_or(VmForkError::InvalidChildSlot)?;
            child.bind_page_table().map_err(|_| VmForkError::InternalError)?;
        }

        // Write page table mappings for both parent and child (map_writept equivalent)
        {
            let mut parent = self.table.get_active(UserSlot::new(self.parent_index))
                .ok_or(VmForkError::ParentNotFound)?;
            unsafe { parent.write_page_table_mappings(); }
        }
        {
            let mut child = self.table.get_active(UserSlot::new(self.child_index))
                .ok_or(VmForkError::InvalidChildSlot)?;
            unsafe { child.write_page_table_mappings(); }
        }

        Ok(child_endpoint)
    }

    /// Copies memory regions with CoW.
    ///
    /// In Minix3, child shares parent's physical pages during fork,
    /// implemented via reference counting. CoW triggers when either process writes.
    fn copy_regions_with_cow(&mut self, _region_count: usize) -> Result<(), VmForkError> {
        // Get parent regions first to avoid borrow issues
        let parent_regions: Vec<VirRegion> = {
            let parent = self.table.get_active(UserSlot::new(self.parent_index))
                .ok_or(VmForkError::ParentNotFound)?;
            
            // Clone all parent regions (this copies metadata but not physical blocks)
            parent.regions().iter().map(|r| clone_region_for_fork(r)).collect()
        };

        // Now add regions to child
        let mut child = self.table.get_active(UserSlot::new(self.child_index))
            .ok_or(VmForkError::InvalidChildSlot)?;

        for mut region in parent_regions {
            // Link physical blocks (share them, increase refcount)
            unsafe {
                link_phys_blocks(&mut region);
            }
            
            // Add region to child's AVL tree
            child.regions_mut().insert(region);
        }

        // Update page tables for both parent and child
        unsafe {
            child.setup_cow_for_all_regions();
        }

        Ok(())
    }
}

/// Clones a region for fork (copies metadata but not physical blocks).
///
/// Corresponds to Minix3's `map_copy_region()` logic.
fn clone_region_for_fork(original: &VirRegion) -> VirRegion {
    let mut new_region = VirRegion::new(original.vaddr, original.length, original.flags);
    new_region.def_memtype = original.def_memtype;
    new_region.remaps = original.remaps;
    new_region.id = original.id;
    new_region.param = original.param.clone();
    
    // Copy physical region pointers (but don't increase refcount yet)
    for (i, phys_opt) in original.physblocks.iter().enumerate() {
        if let Some(phys) = phys_opt {
            let offset = VirBytes((i as u64) * 4096);
            let mut new_phys = PhysRegion::new(offset);
            new_phys.ph = phys.ph;  // Share the same physical block
            new_phys.memtype = phys.memtype;
            new_region.physblocks[i] = Some(Box::new(new_phys));
        }
    }
    
    new_region
}

/// Links physical blocks by increasing their reference counts.
///
/// # Safety
/// Caller must ensure all PhysRegions have valid `ph` pointers.
unsafe fn link_phys_blocks(region: &mut VirRegion) {
    for phys_opt in region.physblocks.iter_mut() {
        if let Some(phys) = phys_opt.as_mut() {
            if let Some(block_ptr) = phys.ph {
                unsafe {
                    (*block_ptr.as_ptr()).add_ref();
                }
            }
        }
    }
}

/// Handles VM_FORK request.
///
/// Main entry point for VM service, called by PM.
pub(crate) fn handle_fork(
    table: &VmProcTable,
    request: &VmForkRequest,
    child_endpoint: Endpoint,
) -> Result<VmForkResponse, VmForkError> {
    let parent_slot = table.vm_isokendpt(request.parent_endpoint)
        .map_err(|_| VmForkError::ParentNotFound)?;

    let child_index = request.child_slot.get();

    let mut ctx = ForkContext::new(table, parent_slot.get(), child_index, child_endpoint);
    let child_endpoint = ctx.do_fork()?;

    Ok(VmForkResponse {
        child_endpoint,
        success: true,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Initializes test slots in the global table.
    /// Slot 40 is set up as a parent process, slot 41 is reset for child.
    fn init_test_slots() {
        let table = VmProcTable::get_global();
        unsafe {
            table.reset_slot(UserSlot::new(40));
            table.reset_slot(UserSlot::new(41));
        }
        let empty = table.get_empty(UserSlot::new(40)).unwrap();
        let mut parent = empty.activate(Endpoint::from_generation_slot(1, 40));
        parent.init_page_table().unwrap();
        parent.init_regions();
    }

    #[test]
    fn test_fork_request_creation() {
        let request = VmForkRequest {
            parent_endpoint: Endpoint::PM,
            child_slot: UserSlot::new(41),
        };

        assert_eq!(request.parent_endpoint, Endpoint::PM);
        assert_eq!(request.child_slot.get(), 41);
    }

    #[test]
    fn test_fork_response_creation() {
        let response = VmForkResponse {
            child_endpoint: Endpoint(100),
            success: true,
        };

        assert!(response.success);
    }

    #[test]
    fn test_fork_error_to_errno() {
        assert_eq!(VmForkError::ParentNotFound.to_errno(), 3);
        assert_eq!(VmForkError::OutOfMemory.to_errno(), 12);
        assert_eq!(VmForkError::InternalError.to_errno(), 5);
    }

    #[test]
    fn test_fork_context_creation() {
        init_test_slots();
        let table = VmProcTable::get_global();
        let ctx = ForkContext::new(
            table,
            40,
            41,
            Endpoint::from_generation_slot(1, 41),
        );

        assert_eq!(ctx.parent_index, 40);
        assert_eq!(ctx.child_index, 41);
    }

    #[test]
    fn test_handle_fork_parent_not_found() {
        init_test_slots();
        let table = VmProcTable::get_global();

        let request = VmForkRequest {
            parent_endpoint: Endpoint::NONE,
            child_slot: UserSlot::new(41),
        };

        let result = handle_fork(table, &request, Endpoint::from_generation_slot(1, 41));
        assert!(matches!(result, Err(VmForkError::ParentNotFound)));
    }

    #[test]
    fn test_handle_fork_success() {
        init_test_slots();
        let table = VmProcTable::get_global();

        let child_slot = UserSlot::new(41);

        let request = VmForkRequest {
            parent_endpoint: Endpoint::from_generation_slot(1, 40),
            child_slot,
        };

        let result = handle_fork(table, &request, Endpoint::from_generation_slot(1, 41));
        assert!(result.is_ok());

        let response = result.unwrap();
        assert!(response.success);
        assert_eq!(response.child_endpoint.slot(), 41);
    }
}
