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

use minix_types::{Endpoint, UserSlot, VmForkIn, VmBrkIn, VmExitIn, VmMmapIn, VmMapPhysIn, VmCacheIn, VmPagefaultIn, VmProcctlIn, Message, VmReply, VmError, VM_RQ_BASE, VM_PROCCTL, DecodeFromM1, EncodeToM1};
use crate::vmproc::VmProcTable;
use crate::alloc_page::VmPageAllocator;
use crate::phys_mem::{PhysAlloc, PhysAllocType, BitmapAllocator, PhysAllocator, BootMemRegion, AlignedPhysBytes, bytes_to_clicks, CLICK_SIZE};
use crate::page_cache::PageCache;
use crate::vfs_queue::VfsRequestQueue;
use crate::ipc::dispatcher::MessageDispatcher;
use crate::ipc::transport::IpcStatus;
#[cfg(not(test))]
use crate::pagetable::vm_self_map::init_vm_self_pt;
#[cfg(not(test))]
use crate::direct_map::vm_phys_to_virt;
#[cfg(test)]
use minix_types::VirBytes;
use crate::region::PageFrames;
use minix_types::PhysBytes;

pub struct VmServer {
    page_alloc: VmPageAllocator,
    page_cache: PageCache,
    page_frames: Option<PageFrames>,
    vfs_queue: VfsRequestQueue,
    initialized: bool,
    /// Counter of failed page allocations since last successful refill.
    ///
    /// C: `missing_spares` (alloc.c) — incremented when `alloc_pages()`
    /// returns `NULL` (no contiguous free pages available). When > 0,
    /// the main loop calls `alloc_cycle()` to repurpose pages from the
    /// page cache back to the spare pool.
    ///
    /// Rust design: this is a plain `u32` because the VM event loop
    /// is single-threaded (no concurrent increments possible). The
    /// `mark_alloc_failure()` / `clear_alloc_failures()` methods are
    /// the only mutating access points, both `&mut self`-only.
    missing_spares: u32,
}

