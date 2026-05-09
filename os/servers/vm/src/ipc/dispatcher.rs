//! Message dispatcher for VM service.
//!
//! Routes incoming IPC messages to appropriate handlers.
//! Supports: fork, brk, munmap, exit, willexit, pagefault, exec_newmem.

use minix_types::{VmRequest, VmResponse, VmError, Endpoint, UserSlot, VirBytes};
use crate::vmproc::VmProcTable;
use crate::alloc_page::VmPageAllocator;
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
        request: VmRequest,
    ) -> VmResponse {
        match request {
            VmRequest::Fork { parent_endpoint, child_slot, child_endpoint } => {
                Self::handle_fork_request(table, parent_endpoint, child_slot, child_endpoint)
            }
            VmRequest::Brk { endpoint, new_addr } => {
                Self::handle_brk_request(table, page_alloc, endpoint, new_addr)
            }
            VmRequest::Munmap { endpoint, addr, length } => {
                Self::handle_munmap_request(table, page_alloc, endpoint, addr, length)
            }
            VmRequest::Exit { endpoint } => {
                Self::handle_exit_request(table, page_alloc, endpoint)
            }
            VmRequest::Willexit { endpoint } => {
                Self::handle_willexit_request(table, endpoint)
            }
            VmRequest::Pagefault { endpoint, vaddr, write } => {
                Self::handle_pagefault_request(table, page_alloc, endpoint, vaddr, write)
            }
            VmRequest::ExecNewmem { endpoint, text_addr, text_len, data_addr, data_len, pc } => {
                Self::handle_exec_newmem_request(table, page_alloc, endpoint, text_addr, text_len, data_addr, data_len, pc)
            }
        }
    }

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

    fn handle_brk_request(
        table: &VmProcTable,
        page_alloc: &mut VmPageAllocator,
        endpoint: Endpoint,
        new_addr: VirBytes,
    ) -> VmResponse {
        let request = brk::BrkRequest {
            endpoint,
            new_brk_addr: new_addr,
        };

        match brk::handle_brk(table, page_alloc, &request) {
            Ok(response) => VmResponse::BrkOk {
                new_addr: response.new_brk_addr,
            },
            Err(e) => VmResponse::Error(Self::brk_error_to_vm_error(e)),
        }
    }

    fn handle_munmap_request(
        table: &VmProcTable,
        page_alloc: &mut VmPageAllocator,
        endpoint: Endpoint,
        addr: VirBytes,
        length: VirBytes,
    ) -> VmResponse {
        let request = munmap::MunmapRequest {
            endpoint,
            addr,
            length,
        };

        match munmap::handle_munmap(table, page_alloc, &request) {
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
        endpoint: Endpoint,
        vaddr: VirBytes,
        write: bool,
    ) -> VmResponse {
        let fault = cow_exec_pf::PageFaultInfo {
            endpoint,
            vaddr,
            write,
        };

        match cow_exec_pf::handle_pagefault(table, page_alloc, &fault) {
            Ok(()) => VmResponse::PagefaultOk,
            Err(e) => VmResponse::Error(Self::pagefault_error_to_vm_error(e)),
        }
    }

    fn handle_exec_newmem_request(
        table: &VmProcTable,
        page_alloc: &mut VmPageAllocator,
        endpoint: Endpoint,
        text_addr: VirBytes,
        text_len: VirBytes,
        data_addr: VirBytes,
        data_len: VirBytes,
        pc: VirBytes,
    ) -> VmResponse {
        let request = cow_exec_pf::ExecNewmemRequest {
            endpoint,
            text_addr,
            text_len,
            data_addr,
            data_len,
            pc,
        };

        match cow_exec_pf::handle_exec_newmem(table, page_alloc, &request) {
            Ok(()) => VmResponse::ExecNewmemOk,
            Err(e) => VmResponse::Error(Self::exec_error_to_vm_error(e)),
        }
    }

    fn fork_error_to_vm_error(e: fork::VmForkError) -> VmError {
        match e {
            fork::VmForkError::ParentNotFound => VmError::InvalidEndpoint,
            fork::VmForkError::InvalidChildSlot => VmError::InvalidAddress,
            fork::VmForkError::ChildSlotNotEmpty => VmError::SlotInUse,
            fork::VmForkError::OutOfMemory => VmError::OutOfMemory,
            fork::VmForkError::InternalError => VmError::InternalError,
        }
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

    fn pagefault_error_to_vm_error(e: cow_exec_pf::PageFaultError) -> VmError {
        match e {
            cow_exec_pf::PageFaultError::ProcessNotFound => VmError::InvalidEndpoint,
            cow_exec_pf::PageFaultError::InvalidAddress => VmError::InvalidAddress,
            cow_exec_pf::PageFaultError::AccessViolation => VmError::AccessViolation,
            cow_exec_pf::PageFaultError::OutOfMemory => VmError::OutOfMemory,
            cow_exec_pf::PageFaultError::InternalError => VmError::InternalError,
        }
    }

    fn exec_error_to_vm_error(e: cow_exec_pf::ExecError) -> VmError {
        match e {
            cow_exec_pf::ExecError::ProcessNotFound => VmError::InvalidEndpoint,
            cow_exec_pf::ExecError::OutOfMemory => VmError::OutOfMemory,
            cow_exec_pf::ExecError::InternalError => VmError::InternalError,
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
            _ => {
                panic!("expected ForkOk, got unexpected response");
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

    #[test]
    fn test_dispatch_with_alloc_fork() {
        init_test_slots();
        let table = VmProcTable::get_global();
        let mut page_alloc = VmPageAllocator::new(PhysAlloc::Bitmap(BitmapAllocator::new_for_test(256)));

        let request = VmRequest::Fork {
            parent_endpoint: Endpoint::from_generation_slot(1, 50),
            child_slot: UserSlot::new(51),
            child_endpoint: Endpoint::from_generation_slot(1, 51),
        };

        let response = MessageDispatcher::dispatch_with_alloc(table, &mut page_alloc, request);

        match response {
            VmResponse::ForkOk { child_endpoint } => {
                assert_eq!(child_endpoint.slot(), 51);
            }
            VmResponse::Error(e) => {
                panic!("expected ForkOk, got error: {:?}", e);
            }
            _ => {
                panic!("expected ForkOk, got unexpected response");
            }
        }
    }

    #[test]
    fn test_dispatch_brk_process_not_found() {
        let table = VmProcTable::get_global();
        let mut page_alloc = VmPageAllocator::new(PhysAlloc::Bitmap(BitmapAllocator::new_for_test(256)));

        let request = VmRequest::Brk {
            endpoint: Endpoint::NONE,
            new_addr: VirBytes(0x5000_0000),
        };

        let response = MessageDispatcher::dispatch_with_alloc(table, &mut page_alloc, request);

        match response {
            VmResponse::Error(VmError::InvalidEndpoint) => {}
            _ => panic!("expected InvalidEndpoint error"),
        }
    }

    #[test]
    fn test_dispatch_munmap_zero_length() {
        let table = VmProcTable::get_global();
        let mut page_alloc = VmPageAllocator::new(PhysAlloc::Bitmap(BitmapAllocator::new_for_test(256)));

        let request = VmRequest::Munmap {
            endpoint: Endpoint::PM,
            addr: VirBytes(0x1000),
            length: VirBytes(0),
        };

        let response = MessageDispatcher::dispatch_with_alloc(table, &mut page_alloc, request);

        match response {
            VmResponse::Error(VmError::InvalidAddress) => {}
            _ => panic!("expected InvalidAddress error"),
        }
    }

    #[test]
    fn test_dispatch_exit_process_not_found() {
        let table = VmProcTable::get_global();
        let mut page_alloc = VmPageAllocator::new(PhysAlloc::Bitmap(BitmapAllocator::new_for_test(256)));

        let request = VmRequest::Exit {
            endpoint: Endpoint::NONE,
        };

        let response = MessageDispatcher::dispatch_with_alloc(table, &mut page_alloc, request);

        match response {
            VmResponse::Error(VmError::InvalidEndpoint) => {}
            _ => panic!("expected InvalidEndpoint error"),
        }
    }

    #[test]
    fn test_dispatch_pagefault_process_not_found() {
        let table = VmProcTable::get_global();
        let mut page_alloc = VmPageAllocator::new(PhysAlloc::Bitmap(BitmapAllocator::new_for_test(256)));

        let request = VmRequest::Pagefault {
            endpoint: Endpoint::NONE,
            vaddr: VirBytes(0x1000),
            write: false,
        };

        let response = MessageDispatcher::dispatch_with_alloc(table, &mut page_alloc, request);

        match response {
            VmResponse::Error(VmError::InvalidEndpoint) => {}
            _ => panic!("expected InvalidEndpoint error"),
        }
    }
}
