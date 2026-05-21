//! Message dispatcher for VM service.
//!
//! Routes incoming IPC messages to appropriate handlers.
//! Supports: fork, brk, munmap, exit, willexit, pagefault, exec_newmem.
//!
//! PFN index model: Updated to use PageFrames/PageSlot instead of PhysRegion.

use minix_types::{VmRequest, VmResponse, VmError, Endpoint, UserSlot, VirBytes};
use crate::vmproc::VmProcTable;
use crate::alloc_page::VmPageAllocator;
use crate::region::PageFrames;
use crate::fork;
use crate::brk;
use crate::munmap;
use crate::exit;
use crate::cow_exec_pf;

pub(crate) struct MessageDispatcher;

impl MessageDispatcher {
    pub(crate) fn dispatch(table: &VmProcTable, request: VmRequest) -> VmResponse {
        match request {
            VmRequest::Fork { parent_endpoint, child_slot, child_endpoint } => {
                Self::handle_fork_request(table, parent_endpoint, child_slot, child_endpoint)
            }
            _ => VmResponse::Error(VmError::NotImplemented),
        }
    }

    pub(crate) fn dispatch_with_alloc(
        table: &VmProcTable,
        page_alloc: &mut VmPageAllocator,
        frames: &mut PageFrames,
        request: VmRequest,
    ) -> VmResponse {
        match request {
            VmRequest::Fork { parent_endpoint, child_slot, child_endpoint } => {
                Self::handle_fork_request(table, parent_endpoint, child_slot, child_endpoint)
            }
            VmRequest::Brk { endpoint, new_addr } => {
                Self::handle_brk_request(table, page_alloc, frames, endpoint, new_addr)
            }
            VmRequest::Munmap { endpoint, addr, length } => {
                Self::handle_munmap_request(table, page_alloc, frames, endpoint, addr, length)
            }
            VmRequest::Exit { endpoint } => {
                Self::handle_exit_request(table, page_alloc, endpoint)
            }
            VmRequest::Willexit { endpoint } => {
                Self::handle_willexit_request(table, endpoint)
            }
            VmRequest::Pagefault { endpoint, vaddr, write } => {
                Self::handle_pagefault_request(table, page_alloc, frames, endpoint, vaddr, write)
            }
            VmRequest::ExecNewmem { endpoint, text_addr, text_len, data_addr, data_len, pc } => {
                let _ = (endpoint, text_addr, text_len, data_addr, data_len, pc);
                VmResponse::Error(VmError::NotImplemented)
            }
        }
    }

    fn handle_fork_request(
        table: &VmProcTable,
        parent_endpoint: Endpoint,
        child_slot: UserSlot,
        child_endpoint: Endpoint,
    ) -> VmResponse {
        let _ = (table, parent_endpoint, child_slot, child_endpoint);
        VmResponse::Error(VmError::NotImplemented)
    }

    fn handle_brk_request(
        table: &VmProcTable,
        page_alloc: &mut VmPageAllocator,
        frames: &mut PageFrames,
        endpoint: Endpoint,
        new_addr: VirBytes,
    ) -> VmResponse {
        let request = brk::BrkRequest {
            endpoint,
            new_brk_addr: new_addr,
        };

        match brk::handle_brk(table, page_alloc, frames, &request) {
            Ok(response) => VmResponse::BrkOk {
                new_addr: response.new_brk_addr,
            },
            Err(e) => VmResponse::Error(Self::brk_error_to_vm_error(e)),
        }
    }

    fn handle_munmap_request(
        table: &VmProcTable,
        page_alloc: &mut VmPageAllocator,
        frames: &mut PageFrames,
        endpoint: Endpoint,
        addr: VirBytes,
        length: VirBytes,
    ) -> VmResponse {
        let request = munmap::MunmapRequest {
            endpoint,
            addr,
            length,
        };

        match munmap::handle_munmap(table, page_alloc, frames, &request) {
            Ok(()) => VmResponse::MunmapOk,
            Err(e) => VmResponse::Error(Self::munmap_error_to_vm_error(e)),
        }
    }

    fn handle_exit_request(
        table: &VmProcTable,
        page_alloc: &mut VmPageAllocator,
        endpoint: Endpoint,
    ) -> VmResponse {
        match exit::handle_vm_exit(table, page_alloc, endpoint) {
            Ok(()) => VmResponse::ExitOk,
            Err(e) => VmResponse::Error(Self::exit_error_to_vm_error(e)),
        }
    }

    fn handle_willexit_request(
        table: &VmProcTable,
        endpoint: Endpoint,
    ) -> VmResponse {
        match exit::handle_vm_willexit(table, endpoint) {
            Ok(()) => VmResponse::WillexitOk,
            Err(e) => VmResponse::Error(Self::exit_error_to_vm_error(e)),
        }
    }

    fn handle_pagefault_request(
        table: &VmProcTable,
        page_alloc: &mut VmPageAllocator,
        frames: &mut PageFrames,
        endpoint: Endpoint,
        vaddr: VirBytes,
        write: bool,
    ) -> VmResponse {
        let _ = (table, page_alloc, frames, endpoint, vaddr, write);
        VmResponse::Error(VmError::NotImplemented)
    }

    fn brk_error_to_vm_error(e: brk::BrkError) -> VmError {
        match e {
            brk::BrkError::ProcessNotFound => VmError::InvalidEndpoint,
            brk::BrkError::InvalidAddress => VmError::InvalidAddress,
            brk::BrkError::OutOfMemory => VmError::OutOfMemory,
            brk::BrkError::AlreadyMapped => VmError::InvalidAddress,
            brk::BrkError::InternalError => VmError::InternalError,
        }
    }

    fn munmap_error_to_vm_error(e: munmap::MunmapError) -> VmError {
        match e {
            munmap::MunmapError::ProcessNotFound => VmError::InvalidEndpoint,
            munmap::MunmapError::InvalidAddress => VmError::InvalidAddress,
            munmap::MunmapError::InvalidLength => VmError::InvalidAddress,
            munmap::MunmapError::NotMapped => VmError::InvalidAddress,
            munmap::MunmapError::InternalError => VmError::InternalError,
        }
    }

    fn exit_error_to_vm_error(e: exit::VmExitError) -> VmError {
        match e {
            exit::VmExitError::ProcessNotFound => VmError::InvalidEndpoint,
            exit::VmExitError::AlreadyExiting => VmError::InternalError,
            exit::VmExitError::InvalidState => VmError::InternalError,
            exit::VmExitError::InternalError => VmError::InternalError,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use minix_types::Endpoint;
    use crate::phys_mem::{BitmapAllocator, PhysAlloc};

    fn init_test_slots() {
        let table = VmProcTable::get_global();
        unsafe {
            table.reset_slot(UserSlot::new(50));
            table.reset_slot(UserSlot::new(51));
        }
        let empty = table.get_empty(UserSlot::new(50)).unwrap();
        let _parent = empty.activate(Endpoint::from_generation_slot(1, 50));
    }
}
