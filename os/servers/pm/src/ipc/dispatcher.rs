//! Message dispatcher for PM service.
//!
//! Routes incoming IPC messages to appropriate handlers.

use minix_types::{PmRequest, PmResponse, PmError, Endpoint};
use minix_types::{VfsRequest, VfsResponse, KernelRequest, KernelResponse};
use minix_types::{VmForkIn, VmForkOut, VmReply};
use crate::mproc::ProcTable;
use crate::fork;

/// Dispatches PM requests to appropriate handlers.
///
/// This is the central routing point for all PM IPC messages.
pub struct MessageDispatcher;

impl MessageDispatcher {
    /// Dispatches a PM request to the appropriate handler.
    ///
    /// # Arguments
    /// * `table` - Reference to the PM process table
    /// * `request` - The incoming PM request
    ///
    /// # Returns
    /// The response to send back to the caller.
    pub fn dispatch(table: &mut ProcTable, request: PmRequest) -> PmResponse {
        match request {
            PmRequest::Fork { caller } => {
                Self::handle_fork_request(table, caller)
            }
        }
    }

    /// Handles fork request from kernel.
    fn handle_fork_request(
        table: &mut ProcTable,
        parent_endpoint: Endpoint,
    ) -> PmResponse {
        match fork::handle_fork(table, parent_endpoint) {
            Ok(child_pid) => PmResponse::ForkParent { child_pid },
            Err(e) => PmResponse::Error(Self::fork_error_to_pm_error(e)),
        }
    }

    /// Converts fork-specific error to generic PM error.
    fn fork_error_to_pm_error(e: fork::ForkCoordError) -> PmError {
        match e {
            fork::ForkCoordError::NoProc => PmError::InvalidEndpoint,
            fork::ForkCoordError::NoMem => PmError::OutOfMemory,
            fork::ForkCoordError::InvalidEndpoint => PmError::InvalidEndpoint,
            fork::ForkCoordError::ProcTableFull => PmError::ProcTableFull,
            fork::ForkCoordError::SlotInUse => PmError::SlotInUse,
            fork::ForkCoordError::VmError => PmError::InternalError,
            fork::ForkCoordError::VfsError => PmError::InternalError,
            fork::ForkCoordError::KernelError => PmError::InternalError,
        }
    }
}

/// Sends a VM_FORK request to VM service.
///
/// This is a placeholder that will be implemented with actual IPC.
pub fn send_vm_fork(request: VmForkIn) -> Result<VmForkOut, fork::ForkCoordError> {
    // TODO: Implement actual IPC communication
    // For now, return success for testing
    Ok(VmForkOut {
        child_endpoint: Endpoint::from_generation_slot(1, 1),
    })
}

/// Sends a request to VFS service.
///
/// This is a placeholder that will be implemented with actual IPC.
pub fn send_vfs_request(_request: VfsRequest) -> Result<VfsResponse, fork::ForkCoordError> {
    // TODO: Implement actual IPC communication
    // For now, return success for testing
    Ok(VfsResponse::ForkOk)
}

/// Sends a request to Kernel.
///
/// This is a placeholder that will be implemented with actual IPC.
pub fn send_kernel_request(_request: KernelRequest) -> Result<KernelResponse, fork::ForkCoordError> {
    // TODO: Implement actual IPC communication
    // For now, return success for testing
    Ok(KernelResponse::ForkOk)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mproc::Lifecycle;

    #[test]
    fn test_dispatch_fork_success() {
        let mut table = ProcTable::new();

        // Initialize a parent process
        let parent_slot = 0;
        table.procs[parent_slot].identity.endpoint = Endpoint::from_generation_slot(1, 0);
        table.procs[parent_slot].identity.id.pid = 100;
        table.procs[parent_slot].state.lifecycle = Lifecycle::Running;

        let request = PmRequest::Fork {
            caller: Endpoint::from_generation_slot(1, 0),
        };

        let response = MessageDispatcher::dispatch(&mut table, request);

        match response {
            PmResponse::ForkParent { child_pid } => {
                assert!(child_pid > 0);
            }
            PmResponse::Error(e) => {
                panic!("expected ForkParent, got error: {:?}", e);
            }
            _ => panic!("unexpected response"),
        }
    }
}
