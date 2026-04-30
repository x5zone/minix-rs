//! Message dispatcher for VM service.
//!
//! Routes incoming IPC messages to appropriate handlers.

use minix_types::{VmRequest, VmResponse, VmError, Endpoint, UserSlot};
use crate::vmproc::VmProcTable;
use crate::fork;

/// Dispatches VM requests to appropriate handlers.
///
/// This is the central routing point for all VM IPC messages.
pub(crate) struct MessageDispatcher;

impl MessageDispatcher {
    /// Dispatches a VM request to the appropriate handler.
    ///
    /// # Arguments
    /// * `table` - Reference to the VM process table
    /// * `request` - The incoming VM request
    ///
    /// # Returns
    /// The response to send back to the caller.
    pub(crate) fn dispatch(table: &VmProcTable, request: VmRequest) -> VmResponse {
        match request {
            VmRequest::Fork { parent_endpoint, child_slot, child_endpoint } => {
                Self::handle_fork_request(table, parent_endpoint, child_slot, child_endpoint)
            }
        }
    }

    /// Handles fork request from PM.
    fn handle_fork_request(
        table: &VmProcTable,
        parent_endpoint: Endpoint,
        child_slot: UserSlot,
        child_endpoint: Endpoint,
    ) -> VmResponse {
        let request = fork::VmForkRequest {
            parent_endpoint,
            child_slot,
        };

        match fork::handle_fork(table, &request, child_endpoint) {
            Ok(response) => VmResponse::ForkOk {
                child_endpoint: response.child_endpoint,
            },
            Err(e) => VmResponse::Error(Self::fork_error_to_vm_error(e)),
        }
    }

    /// Converts fork-specific error to generic VM error.
    fn fork_error_to_vm_error(e: fork::VmForkError) -> VmError {
        match e {
            fork::VmForkError::ParentNotFound => VmError::InvalidEndpoint,
            fork::VmForkError::InvalidChildSlot => VmError::InvalidAddress,
            fork::VmForkError::ChildSlotNotEmpty => VmError::SlotInUse,
            fork::VmForkError::OutOfMemory => VmError::OutOfMemory,
            fork::VmForkError::InternalError => VmError::InternalError,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use minix_types::Endpoint;

    /// Initializes test slots in the global table.
    /// Slot 50 is set up as a parent process, slot 51 is reset.
    fn init_test_slots() {
        let table = VmProcTable::get_global();
        unsafe {
            table.reset_slot(UserSlot::new(50));
            table.reset_slot(UserSlot::new(51));
        }
        let empty = table.get_empty(UserSlot::new(50)).unwrap();
        let mut parent = empty.activate(Endpoint::from_generation_slot(1, 50));
        parent.init_page_table().unwrap();
        parent.init_regions();
    }

    #[test]
    fn test_dispatch_fork_success() {
        init_test_slots();
        let table = VmProcTable::get_global();

        let request = VmRequest::Fork {
            parent_endpoint: Endpoint::from_generation_slot(1, 50),
            child_slot: UserSlot::new(51),
            child_endpoint: Endpoint::from_generation_slot(1, 51),
        };

        let response = MessageDispatcher::dispatch(table, request);

        match response {
            VmResponse::ForkOk { child_endpoint } => {
                assert_eq!(child_endpoint.slot(), 51);
            }
            VmResponse::Error(e) => {
                panic!("expected ForkOk, got error: {:?}", e);
            }
        }
    }

    #[test]
    fn test_dispatch_fork_parent_not_found() {
        let table = VmProcTable::get_global();
        unsafe {
            table.reset_slot(UserSlot::new(50));
            table.reset_slot(UserSlot::new(51));
        }

        let request = VmRequest::Fork {
            parent_endpoint: Endpoint::NONE,
            child_slot: UserSlot::new(51),
            child_endpoint: Endpoint::from_generation_slot(1, 51),
        };

        let response = MessageDispatcher::dispatch(table, request);

        match response {
            VmResponse::Error(VmError::InvalidEndpoint) => {}
            _ => panic!("expected InvalidEndpoint error"),
        }
    }

    #[test]
    fn test_dispatch_fork_slot_in_use() {
        init_test_slots();
        let table = VmProcTable::get_global();

        // Occupy slot 51
        let empty = table.get_empty(UserSlot::new(51)).unwrap();
        let mut child = empty.activate(Endpoint::from_generation_slot(1, 51));
        child.init_page_table().unwrap();
        child.init_regions();

        let request = VmRequest::Fork {
            parent_endpoint: Endpoint::from_generation_slot(1, 50),
            child_slot: UserSlot::new(51),
            child_endpoint: Endpoint::from_generation_slot(2, 51),
        };

        let response = MessageDispatcher::dispatch(table, request);

        match response {
            VmResponse::Error(VmError::SlotInUse) => {}
            _ => panic!("expected SlotInUse error"),
        }
    }
}
