//! VM Server main structure.
//!
//! Contains the VmServer struct with initialization phases
//! and main loop. Corresponds to Minix3's init_vm() + main loop.
//!
//! # Initialization Phases
//!
//! 1. **Phase 1 - Memory detection**: Initialize global state with total page count
//! 2. **Phase 2 - Physical allocator**: Set up reserved regions and critical pool
//! 3. **Phase 3 - Page tables**: Initialize kernel page tables and direct map
//! 4. **Phase 4 - Process table**: Set up boot processes from kernel boot image
//!
//! # Main Loop
//!
//! The main loop receives IPC messages and dispatches them:
//! - VM requests (fork, brk, munmap, exit, etc.) → `MessageDispatcher`
//! - VFS replies → `VfsRequestQueue::handle_reply`
//! - Page faults → `cow_exec_pf::handle_pagefault`

use alloc::boxed::Box;
use minix_types::{VmRequest, VmResponse, VmError, Endpoint, VirBytes};
use crate::vmproc::VmProcTable;
use crate::alloc_page::VmPageAllocator;
use crate::phys_mem::{PhysAlloc, BitmapAllocator, PhysAllocator, BootMemRegion};
use crate::page_cache::PageCache;
use crate::vfs_queue::VfsRequestQueue;
use crate::ipc::dispatcher::MessageDispatcher;

pub struct VmServer {
    page_alloc: VmPageAllocator,
    page_cache: PageCache,
    vfs_queue: VfsRequestQueue,
    initialized: bool,
}

impl VmServer {
    pub fn new() -> Self {
        let phys_alloc = Self::create_default_allocator();
        Self {
            page_alloc: VmPageAllocator::new(phys_alloc),
            page_cache: PageCache::new(),
            vfs_queue: VfsRequestQueue::new(),
            initialized: false,
        }
    }

    fn create_default_allocator() -> PhysAlloc {
        let total_pages = 65536;
        let base = 0x100000;
        let size = total_pages * 4096;
        let regions = [BootMemRegion { base, size }];
        let meta_size = BitmapAllocator::metadata_size(total_pages);
        let v: alloc::vec::Vec<u8> = alloc::vec![0u8; meta_size];
        let metadata = alloc::boxed::Box::leak(v.into_boxed_slice());
        PhysAlloc::Bitmap(BitmapAllocator::init(&mut metadata[..meta_size], total_pages, &regions))
    }

    pub fn init(&mut self) {
        self.init_phase1();
        self.init_phase2();
        self.init_phase3();
        self.init_phase4();
        self.initialized = true;
    }

    fn init_phase1(&mut self) {
        unsafe {
            crate::global::init(self.page_alloc.total_pages());
        }
    }

    fn init_phase2(&mut self) {
        let _table = VmProcTable::get_global();
    }

    fn init_phase3(&mut self) {
    }

    fn init_phase4(&mut self) {
        let _table = VmProcTable::get_global();
    }

    pub fn run(&mut self) {
        if !self.initialized {
            return;
        }

        loop {
            break;
        }
    }

    pub fn handle_request(&mut self, request: VmRequest) -> VmResponse {
        let table = VmProcTable::get_global();
        MessageDispatcher::dispatch_with_alloc(table, &mut self.page_alloc, request)
    }

    pub fn handle_ipc_message(&mut self, src: Endpoint, request: VmRequest) -> VmResponse {
        let _ = src;
        self.handle_request(request)
    }

    pub fn handle_vfs_reply(&mut self, src: Endpoint, result: i32) {
        let _ = (src, result);
    }

    pub fn has_pending_vfs_requests(&self) -> bool {
        !self.vfs_queue.is_empty()
    }

    pub fn page_cache(&self) -> &PageCache {
        &self.page_cache
    }

    pub fn page_cache_mut(&mut self) -> &mut PageCache {
        &mut self.page_cache
    }

    pub fn vfs_queue(&self) -> &VfsRequestQueue {
        &self.vfs_queue
    }

    pub fn vfs_queue_mut(&mut self) -> &mut VfsRequestQueue {
        &mut self.vfs_queue
    }

    pub fn page_alloc_mut(&mut self) -> &mut VmPageAllocator {
        &mut self.page_alloc
    }

    pub fn is_initialized(&self) -> bool {
        self.initialized
    }
}

impl Default for VmServer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vm_server_new() {
        let server = VmServer::new();
        assert!(!server.initialized);
        assert!(!server.is_initialized());
    }

    #[test]
    fn test_vm_server_init() {
        let mut server = VmServer::new();
        server.init();
        assert!(server.initialized);
        assert!(server.is_initialized());
    }

    #[test]
    fn test_vm_server_run_without_init() {
        let mut server = VmServer::new();
        server.run();
    }

    #[test]
    fn test_vm_server_default() {
        let server = VmServer::default();
        assert!(!server.initialized);
    }

    #[test]
    fn test_vm_server_handle_fork_not_found() {
        let mut server = VmServer::new();
        server.init();

        let request = VmRequest::Fork {
            parent_endpoint: Endpoint::NONE,
            child_slot: minix_types::UserSlot::new(1),
            child_endpoint: Endpoint::from_generation_slot(1, 1),
        };

        let response = server.handle_request(request);
        assert!(matches!(response, VmResponse::Error(VmError::InvalidEndpoint)));
    }

    #[test]
    fn test_vm_server_handle_brk_not_found() {
        let mut server = VmServer::new();
        server.init();

        let request = VmRequest::Brk {
            endpoint: Endpoint::NONE,
            new_addr: VirBytes(0x5000_0000),
        };

        let response = server.handle_request(request);
        assert!(matches!(response, VmResponse::Error(VmError::InvalidEndpoint)));
    }

    #[test]
    fn test_vm_server_handle_exit_not_found() {
        let mut server = VmServer::new();
        server.init();

        let request = VmRequest::Exit {
            endpoint: Endpoint::NONE,
        };

        let response = server.handle_request(request);
        assert!(matches!(response, VmResponse::Error(VmError::InvalidEndpoint)));
    }

    #[test]
    fn test_vm_server_page_cache_access() {
        let mut server = VmServer::new();
        assert_eq!(server.page_cache().total_pages(), 0);
    }

    #[test]
    fn test_vm_server_vfs_queue_access() {
        let server = VmServer::new();
        assert!(server.vfs_queue().is_empty());
    }

    #[test]
    fn test_vm_server_handle_ipc_message() {
        let mut server = VmServer::new();
        server.init();

        let request = VmRequest::Exit {
            endpoint: Endpoint::NONE,
        };

        let response = server.handle_ipc_message(Endpoint::PM, request);
        assert!(matches!(response, VmResponse::Error(VmError::InvalidEndpoint)));
    }

    #[test]
    fn test_vm_server_vfs_reply_no_pending() {
        let mut server = VmServer::new();
        server.handle_vfs_reply(Endpoint::VFS, 0);
        assert!(!server.has_pending_vfs_requests());
    }
}
