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

use minix_types::{Endpoint, UserSlot, VirBytes, VmForkIn, VmBrkIn, VmExitIn, VmMmapIn, VmMapPhysIn, VmCacheIn, VmPagefaultIn, Message, MessageM1, VmReply, VmError, VM_RQ_BASE, DecodeFromM1, EncodeToM1};
use crate::vmproc::VmProcTable;
use crate::alloc_page::VmPageAllocator;
use crate::direct_map::vm_phys_to_virt;
use crate::pagetable::init_vm_self_pt;
use crate::phys_mem::{PhysAlloc, PhysAllocType, BitmapAllocator, BuddyAllocator, PhysAllocator, BootMemRegion, AlignedPhysBytes, bytes_to_clicks, CLICK_SIZE, BUDDY_THRESHOLD_PAGES};
use crate::page_cache::PageCache;
use crate::vfs_queue::VfsRequestQueue;
use crate::ipc::dispatcher::MessageDispatcher;
use crate::region::PageFrames;
use crate::acl::AclMask;
use minix_types::PhysBytes;

pub struct VmServer {
    page_alloc: VmPageAllocator,
    page_cache: PageCache,
    page_frames: Option<PageFrames>,
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
            page_frames: None,
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

        // Initialize PageFrames after total_pages is known
        let total_phys = PhysBytes(self.page_alloc.total_pages() as u64 * crate::region::PAGE_SIZE);
        self.page_frames = Some(PageFrames::new(total_phys));

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

    pub fn run(&mut self) -> ! {
        assert!(self.initialized, "VmServer::run() called before init()");

        loop {
            // C: if(missing_spares > 0) alloc_cycle();
            // TODO: CriticalPool refill when page_alloc needs spares

            // C: sef_receive_status(ANY, &msg, &rcv_sts)
            let (msg, rcv_sts) = match ipc_receive() {
                Ok(v) => v,
                Err(e) => panic!("ipc_receive() error: {:?}", e),
            };

            // C: if(is_ipc_notify(rcv_sts)) { continue; }
            if is_ipc_notify(&rcv_sts) {
                continue;
            }

            // C: who_e = msg.m_source; vm_isokendpt(who_e, &caller_slot);
            let who_e = msg.m_source;
            let caller_slot = match VmProcTable::get_global().vm_isokendpt(who_e) {
                Ok(slot) => slot,
                Err(_) => panic!("invalid caller {:?}", who_e),
            };

            let action = self.dispatch_on_msg(&msg, &rcv_sts, caller_slot);

            // C: if(result != SUSPEND) { ipc_send(who_e, &msg); }
            match action {
                DispatchAction::Reply(reply) => {
                    let code = reply_to_errno(reply.clone());
                    let mut reply_msg = msg.clone();
                    reply_msg.m_type = code;
                    encode_reply_data(reply.clone(), &mut reply_msg);
                    ipc_send(who_e, &reply_msg)
                        .unwrap_or_else(|e| panic!("ipc_send() error: {:?}", e));
                }
                DispatchAction::Suspend => {}
                DispatchAction::NoReply => {}
            }
        }
    }
}

/// Three reply actions — maps to C main.c:172-193.
enum DispatchAction {
    Reply(VmReply),
    Suspend,
    NoReply,
}

