//! Message dispatcher for VM service.
//!
//! Routes decoded IPC messages (per-link `In` types) to handler functions
//! and returns unified `VmReply` enum for encoding back to transport layer.
//!
//! # Architecture
//!
//! ```
//! Message (transport) → VmXxxIn::decode() → dispatch_xxx() → VmReply → encode → Message
//! ```
//!
//! Supports: fork, brk, munmap, exit, willexit, pagefault, exec_newmem.

use minix_types::{
    Endpoint, UserSlot, VirBytes,
    VmForkIn, VmBrkIn, VmMunmapIn, VmExitIn, VmWillexitIn, VmPagefaultIn, VmExecNewmemIn,
    VmForkOut, VmBrkOut,
    VmReply, VmError,
};
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
    pub(crate) fn dispatch_with_alloc(
        table: &VmProcTable,
        page_alloc: &mut VmPageAllocator,
        frames: &mut PageFrames,
        request: VmForkIn,
    ) -> VmReply {
        Self::dispatch_fork(table, page_alloc, frames, request)
    }

    fn dispatch_fork(
        table: &VmProcTable,
        _page_alloc: &mut VmPageAllocator,
        frames: &mut PageFrames,
        request: VmForkIn,
    ) -> VmReply {
        match fork::do_fork(table, frames, request.parent_endpoint, request.child_slot) {
            Ok(child_endpoint) => VmReply::Fork(VmForkOut {
                child_endpoint,
            }),
            Err(fork::ForkError::InvalidEndpoint) => VmReply::Error(VmError::InvalidEndpoint),
            Err(fork::ForkError::InvalidSlot) => VmReply::Error(VmError::SlotInUse),
            Err(fork::ForkError::SlotInUse) => VmReply::Error(VmError::SlotInUse),
            Err(fork::ForkError::NoMemory) => VmReply::Error(VmError::OutOfMemory),
            Err(fork::ForkError::PageNotMapped) => VmReply::Error(VmError::PageNotMapped),
            Err(fork::ForkError::MemType(_)) => VmReply::Error(VmError::MemType),
        }
    }

    pub(crate) fn dispatch_brk(
        table: &VmProcTable,
        page_alloc: &mut VmPageAllocator,
        frames: &mut PageFrames,
        request: VmBrkIn,
    ) -> VmReply {
        let brk_req = brk::BrkRequest {
            endpoint: request.endpoint,
            new_brk_addr: request.new_addr,
        };

        match brk::handle_brk(table, page_alloc, frames, &brk_req) {
            Ok(response) => VmReply::Brk(VmBrkOut {
                new_addr: response.new_brk_addr,
            }),
            Err(e) => VmReply::Error(Self::brk_error_to_vm_error(e)),
        }
    }

    pub(crate) fn dispatch_munmap(
        table: &VmProcTable,
        page_alloc: &mut VmPageAllocator,
        frames: &mut PageFrames,
        request: VmMunmapIn,
    ) -> VmReply {
        let munmap_req = munmap::MunmapRequest {
            endpoint: request.endpoint,
            addr: request.addr,
            length: request.length,
        };

        match munmap::handle_munmap(table, page_alloc, frames, &munmap_req) {
            Ok(()) => VmReply::Munmap,
            Err(e) => VmReply::Error(Self::munmap_error_to_vm_error(e)),
        }
    }

    pub(crate) fn dispatch_exit(
        table: &VmProcTable,
        page_alloc: &mut VmPageAllocator,
        request: VmExitIn,
    ) -> VmReply {
        match exit::handle_vm_exit(table, page_alloc, request.endpoint) {
            Ok(()) => VmReply::Exit,
            Err(e) => VmReply::Error(Self::exit_error_to_vm_error(e)),
        }
    }

    pub(crate) fn dispatch_willexit(
        table: &VmProcTable,
        request: VmWillexitIn,
    ) -> VmReply {
        match exit::handle_vm_willexit(table, request.endpoint) {
            Ok(()) => VmReply::Willexit,
            Err(e) => VmReply::Error(Self::exit_error_to_vm_error(e)),
        }
    }

    pub(crate) fn dispatch_pagefault(
        table: &VmProcTable,
        page_alloc: &mut VmPageAllocator,
        frames: &mut PageFrames,
        request: VmPagefaultIn,
    ) -> VmReply {
        let _ = (table, page_alloc, frames, request);
        VmReply::Error(VmError::NotImplemented)
    }

    pub(crate) fn dispatch_exec_newmem(
        table: &VmProcTable,
        page_alloc: &mut VmPageAllocator,
        frames: &mut PageFrames,
        request: VmExecNewmemIn,
    ) -> VmReply {
        let _ = (table, page_alloc, frames, request);
        VmReply::Error(VmError::NotImplemented)
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