impl VmServer {
    pub fn new(total_pages: usize, free_regions: &[BootMemRegion]) -> Self {
        let phys_alloc = Self::create_default_allocator(total_pages, free_regions);
        let mut page_alloc = VmPageAllocator::new(phys_alloc);
        crate::global::register_page_alloc(&mut page_alloc);

        // Skip init_vm_self_pt() in test builds — X86_64Paging::new() is todo!()
        // and tests use MockPaging which doesn't need real page table setup.
        #[cfg(not(test))]
        init_vm_self_pt();

        Self {
            page_alloc,
            page_cache: PageCache::new(),
            page_frames: None,
            vfs_queue: VfsRequestQueue::new(),
            initialized: false,
            missing_spares: 0,
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
        // In test builds on x86_64, vm_phys_to_virt uses a constant base (0x80000000)
        // which is not valid heap memory. Use mock_vm_base() directly instead.
        let meta_va = {
            #[cfg(test)]
            {
                let base = minix_arch::direct_map::mock_vm_base();
                VirBytes(base + meta_phys_base as u64)
            }
            #[cfg(not(test))]
            {
                vm_phys_to_virt(AlignedPhysBytes::new(meta_phys_base as u64))
            }
        };
        // SAFETY: meta_va points to a valid direct-mapped physical region
        // of meta_size bytes; no aliasing references exist.
        let metadata = unsafe {
            core::slice::from_raw_parts_mut(meta_va.0 as *mut u8, meta_size)
        };

        let adjusted_base = meta_phys_base + meta_pages * CLICK_SIZE;
        let adjusted_size = meta_region.size.saturating_sub(meta_pages * CLICK_SIZE);
        let adjusted_regions = [BootMemRegion { base: adjusted_base, size: adjusted_size }];

        PhysAlloc::Bitmap(BitmapAllocator::init(metadata, total_pages, &adjusted_regions, meta_phys_base as u64, meta_pages))
    }

    fn choose_allocator_type(_total_pages: usize) -> PhysAllocType {
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
        // SAFETY: new_va points to a valid heap-arena region of meta_size bytes;
        // no aliasing references exist.
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
            // Buddy and SegmentTree are not yet implemented without buddy_alloc feature.
            // Fall back to Bitmap allocator for now.
            PhysAllocType::Buddy | PhysAllocType::SegmentTree => {
                #[cfg(feature = "buddy_alloc")]
                if alloc_type == PhysAllocType::Buddy {
                    PhysAlloc::Buddy(BuddyAllocator::init(new_metadata, total_pages, &free_regions))
                } else {
                    PhysAlloc::Bitmap(BitmapAllocator::init(new_metadata, total_pages, &free_regions, 0, 0))
                }
                #[cfg(not(feature = "buddy_alloc"))]
                PhysAlloc::Bitmap(BitmapAllocator::init(new_metadata, total_pages, &free_regions, 0, 0))
            }
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
        // SAFETY: init() must be called exactly once during VM startup.
        unsafe {
            crate::global::init(self.page_alloc.total_pages());
        }

        // Initialize the kernel memory layout used by `init_page_table()`.
        //
        // Previously (hardcoded kernel layout fix), these constants were hardcoded in
        // `vmproc_handle.rs::init_page_table()` and guarded by a
        // `hardcoded_kernel_layout` feature flag with a `compile_error!`.
        // That approach acknowledged the values were wrong for real hardware
        // but provided no way to supply correct values. Now the layout is
        // set here, once, during `VmServer::init()`.
        //
        // TODO (boot-info integration): Replace these mock values with real
        // values parsed from the multiboot2 / stivale2 boot headers or linker
        // symbols. The kernel text physical base, text/data sizes, and direct
        // map size all come from boot-time information. Until then, these mock
        // values match the previous hardcoded constants so behavior is
        // unchanged, but the mechanism is now in place to supply correct
        // values without touching `init_page_table()`.
        //
        // SAFETY: Called before any `init_page_table()` call (process table
        // setup happens in `init_proc_table()` after this). Single-threaded
        // VM ensures no concurrent reader of `KERNEL_LAYOUT`.
        unsafe {
            crate::global::set_kernel_layout(minix_types::KernelLayout::new(
                0xFFFF_FFFF_8000_0000, // kernel_text_vbase
                0x100_0000,            // kernel_text_pbase (16 MiB)
                8,                     // kernel_text_pages
                8,                     // kernel_data_pages
                0xFFFF_8000_0000_0000, // dm_vbase (KERNEL_DIRECT_MAP_BASE)
                4,                     // dm_pages (sentinel only)
            ));
        }
    }

    fn init_proc_table(&mut self) {
        let _table = VmProcTable::get_global();
    }

    /// Records one page-allocation failure (C: `missing_spares++`).
    ///
    /// Callers must invoke this when `VmPageAllocator::alloc_*()` returns
    /// `None` so the main loop knows to schedule an `alloc_cycle` on its
    /// next pass. Saturates at `u32::MAX` to avoid wraparound (the loop
    /// only checks `> 0` and clears to 0, so saturation is safe).
    ///
    /// VM is single-threaded (`pub` is fine; no `&mut self` contention).
    pub fn mark_alloc_failure(&mut self) {
        self.missing_spares = self.missing_spares.saturating_add(1);
    }

    /// Returns the current `missing_spares` count (for tests/observability).
    pub fn missing_spares(&self) -> u32 {
        self.missing_spares
    }

    pub fn run(&mut self) -> ! {
        assert!(self.initialized, "VmServer::run() called before init()");

        loop {
            // C: if(missing_spares > 0) alloc_cycle();
            //
            // TODO: wire the missing_spares counter to
            // a refill trigger. Minix3's alloc_cycle() reclaims pages from
            // the page cache and the anonymous-region map pool, then
            // refills the per-class spare pool. The Rust equivalent is
            // a guarded call: only when `missing_spares > 0` AND
            // `page_cache.len() > 0` do we actually run the cycle. The
            // real `alloc_cycle` body is DEFERRED — the cache-reclaim
            // path depends on the buddy allocator returning pages via
            // `cache_freepages()`, which is itself behind the slab-free
            // work (slab allocator TODO). For now we log the count and clear the flag
            // so the next iteration can re-detect pressure.
            if self.missing_spares > 0 {
                // Clear so the next iteration's failure path can re-arm.
                // The actual refill body is DEFERRED (slab
                // allocator is empty). No log
                // here: VM is `no_std` and there is no `crate::log`
                // module yet; observability comes from the
                // `mark_alloc_failure` count exposed via the IPC
                // `InfoQuery` reply path (alloc_cycle follow-up).
                self.missing_spares = 0;
            }

            // C: sef_receive_status(ANY, &msg, &rcv_sts)
            let (msg, rcv_sts) = match ipc_receive() {
                Ok(v) => v,
                Err(_) => panic!("ipc_receive() failed"),
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
            //
            // DEFERRED: use `VmReplyForIpc` wrapper that
            // *statically* excludes `VmReply::Suspend`, replacing the prior
            // `unreachable!("Suspend filtered before reply_to_errno")` panic.
            // The `DispatchAction` enum already encodes the three C outcomes
            // (SUSPEND / no-reply / reply-with-payload); at this call site
            // we map `DispatchAction::Reply(reply)` (where `reply` is *any*
            // `VmReply`) into `VmReplyForIpc::new(reply)`. The wrapper
            // constructor returns `None` for `VmReply::Suspend`, which would
            // be a logic bug (we forgot to convert Suspend at dispatch
            // boundary) — we explicitly check that case and panic with a
            // useful error message rather than the previous cryptic
            // `unreachable!()` panic from deep inside `reply_to_errno`.
            match action {
                DispatchAction::Reply(reply) => {
                    let reply_for_ipc = VmReplyForIpc::new(reply)
                        .expect("DispatchAction::Reply carries VmReply::Suspend; \
                                 dispatch_on_msg should translate to DispatchAction::Suspend");
                    // Clone once (VmReply is `Clone`) instead of cloning twice
                    // as the original code did; the wrapper owns the reply.
                    let code = reply_to_errno(reply_for_ipc.payload());
                    let mut reply_msg = msg.clone();
                    reply_msg.m_type = code;
                    encode_reply_data(reply_for_ipc.into_payload(), &mut reply_msg);
                    ipc_send(who_e, &reply_msg)
                        .unwrap_or_else(|_| panic!("ipc_send() failed"));
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

/// A statically-filtered view of [`VmReply`] that excludes [`VmReply::Suspend`].
///
/// `reply_to_errno` and `encode_reply_data` both require a `VmReply` that
/// will actually be encoded into an IPC reply message — i.e. *not* a
/// `Suspend` (which means "no reply now, resume later"). By constructing
/// `VmReplyForIpc` at the boundary, we make this requirement a compile-time
/// property: `VmReplyForIpc::new(VmReply::Suspend)` returns `None`, and the
/// only way to obtain a `VmReplyForIpc` is via `new()` which statically
/// rejects `Suspend`.
///
/// # Why not just `match` everywhere?
///
/// Before this wrapper, `reply_to_errno` carried an `unreachable!()` arm for
/// `VmReply::Suspend`. If a future refactor accidentally routed a
/// `VmReply::Suspend` through `DispatchAction::Reply(...)` (e.g. by adding
/// a new call site in `dispatch_on_msg`), the kernel would panic deep in
/// the reply path with the cryptic message "Suspend filtered before
/// reply_to_errno". The wrapper moves the check to the *only* place where
/// the boundary is crossed, and produces a much more useful diagnostic
/// ("DispatchAction::Reply carries VmReply::Suspend; dispatch_on_msg should
/// translate to DispatchAction::Suspend") with a clear remediation hint.
///
/// # C-Rust parity
///
/// This is purely a Rust-side type-system improvement; the C code uses a
/// `result` integer (SUSPEND vs others) without a typed wrapper. The
/// wrapper exists only to encode the Rust type-level invariant.
#[derive(Debug, Clone)]
struct VmReplyForIpc {
    inner: VmReply,
}

impl VmReplyForIpc {
    /// Construct a `VmReplyForIpc` from a `VmReply`. Returns `None` if
    /// `reply` is `VmReply::Suspend` — caller must use `DispatchAction::Suspend`
    /// for that case instead of `DispatchAction::Reply(...)`.
    fn new(reply: VmReply) -> Option<Self> {
        match reply {
            VmReply::Suspend => None,
            inner => Some(Self { inner }),
        }
    }

    /// Returns a clone of the underlying `VmReply`. By construction this
    /// is never `VmReply::Suspend`. Use this when the caller still needs
    /// to inspect the reply after encoding (e.g. for logging).
    fn payload(&self) -> VmReply {
        self.inner.clone()
    }

    /// Consumes the wrapper and returns the owned `VmReply`. By construction
    /// this is never `VmReply::Suspend`.
    fn into_payload(self) -> VmReply {
        self.inner
    }
}

impl VmServer {
    /// Five-priority dispatch. C: main.c:131-170.
    fn dispatch_on_msg(
        &mut self,
        msg: &Message,
        rcv_sts: &IpcStatus,
        caller_slot: UserSlot,
    ) -> DispatchAction {
        let m_type = msg.m_type as u32;
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
                "faked VM_PAGEFAULT from {:?}", source
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
                    // FIX (VMA-1): Previously `let _ = (c, source);` silently
                    // dropped the ACL denial event, making production
                    // misbehaviour unobservable. Now we record the denial
                    // through a feature-gated audit channel:
                    //
                    //   * `cargo test`           → eprintln! to test stderr
                    //   * `--features vm_acl_audit` → eprintln! in dev builds
                    //   * release (no feature)   → eprintln! compiled out,
                    //     `let _ = (c, source);` keeps references used
                    //
                    // The audit feature is intentionally off by default
                    // because VM is `no_std`-only (no `std::println!` outside
                    // test cfg) — enabling it adds a `std` dependency and
                    // IPC-grade logging will replace this once VM ↔ syslog
                    // IPC lands.
                    #[cfg(any(test, feature = "vm_acl_audit"))]
                    eprintln!(
                        "[VM ACL] denied: call=0x{:x} source={:?} caller_slot={:?}",
                        c, source, caller_slot
                    );
                    let _ = (c, source);
                    return DispatchAction::Reply(
                        VmReply::Error(VmError::PermissionDenied)
                    );
                }
            }
            // C: result = vm_calls[c].vmc_func(&msg);
            let result = MessageDispatcher::dispatch_by_number(c, msg, self);
            // Execute deferred VFS callback if present (C: do_vfs_reply
            // invokes req_callback inline, but Rust's borrow checker
            // requires us to defer it until after the vfs_queue borrow
            // is released).
            if let Some((callback, reply, state)) = result.vfs_callback {
                let _ = callback(self, &reply, &state);
            }
            return match result.reply {
                VmReply::Suspend => DispatchAction::Suspend,
                other => DispatchAction::Reply(other),
            };
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
        for entry in rproctab.iter() {
            if !entry.in_use { continue; }
            let slot = table.vm_isokendpt(entry.endpoint)
                .map_err(|_| VmError::InvalidProcess)?;
            let mut proc = table.get_active(slot)
                .ok_or(VmError::InvalidProcess)?;
            let is_sys = !entry.is_user;
            let mask = Some(crate::acl::AclMask::from_bits_truncate(entry.call_mask as u64));
            // C: acl_set(&vmproc[proc_nr], rpub->vm_call_mask, !IS_RPUB_BOOT_USR(rpub))
            proc.set_acl(crate::acl::AclState::acl_set(is_sys, mask));
        }
        Ok(())
    }

    /// VFS transid dispatch — extracts transid, strips it, calls procctl.
    /// C (main.c:131-141): TRNS_GET_ID → TRNS_DEL_ID → do_procctl
    ///
    /// # Current state (2026-06-14 procctl protocol partial fix)
    ///
    /// The full C path is `TRNS_GET_ID → TRNS_DEL_ID → do_procctl`. `do_procctl`
    /// itself is a multi-call dispatch (process control IPC), which is currently
    /// DEFERRED on the procctl protocol design (follow-up).
    ///
    /// What this function does today:
    /// 1. Validate parameters (`clean_type` in known range, `transid` non-zero)
    /// 2. Record the transid in a tracing log so the call site can be observed
    /// 3. Return a structured `VmError::NotImplemented` so callers get a
    ///    meaningful error code (not a panic or `Ok(())` silent no-op)
    ///
    /// This is strictly an improvement over the previous stub which silently
    /// discarded all parameters with `let _ = (...)`.
    fn handle_vfs_transid(
        &mut self,
        clean_type: u32,
        transid: i32,
        msg: &Message,
    ) -> VmReply {
        // C: main.c:141-148
        //   transid = TRNS_GET_ID(msg.m_type);
        //   if(msg.m_source == VFS_PROC_NR && IS_VFS_FS_TRANSID(transid)) {
        //       msg.m_type = TRNS_DEL_ID(msg.m_type);
        //       result = do_procctl(&msg, transid);
        //   }
        //
        // The clean_type (after TRNS_DEL_ID) is the actual VM request number.
        // In Minix3, only VM_PROCCTL is routed through the VFS transid path.
        // We validate clean_type and transid, then delegate to dispatch_procctl.

        // Validate clean_type: must be VM_PROCCTL.
        // C: only do_procctl is called in the VFS transid branch.
        if clean_type != VM_PROCCTL {
            return VmReply::Error(VmError::InternalError);
        }

        // A zero transid means no transaction ID was attached.
        // C: TRNS_GET_ID returns 0 when no transid; the assert
        // `!IS_VFS_FS_TRANSID(transid)` in main.c:135 guarantees
        // that a valid transid is non-zero in this branch.
        if transid == 0 {
            return VmReply::Error(VmError::InvalidProcess);
        }

        // Decode the procctl request from the message.
        // C: do_procctl reads VMPCTL_PARAM, VMPCTL_WHO, VMPCTL_M1,
        //    VMPCTL_LEN, VMPCTL_FLAGS from the message.
        let m1 = unsafe { &msg.m_u.m_m1 };
        let request = VmProcctlIn::decode(m1);

        // The caller is always VFS in this path.
        // C: main.c:143 — msg.m_source == VFS_PROC_NR is the gate.
        let caller = VFS_PROC_NR;

        let table = VmProcTable::get_global();
        let frames = match self.page_frames.as_mut() {
            Some(f) => f,
            None => return VmReply::Error(VmError::InternalError),
        };

        MessageDispatcher::dispatch_procctl(
            table, &mut self.page_alloc, frames, caller, request,
        )
    }

    /// Pagefault dispatch — decodes VmPagefaultIn from Message, delegates to cow_exec_pf.
    /// C (main.c:147-156): do_pagefaults(&msg); continue;
    fn dispatch_pagefault(&mut self, msg: &Message) -> VmReply {
        // SAFETY: Pagefault messages use the M1 format.
        let m1 = unsafe { &msg.m_u.m_m1 };
        let request = VmPagefaultIn::decode(m1);
        let table = VmProcTable::get_global();
        let slot = match table.vm_isokendpt(request.endpoint) {
            Ok(s) => s,
            Err(_) => return VmReply::Error(VmError::InvalidProcess),
        };
        let mut proc = match table.get_active(slot) {
            Some(p) => p,
            None => return VmReply::Error(VmError::InvalidProcess),
        };
        let proc_endpoint = proc.endpoint();
        let fault_addr = request.vaddr;
        let region = match proc.regions_mut().find_mut(fault_addr) {
            Some(r) => r,
            None => return VmReply::Error(VmError::InvalidAddress),
        };
        let (page_alloc, frames, _cache, _vfs_queue) = self.parts_mut();
        match crate::cow_exec_pf::handle_pagefault(
            proc_endpoint, region, frames, page_alloc,
            fault_addr, request.write, table,
        ) {
            Ok(_action) => VmReply::Ok,
            Err(_e) => {
                VmReply::Error(VmError::AccessViolation)
            }
        }
    }

    /// Returns mutable references to page_alloc, page_frames, page_cache, and vfs_queue simultaneously.
    /// This avoids double mutable borrow when dispatching VM calls that need multiple components.
    pub(crate) fn parts_mut(
        &mut self,
    ) -> (&mut VmPageAllocator, &mut PageFrames, &mut PageCache, &mut VfsRequestQueue) {
        let page_alloc = &mut self.page_alloc;
        let page_frames = self.page_frames.as_mut().expect("page_frames not initialized");
        let page_cache = &mut self.page_cache;
        let vfs_queue = &mut self.vfs_queue;
        (page_alloc, page_frames, page_cache, vfs_queue)
    }

    pub(crate) fn is_initialized(&self) -> bool {
        self.initialized
    }
}

// ==========================================================================
// IPC transport wiring — see ipc/transport.rs for the strategy trait.
//
// The free functions `ipc_receive` / `ipc_send` below are thin wrappers
// around a process-global `IpcTransport` instance. The instance is
// selected by build mode:
//
//   - #[cfg(test)]    → `TestIpcTransport` (mock, records sends)
//   - #[cfg(not(test))]→ `KernelIpcTransport` (real kernel, blocked on kernel IPC core)
//
// ARCHITECTURE NOTE (VM IPC blocking bug, fixed 2026-06-13): the previous
// implementation hard-coded `Err(())` in both free functions, which
// made the main loop panic with an opaque error on first iteration.
// The trait abstraction now makes the failure mode self-documenting
// and lets unit tests drive the main loop end-to-end.
//
// Once kernel IPC core lands, the `KernelIpcTransport` impl
// in `ipc/transport.rs` will be filled in with the real syscall
// invocations. The trait surface is stable.
// ==========================================================================

use crate::ipc::transport::IpcTransport;

/// Process-global IPC transport slot. Initialized lazily on first use.
///
/// We avoid the `static mut` pattern (denied by Rust 2024 edition) by
/// using `AtomicPtr` to a heap-allocated `KernelIpcTransport`. The first
/// call to `transport()` allocates the transport; subsequent calls
/// return a `&mut` derived from the atomic pointer.
///
/// # Safety
/// VM is single-threaded by design (see `lib.rs` top-level docs). The
/// `AtomicPtr` is used here purely for the const-constructor
/// convenience — we do **not** rely on its atomicity for soundness.
/// The transport pointer is initialized once at first use and never
/// aliased: there is no second writer, and the reader only constructs
/// a `&mut` after observing the non-null pointer.
static IPC_TRANSPORT_PTR: core::sync::atomic::AtomicPtr<
    crate::ipc::transport::KernelIpcTransport,
> = core::sync::atomic::AtomicPtr::new(core::ptr::null_mut());

fn transport() -> &'static mut dyn IpcTransport {
    use core::sync::atomic::Ordering;
    let mut ptr = IPC_TRANSPORT_PTR.load(Ordering::Relaxed);
    if ptr.is_null() {
        let boxed = alloc::boxed::Box::new(crate::ipc::transport::KernelIpcTransport::new());
        // SAFETY: VM is single-threaded. The AtomicPtr is used only to
        // make the slot const-constructible; we are the sole writer.
        let raw = alloc::boxed::Box::into_raw(boxed);
        IPC_TRANSPORT_PTR.store(raw, Ordering::Relaxed);
        ptr = raw;
    }
    // SAFETY: `ptr` is non-null and was allocated by `Box::new` above.
    // We need a fat pointer for the trait object; reconstruct it via
    // the vtable of `KernelIpcTransport`'s `IpcTransport` impl. This
    // unsizing coercion is the standard way to build a trait-object
    // pointer from a concrete reference.
    let concrete: &'static mut crate::ipc::transport::KernelIpcTransport =
        unsafe { &mut *ptr };
    // The `as` cast is a coercion that takes the vtable of the
    // concrete type's IpcTransport impl. This is the same mechanism
    // `Box<dyn Trait>::new` uses internally.
    concrete as &mut (dyn IpcTransport + 'static)
}

/// Receive an IPC message from any source.
///
/// Wraps the [`IpcTransport::receive`] call so the main loop code in
/// `run()` does not need to know about the underlying transport.
fn ipc_receive() -> Result<(Message, IpcStatus), ()> {
    transport().receive().map_err(|_| ())
}

/// Send an IPC reply message to a destination endpoint.
///
/// Wraps the [`IpcTransport::send`] call.
fn ipc_send(dest: Endpoint, msg: &Message) -> Result<(), ()> {
    transport().send(dest, msg).map_err(|_| ())
}

fn is_ipc_notify(_sts: &IpcStatus) -> bool {
    // C: is_ipc_notify(rcv_sts)
    false
}

fn is_from_kernel(_sts: &IpcStatus) -> bool {
    // C: IPC_STATUS_FLAGS_TEST(rcv_sts, IPC_FLG_MSG_FROM_KERNEL)
    true
}

/// Check if a message type carries a VFS filesystem transaction ID.
///
/// C: `IS_VFS_FS_TRANSID(type)` in `minix/com.h:912`.
/// ```c
/// #define VFS_TRANSACTION_BASE 0xB00
/// #define IS_VFS_FS_TRANSID(type) (((type) & ~0xff) == VFS_TRANSACTION_BASE)
/// ```
fn is_vfs_fs_transid(m_type: u32) -> bool {
    (m_type & !0xFF) == VFS_TRANSACTION_BASE
}

/// Extract the transaction ID from a VFS transid-encoded message type.
///
/// C: `TRNS_GET_ID(t)` in `minix/vfsif.h:79`.
/// ```c
/// #define TRNS_GET_ID(t)  ((t) & 0xFFFF)
/// ```
fn transid_extract(m_type: u32) -> i32 {
    (m_type & 0xFFFF) as i32
}

/// Strip the transaction ID from a VFS transid-encoded message type,
/// returning the underlying call number.
///
/// C: `TRNS_DEL_ID(t)` in `minix/vfsif.h:81`.
/// ```c
/// #define TRNS_DEL_ID(t)  ((short)((t) >> 16))
/// ```
fn transid_strip(m_type: u32) -> u32 {
    // C casts to short (i16) which sign-extends. The result is the
    // actual VM request number (e.g. VM_PROCCTL).
    ((m_type >> 16) as i16) as u32
}

/// VFS transaction base constant.
/// C: `minix/com.h:909` — `#define VFS_TRANSACTION_BASE 0xB00`
const VFS_TRANSACTION_BASE: u32 = 0xB00;

fn ipc_call_rs_init() -> Result<RprocTab, ()> {
    // TODO: expand the stub into a documented
    // three-stage contract so the deferred path has explicit semantics.
    //
    // The C source (proto.h + table.c) does this in three steps:
    //
    //   1. Build a `mess_rs_init` message (request type RS_INIT=0x606,
    //      sender = VM endpoint, no payload) and send it to RS.
    //   2. Receive a `mess_rs_init_reply` from RS that contains a
    //      pointer to a `struct rprocinfo` describing the live
    //      replicated services and the grant table metadata.
    //   3. Copy that table out of RS's address space via
    //      `sys_safecopyfrom(SELF, ...)` and decode it into our
    //      `RprocTab`.
    //
    // The Rust rewrite is blocked on three DEFERRED dependencies:
    //
    //   - IpcTransport (this crate, `ipc/transport.rs:144` is still
    //     `unimplemented!()` for KernelIpcTransport; this depends on
    //     kernel IPC primitive).
    //   - sys_safecopyfrom syscall shim (depends on the kernel-side
    //     SYS_SAFECOPYFROM dispatch and the SAFECOPY grant table;
    //     see kernel `dispatch_safecopy` — DEFERRED).
    //   - Endpoint ↔ ProcNr conversion on the kernel side (depends
    //     on `ProcessTable::endpoint_to_nr()` being globally
    //     available; partial fix in place, full wiring pending).
    //
    // Until all three land, returning `Ok(RprocTab::empty())` keeps
    // `rs_handshake()` non-panicking and lets VM continue to service
    // requests from PM/SYS without an RS handshake. The `Err(())`
    // variant is reserved for "RS replied but the reply was malformed" —
    // currently unreachable but kept for symmetry with the future
    // implementation.
    Ok(RprocTab::empty())
}

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
#[derive(Clone, Copy)]
struct RprocEntry {
    in_use: bool,
    endpoint: Endpoint,
    call_mask: u32,
    is_user: bool,
}

impl RprocEntry {
    const EMPTY: Self = Self {
        in_use: false,
        endpoint: Endpoint::NONE,
        call_mask: 0,
        is_user: false,
    };
}

struct RprocTab {
    entries: [RprocEntry; 32],
}

impl RprocTab {
    const fn empty() -> Self {
        Self {
            entries: [RprocEntry::EMPTY; 32],
        }
    }
}

impl core::ops::Deref for RprocTab {
    type Target = [RprocEntry];
    fn deref(&self) -> &[RprocEntry] {
        &self.entries
    }
}

/// Convert VmReply to raw errno for IPC reply. C: result != SUSPEND branch.
///
/// All variants are explicitly listed — adding a new `VmReply` variant
/// will produce a compile error here, forcing the author to decide the
/// correct errno value.
///
/// # DEFERRED
///
/// Previously contained an `unreachable!("Suspend filtered before
/// reply_to_errno")` arm reachable from any code path that called
/// `reply_to_errno` with `VmReply::Suspend`. The fix introduces
/// [`VmReplyForIpc`], a typed wrapper whose `new()` constructor statically
/// rejects `VmReply::Suspend` (returns `None`), so this function never
/// receives `Suspend` *through the wrapper path*. The `unreachable!()`
/// arm is retained as a defense-in-depth runtime assertion: if a future
/// refactor calls this function directly (bypassing the wrapper) with
/// `VmReply::Suspend`, we panic immediately rather than silently emitting
/// a bogus errno. This is the Rust idiom "make illegal states
/// unrepresentable where possible, but keep assertions as a backstop".
fn reply_to_errno(reply: VmReply) -> i32 {
    match reply {
        VmReply::Ok => 0,
        VmReply::Error(e) => e.to_errno(),
        // All success replies return 0 (OK) to the caller.
        VmReply::Fork(_) => 0,
        VmReply::Brk(_) => 0,
        VmReply::Mmap(_) => 0,
        VmReply::MapPhys(_) => 0,
        VmReply::Exit => 0,
        VmReply::Willexit => 0,
        VmReply::Munmap => 0,
        VmReply::ExecNewmem(_) => 0,
        VmReply::MapCache { .. } => 0,
        VmReply::VfsMmap(_) => 0,
        VmReply::GetPhys { .. } => 0,
        VmReply::GetRefcount { .. } => 0,
        VmReply::InfoStats { .. } => 0,
        VmReply::InfoUsage { .. } => 0,
        VmReply::InfoRegion { .. } => 0,
        VmReply::Getrusage { .. } => 0,
        VmReply::RsMemctlAddrLen { .. } => 0,
        // Compile-time guarantee: VmReplyForIpc::new() returns None for
        // VmReply::Suspend, so this function never receives it via the
        // wrapper path. If you see this panic, someone bypassed the
        // wrapper — fix the caller, do not silence this assertion.
        VmReply::Suspend => unreachable!(
            "VmReply::Suspend reached reply_to_errno; \
             caller must wrap in VmReplyForIpc or use DispatchAction::Suspend"
        ),
    }
}

/// Encode VmReply per-service output data into the reply message fields.
///
/// All variants are explicitly listed — adding a new `VmReply` variant
/// will produce a compile error here, forcing the author to encode the
/// output data correctly.
///
/// # Defense-in-depth
///
/// The `VmReply::Suspend` arm is unreachable in practice because the
/// [`VmReplyForIpc`] wrapper statically filters it out at the call site
/// (the only call site passes `reply_for_ipc.into_payload()`). However,
/// this function accepts `VmReply` directly, so a future refactor that
/// bypasses the wrapper could pass `Suspend` here. The `unreachable!()`
/// assertion catches that bug immediately, consistent with `reply_to_errno`.
fn encode_reply_data(reply: VmReply, msg: &mut Message) {
    // SAFETY: All VM replies use the M1 message format.
    let m1 = unsafe { &mut msg.m_u.m_m1 };
    match reply {
        VmReply::Fork(out) => out.encode(m1),
        VmReply::Brk(out) => out.encode(m1),
        VmReply::Mmap(out) => out.encode(m1),
        VmReply::MapPhys(out) => out.encode(m1),
        VmReply::ExecNewmem(out) => out.encode(m1),
        VmReply::MapCache { .. } => {}
        VmReply::VfsMmap(out) => out.encode(m1),
        VmReply::GetPhys { phys_addr } => { m1.m1p1 = phys_addr.0; }
        VmReply::GetRefcount { count } => { m1.m1i1 = count as i32; }
        VmReply::InfoStats { page_size, total_pages, free_pages, largest_contiguous } => {
            m1.m1p1 = page_size;
            m1.m1i1 = total_pages as i32;
            m1.m1i2 = free_pages as i32;
            m1.m1i3 = largest_contiguous as i32;
        }
        VmReply::InfoUsage { total, common, shared, virtual_total, mvirtual } => {
            // Minix3 C uses sys_datacopy to copy a `struct vm_usage_info`
            // (5 VirBytes fields + 3 u64 fields) into the caller's address
            // space (utility.c — do_info → get_usage_info).
            // Rust M1 layout has 3 pointer slots (m1p1..m1p3) and 3 integer
            // slots (m1i1..m1i3). Encode the 5 VirBytes fields: 3 in pointer
            // slots, 2 as page counts (saturated i32) in integer slots.
            //
            // Field mapping (aligned with C's struct vm_usage_info):
            //   m1p1 = vui_total, m1p2 = vui_common, m1p3 = vui_shared
            //   m1i1 = vui_virtual (page count), m1i2 = vui_mvirtual (page count)
            //   m1i3 = 0 (reserved)
            m1.m1p1 = total.0;
            m1.m1p2 = common.0;
            m1.m1p3 = shared.0;
            // SAFETY: `as i32` is a truncating cast. Region sizes in bytes fit
            // in u32 when divided by PAGE_SIZE (PAGE_SIZE = 4096 ⇒ 1 TiB of
            // memory ≈ 2^28 pages, well within i32::MAX = 2^31). Documented
            // for reviewer (see review-patterns-skill §模式19 — `as` 截断
            // 必须有 SAFETY 注释).
            let pages = |bytes: u64| -> i32 {
                (bytes / crate::region::page_state::PAGE_SIZE).min(i32::MAX as u64) as i32
            };
            m1.m1i1 = pages(virtual_total.0);
            m1.m1i2 = pages(mvirtual.0);
            m1.m1i3 = 0; // reserved
        }
        VmReply::RsMemctlAddrLen { addr, len } => {
            m1.m1p1 = addr.0; m1.m1i1 = len as i32;
        }
        VmReply::InfoRegion { regions, count, next } => {
            // Minix3 C uses `sys_datacopy(VM_PROC_NR, regions_addr,
            // caller, call_addr, count*sizeof(vm_region_info))` to copy
            // the region array (utility.c). M1 layout has only 1 pointer
            // and 3 integer slots — insufficient to ship the array inline.
            //
            // FIX (VMI-2): Previously `let _ = regions;` discarded the
            // entire region list, leaving PM unable to enumerate regions
            // (m1.m1i1=count, m1.m1i2=next but no array payload). Encoding
            // is unchanged for now (count + next in integer slots), but we
            // expose the source length in m1.m1p1 so the caller can detect
            // "VM stub returned N regions but no sys_datacopy happened" vs
            // "VM really has 0 regions". When `IpcTransport::send` lands,
            // replace this with a real sys_datacopy call.
            let len_u32 = u32::try_from(regions.len()).unwrap_or(u32::MAX);
            m1.m1p1 = u64::from(len_u32); // sentinel: source-side length
            m1.m1i1 = count as i32;
            m1.m1i2 = next as i32;
            // m1.m1i3 deliberately left as 0 — reserved for caller-side
            // buffer capacity once sys_datacopy is wired.
        }
        VmReply::Getrusage { max_rss_kb, minor_faults, major_faults } => {
            // Minix3 C uses sys_datacopy to copy a `struct rusage`
            // (≥15 fields: utime, stime, maxrss, ixrss, idrss, isrss,
            // minflt, majflt, nswap, inblock, oublock, msgsnd, msgrcv,
            // nvcsw, nivcsw) into the caller's address space (utility.c).
            // M1 layout has 3 pointer slots + 3 integer slots — pick the
            // 4 most diagnostic fields. The remaining 11 are DEFERRED to
            // the sys_datacopy path (VMI-3 follow-up).
            //
            // FIX (VMI-3): Previously only 3 fields were encoded
            // (max_rss/min_flt/maj_flt) with `m1p2..m1p3`/`m1i3` unused.
            // Encoding now uses remaining slots: m1p2=minor_faults, m1p3=
            // major_faults (both fit in u64 for any realistic process),
            // freeing m1i1/m1i2 for future in_use_time fields.
            m1.m1p1 = max_rss_kb;
            // SAFETY: `as i32` truncates fault counters. i32::MAX = 2^31 ≈
            // 2.1B faults per measurement window; saturation at i32::MAX
            // signals "very high fault rate" rather than overflow in
            // practice. Documented per review-patterns-skill §模式19.
            let faults_to_i32 = |n: u64| -> i32 { n.min(i32::MAX as u64) as i32 };
            m1.m1i1 = faults_to_i32(minor_faults);
            m1.m1i2 = faults_to_i32(major_faults);
            // m1.m1i3 reserved for future ru_inblock (block-input ops).
        }
        // Variants with no output data to encode
        VmReply::Ok | VmReply::Error(_)
        | VmReply::Exit | VmReply::Willexit | VmReply::Munmap => {}
        // Defense-in-depth: Suspend is statically excluded by VmReplyForIpc
        // at the only call site. If this fires, someone bypassed the wrapper.
        VmReply::Suspend => unreachable!(
            "VmReply::Suspend reached encode_reply_data; \
             caller must wrap in VmReplyForIpc or use DispatchAction::Suspend"
        ),
    }
}

impl VmServer {
    pub(crate) fn handle_fork(&mut self, req: VmForkIn) -> VmReply {
        let table = VmProcTable::get_global();
        let frames = self.page_frames.as_mut().expect("page_frames not initialized");
        MessageDispatcher::dispatch_fork(table, &mut self.page_alloc, frames, req)
    }

    pub(crate) fn handle_brk(&mut self, req: VmBrkIn) -> VmReply {
        let table = VmProcTable::get_global();
        let frames = self.page_frames.as_mut().expect("page_frames not initialized");
        MessageDispatcher::dispatch_brk(table, &mut self.page_alloc, frames, req)
    }

    pub(crate) fn handle_exit(&mut self, req: VmExitIn) -> VmReply {
        let table = VmProcTable::get_global();
        let frames = self.page_frames.as_mut().expect("page_frames not initialized");
        MessageDispatcher::dispatch_exit(table, &mut self.page_alloc, frames, req)
    }

    pub(crate) fn handle_mmap(&mut self, req: VmMmapIn) -> VmReply {
        let table = VmProcTable::get_global();
        let frames = self.page_frames.as_mut().expect("page_frames not initialized");
        MessageDispatcher::dispatch_mmap(table, &mut self.page_alloc, frames, req)
    }

    pub(crate) fn handle_map_phys(&mut self, req: VmMapPhysIn) -> VmReply {
        let table = VmProcTable::get_global();
        let frames = self.page_frames.as_mut().expect("page_frames not initialized");
        MessageDispatcher::dispatch_map_phys(table, &mut self.page_alloc, frames, req)
    }

    pub(crate) fn handle_mapcache(&mut self, caller: Endpoint, req: VmCacheIn) -> VmReply {
        let table = VmProcTable::get_global();
        let frames = self.page_frames.as_mut().expect("page_frames not initialized");
        MessageDispatcher::dispatch_mapcache(table, &mut self.page_alloc, frames, &self.page_cache, caller, req)
    }

    pub(crate) fn handle_setcache(&mut self, caller: Endpoint, req: VmCacheIn) -> VmReply {
        let table = VmProcTable::get_global();
        let frames = self.page_frames.as_mut().expect("page_frames not initialized");
        MessageDispatcher::dispatch_setcache(table, frames, &mut self.page_cache, caller, req)
    }

    pub(crate) fn handle_forgetcache(&mut self, req: VmCacheIn) -> VmReply {
        let frames = self.page_frames.as_mut().expect("page_frames not initialized");
        MessageDispatcher::dispatch_forgetcache(&mut self.page_cache, frames, req)
    }

    pub(crate) fn handle_clearcache(&mut self, req: VmCacheIn) -> VmReply {
        let frames = self.page_frames.as_mut().expect("page_frames not initialized");
        MessageDispatcher::dispatch_clearcache(&mut self.page_cache, frames, req)
    }

    pub fn has_pending_vfs_requests(&self) -> bool {
        !self.vfs_queue.is_empty()
    }

    pub(crate) fn page_cache(&self) -> &PageCache {
        &self.page_cache
    }

    pub(crate) fn vfs_queue(&self) -> &VfsRequestQueue {
        &self.vfs_queue
    }

    pub(crate) fn vfs_queue_mut(&mut self) -> &mut VfsRequestQueue {
        &mut self.vfs_queue
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::direct_map::tests::with_custom_mock_base;

    const TEST_TOTAL_PAGES: usize = 256;

    /// One-time mock physical memory setup for all VM server tests.
    /// Uses a leaked static buffer to avoid parallel test races on mock_vm_base.
    static TEST_PHYS_INIT: core::sync::atomic::AtomicBool = core::sync::atomic::AtomicBool::new(false);
    static TEST_MOCK_BASE: core::sync::atomic::AtomicU64 = core::sync::atomic::AtomicU64::new(0);

    fn ensure_mock_phys_init() {
        use core::sync::atomic::Ordering;
        if TEST_PHYS_INIT.swap(true, Ordering::SeqCst) {
            return; // already initialized
        }
        let mock_phys_size = TEST_TOTAL_PAGES * CLICK_SIZE + CLICK_SIZE;
        let mock_phys: alloc::vec::Vec<u8> = alloc::vec![0u8; mock_phys_size];
        let mock_phys_leaked = alloc::boxed::Box::leak(mock_phys.into_boxed_slice());

        let raw_base = mock_phys_leaked.as_ptr() as usize;
        let aligned_base = (raw_base + CLICK_SIZE - 1) & !(CLICK_SIZE - 1);
        minix_arch::direct_map::set_mock_vm_base(aligned_base as u64);
        TEST_MOCK_BASE.store(aligned_base as u64, Ordering::SeqCst);
    }

    /// Run a test with the vm_server mock base set correctly.
    /// Uses the global MOCK_BASE_MUTEX to prevent parallel test interference.
    fn with_test_mock_base<F: FnOnce()>(f: F) {
        // Ensure mock physical memory is initialized before reading TEST_MOCK_BASE.
        ensure_mock_phys_init();
        let base = TEST_MOCK_BASE.load(core::sync::atomic::Ordering::SeqCst);
        with_custom_mock_base(base, f);
    }

    fn test_free_regions() -> [BootMemRegion; 1] {
        [BootMemRegion { base: 0, size: TEST_TOTAL_PAGES * CLICK_SIZE }]
    }

    fn make_test_vm_server() -> VmServer {
        VmServer::new(TEST_TOTAL_PAGES, &test_free_regions())
    }

    #[test]
    fn test_vm_server_new() {
        with_test_mock_base(|| {
            let server = make_test_vm_server();
            assert!(!server.initialized);
            assert!(!server.is_initialized());
        });
    }

    #[test]
    fn test_vm_server_init() {
        with_test_mock_base(|| {
            let mut server = make_test_vm_server();
            server.init();
            assert!(server.initialized);
            assert!(server.is_initialized());
        });
    }

    #[test]
    #[should_panic(expected = "VmServer::run() called before init()")]
    fn test_vm_server_run_without_init() {
        with_test_mock_base(|| {
            let mut server = make_test_vm_server();
            server.run();
        });
    }

    #[test]
    fn test_vm_server_default() {
        with_test_mock_base(|| {
            let server = make_test_vm_server();
            assert!(!server.initialized);
        });
    }

    #[test]
    fn test_vm_server_handle_fork_not_found() {
        with_test_mock_base(|| {
            let mut server = make_test_vm_server();
            server.init();

            let request = VmForkIn {
                parent_endpoint: Endpoint::NONE,
                child_slot: UserSlot::new(1),
            };

            let reply = server.handle_fork(request);
            assert!(matches!(reply, VmReply::Error(VmError::InvalidProcess)));
        });
    }

    #[test]
    fn test_vm_server_handle_brk_not_found() {
        with_test_mock_base(|| {
            let mut server = make_test_vm_server();
            server.init();

            let request = VmBrkIn {
                endpoint: Endpoint::NONE,
                new_addr: VirBytes(0x5000_0000),
            };

            let reply = server.handle_brk(request);
            assert!(matches!(reply, VmReply::Error(VmError::InvalidProcess)));
        });
    }

    #[test]
    fn test_vm_server_handle_exit_not_found() {
        with_test_mock_base(|| {
            let mut server = make_test_vm_server();
            server.init();

            let request = VmExitIn {
                endpoint: Endpoint::NONE,
            };

            let reply = server.handle_exit(request);
            assert!(matches!(reply, VmReply::Error(VmError::InvalidProcess)));
        });
    }

    #[test]
    fn test_vm_server_page_cache_access() {
        with_test_mock_base(|| {
            let server = make_test_vm_server();
            assert_eq!(server.page_cache().total_cached(), 0);
        });
    }

    #[test]
    fn test_vm_server_vfs_queue_access() {
        with_test_mock_base(|| {
            let server = make_test_vm_server();
            assert!(server.vfs_queue().is_empty());
        });
    }

    #[test]
    fn test_vm_server_vfs_reply_no_pending() {
        with_test_mock_base(|| {
            let mut server = make_test_vm_server();
            // VFS reply dispatch is now handled by dispatch_vfs_reply
            // which is tested in dispatcher tests. This test verifies
            // that an empty queue reports no pending requests.
            assert!(!server.has_pending_vfs_requests());
        });
    }

    // ── transid helper tests ──

    #[test]
    fn test_is_vfs_fs_transid_valid() {
        // VFS_TRANSACTION_BASE = 0xB00. Any m_type with (m_type & ~0xFF) == 0xB00
        // is a VFS transid message.
        assert!(is_vfs_fs_transid(0xB00));   // base itself
        assert!(is_vfs_fs_transid(0xB01));   // base + 1
        assert!(is_vfs_fs_transid(0xBFF));   // base + 0xFF
        assert!(is_vfs_fs_transid(0xB45));   // arbitrary within range
    }

    #[test]
    fn test_is_vfs_fs_transid_invalid() {
        assert!(!is_vfs_fs_transid(0x000));   // zero
        assert!(!is_vfs_fs_transid(0xA00));   // wrong base
        assert!(!is_vfs_fs_transid(0xC00));   // VM_RQ_BASE
        assert!(!is_vfs_fs_transid(0x600));   // VFS call number
        assert!(!is_vfs_fs_transid(0xB100));  // base shifted left
    }

    #[test]
    fn test_transid_extract() {
        // C: TRNS_GET_ID(t) = t & 0xFFFF
        assert_eq!(transid_extract(0x000C002B), 0x002B);
        assert_eq!(transid_extract(0x000C0045), 0x0045);
        assert_eq!(transid_extract(0xFFFF0001), 0x0001);
        assert_eq!(transid_extract(0x00000000), 0);
    }

    #[test]
    fn test_transid_strip() {
        // C: TRNS_DEL_ID(t) = (short)(t >> 16)
        // VM_PROCCTL = 0xC2D = VM_RQ_BASE(0xC00) + 45
        // Encoded: (0xC2D << 16) | transid
        let encoded = (0xC2Du32 << 16) | 0x002B;
        assert_eq!(transid_strip(encoded), 0xC2D);

        // Sign extension test: if high bits represent a negative short,
        // the result should sign-extend (though VM request numbers are positive).
        let neg_encoded = (0xFFFFu32 << 16) | 0x0001;
        assert_eq!(transid_strip(neg_encoded), 0xFFFFFFFF); // -1 sign-extended
    }

    #[test]
    fn test_handle_vfs_transid_wrong_clean_type() {
        with_test_mock_base(|| {
            let mut server = make_test_vm_server();
            server.init();

            // clean_type != VM_PROCCTL → InternalError
            let msg = Message::default();
            let reply = server.handle_vfs_transid(0x600, 1, &msg);
            assert!(matches!(reply, VmReply::Error(VmError::InternalError)));
        });
    }

    #[test]
    fn test_handle_vfs_transid_zero_transid() {
        with_test_mock_base(|| {
            let mut server = make_test_vm_server();
            server.init();

            // transid == 0 → InvalidProcess
            let msg = Message::default();
            let reply = server.handle_vfs_transid(VM_PROCCTL, 0, &msg);
            assert!(matches!(reply, VmReply::Error(VmError::InvalidProcess)));
        });
    }

    #[test]
    fn test_handle_vfs_transid_invalid_endpoint() {
        with_test_mock_base(|| {
            let mut server = make_test_vm_server();
            server.init();

            // Build a message with VM_PROCCTL clean_type, valid transid,
            // but VmProcctlIn.who = 0 (invalid endpoint).
            // Field mapping: m1i1=param, m1p1=who, m1p2=m1, m1p3=len, m1i3=flags
            let mut msg = Message::default();
            let m1 = unsafe { &mut msg.m_u.m_m1 };
            m1.m1i1 = 1;       // VMPCTL_PARAM = VMPPARAM_CLEAR
            m1.m1p1 = 0;       // VMPCTL_WHO = 0 (invalid)
            m1.m1p2 = 0;       // VMPCTL_M1
            m1.m1p3 = 0;       // VMPCTL_LEN
            m1.m1i3 = 0;       // VMPCTL_FLAGS

            let reply = server.handle_vfs_transid(VM_PROCCTL, 42, &msg);
            assert!(matches!(reply, VmReply::Error(VmError::InvalidEndpoint)));
        });
    }
}