impl VmServer {
    /// Five-priority dispatch. C: main.c:131-170.
    fn dispatch_on_msg(
        &mut self,
        msg: &Message,
        rcv_sts: &IpcStatus,
        caller_slot: UserSlot,
    ) -> DispatchAction {
        let m_type = msg.m_type;
        let source = msg.m_source;

        // Priority 1: VFS transid (main.c:131-141)
        if source == VFS_PROC_NR && is_vfs_fs_transid(m_type) {
            let transid = transid_extract(m_type);
            let clean_type = transid_strip(m_type);
            let result = self.handle_vfs_transid(clean_type, transid, msg);
            return DispatchAction::Reply(result);
        }

        // Priority 2: RS_INIT (main.c:142-146)
        if m_type == RS_INIT && source == RS_PROC_NR {
            self.rs_handshake()
                .expect("rs_handshake failed");
            return DispatchAction::Suspend;
        }

        // Priority 3: VM_PAGEFAULT (main.c:147-156)
        if m_type == VM_PAGEFAULT {
            debug_assert!(
                is_from_kernel(rcv_sts),
                "faked VM_PAGEFAULT from {}", source
            );
            let _ = self.dispatch_pagefault(msg);
            return DispatchAction::NoReply;
        }

        // Priority 4: Normal VM calls (main.c:157-168)
        if let Some(c) = callnr(m_type) {
            // C: acl_check(&vmproc[caller_slot], c)
            let table = VmProcTable::get_global();
            if let Some(proc) = table.get_active(caller_slot) {
                if proc.acl_check(c as u32).is_err() {
                    // TODO: log::warn!("unauthorized call {} by {:?}", c, source);
                    let _ = (c, source);
                    return DispatchAction::Reply(
                        VmReply::Error(VmError::PermissionDenied)
                    );
                }
            }
            // C: result = vm_calls[c].vmc_func(&msg);
            let reply = MessageDispatcher::dispatch_by_number(c, msg, self);
            return DispatchAction::Reply(reply);
        }

        // Priority 5: Invalid request → ENOSYS (main.c:170)
        DispatchAction::Reply(VmReply::Error(VmError::NotImplemented))
    }

    /// RS handshake — replaces C's sef_startup() + sef_cb_init_fresh().
    /// C (main.c:237-250): sys_safecopyfrom → map_service for each rprocpub entry.
    fn rs_handshake(&mut self) -> Result<(), VmError> {
        let table = VmProcTable::get_global();

        // 1. Send RS_INIT, receive rproctab
        // C: sys_safecopyfrom(RS_PROC_NR, info->rproctab_gid, 0, rprocpub, ...)
        let rproctab = ipc_call_rs_init()
            .map_err(|_| VmError::InternalError)?;

        // 2. Register ACL for each boot service
        // C: for(i=0; i<NR_BOOT_PROCS; i++) if(rprocpub[i].in_use) map_service(&rprocpub[i]);
        for entry in &rproctab {
            if !entry.in_use { continue; }
            let slot = table.vm_isokendpt(entry.endpoint)
                .map_err(|_| VmError::InvalidProcess)?;
            let mut proc = table.get_active(slot)
                .ok_or(VmError::InvalidProcess)?;
            let is_sys = !entry.is_user;
            let mask = Some(crate::acl::AclMask::from_raw(entry.call_mask));
            // C: acl_set(&vmproc[proc_nr], rpub->vm_call_mask, !IS_RPUB_BOOT_USR(rpub))
            proc.set_acl(crate::acl::AclState::acl_set(is_sys, mask));
        }
        Ok(())
    }

    /// Signal handler — replaces C's sef_cb_signal_handler().
    /// C (main.c:737-750): case SIGKMEM: do_memory(); alloc_cycle(); pt_clearmapcache();
    fn handle_signal(&mut self, signo: i32) {
        match signo {
            SIGKMEM => {
                // C: do_memory(); — kernel memory request
                // TODO: implement do_memory() equivalent
            }
            _ => {}
        }
        // C: if(missing_spares > 0) alloc_cycle();
        // TODO: CriticalPool refill
        // C: pt_clearmapcache();
        // TODO: clear page table map cache
    }

    /// VFS transid dispatch — extracts transid, strips it, calls procctl.
    /// C (main.c:131-141): TRNS_GET_ID → TRNS_DEL_ID → do_procctl
    fn handle_vfs_transid(
        &mut self,
        clean_type: u32,
        transid: i32,
        msg: &Message,
    ) -> VmReply {
        let _ = (clean_type, transid, msg);
        // TODO: implement do_procctl equivalent
        VmReply::Error(VmError::NotImplemented)
    }

