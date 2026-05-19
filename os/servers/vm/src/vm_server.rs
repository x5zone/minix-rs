//! VM Server main structure.
//!
//! Contains the VmServer struct with initialization phases
//! and main loop. Corresponds to Minix3's init_vm() + main loop.
//!
//! # Initialization Phases
//!
//! 1. **Phase 1 - Memory detection**: Initialize global state with total page count
//! 2. **Phase 2 - Process table**: Set up boot processes from kernel boot image
//! 3. **Phase 3 - Page tables**: Initialize kernel page tables and direct map (reserved)
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
use crate::pagetable::init_vm_self_pt;
use crate::phys_mem::{PhysAlloc, PhysAllocType, BitmapAllocator, BuddyAllocator, PhysAllocator, BootMemRegion, AlignedPhysBytes, bytes_to_clicks, CLICK_SIZE, BUDDY_THRESHOLD_PAGES};
use crate::page_cache::PageCache;
use crate::vfs_queue::VfsRequestQueue;
use crate::ipc::dispatcher::MessageDispatcher;
use crate::region::PageFrames;
use minix_types::PhysBytes;

pub struct VmServer {
    page_alloc: VmPageAllocator,
    page_cache: PageCache,
    page_frames: PageFrames,
    vfs_queue: VfsRequestQueue,
    initialized: bool,
}

impl VmServer {
    pub fn new(total_pages: usize, free_regions: &[BootMemRegion]) -> Self {
        let phys_alloc = Self::create_default_allocator(total_pages, free_regions);
        let mut page_alloc = VmPageAllocator::new(phys_alloc);
        crate::global::register_page_alloc(&mut page_alloc);

        init_vm_self_pt();

        Self {
            page_alloc,
            page_cache: PageCache::new(),
            page_frames: PageFrames::new(PhysBytes(0)),
            vfs_queue: VfsRequestQueue::new(),
            initialized: false,
        }
    }

    fn create_default_allocator(total_pages: usize, free_regions: &[BootMemRegion]) -> PhysAlloc {
        let meta_size = BitmapAllocator::metadata_size(total_pages);
        let meta_pages = bytes_to_clicks(meta_size);

        let meta_region = free_regions.iter()
            .find(|r| {
                (r.base as u64) < crate::direct_map::VM_DIRECT_MAP_SIZE
                && r.size >= meta_pages * CLICK_SIZE
            })
            .expect("no free region in Direct Map range large enough for allocator metadata");

        let meta_phys_base = meta_region.base;
        let meta_va = vm_phys_to_virt(AlignedPhysBytes::new(meta_phys_base as u64));
        let metadata = unsafe {
            core::slice::from_raw_parts_mut(meta_va.0 as *mut u8, meta_size)
        };

        let adjusted_base = meta_phys_base + meta_pages * CLICK_SIZE;
        let adjusted_size = meta_region.size.saturating_sub(meta_pages * CLICK_SIZE);
        let adjusted_regions = [BootMemRegion { base: adjusted_base, size: adjusted_size }];

        PhysAlloc::Bitmap(BitmapAllocator::init(metadata, total_pages, &adjusted_regions, meta_phys_base as u64, meta_pages))
    }

    fn choose_allocator_type(total_pages: usize) -> PhysAllocType {
        #[cfg(feature = "buddy_alloc")]
        {
            if total_pages > BUDDY_THRESHOLD_PAGES {
                return PhysAllocType::Buddy;
            }
        }
        PhysAllocType::Bitmap
    }

    fn relocate(&mut self) {
        let (total_pages, old_pa_base, old_pa_pages) = {
            let phys_alloc = self.page_alloc.phys_alloc();
            let bitmap = phys_alloc.as_bitmap().expect("relocate: bootstrap allocator must be Bitmap");
            let (pa_base, pa_pages) = bitmap.metadata_pa_range();
            assert!(pa_pages > 0, "relocate: no BumpBuf metadata to relocate (already relocated?)");
            (bitmap.total_count(), pa_base, pa_pages)
        };

        let alloc_type = Self::choose_allocator_type(total_pages);

        let meta_size = alloc_type.metadata_size(total_pages);
        let pages = bytes_to_clicks(meta_size);
        let new_va = crate::global::heap_arena_grow(pages, &mut self.page_alloc)
            .expect("relocate: failed to allocate new metadata via HeapArena");
        let new_metadata = unsafe {
            core::slice::from_raw_parts_mut(new_va as *mut u8, meta_size)
        };

        let mut free_regions: alloc::vec::Vec<BootMemRegion> = alloc::vec![];
        {
            let phys_alloc = self.page_alloc.phys_alloc();
            phys_alloc.available_regions(&mut |base_page, num_pages| {
                free_regions.push(BootMemRegion {
                    base: base_page * CLICK_SIZE,
                    size: num_pages * CLICK_SIZE,
                });
            });
        }

        let new_alloc = match alloc_type {
            PhysAllocType::Bitmap => {
                PhysAlloc::Bitmap(BitmapAllocator::init(new_metadata, total_pages, &free_regions, 0, 0))
            }
            #[cfg(feature = "buddy_alloc")]
            PhysAllocType::Buddy => {
                PhysAlloc::Buddy(BuddyAllocator::init(new_metadata, total_pages, &free_regions))
            }
            _ => unreachable!(),
        };

        {
            let phys_alloc = self.page_alloc.phys_alloc_mut();
            *phys_alloc = new_alloc;
            let old_pa = AlignedPhysBytes::new(old_pa_base);
            phys_alloc.free_mem(old_pa, old_pa_pages);
        }
    }

    pub fn init(&mut self) {
        // Relocation requires real page tables (vm_self_mappages); skipped in tests.
        #[cfg(not(test))]
        self.relocate();

        // Phase 1: Memory detection — initialize global state with total page count
        self.init_global_state();

        // Phase 2: Process table — set up boot processes from kernel boot image
        self.init_proc_table();

        // Phase 3: Page tables — initialize kernel page tables and direct map (reserved)

        self.initialized = true;
    }

    fn init_global_state(&mut self) {
        unsafe {
            crate::global::init(self.page_alloc.total_pages());
        }
    }

    fn init_proc_table(&mut self) {
        let _table = VmProcTable::get_global();
    }

    pub fn run(&mut self) {
        assert!(self.initialized, "VmServer::run() called before init()");

        loop {
            break;
        }
    }

    pub fn handle_request(&mut self, request: VmRequest) -> VmResponse {
        let table = VmProcTable::get_global();
        MessageDispatcher::dispatch_with_alloc(table, &mut self.page_alloc, &mut self.page_frames, request)
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
        minix_arch::direct_map::set_mock_vm_base(aligned_base as u64);

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
        minix_arch::direct_map::set_mock_vm_base(aligned_base as u64);

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
    #[should_panic(expected = "VmServer::run() called before init()")]
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
        assert!(matches!(response, VmResponse::Error(VmError::NotImplemented)));
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
        assert_eq!(server.page_cache().total_cached(), 0);
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
