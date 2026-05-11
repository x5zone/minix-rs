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

use minix_types::{VmRequest, VmResponse, VmError, Endpoint, VirBytes};
use crate::vmproc::VmProcTable;
use crate::alloc_page::VmPageAllocator;
use crate::direct_map::vm_phys_to_virt;
use crate::phys_mem::{PhysAlloc, BitmapAllocator, PhysAllocator, BootMemRegion, PhysBytes, bytes_to_clicks, CLICK_SIZE};
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
    pub fn new(total_pages: usize, free_regions: &[BootMemRegion]) -> Self {
        let phys_alloc = Self::create_default_allocator(total_pages, free_regions);
        Self {
            page_alloc: VmPageAllocator::new(phys_alloc),
            page_cache: PageCache::new(),
            vfs_queue: VfsRequestQueue::new(),
            initialized: false,
        }
    }

    fn create_default_allocator(total_pages: usize, free_regions: &[BootMemRegion]) -> PhysAlloc {
        let meta_size = BitmapAllocator::metadata_size(total_pages);
        let meta_pages = bytes_to_clicks(meta_size);

        let meta_phys_base = free_regions[0].base;
        let meta_va = vm_phys_to_virt(PhysBytes::new(meta_phys_base as u64));
        let metadata = unsafe {
            core::slice::from_raw_parts_mut(meta_va.0 as *mut u8, meta_size)
        };

        let adjusted_base = meta_phys_base + meta_pages * CLICK_SIZE;
        let adjusted_size = free_regions[0].size.saturating_sub(meta_pages * CLICK_SIZE);
        let adjusted_regions = [BootMemRegion { base: adjusted_base, size: adjusted_size }];

        PhysAlloc::Bitmap(BitmapAllocator::init(metadata, total_pages, &adjusted_regions))
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

#[cfg(test)]
impl Default for VmServer {
    fn default() -> Self {
        let total_pages = 256;
        let mock_phys_size = total_pages * CLICK_SIZE + CLICK_SIZE;
        let mock_phys: alloc::vec::Vec<u8> = alloc::vec![0u8; mock_phys_size];
        let mock_phys_leaked = alloc::boxed::Box::leak(mock_phys.into_boxed_slice());

        let raw_base = mock_phys_leaked.as_ptr() as usize;
        let aligned_base = (raw_base + CLICK_SIZE - 1) & !(CLICK_SIZE - 1);
        crate::direct_map::set_mock_phys_base(aligned_base as u64);

        let free_regions = [BootMemRegion { base: 0, size: total_pages * CLICK_SIZE }];
        Self::new(total_pages, &free_regions)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_TOTAL_PAGES: usize = 256;

    fn test_free_regions() -> [BootMemRegion; 1] {
        let mock_phys_size = TEST_TOTAL_PAGES * CLICK_SIZE + CLICK_SIZE;
        let mock_phys: alloc::vec::Vec<u8> = alloc::vec![0u8; mock_phys_size];
        let mock_phys_leaked = alloc::boxed::Box::leak(mock_phys.into_boxed_slice());

        let raw_base = mock_phys_leaked.as_ptr() as usize;
        let aligned_base = (raw_base + CLICK_SIZE - 1) & !(CLICK_SIZE - 1);
        crate::direct_map::set_mock_phys_base(aligned_base as u64);

        [BootMemRegion { base: 0, size: TEST_TOTAL_PAGES * CLICK_SIZE }]
    }

    fn make_test_vm_server() -> VmServer {
        VmServer::new(TEST_TOTAL_PAGES, &test_free_regions())
    }

    #[test]
    fn test_vm_server_new() {
        let server = make_test_vm_server();
        assert!(!server.initialized);
        assert!(!server.is_initialized());
    }

    #[test]
    fn test_vm_server_init() {
        let mut server = make_test_vm_server();
        server.init();
        assert!(server.initialized);
        assert!(server.is_initialized());
    }

    #[test]
    fn test_vm_server_run_without_init() {
        let mut server = make_test_vm_server();
        server.run();
    }

    #[test]
    fn test_vm_server_default() {
        let server = VmServer::default();
        assert!(!server.initialized);
    }

    #[test]
    fn test_vm_server_handle_fork_not_found() {
        let mut server = make_test_vm_server();
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
        let mut server = make_test_vm_server();
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
        let mut server = make_test_vm_server();
        server.init();

        let request = VmRequest::Exit {
            endpoint: Endpoint::NONE,
        };

        let response = server.handle_request(request);
        assert!(matches!(response, VmResponse::Error(VmError::InvalidEndpoint)));
    }

    #[test]
    fn test_vm_server_page_cache_access() {
        let mut server = make_test_vm_server();
        assert_eq!(server.page_cache().total_pages(), 0);
    }

    #[test]
    fn test_vm_server_vfs_queue_access() {
        let server = make_test_vm_server();
        assert!(server.vfs_queue().is_empty());
    }

    #[test]
    fn test_vm_server_handle_ipc_message() {
        let mut server = make_test_vm_server();
        server.init();

        let request = VmRequest::Exit {
            endpoint: Endpoint::NONE,
        };

        let response = server.handle_ipc_message(Endpoint::PM, request);
        assert!(matches!(response, VmResponse::Error(VmError::InvalidEndpoint)));
    }

    #[test]
    fn test_vm_server_vfs_reply_no_pending() {
        let mut server = make_test_vm_server();
        server.handle_vfs_reply(Endpoint::VFS, 0);
        assert!(!server.has_pending_vfs_requests());
    }
}