    /// Pagefault dispatch — decodes VmPagefaultIn from Message, delegates to cow_exec_pf.
    /// C (main.c:147-156): do_pagefaults(&msg); continue;
    fn dispatch_pagefault(&mut self, msg: &Message) -> VmReply {
        let request = VmPagefaultIn::decode(msg);
        let table = VmProcTable::get_global();
        let slot = match table.vm_isokendpt(request.endpoint) {
            Ok(s) => s,
            Err(_) => return VmReply::Error(VmError::InvalidProcess),
        };
        let proc = match table.get_active(slot) {
            Some(p) => p,
            None => return VmReply::Error(VmError::InvalidProcess),
        };
        let fault_addr = request.vaddr;
        let region = match proc.regions().find(fault_addr) {
            Some(r) => r,
            None => return VmReply::Error(VmError::InvalidAddress),
        };
        let frames = self.page_frames.as_mut()
            .expect("page_frames not initialized");
        match crate::cow_exec_pf::handle_pagefault(
            proc, region, frames, self.page_alloc_mut(),
            fault_addr, request.write,
        ) {
            Ok(_action) => VmReply::Ok,
            Err(_e) => {
                // For SharedMemory: ev_pagefault returns NotSupported when the
                // shared page isn't mapped. The real implementation requires
                // cross-process PFN sharing (see memtype.rs SharedMemory::ev_pagefault).
                VmReply::Error(VmError::AccessViolation)
            }
        }
    }

    pub(crate) fn page_frames_mut(&mut self) -> &mut PageFrames {
        self.page_frames.as_mut().expect("page_frames not initialized")
    }

    pub(crate) fn is_initialized(&self) -> bool {
        self.initialized
    }
}

// ==========================================================================
// IPC stubs — trait abstracted replacements for C's ipc_send / sef_receive_status
// ==========================================================================

/// IPC receive status. C: rcv_sts from sef_receive_status().
struct IpcStatus {
    _flags: u32,
}

fn ipc_receive() -> Result<(Message, IpcStatus), ()> {
    // TODO: implement via IpcTransport trait
    Err(())
}

fn ipc_send(_dest: Endpoint, _msg: &Message) -> Result<(), ()> {
    // TODO: implement via IpcTransport trait
    Ok(())
}

fn is_ipc_notify(_sts: &IpcStatus) -> bool {
    // C: is_ipc_notify(rcv_sts)
    false
}

fn is_from_kernel(_sts: &IpcStatus) -> bool {
    // C: IPC_STATUS_FLAGS_TEST(rcv_sts, IPC_FLG_MSG_FROM_KERNEL)
    true
}

fn is_vfs_fs_transid(m_type: u32) -> bool {
    // C: IS_VFS_FS_TRANSID(transid)
    let _ = m_type;
    false
}

fn transid_extract(m_type: u32) -> i32 {
    // C: TRNS_GET_ID(msg.m_type)
    let _ = m_type;
    0
}

fn transid_strip(m_type: u32) -> u32 {
    // C: TRNS_DEL_ID(msg.m_type)
    m_type
}

fn ipc_call_rs_init() -> Result<RprocTab, ()> {
    // TODO: implement RS_INIT IPC exchange
    Err(())
}

const SIGKMEM: i32 = 18;
const VFS_PROC_NR: Endpoint = Endpoint(2);
const RS_PROC_NR: Endpoint = Endpoint(1);
const RS_INIT: u32 = 0x606;
const VM_PAGEFAULT: u32 = 0xCFF;
const NR_VM_CALLS: usize = 64;

// ==========================================================================
// Helpers
// ==========================================================================

/// C: CALLNUMBER(c) with bounds check. main.c:55.
fn callnr(m_type: u32) -> Option<usize> {
    let c = m_type.checked_sub(VM_RQ_BASE)?;
    if (c as usize) < NR_VM_CALLS {
        Some(c as usize)
    } else {
        None
    }
}

/// RprocTab — equivalent to C's struct rprocpub[NR_SYS_PROCS].
struct RprocEntry {
    in_use: bool,
    endpoint: Endpoint,
    call_mask: u32,
    is_user: bool,
}

struct RprocTab {
    entries: [RprocEntry; 32],
}

impl core::ops::Deref for RprocTab {
    type Target = [RprocEntry];
    fn deref(&self) -> &[RprocEntry] {
        &self.entries
    }
}

/// Convert VmReply to raw errno for IPC reply. C: result != SUSPEND branch.
fn reply_to_errno(reply: VmReply) -> i32 {
    match reply {
        VmReply::Ok => 0,
        VmReply::Suspend => unreachable!("Suspend filtered before reply_to_errno"),
        VmReply::Error(e) => e.to_errno(),
        VmReply::Fork(_) | VmReply::Brk(_) | VmReply::Mmap(_)
        | VmReply::MapPhys(_) | VmReply::Exit | VmReply::Willexit
        | VmReply::Munmap | VmReply::ExecNewmem(_)
        | VmReply::MapCache { .. } | VmReply::VfsMmap(_)
        | VmReply::GetPhys { .. } | VmReply::GetRefcount { .. }
        | VmReply::InfoStats { .. } | VmReply::InfoUsage { .. }
        | VmReply::InfoRegion { .. } | VmReply::Getrusage { .. }
        | VmReply::RsMemctlAddrLen { .. } => 0,
    }
}

/// Encode VmReply per-service output data into the reply message fields.
fn encode_reply_data(reply: VmReply, msg: &mut Message) {
    match reply {
        VmReply::Fork(out) => out.encode(msg),
        VmReply::Brk(out) => out.encode(msg),
        VmReply::Mmap(out) => out.encode(msg),
        VmReply::MapPhys(out) => out.encode(msg),
        VmReply::ExecNewmem(out) => out.encode(msg),
        VmReply::MapCache { .. } => {}
        VmReply::VfsMmap(out) => out.encode(msg),
        VmReply::GetPhys { phys_addr } => { msg.m1_p1 = phys_addr.0; }
        VmReply::GetRefcount { count } => { msg.m1_i1 = count as i32; }
        VmReply::InfoStats { page_size, total_pages, free_pages, largest_contiguous } => {
            msg.m1_p1 = page_size;
            msg.m1_i1 = total_pages as i32;
            msg.m1_i2 = free_pages as i32;
            msg.m1_i3 = largest_contiguous as i32;
        }
        VmReply::InfoUsage { total, shared, text, data, stack } => {
            msg.m1_p1 = total.0; msg.m1_p2 = shared.0;
            msg.m1_p3 = text.0; msg.m1_p4 = data.0;
            msg.m1_p5 = stack.0;
        }
        VmReply::RsMemctlAddrLen { addr, len } => {
            msg.m1_p1 = addr.0; msg.m1_i1 = len as i32;
        }
        _ => {}
    }
}

impl VmServer {
    pub fn handle_fork(&mut self, req: VmForkIn) -> VmReply {
        let table = VmProcTable::get_global();
        let frames = self.page_frames.as_mut().expect("page_frames not initialized");
        MessageDispatcher::dispatch_fork(table, &mut self.page_alloc, frames, req)
    }

    pub fn handle_brk(&mut self, req: VmBrkIn) -> VmReply {
        let table = VmProcTable::get_global();
        let frames = self.page_frames.as_mut().expect("page_frames not initialized");
        MessageDispatcher::dispatch_brk(table, &mut self.page_alloc, frames, req)
    }

    pub fn handle_exit(&mut self, req: VmExitIn) -> VmReply {
        let table = VmProcTable::get_global();
        let frames = self.page_frames.as_mut().expect("page_frames not initialized");
        MessageDispatcher::dispatch_exit(table, &mut self.page_alloc, frames, req)
    }

    pub fn handle_mmap(&mut self, req: VmMmapIn) -> VmReply {
        let table = VmProcTable::get_global();
        let frames = self.page_frames.as_mut().expect("page_frames not initialized");
        MessageDispatcher::dispatch_mmap(table, &mut self.page_alloc, frames, req)
    }

    pub fn handle_map_phys(&mut self, req: VmMapPhysIn) -> VmReply {
        let table = VmProcTable::get_global();
        let frames = self.page_frames.as_mut().expect("page_frames not initialized");
        MessageDispatcher::dispatch_map_phys(table, &mut self.page_alloc, frames, req)
    }

    pub fn handle_mapcache(&mut self, req: VmCacheIn) -> VmReply {
        let table = VmProcTable::get_global();
        let frames = self.page_frames.as_mut().expect("page_frames not initialized");
        MessageDispatcher::dispatch_mapcache(table, &mut self.page_alloc, frames, &mut self.page_cache, req)
    }

    pub fn handle_setcache(&mut self, req: VmCacheIn) -> VmReply {
        let table = VmProcTable::get_global();
        let frames = self.page_frames.as_mut().expect("page_frames not initialized");
        MessageDispatcher::dispatch_setcache(table, frames, &mut self.page_cache, req)
    }

    pub fn handle_forgetcache(&mut self, req: VmCacheIn) -> VmReply {
        let frames = self.page_frames.as_mut().expect("page_frames not initialized");
        MessageDispatcher::dispatch_forgetcache(&mut self.page_cache, frames, req)
    }

    pub fn handle_clearcache(&mut self, req: VmCacheIn) -> VmReply {
        let frames = self.page_frames.as_mut().expect("page_frames not initialized");
        MessageDispatcher::dispatch_clearcache(&mut self.page_cache, frames, req)
    }

    pub fn handle_vfs_reply(&mut self, src: Endpoint, result: i32) {
        use crate::vfs_queue::VfsReply;
        let active_id = match self.vfs_queue.active_req_id() {
            Some(id) => id,
            None => return,
        };
        let reply = VfsReply {
            req_id: active_id,
            result,
            data_phys: None,
            fd: 0,
            dev: 0,
            ino: 0,
            size_pages: 0,
        };
        match self.vfs_queue.handle_reply(reply) {
            Ok(Some((callback, reply, state))) => {
                let _ = callback(self, &reply, &state);
            }
            Ok(None) => {}
            Err(_) => {}
        }
        let _ = src;
    }

    pub fn has_pending_vfs_requests(&self) -> bool {
        !self.vfs_queue.is_empty()
    }

    pub(crate) fn page_cache(&self) -> &PageCache {
        &self.page_cache
    }

    pub(crate) fn page_cache_mut(&mut self) -> &mut PageCache {
        &mut self.page_cache
    }

    pub(crate) fn vfs_queue(&self) -> &VfsRequestQueue {
        &self.vfs_queue
    }

    pub(crate) fn vfs_queue_mut(&mut self) -> &mut VfsRequestQueue {
        &mut self.vfs_queue
    }

    pub(crate) fn page_alloc_mut(&mut self) -> &mut VmPageAllocator {
        &mut self.page_alloc
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

        let request = VmForkIn {
            parent_endpoint: Endpoint::NONE,
            child_slot: UserSlot::new(1),
        };

        let reply = server.handle_fork(request);
        assert!(matches!(reply, VmReply::Error(VmError::InvalidProcess)));
    }

    #[test]
    fn test_vm_server_handle_brk_not_found() {
        let mut server = make_test_vm_server();
        server.init();

        let request = VmBrkIn {
            endpoint: Endpoint::NONE,
            new_addr: VirBytes(0x5000_0000),
        };

        let reply = server.handle_brk(request);
        assert!(matches!(reply, VmReply::Error(VmError::InvalidProcess)));
    }

    #[test]
    fn test_vm_server_handle_exit_not_found() {
        let mut server = make_test_vm_server();
        server.init();

        let request = VmExitIn {
            endpoint: Endpoint::NONE,
        };

        let reply = server.handle_exit(request);
        assert!(matches!(reply, VmReply::Error(VmError::InvalidProcess)));
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
    fn test_vm_server_vfs_reply_no_pending() {
        let mut server = make_test_vm_server();
        server.handle_vfs_reply(Endpoint::VFS, 0);
        assert!(!server.has_pending_vfs_requests());
    }
}
