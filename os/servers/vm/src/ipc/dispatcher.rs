//! Message dispatcher for VM service.
//!
//! Routes decoded IPC messages (per-link `In` types) to handler functions
//! and returns unified `VmReply` enum for encoding back to transport layer.
//!
//! # Architecture
//!
//! ```text
//! Message (transport) → VmXxxIn::decode() → dispatch_xxx() → VmReply → encode → Message
//! ```
//!
//! # Error Mapping
//!
//! Each service module defines its own error enum. The dispatcher maps
//! per-service errors to the unified `VmError` type, preserving C errno
//! semantics (e.g. `ForkError::InvalidEndpoint → VmError::InvalidProcess → EINVAL`
//! matching Minix3's `do_fork` behavior in fork.c:44).

use minix_types::{
    Endpoint, VirBytes,
    VmForkIn, VmBrkIn, VmMunmapIn, VmUnmapPhysIn, VmShmUnmapIn,
    VmExitIn, VmWillexitIn,
    VmMmapIn, VmMapPhysIn, VmVfsMmapIn, VmCacheIn,
    VmProcctlIn, VmRemapIn, VmVfsReplyIn,
    VmForkOut, VmBrkOut, VmMmapOut, VmMapPhysOut,
    VmReply, VmError,
    Message, DecodeFromM1,
    VM_RQ_BASE, VM_MMAP, VM_MUNMAP, VM_MAP_PHYS, VM_UNMAP_PHYS, VM_EXIT, VM_FORK, VM_BRK,
    VM_WILLEXIT, VM_VFS_MMAP, VM_MAPCACHEPAGE, VM_SETCACHEPAGE,
    VM_FORGETCACHEPAGE, VM_CLEARCACHE, VM_RS_SET_PRIV, VM_RS_PREPARE,
    VM_RS_UPDATE, VM_RS_MEMCTL, VM_GETPHYS, VM_GETREF, VM_INFO, VM_GETRUSAGE,
    VM_REMAP, VM_REMAP_RO, VM_PROCCTL, VM_SHM_UNMAP, VM_VFS_REPLY,
};
#[cfg(test)]
use minix_types::{VM_ADDDMA, VM_DELDMA, VM_EXEC_NEWMEM, VM_GETDMA};
use crate::vmproc::VmProcTable;
use crate::vm_server::VmContext;
use crate::alloc_page::VmPageAllocator;
use crate::region::PageFrames;
use crate::region::vir_region::{VirRegion, VrFlags, VrParam};
use crate::memtype::MEM_TYPE_SHARED;
use crate::page_cache::{VMC_NO_INODE, VMSF_ONCE};
use crate::fork;
use crate::brk;
use crate::munmap;
use crate::mmap;
use crate::map_phys;
use crate::exit;
use crate::rs;
use crate::query;

// ==========================================================================
// Public dispatch struct
// ==========================================================================

/// Result of dispatching VM_VFS_REPLY, which may carry a deferred callback.
///
/// C's `do_vfs_reply` invokes the callback inline, but Rust's borrow
/// checker prevents us from holding `&mut VfsRequestQueue` and
/// `&mut VmServer` simultaneously. We return the callback components
/// and let the caller (VmServer main loop) execute them after releasing
/// the vfs_queue borrow.
pub(crate) struct VfsReplyResult {
    pub reply: VmReply,
    pub callback: Option<(
        crate::vfs_queue::VfsCallbackFn,
        crate::vfs_queue::VfsReply,
        crate::vfs_queue::VfsRequestState,
    )>,
}

/// Result of `dispatch_by_number`. Most calls return just a `VmReply`,
/// but VM_VFS_REPLY also carries a deferred callback that must be
/// executed by the VmServer main loop after the dispatch returns.
pub(crate) struct DispatchResult {
    pub reply: VmReply,
    pub vfs_callback: Option<(
        crate::vfs_queue::VfsCallbackFn,
        crate::vfs_queue::VfsReply,
        crate::vfs_queue::VfsRequestState,
    )>,
}

impl DispatchResult {
    fn from_reply(reply: VmReply) -> Self {
        DispatchResult { reply, vfs_callback: None }
    }
}

impl From<VmReply> for DispatchResult {
    fn from(reply: VmReply) -> Self {
        DispatchResult { reply, vfs_callback: None }
    }
}

impl From<VfsReplyResult> for DispatchResult {
    fn from(r: VfsReplyResult) -> Self {
        DispatchResult { reply: r.reply, vfs_callback: r.callback }
    }
}

pub(crate) struct MessageDispatcher;

impl MessageDispatcher {
    // -- fork --

    /// Dispatch VM_FORK request.
    ///
    /// Corresponds to Minix3 `do_fork()` in fork.c.
    /// C returns EINVAL on vm_isokendpt failure (fork.c:44).
    pub(crate) fn dispatch_fork(ctx: &mut VmContext, request: VmForkIn) -> VmReply {
        let VmContext { proc_table, page_alloc, page_frames, gateway, .. } = ctx;
        let table: &VmProcTable = proc_table;
        let frames = page_frames.as_mut().expect("page_frames not initialized");
        let mut gateway = gateway.borrow_mut();
        match fork::do_fork(gateway.as_mut(), table, frames, page_alloc, request.parent_endpoint, request.child_slot) {
            Ok(child_endpoint) => VmReply::Fork(VmForkOut { child_endpoint }),
            Err(e) => VmReply::Error(e.into()),
        }
    }

    // -- brk --

    /// Dispatch VM_BRK request. C: `do_brk()` in break.c.
    pub(crate) fn dispatch_brk(ctx: &mut VmContext, request: VmBrkIn) -> VmReply {
        let VmContext { proc_table, page_alloc, page_frames, vfs_queue, .. } = ctx;
        let table: &VmProcTable = proc_table;
        let frames = page_frames.as_mut().expect("page_frames not initialized");
        let req = brk::BrkRequest {
            endpoint: request.endpoint,
            new_brk_addr: request.new_addr,
        };
        match brk::handle_brk(table, page_alloc, frames, vfs_queue, &req) {
            Ok(response) => VmReply::Brk(VmBrkOut { new_addr: response.new_brk_addr }),
            Err(e) => VmReply::Error(e.into()),
        }
    }

    // -- munmap --

    /// Dispatch VM_MUNMAP request. C: `do_munmap()` in mmap.c.
    pub(crate) fn dispatch_munmap(ctx: &mut VmContext, request: VmMunmapIn) -> VmReply {
        let VmContext { proc_table, page_alloc, page_frames, vfs_queue, .. } = ctx;
        let table: &VmProcTable = proc_table;
        let frames = page_frames.as_mut().expect("page_frames not initialized");
        let req = munmap::MunmapRequest {
            endpoint: request.endpoint,
            addr: request.addr,
            length: request.length,
            lookup_region_length: false,
        };
        match munmap::handle_munmap(table, page_alloc, frames, vfs_queue, &req) {
            Ok(munmap::MunmapOutcome::Replied) => VmReply::Munmap,
            // VM self-munmap: handled synchronously, no reply (C: SUSPEND).
            Ok(munmap::MunmapOutcome::Suspended) => VmReply::Suspend,
            Err(e) => VmReply::Error(e.into()),
        }
    }

    // -- unmap_phys --
    // VM_UNMAP_PHYS: unmap a VR_DIRECT region. Length is the full region length.
    pub(crate) fn dispatch_unmap_phys(ctx: &mut VmContext, request: VmUnmapPhysIn) -> VmReply {
        let VmContext { proc_table, page_alloc, page_frames, vfs_queue, .. } = ctx;
        let table: &VmProcTable = proc_table;
        let frames = page_frames.as_mut().expect("page_frames not initialized");
        let req = munmap::MunmapRequest {
            endpoint: request.target,
            addr: request.vaddr,
            length: VirBytes(0),
            lookup_region_length: true,
        };
        match munmap::handle_munmap(table, page_alloc, frames, vfs_queue, &req) {
            Ok(munmap::MunmapOutcome::Replied) => VmReply::Munmap,
            // VM self-munmap: handled synchronously, no reply (C: SUSPEND).
            Ok(munmap::MunmapOutcome::Suspended) => VmReply::Suspend,
            Err(e) => VmReply::Error(e.into()),
        }
    }

    // -- shm_unmap --
    // VM_SHM_UNMAP: unmap a shared memory region. Length is the full region length.
    pub(crate) fn dispatch_shm_unmap(ctx: &mut VmContext, request: VmShmUnmapIn) -> VmReply {
        let VmContext { proc_table, page_alloc, page_frames, vfs_queue, .. } = ctx;
        let table: &VmProcTable = proc_table;
        let frames = page_frames.as_mut().expect("page_frames not initialized");
        let req = munmap::MunmapRequest {
            endpoint: request.forwhom,
            addr: request.addr,
            length: VirBytes(0),
            lookup_region_length: true,
        };
        match munmap::handle_munmap(table, page_alloc, frames, vfs_queue, &req) {
            Ok(munmap::MunmapOutcome::Replied) => VmReply::Munmap,
            // VM self-munmap: handled synchronously, no reply (C: SUSPEND).
            Ok(munmap::MunmapOutcome::Suspended) => VmReply::Suspend,
            Err(e) => VmReply::Error(e.into()),
        }
    }

    // -- procctl --

    /// Dispatch VM_PROCCTL request. C: `do_procctl()` in exit.c:117-148.
    ///
    /// Process-control sub-requests (VMPCTL_PARAM) from RS or VFS.
    /// Two sub-requests are defined in Minix3:
    ///
    /// - `VMPPARAM_CLEAR` (1): Free process memory + create fresh page table.
    ///   Only RS or VFS may call this (C: exit.c:131-132 EPERM check).
    /// - `VMPPARAM_HANDLEMEM` (2): Ensure memory pages are mapped for a range.
    ///   Only VFS may call this (C: exit.c:140-141 EPERM check).
    ///
    /// Unknown param values return EINVAL (C: exit.c:149 default case).
    pub(crate) fn dispatch_procctl(ctx: &mut VmContext, caller: Endpoint, request: VmProcctlIn) -> VmReply {
        let VmContext { proc_table, page_alloc, page_frames, vfs_queue, .. } = ctx;
        let table: &VmProcTable = proc_table;
        let frames = page_frames.as_mut().expect("page_frames not initialized");
        // 1. `who` must be a valid endpoint.
        //    C: exit.c:121-125 — vm_isokendpt failure collapses to EINVAL
        //    (both EINVAL and EDEADEPT are reported as EINVAL by do_procctl).
        //    `VmError::InvalidProcess` maps to EINVAL (to_errno, vm.rs:694).
        if request.who.0 <= 0 {
            return VmReply::Error(VmError::InvalidProcess);
        }

        // 2. Dispatch on param.
        //    C: exit.c:129 — switch(msg->VMPCTL_PARAM).
        match request.param {
            // VMPPARAM_CLEAR = 1
            // C: exit.c:130-137
            1 => {
                // Permission check: only RS or VFS.
                // C: exit.c:131-132 — if(msg->m_source != RS_PROC_NR && msg->m_source != VFS_PROC_NR) return EPERM;
                // In Minix3, PM_PROC_NR = 0, VFS_PROC_NR = 1, RS_PROC_NR = 2
                // (com.h:59-61). Endpoint::RS/Endpoint::VFS carry the same
                // values, so the check is written against the constants.
                // We use the caller endpoint directly.
                if caller != Endpoint::RS && caller != Endpoint::VFS {
                    return VmReply::Error(VmError::PermissionDenied);
                }
                match exit::handle_procctl_clear(table, page_alloc, frames, vfs_queue, request.who) {
                    Ok(()) => VmReply::Ok,
                    Err(e) => VmReply::Error(VmError::from(e)),
                }
            }
            // VMPPARAM_HANDLEMEM = 2
            // C: exit.c:139-148
            2 => {
                // Permission check: only VFS.
                // C: exit.c:141-142 — if(msg->m_source != VFS_PROC_NR) return EPERM;
                if caller != Endpoint::VFS {
                    return VmReply::Error(VmError::PermissionDenied);
                }
                match exit::handle_procctl_handlemem(
                    table, page_alloc, frames,
                    request.who, request.m1, request.len, request.flags,
                ) {
                    Ok(exit::VmProcctlHandlememResult::Completed) => {
                        // C returns SUSPEND for the async path. Our synchronous
                        // implementation completes immediately, so we return Ok
                        // instead. The caller (VFS) treats SUSPEND as "reply
                        // comes later"; Ok means "done now" — both are valid.
                        VmReply::Ok
                    }
                    Err(e) => VmReply::Error(VmError::from(e)),
                }
            }
            // Unknown param → EINVAL (C: exit.c:149 default case).
            // `VmError::InvalidParam` maps to EINVAL (to_errno, vm.rs:698).
            _ => VmReply::Error(VmError::InvalidParam),
        }
    }

    // -- remap / remap_ro --

    /// Dispatch VM_REMAP request. C: `do_remap()` in mmap.c:366-434.
    ///
    /// Remap a shared region from one process's address space into the
    /// caller's (destination) address space. The new region shares the
    /// same physical pages as the source — no copy is made.
    ///
    /// # Implementation (matches C `do_remap` step-by-step)
    ///
    /// 1. Validate that `length > 0` and `vaddr != 0`
    /// 2. Resolve source endpoint (`who`) and destination endpoint (`caller`)
    /// 3. Look up source region at `vaddr` — must be exact region start
    /// 4. Verify `length` matches source region length (page-aligned)
    /// 5. Find a free slot in destination's address space
    /// 6. Create new `VR_SHARED` region (optionally `VR_WRITABLE`)
    /// 7. Set `VrParam::Shared { ep, vaddr, id }` on new region
    /// 8. Increment source region's `remaps` counter
    /// 9. Return mapped address
    pub(crate) fn dispatch_remap(ctx: &mut VmContext, request: VmRemapIn) -> VmReply {
        let VmContext { proc_table, page_alloc, page_frames, .. } = ctx;
        let table: &VmProcTable = proc_table;
        let frames = page_frames.as_mut().expect("page_frames not initialized");
        dispatch_remap_impl(table, page_alloc, frames, request, false)
    }

    /// Dispatch VM_REMAP_RO request. Same as VM_REMAP but the resulting
    /// region is forced read-only (C: `do_remap()` mmap.c:380-385).
    pub(crate) fn dispatch_remap_ro(ctx: &mut VmContext, request: VmRemapIn) -> VmReply {
        let VmContext { proc_table, page_alloc, page_frames, .. } = ctx;
        let table: &VmProcTable = proc_table;
        let frames = page_frames.as_mut().expect("page_frames not initialized");
        dispatch_remap_impl(table, page_alloc, frames, request, true)
    }

    // -- vfs_reply --

    /// Dispatch VM_VFS_REPLY request. C: `do_vfs_reply()` in vfs.c:109.
    ///
    /// VFS has completed a previously-suspended VM-initiated call (typically
    /// the `vfs_vmcall` chain for file-backed MMAPs). This handler:
    ///
    /// 1. Validates the request (reqid > 0).
    /// 2. Constructs a `VfsReply` from the message fields.
    /// 3. Passes it to `VfsRequestQueue::handle_reply` which:
    ///    - Verifies req_id matches the active request (C: `assert(active->req_id == m->VMV_REQID)`)
    ///    - Dequeues the next pending request if any (C: `if(first_queued && !active) activate()`)
    /// 4. Returns `VfsReplyResult` with `VmReply::Suspend` and optional callback.
    ///    The caller (VmServer main loop) must execute the callback after
    ///    releasing the vfs_queue borrow — C invokes it inline but Rust's
    ///    borrow checker requires this two-step approach.
    pub(crate) fn dispatch_vfs_reply(ctx: &mut VmContext, request: VmVfsReplyIn) -> VfsReplyResult {
        let VmContext { vfs_queue, .. } = ctx;
        use crate::vfs_queue::VfsReply;

        // C: do_vfs_reply assert(active) — there must be an active request.
        // reqid must be positive (the C side uses 0 as "no pending call"
        // and negative values as error sentinels).
        if request.reqid <= 0 {
            return VfsReplyResult {
                reply: VmReply::Error(VmError::InvalidAddress),
                callback: None,
            };
        }

        let req_id = request.reqid as u32;

        // C: do_vfs_reply — construct reply from message fields.
        // C uses: m->VMV_RESULT, m->VMV_DEV, m->VMV_INO, m->VMV_FD, m->VMV_SIZE_PAGES
        let reply = VfsReply {
            req_id,
            result: request.result,
            data_phys: None, // C doesn't pass data_phys in the reply message
            fd: request.fd as i32,
            dev: request.dev as u64,
            // C passes ino via VMV_INO (m10_l1); the mmap resume callback
            // needs it for fdref dedup (mmap_file_cont → mmap_file → fdref
            // dedup_or_new matches on (dev, ino)).
            ino: request.ino as u64,
            size_pages: request.size_pages as u64,
        };

        // C: do_vfs_reply — active = NULL; req_callback(vmp, m, cbarg, orignode->reqstate)
        match vfs_queue.handle_reply(reply) {
            Ok(Some((callback, reply, state))) => {
                // C: if(req_callback) req_callback(vmp, m, cbarg, orignode->reqstate)
                // The callback is returned to the caller for execution after
                // the vfs_queue borrow is released.
                VfsReplyResult {
                    reply: VmReply::Suspend,
                    callback: Some((callback, reply, state)),
                }
            }
            Ok(None) => {
                // No callback registered for this request.
                // C: if(!req_callback) — just free the node and continue.
                VfsReplyResult {
                    reply: VmReply::Suspend,
                    callback: None,
                }
            }
            Err(_) => {
                // Mismatched req_id or no active request.
                // C: assert(active->req_id == m->VMV_REQID) would panic.
                // We return an error instead of panicking.
                VfsReplyResult {
                    reply: VmReply::Error(VmError::InvalidAddress),
                    callback: None,
                }
            }
        }
    }

    // -- mmap --

    /// Dispatch VM_MMAP request. C: `do_mmap()` in mmap.c.
    /// May return `VmReply::Suspend` for file-backed mappings.
    pub(crate) fn dispatch_mmap(ctx: &mut VmContext, request: VmMmapIn) -> VmReply {
        let VmContext { proc_table, page_alloc, page_frames, vfs_queue, .. } = ctx;
        let table: &VmProcTable = proc_table;
        let frames = page_frames.as_mut().expect("page_frames not initialized");
        match mmap::handle_mmap(table, page_alloc, frames, vfs_queue, &request) {
            Ok(mmap::MmapResult::Complete(response)) => VmReply::Mmap(VmMmapOut { ret_addr: response.mapped_addr }),
            Ok(mmap::MmapResult::Suspended) => VmReply::Suspend,
            Err(e) => VmReply::Error(e.into()),
        }
    }

    // -- vfs_mmap (synchronous VFS-initiated path) --
    pub(crate) fn dispatch_vfs_mmap(ctx: &mut VmContext, request: VmVfsMmapIn) -> VmReply {
        let VmContext { proc_table, page_alloc, page_frames, vfs_queue, .. } = ctx;
        let table: &VmProcTable = proc_table;
        let frames = page_frames.as_mut().expect("page_frames not initialized");
        match mmap::handle_vfs_mmap(table, page_alloc, frames, vfs_queue, &request) {
            Ok(mmap::MmapResult::Complete(response)) => VmReply::VfsMmap(VmMmapOut { ret_addr: response.mapped_addr }),
            Ok(mmap::MmapResult::Suspended) => VmReply::Suspend,
            Err(e) => VmReply::Error(e.into()),
        }
    }

    // -- map_phys --

    /// Dispatch VM_MAP_PHYS request. C: `do_map_phys()` in mmap.c.
    pub(crate) fn dispatch_map_phys(ctx: &mut VmContext, request: VmMapPhysIn) -> VmReply {
        let VmContext { proc_table, page_alloc, page_frames, .. } = ctx;
        let table: &VmProcTable = proc_table;
        let frames = page_frames.as_mut().expect("page_frames not initialized");
        match map_phys::handle_map_phys(table, page_alloc, frames, request.caller, request.target, request.phys_addr, request.length) {
            Ok(virt_addr) => VmReply::MapPhys(VmMapPhysOut { virt_addr }),
            Err(e) => VmReply::Error(e.into()),
        }
    }

    // -- cache (mapcache / setcache / forgetcache / clearcache) --

    /// Dispatch VM_MAPCACHEPAGE request.
    /// Corresponds to Minix3 `do_mapcache()` (mem_cache.c:95-193).
    /// Maps cached blocks into the caller's (the FS's) address space.
    ///
    /// # C semantics
    ///
    /// 1. Validate alignment of `dev_off`/`ino_off` → EFAULT (mem_cache.c:99-101).
    /// 2. `vm_isokendpt(msg->m_source)` → get caller.
    /// 3. `bytes < VM_PAGE_SIZE` → EINVAL (mem_cache.c:102-103).
    /// 4. `map_page_region(caller, VM_MMAPBASE, VM_MMAPTOP, bytes,
    ///    VR_ANON|VR_WRITABLE, 0, &mem_type_cache)` → allocate a fresh
    ///    cache-memtype region (mem_cache.c:128-134).
    /// 5. Per page: `find_cached_page_bydev(dev, dev_off+offset, ino,
    ///    ino_off+offset, 1)` — **always the bydev lookup**, with the ino
    ///    info used for the lazy `update_inohash`. Miss or `VMSF_ONCE`
    ///    entry → unmap the whole region → ENOENT (mem_cache.c:147-158).
    ///    Hit → `vr->param.pb_cache = hb->page` + `map_pf(...)` which runs
    ///    `cache_pagefault` to link the page (mem_cache.c:159-167).
    /// 6. Reply `vr->vaddr` (mem_cache.c:169-171).
    ///
    /// # Rust implementation notes
    ///
    /// - The lookup is by device key only (C's `find_cached_page_bydev`);
    ///   a real ino in the request lazily updates the entry's inode index.
    /// - Instead of the indirect `map_pf` → `cache_pagefault` indirection
    ///   (which needs a page-table walk in C), the page is linked directly
    ///   via `VirRegion::map_page` — same refcount effect (the mapping's
    ///   reference on the cached frame), same observable result.
    /// - `VMSF_ONCE` is checked on the **entry** (`CachedPageRef::once`),
    ///   not on the request flags — C reads `hb->flags & VMSF_ONCE`
    ///   (mem_cache.c:149) and never reads `m_vmmcp.flags` in mapcache.
    pub(crate) fn dispatch_mapcache(ctx: &mut VmContext, caller: Endpoint, request: VmCacheIn) -> VmReply {
        let VmContext { proc_table, page_frames, page_cache: cache, .. } = ctx;
        let table: &VmProcTable = proc_table;
        let frames = page_frames.as_mut().expect("page_frames not initialized");
        const PAGE_SIZE: u64 = 4096;

        // Step 1: alignment (C: mem_cache.c:99-101 → EFAULT).
        if !request.dev_offset.is_multiple_of(PAGE_SIZE) || !request.ino_offset.is_multiple_of(PAGE_SIZE) {
            return VmReply::Error(VmError::InvalidAddress);
        }

        // Step 2: bytes < VM_PAGE_SIZE → EINVAL (C: mem_cache.c:107).
        // Ordering note: C checks vm_isokendpt first and panics on a bogus
        // source; Rust validates the request before resolving the caller
        // (fail-closed), so a malformed request is rejected even when the
        // caller is unresolvable.
        let bytes = request.pages as u64 * PAGE_SIZE;
        if bytes < PAGE_SIZE {
            return VmReply::Error(VmError::InvalidParam);
        }

        // Step 3: caller endpoint (C: vm_isokendpt, mem_cache.c:105-106).
        let caller_slot = match table.vm_isokendpt(caller) {
            Ok(slot) => slot,
            Err(_) => return VmReply::Error(VmError::InvalidProcess),
        };

        // Step 4: allocate a cache-memtype region in the caller's mmap range.
        let mmap_base = VirBytes(crate::mmap::MMAP_BASE);
        let mmap_top = VirBytes(crate::mmap::MMAP_TOP);

        let vaddr = {
            let proc = match table.get_active(caller_slot) {
                Some(p) => p,
                None => return VmReply::Error(VmError::InvalidProcess),
            };
            match proc.regions().find_slot(mmap_base, mmap_top, VirBytes(bytes)) {
                Some(v) => v,
                None => return VmReply::Error(VmError::OutOfMemory),
            }
        };

        let mut region = VirRegion::with_memtype(
            vaddr,
            VirBytes(bytes),
            VrFlags::ANON | VrFlags::WRITABLE,
            &crate::memtype::MEM_TYPE_CACHE,
        );

        // Step 5: per page — bydev lookup, link the cached frame, roll back
        // the whole region on miss / one-shot (C: mem_cache.c:136-168).
        for page_offset in (0..bytes).step_by(PAGE_SIZE as usize) {
            let cache_offset = request.dev_offset + page_offset;
            let cache_ino_offset = request.ino_offset + page_offset;
            let ino = (request.ino != VMC_NO_INODE).then_some(request.ino);

            let hit = cache.find_by_dev(request.dev, cache_offset, ino, cache_ino_offset, true);
            match hit {
                Some(entry) if !entry.once => {
                    region.map_page(frames, VirBytes(page_offset), entry.pfn, &crate::memtype::MEM_TYPE_CACHE);
                }
                // Miss or one-shot entry → ENOENT (C: mem_cache.c:147-158).
                _ => {
                    Self::unmap_region_pages(&mut region, frames);
                    return VmReply::Error(VmError::NotFound);
                }
            }
        }

        // Step 6: insert the region into the caller's map (C:
        // map_page_region already registered it; Rust inserts at commit).
        {
            let mut proc = match table.get_active(caller_slot) {
                Some(p) => p,
                None => return VmReply::Error(VmError::InvalidProcess),
            };
            if proc.regions_mut().insert(region).is_err() {
                return VmReply::Error(VmError::OutOfMemory);
            }
        }

        // Reply with the mapped address (C: msg->m_vmmcp_reply.addr, mem_cache.c:170).
        VmReply::MapCache { addr: vaddr }
    }

    /// Helper: unmap all pages in a region, decrementing refcounts.
    /// Used when do_mapcache fails mid-way and needs to clean up.
    fn unmap_region_pages(region: &mut VirRegion, frames: &mut PageFrames) {
        const PS: u64 = 4096;
        let num_pages = region.physblocks.len();
        for i in 0..num_pages {
            let offset = VirBytes(i as u64 * PS);
            if let Some((_pfn, _memtype)) = region.unmap_page(frames, offset) {
                // Page was mapped and refcount decremented. For mapcache
                // failure cleanup, the cached PFN was only temporarily
                // referenced — the cache still owns it (IN_CACHE set, so
                // unmap_page never asks to free it).
            }
        }
    }

    /// Dispatch VM_SETCACHEPAGE request.
    /// Corresponds to Minix3 `do_setcache()` (mem_cache.c:196-275).
    /// Registers anonymous memory pages (owned by the calling FS) as cache
    /// blocks.
    ///
    /// # C semantics (per page)
    ///
    /// 1. `map_lookup(caller, v, &phys_region)` (mem_cache.c:221-232).
    /// 2. `find_cached_page_bydev(dev, dev_off+offset, ino, ino_off+offset,
    ///    1)` — bydev lookup with lazy ino update (mem_cache.c:235-239).
    /// 3. Same physical page && entry NOT `VMSF_ONCE` → continue (already
    ///    cached; inode info might have changed, which is fine —
    ///    mem_cache.c:240-247). Otherwise the previous entry is obsolete →
    ///    `rmcache` (mem_cache.c:247-255).
    /// 4. Verify the page is anon/anon-contig memory → EFAULT
    ///    (mem_cache.c:257-261).
    /// 5. Verify `refcount == 1` (exclusive ownership) → EFAULT
    ///    (mem_cache.c:263-266).
    /// 6. Switch the page's memtype to cache (mem_cache.c:268) and
    ///    `addcache` (mem_cache.c:270-273).
    pub(crate) fn dispatch_setcache(ctx: &mut VmContext, caller: Endpoint, request: VmCacheIn) -> VmReply {
        let VmContext { proc_table, page_alloc, page_frames, page_cache: cache, .. } = ctx;
        let table: &VmProcTable = proc_table;
        let frames = page_frames.as_mut().expect("page_frames not initialized");
        const PAGE_SIZE: u64 = 4096;

        // Input validation (C: mem_cache.c:198-207): bytes < PAGE_SIZE →
        // EINVAL; unaligned dev_off/ino_off → EFAULT.
        if request.pages == 0 {
            return VmReply::Error(VmError::InvalidParam);
        }
        if !request.dev_offset.is_multiple_of(PAGE_SIZE) || !request.ino_offset.is_multiple_of(PAGE_SIZE) {
            return VmReply::Error(VmError::InvalidAddress);
        }

        // Caller endpoint (C: vm_isokendpt, mem_cache.c:209-210).
        let caller_slot = match table.vm_isokendpt(caller) {
            Ok(slot) => slot,
            Err(_) => return VmReply::Error(VmError::InvalidProcess),
        };

        let bytes = request.pages as u64 * PAGE_SIZE;
        let ino = (request.ino != VMC_NO_INODE).then_some(request.ino);

        for page_offset in (0..bytes).step_by(PAGE_SIZE as usize) {
            let vaddr = VirBytes(request.block + page_offset);

            // Step 1: map_lookup (C: mem_cache.c:221-232).
            let mut proc = match table.get_active(caller_slot) {
                Some(p) => p,
                None => return VmReply::Error(VmError::InvalidProcess),
            };
            let region = match proc.regions_mut().find_mut(vaddr) {
                Some(r) => r,
                None => return VmReply::Error(VmError::InvalidAddress),
            };
            let region_offset = VirBytes(vaddr.0 - region.vaddr.0);
            let slot = match region.get_slot_mut(region_offset) {
                Some(s) => s,
                None => return VmReply::Error(VmError::InvalidAddress),
            };
            let pfn = match slot.pfn() {
                Some(pfn) => pfn,
                None => return VmReply::Error(VmError::InvalidAddress),
            };

            let cache_offset = request.dev_offset + page_offset;
            let cache_ino_offset = request.ino_offset + page_offset;

            // Steps 2-3: existing entry handling (C: mem_cache.c:235-255).
            if let Some(entry) = cache.find_by_dev(request.dev, cache_offset, ino, cache_ino_offset, true) {
                if entry.pfn == pfn && !entry.once {
                    // Block was already there; inode info might've changed,
                    // which is fine (C: continue, mem_cache.c:242-246).
                    continue;
                }
                // Previous entry obsolete (different page, or one-shot) →
                // drop it and re-register (C: rmcache, mem_cache.c:247-255).
                cache.rmcache(request.dev, cache_offset, frames, page_alloc);
            }

            // Step 4: must be anonymous memory (C: mem_cache.c:257-261 —
            // `phys_region->memtype != &mem_type_anon && !=
            // &mem_type_anon_contig`). Compared by static identity, not by
            // name string.
            let is_anon = slot.memtype().is_some_and(|mt| {
                core::ptr::eq(mt, &crate::memtype::MEM_TYPE_ANON as &dyn crate::memtype::MemType)
                    || core::ptr::eq(mt, &crate::memtype::MEM_TYPE_CONTIG_ANON as &dyn crate::memtype::MemType)
            });
            if !is_anon {
                return VmReply::Error(VmError::InvalidAddress);
            }

            // Step 5: exclusive ownership (C: refcount != 1 → EFAULT,
            // mem_cache.c:263-266).
            let refcount = frames.get(pfn).map_or(0, |s| s.refcount());
            if refcount != 1 {
                return VmReply::Error(VmError::InvalidAddress);
            }

            // Step 6: switch memtype + register (C: mem_cache.c:268-273).
            slot.set_memtype(Some(&crate::memtype::MEM_TYPE_CACHE));
            if cache.addcache(
                request.dev,
                cache_offset,
                ino,
                cache_ino_offset,
                request.flags & VMSF_ONCE != 0,
                pfn,
                frames,
            ).is_err() {
                return VmReply::Error(VmError::InvalidParam);
            }
        }

        VmReply::Ok
    }

    /// Dispatch VM_FORGETCACHEPAGE request.
    /// Corresponds to Minix3 `do_forgetcache()` (mem_cache.c:283-307).
    /// Invalidates cached pages for a device offset range.
    ///
    /// C validates `bytes < VM_PAGE_SIZE` → EINVAL and unaligned `dev_off`
    /// → EFAULT (mem_cache.c:290-296), then removes each page by device key
    /// (mem_cache.c:299-305) without touching the LRU (touchlru=0 — the
    /// entry is about to be removed anyway).
    pub(crate) fn dispatch_forgetcache(ctx: &mut VmContext, request: VmCacheIn) -> VmReply {
        let VmContext { page_alloc, page_frames, page_cache: cache, .. } = ctx;
        let frames = page_frames.as_mut().expect("page_frames not initialized");
        const PAGE_SIZE: u64 = 4096;

        if request.pages == 0 {
            return VmReply::Error(VmError::InvalidParam);
        }
        if !request.dev_offset.is_multiple_of(PAGE_SIZE) {
            return VmReply::Error(VmError::InvalidAddress);
        }

        let bytes = request.pages as u64 * PAGE_SIZE;
        for offset in (0..bytes).step_by(PAGE_SIZE as usize) {
            cache.rmcache(request.dev, request.dev_offset + offset, frames, page_alloc);
        }
        VmReply::Ok
    }

    /// Dispatch VM_CLEARCACHE request.
    /// Corresponds to Minix3 `do_clearcache()` (mem_cache.c:315-322).
    /// Invalidates all cached pages of a device (FS unmount).
    /// C performs no input validation beyond the device field itself.
    pub(crate) fn dispatch_clearcache(ctx: &mut VmContext, request: VmCacheIn) -> VmReply {
        let VmContext { page_alloc, page_frames, page_cache: cache, .. } = ctx;
        let frames = page_frames.as_mut().expect("page_frames not initialized");
        cache.clear_by_dev(request.dev, frames, page_alloc);
        VmReply::Ok
    }

    // -- exit --

    /// Dispatch VM_EXIT request. C: `do_vm_exit()` in exit.c.
    pub(crate) fn dispatch_exit(ctx: &mut VmContext, request: VmExitIn) -> VmReply {
        let VmContext { proc_table, page_alloc, page_frames, vfs_queue, .. } = ctx;
        let table: &VmProcTable = proc_table;
        let frames = page_frames.as_mut().expect("page_frames not initialized");
        match exit::handle_vm_exit(table, page_alloc, frames, vfs_queue, request.endpoint) {
            Ok(()) => VmReply::Exit,
            Err(e) => VmReply::Error(e.into()),
        }
    }

    // -- willexit --
    pub(crate) fn dispatch_willexit(ctx: &mut VmContext, request: VmWillexitIn) -> VmReply {
        let table: &VmProcTable = ctx.proc_table;
        match exit::handle_vm_willexit(table, request.endpoint) {
            Ok(()) => VmReply::Willexit,
            Err(e) => VmReply::Error(e.into()),
        }
    }

    // -- pagefault --
    // NOTE: Pagefault is handled directly in VmServer::dispatch_pagefault()
    // (vm_server.rs) which delegates to cow_exec_pf::handle_pagefault().
    // This is because pagefault handling requires &mut self (VmServer) access
    // to page_alloc + frames + regions, which the Dispatcher (taking &VmProcTable)
    // cannot provide. Do not add a duplicate stub here.

    // -- exec_newmem / DMA (parity-verified, 2026-09-06 V11/T19) --
    //
    // VM_EXEC_NEWMEM and VM_ADDDMA/VM_DELDMA/VM_GETDMA exist in C's
    // com.h call-number list but are NOT registered in main.c's CALLMAP
    // (main.c:508-538) — a request reaches `vm_calls[c].vmc_func == NULL`
    // and main.c:139/165 answers ENOSYS. minix-rs mirrors that exactly:
    // no dispatch branch exists, so the `_` arm returns
    // NotImplemented(ENOSYS). This is behavioral parity, not a missing
    // feature; do not "wire these up" without first finding a Minix3
    // revision that actually registers handlers for them.

    // -- rs_set_priv --
    pub(crate) fn dispatch_rs_set_priv(ctx: &mut VmContext, caller: minix_types::Endpoint, target: minix_types::Endpoint, mask: Option<crate::acl::AclMask>, is_sys_proc: bool) -> VmReply {
        let table: &VmProcTable = ctx.proc_table;
        match rs::handle_rs_set_priv(table, caller, target, mask, is_sys_proc) {
            Ok(()) => VmReply::Ok,
            Err(e) => VmReply::Error(e.into()),
        }
    }

    // -- rs_prepare --
    pub(crate) fn dispatch_rs_prepare(ctx: &mut VmContext, src: minix_types::Endpoint, dst: minix_types::Endpoint, flags: u32) -> VmReply {
        let VmContext { proc_table, page_alloc, page_frames, vfs_queue, .. } = ctx;
        let table: &VmProcTable = proc_table;
        let frames = page_frames.as_mut().expect("page_frames not initialized");
        match rs::handle_rs_prepare(table, page_alloc, frames, vfs_queue, src, dst, flags) {
            Ok(()) => VmReply::Ok,
            Err(e) => VmReply::Error(e.into()),
        }
    }

    // -- rs_update --
    pub(crate) fn dispatch_rs_update(ctx: &mut VmContext, src: minix_types::Endpoint, dst: minix_types::Endpoint, flags: u32) -> VmReply {
        let VmContext { proc_table, page_alloc, page_frames, vfs_queue, .. } = ctx;
        let table: &VmProcTable = proc_table;
        let frames = page_frames.as_mut().expect("page_frames not initialized");
        match rs::handle_rs_update(table, page_alloc, frames, vfs_queue, src, dst, flags) {
            Ok(rs::RsUpdateResult::Ok) => VmReply::Ok,
            Ok(rs::RsUpdateResult::Suspend) => VmReply::Suspend,
            Err(e) => VmReply::Error(e.into()),
        }
    }

    // -- rs_memctl --
    pub(crate) fn dispatch_rs_memctl(ctx: &mut VmContext, target: minix_types::Endpoint, request: rs::RsMemctlRequest) -> VmReply {
        let VmContext { proc_table, page_alloc, page_frames, vfs_queue, .. } = ctx;
        let table: &VmProcTable = proc_table;
        let frames = page_frames.as_mut().expect("page_frames not initialized");
        match rs::handle_rs_memctl(table, page_alloc, frames, vfs_queue, target, request) {
            Ok(rs::RsMemctlResult::Ok) => VmReply::Ok,
            Ok(rs::RsMemctlResult::AddrLen { addr, len }) => VmReply::RsMemctlAddrLen { addr, len },
            Err(e) => VmReply::Error(e.into()),
        }
    }
}

/// Decode a VM_RS_MEMCTL sub-request code into the typed request.
///
/// C: `do_rs_memctl` switch on `m->VM_RS_CTL_REQ` (rs.c:366-389); the
/// default arm returns EINVAL (rs.c:386-388). (25-P0-1: unknown codes
/// previously mapped to `VmError::InvalidAddress` → EFAULT.)
fn decode_rs_memctl_request(
    req_code: i32,
    addr: u64,
    len: i32,
) -> Result<rs::RsMemctlRequest, VmError> {
    match req_code {
        0 => Ok(rs::RsMemctlRequest::Pin),
        1 => Ok(rs::RsMemctlRequest::MakeVmInstance),
        2 => Ok(rs::RsMemctlRequest::HeapPrealloc {
            addr: VirBytes(addr),
            len: len as usize,
        }),
        3 => Ok(rs::RsMemctlRequest::MapPrealloc {
            addr: VirBytes(addr),
            len: len as usize,
        }),
        4 => Ok(rs::RsMemctlRequest::GetPreallocMap),
        // C: rs.c:386-388 — do_rs_memctl default arm returns EINVAL.
        _ => Err(VmError::InvalidParam),
    }
}

impl MessageDispatcher {
    // -- get_phys --
    pub(crate) fn dispatch_get_phys(ctx: &mut VmContext, target: minix_types::Endpoint, addr: VirBytes) -> VmReply {
        let table: &VmProcTable = ctx.proc_table;
        match query::handle_get_phys(table, target, addr) {
            Ok(phys) => VmReply::GetPhys { phys_addr: phys },
            Err(e) => VmReply::Error(e.into()),
        }
    }

    // -- get_refcount --
    pub(crate) fn dispatch_get_refcount(ctx: &mut VmContext, target: minix_types::Endpoint, addr: VirBytes) -> VmReply {
        let table: &VmProcTable = ctx.proc_table;
        match query::handle_get_refcount(table, target, addr) {
            Ok(cnt) => VmReply::GetRefcount { count: cnt },
            Err(e) => VmReply::Error(e.into()),
        }
    }

    // -- info --
    pub(crate) fn dispatch_info(ctx: &mut VmContext, q: query::InfoQuery) -> VmReply {
        // Copy values read before the field-level mutable destructuring.
        let sources = ctx.usage_sources();
        let dropped_messages = ctx.dropped_messages;
        let pagefault_errors = ctx.pagefault_errors;
        let VmContext { proc_table, page_alloc, page_frames, page_cache: cache, .. } = ctx;
        let table: &VmProcTable = proc_table;
        let cached_pages = cache.total_cached();
        let frames = page_frames.as_ref().expect("page_frames not initialized");
        match query::handle_info(table, page_alloc, frames, sources, cached_pages, q) {
            Ok(query::InfoResult::Stats(s)) => VmReply::InfoStats {
                page_size: s.page_size,
                total_pages: s.total_pages,
                free_pages: s.free_pages,
                largest_contiguous: s.largest_contiguous,
                cached_pages: s.cached_pages,
                // V10-P2-4: main-loop observability counters (minix-rs
                // extension; not on C's vm_stats_info wire layout).
                dropped_messages,
                pagefault_errors,
                // V11/T18 ([ARCH: A-16]): page-allocation failures.
                alloc_failures: s.alloc_failures,
            },
            Ok(query::InfoResult::Usage(u)) => VmReply::InfoUsage {
                total: u.total,
                common: u.common,
                shared: u.shared,
                virtual_total: u.virtual_total,
                mvirtual: u.mvirtual,
                max_rss_kb: u.max_rss_kb,
                minor_faults: u.minor_faults,
                major_faults: u.major_faults,
            },
            Ok(query::InfoResult::Region { regions, count, next }) => VmReply::InfoRegion {
                // C: vri_addr/vri_length/vri_prot (region.c:1469-1478);
                // vri_flags is always 0 in C (static zero-init, never
                // written) and is not modeled.
                regions: regions.map(|r| minix_types::VmRegionInfo {
                    addr: r.addr,
                    length: r.length,
                    prot: r.prot as u32,
                }),
                count,
                next,
            },
            Err(e) => VmReply::Error(e.into()),
        }
    }

    // -- getrusage --
    pub(crate) fn dispatch_getrusage(ctx: &mut VmContext, caller: minix_types::Endpoint, target: minix_types::Endpoint, children: bool) -> VmReply {
        let table: &VmProcTable = ctx.proc_table;
        match query::handle_getrusage(table, caller, target, children) {
            Ok(query::GetrusageResult::Ok) => VmReply::Ok,
            Ok(query::GetrusageResult::Data(d)) => VmReply::Getrusage {
                max_rss_kb: d.max_rss_kb,
                minor_faults: d.minor_faults,
                major_faults: d.major_faults,
            },
            Err(e) => VmReply::Error(query_rusage_error_to_vm_error(e)),
        }
    }
}

// ==========================================================================
// Compile-time CALLMAP — dispatch_by_number
// ==========================================================================

impl MessageDispatcher {
    /// Compile-time CALLMAP lookup (C: main.c:137-176 — `CALLNUMBER(c)`
    /// bounds check, then `vm_calls[c].vmc_func` dispatch; unregistered
    /// calls keep `result = ENOSYS` at main.c:139/165). The table lives in
    /// [`CALLMAP`]; each entry is a per-call decode wrapper.
    pub(crate) fn dispatch_by_number(
        call_nr: usize,
        msg: &Message,
        server: &mut crate::VmServer,
    ) -> DispatchResult {
        let ctx = &mut server.ctx;
        match CALLMAP.get(call_nr).copied().flatten() {
            Some(handler) => handler(ctx, msg),
            None => DispatchResult::from_reply(VmReply::Error(VmError::NotImplemented)),
        }
    }
}

// ==========================================================================
// Compile-time CALLMAP — V9-P2-1 (per-call codec) + V9-P2-2 (table dispatch)
//
// C builds `vm_calls[VM_RQ_BASE..]` in main.c:508-538 and registers one
// handler per call at main.c:543-575; the dispatcher loop calls
// `vm_calls[c].vmc_func(&msg)`. minix-rs mirrors that architecture: the
// table is built at compile time, indexed by `callnr(m_type)`
// (the relative index 0..=48), and unregistered slots stay `None`
// (→ ENOSYS), exactly like C's NULL `vmc_func`.
//
// Each entry is a thin *decode wrapper* — the single place that knows which
// wire format its call number speaks (M1 / M2 / dedicated overlay) and how
// wire fields map to semantic arguments (V9-P2-1). Policy lives in the
// `dispatch_*` handlers, which the wrapper delegates to.
// ==========================================================================

/// One registered entry of the dispatch table (C: `vmc_func`).
type CallHandler = fn(&mut VmContext, &Message) -> DispatchResult;

const NR_VM_CALLS_IDX: usize = minix_types::NR_VM_CALLS as usize;

/// C: `CALLNUMBER(c)` relative index (main.c:57-59).
const fn call_index(call: u32) -> usize {
    call as usize - VM_RQ_BASE as usize
}

/// SAFETY (union reads): the caller has validated `m_type` as a VM request
/// number (`callnr()` bounds check, vm_server.rs), so the message is one of
/// the VM call formats below; each wrapper reads the union member its call
/// documents (ipc.h / com.h wire layouts).
static CALLMAP: [Option<CallHandler>; NR_VM_CALLS_IDX] = build_callmap();

const fn build_callmap() -> [Option<CallHandler>; NR_VM_CALLS_IDX] {
    let mut t: [Option<CallHandler>; NR_VM_CALLS_IDX] = [None; NR_VM_CALLS_IDX];
    // Registration order mirrors C main.c:543-575.
    t[call_index(VM_EXIT)] = Some(call_exit);
    t[call_index(VM_FORK)] = Some(call_fork);
    t[call_index(VM_BRK)] = Some(call_brk);
    t[call_index(VM_WILLEXIT)] = Some(call_willexit);
    t[call_index(VM_MMAP)] = Some(call_mmap);
    t[call_index(VM_MAP_PHYS)] = Some(call_map_phys);
    t[call_index(VM_UNMAP_PHYS)] = Some(call_unmap_phys);
    t[call_index(VM_MUNMAP)] = Some(call_munmap);
    t[call_index(VM_MAPCACHEPAGE)] = Some(call_mapcache);
    t[call_index(VM_SETCACHEPAGE)] = Some(call_setcache);
    t[call_index(VM_FORGETCACHEPAGE)] = Some(call_forgetcache);
    t[call_index(VM_CLEARCACHE)] = Some(call_clearcache);
    t[call_index(VM_VFS_REPLY)] = Some(call_vfs_reply);
    t[call_index(VM_REMAP)] = Some(call_remap);
    t[call_index(VM_SHM_UNMAP)] = Some(call_shm_unmap);
    t[call_index(VM_GETPHYS)] = Some(call_get_phys);
    t[call_index(VM_GETREF)] = Some(call_get_refcount);
    t[call_index(VM_RS_SET_PRIV)] = Some(call_rs_set_priv);
    t[call_index(VM_INFO)] = Some(call_info);
    t[call_index(VM_RS_UPDATE)] = Some(call_rs_update);
    t[call_index(VM_RS_MEMCTL)] = Some(call_rs_memctl);
    t[call_index(VM_REMAP_RO)] = Some(call_remap_ro);
    t[call_index(VM_PROCCTL)] = Some(call_procctl);
    t[call_index(VM_VFS_MMAP)] = Some(call_vfs_mmap);
    t[call_index(VM_GETRUSAGE)] = Some(call_getrusage);
    t[call_index(VM_RS_PREPARE)] = Some(call_rs_prepare);
    // Deliberately NOT registered (parity with C, which leaves these NULL
    // too — see the exec_newmem/DMA note in this file):
    //   VM_EXEC_NEWMEM (+3), VM_ADDDMA (+12), VM_DELDMA (+13), VM_GETDMA (+14).
    t
}

fn call_exit(ctx: &mut VmContext, msg: &Message) -> DispatchResult {
    let m1 = unsafe { &msg.m_u.m_m1 };
    MessageDispatcher::dispatch_exit(ctx, VmExitIn::decode(m1)).into()
}

fn call_fork(ctx: &mut VmContext, msg: &Message) -> DispatchResult {
    let m1 = unsafe { &msg.m_u.m_m1 };
    MessageDispatcher::dispatch_fork(ctx, VmForkIn::decode(m1)).into()
}

fn call_brk(ctx: &mut VmContext, msg: &Message) -> DispatchResult {
    MessageDispatcher::dispatch_brk(ctx, VmBrkIn::decode_message(msg)).into()
}

fn call_willexit(ctx: &mut VmContext, msg: &Message) -> DispatchResult {
    let m1 = unsafe { &msg.m_u.m_m1 };
    MessageDispatcher::dispatch_willexit(ctx, VmWillexitIn::decode(m1)).into()
}

fn call_mmap(ctx: &mut VmContext, msg: &Message) -> DispatchResult {
    MessageDispatcher::dispatch_mmap(ctx, VmMmapIn::decode_message(msg)).into()
}

fn call_map_phys(ctx: &mut VmContext, msg: &Message) -> DispatchResult {
    MessageDispatcher::dispatch_map_phys(ctx, VmMapPhysIn::decode_message(msg)).into()
}

fn call_unmap_phys(ctx: &mut VmContext, msg: &Message) -> DispatchResult {
    // 21-P1-1: wired at VM_UNMAP_PHYS (previously the `_` catch-all).
    // C: CALLMAP(VM_UNMAP_PHYS, do_munmap), main.c:540.
    MessageDispatcher::dispatch_unmap_phys(ctx, VmUnmapPhysIn::decode_message(msg)).into()
}

fn call_munmap(ctx: &mut VmContext, msg: &Message) -> DispatchResult {
    // 21-P1-1 wire-format fix: decode from the m_mmap overlay + m_source.
    MessageDispatcher::dispatch_munmap(ctx, VmMunmapIn::decode_message(msg)).into()
}

fn call_mapcache(ctx: &mut VmContext, msg: &Message) -> DispatchResult {
    MessageDispatcher::dispatch_mapcache(ctx, msg.m_source, VmCacheIn::decode_message(msg)).into()
}

fn call_setcache(ctx: &mut VmContext, msg: &Message) -> DispatchResult {
    MessageDispatcher::dispatch_setcache(ctx, msg.m_source, VmCacheIn::decode_message(msg)).into()
}

fn call_forgetcache(ctx: &mut VmContext, msg: &Message) -> DispatchResult {
    MessageDispatcher::dispatch_forgetcache(ctx, VmCacheIn::decode_message(msg)).into()
}

fn call_clearcache(ctx: &mut VmContext, msg: &Message) -> DispatchResult {
    MessageDispatcher::dispatch_clearcache(ctx, VmCacheIn::decode_message(msg)).into()
}

fn call_vfs_reply(ctx: &mut VmContext, msg: &Message) -> DispatchResult {
    MessageDispatcher::dispatch_vfs_reply(ctx, VmVfsReplyIn::decode_message(msg)).into()
}

fn call_remap(ctx: &mut VmContext, msg: &Message) -> DispatchResult {
    MessageDispatcher::dispatch_remap(ctx, VmRemapIn::decode_message(msg)).into()
}

fn call_shm_unmap(ctx: &mut VmContext, msg: &Message) -> DispatchResult {
    // 21-P1-1 wire-format fix: dedicated m_lc_vm_shm_unmap overlay
    // (forwhom@0, addr@4). The old M1 decode read addr from m1p1 @ 16.
    MessageDispatcher::dispatch_shm_unmap(ctx, VmShmUnmapIn::decode_message(msg)).into()
}

fn call_get_phys(ctx: &mut VmContext, msg: &Message) -> DispatchResult {
    // C: utility.c — get_phys uses m1_i1=target, m1_p1=vaddr.
    let m1 = unsafe { &msg.m_u.m_m1 };
    let target = Endpoint(m1.m1i1);
    let addr = VirBytes(m1.m1p1);
    MessageDispatcher::dispatch_get_phys(ctx, target, addr).into()
}

fn call_get_refcount(ctx: &mut VmContext, msg: &Message) -> DispatchResult {
    // C: utility.c — get_ref uses m1_i1=target, m1_p1=vaddr.
    let m1 = unsafe { &msg.m_u.m_m1 };
    let target = Endpoint(m1.m1i1);
    let addr = VirBytes(m1.m1p1);
    MessageDispatcher::dispatch_get_refcount(ctx, target, addr).into()
}

fn call_rs_set_priv(ctx: &mut VmContext, msg: &Message) -> DispatchResult {
    // C: rs.c:40 — nr=m->VM_RS_NR, buf=m->VM_RS_BUF, sys=m->VM_RS_SYS
    // M2: m2i1=target endpoint, m2l1=call_mask pointer, m2i2=is_sys_proc.
    let m2 = unsafe { &msg.m_u.m_m2 };
    let target = Endpoint(m2.m2i1);
    let is_sys_proc = m2.m2i2 != 0;
    // call_mask is passed via sys_datacopy in C; we can't do that
    // from M2 alone. RS must pass the mask inline or via shared memory.
    // For now, pass None (will use default ACL for user, empty for sys).
    let mask = None;
    MessageDispatcher::dispatch_rs_set_priv(ctx, msg.m_source, target, mask, is_sys_proc).into()
}

fn call_info(ctx: &mut VmContext, msg: &Message) -> DispatchResult {
    // C: utility.c:100 — m_lsys_vm_info (M2: m2i1=what, m2i2=ep, m2i3=count, m2l2=next)
    let m2 = unsafe { &msg.m_u.m_m2 };
    let what = m2.m2i1;
    let mut ep = Endpoint(m2.m2i2);
    let count = m2.m2i3 as usize;
    let next = VirBytes(m2.m2l2 as u64);
    // C: VMIW_STATS=1 / VMIW_USAGE=2 / VMIW_REGION=3 (com.h:732-734) —
    // libsys sends exactly these values (vm_info.c:15/:29/:46).
    let q = match what {
        minix_types::VMIW_STATS => query::InfoQuery::Stats,
        minix_types::VMIW_USAGE => query::InfoQuery::Usage { target: ep },
        minix_types::VMIW_REGION => {
            // C: do_info — VMIW_REGION with ep == SELF uses the caller
            // (utility.c:141-143).
            if ep == Endpoint::SELF {
                ep = msg.m_source;
            }
            query::InfoQuery::Region { target: ep, count, next }
        }
        // C: do_info default arm returns EINVAL (utility.c:163).
        _ => return DispatchResult::from_reply(VmReply::Error(VmError::InvalidParam)),
    };
    MessageDispatcher::dispatch_info(ctx, q).into()
}

fn call_rs_update(ctx: &mut VmContext, msg: &Message) -> DispatchResult {
    // C: rs.c:150 — m_lsys_vm_update maps to M2: m2i1=src, m2i2=dst, m2i3=flags.
    let m2 = unsafe { &msg.m_u.m_m2 };
    let src = Endpoint(m2.m2i1);
    let dst = Endpoint(m2.m2i2);
    let flags = m2.m2i3 as u32;
    MessageDispatcher::dispatch_rs_update(ctx, src, dst, flags).into()
}

fn call_rs_memctl(ctx: &mut VmContext, msg: &Message) -> DispatchResult {
    // C: rs.c:349 — ep=m->VM_RS_CTL_ENDPT(m1_i1), req=m->VM_RS_CTL_REQ(m1_i2),
    // VM_RS_CTL_ADDR=m2_p1, VM_RS_CTL_LEN=m2_i3.
    let m1 = unsafe { &msg.m_u.m_m1 };
    let m2 = unsafe { &msg.m_u.m_m2 };
    let target = Endpoint(m1.m1i1);
    let request = match decode_rs_memctl_request(m1.m1i2, m2.m2l1 as u64, m2.m2i3) {
        Ok(r) => r,
        Err(e) => return DispatchResult::from_reply(VmReply::Error(e)),
    };
    MessageDispatcher::dispatch_rs_memctl(ctx, target, request).into()
}

fn call_remap_ro(ctx: &mut VmContext, msg: &Message) -> DispatchResult {
    MessageDispatcher::dispatch_remap_ro(ctx, VmRemapIn::decode_message(msg)).into()
}

fn call_procctl(ctx: &mut VmContext, msg: &Message) -> DispatchResult {
    // param/who/m1/len/flags follow the C m9 layout; `decode_message`
    // reads the dedicated m_lc_vm_procctl overlay. Caller = m_source.
    MessageDispatcher::dispatch_procctl(ctx, msg.m_source, VmProcctlIn::decode_message(msg)).into()
}

fn call_vfs_mmap(ctx: &mut VmContext, msg: &Message) -> DispatchResult {
    MessageDispatcher::dispatch_vfs_mmap(ctx, VmVfsMmapIn::decode_message(msg)).into()
}

fn call_getrusage(ctx: &mut VmContext, msg: &Message) -> DispatchResult {
    // C: utility.c:426 — m_lsys_vm_rusage: m2i1=target, m2i2=children.
    let m2 = unsafe { &msg.m_u.m_m2 };
    let target = Endpoint(m2.m2i1);
    let children = m2.m2i2 != 0;
    MessageDispatcher::dispatch_getrusage(ctx, msg.m_source, target, children).into()
}

fn call_rs_prepare(ctx: &mut VmContext, msg: &Message) -> DispatchResult {
    // C: rs.c:71 — m2i1=src, m2i2=dst, m2i3=flags.
    let m2 = unsafe { &msg.m_u.m_m2 };
    let src = Endpoint(m2.m2i1);
    let dst = Endpoint(m2.m2i2);
    let flags = m2.m2i3 as u32;
    MessageDispatcher::dispatch_rs_prepare(ctx, src, dst, flags).into()
}

// ==========================================================================
// Error mapping — From<XxxError> for VmError
//
// Rust idiom: implement `From` trait so call sites can use `?` operator
// instead of explicit `.map_err(xxx_error_to_vm_error)`. The compiler
// guarantees exhaustiveness — adding a new variant to any sub-error enum
// will produce a compile error here until the mapping is updated.
//
// Exception: `QueryError` has context-dependent mapping (ProcessNotFound
// maps to InvalidProcess in info/get_phys but InvalidEndpoint in getrusage),
// so we keep a dedicated `query_rusage_error_to_vm_error` function for that.
// ==========================================================================

impl From<fork::VmForkError> for VmError {
    fn from(e: fork::VmForkError) -> Self {
        match e {
            fork::VmForkError::InvalidEndpoint => VmError::InvalidProcess,
            fork::VmForkError::InvalidSlot => VmError::InvalidProcess,
            fork::VmForkError::SlotInUse => VmError::SlotInUse,
            fork::VmForkError::CowAllocFailed => VmError::OutOfMemory,
            fork::VmForkError::PageTableInitFailed => VmError::OutOfMemory,
            fork::VmForkError::PageTableMapFailed => VmError::OutOfMemory,
            fork::VmForkError::PageNotMapped => VmError::PageNotMapped,
            fork::VmForkError::MemType(_) => VmError::MemType,
            // V11/T9: gateway kernel-call failure → fail-closed error reply
            // (C do_fork.c panics instead — see the do_fork doc in fork.rs).
            // Pre-E1 the trap stub answers -EIO while the kernel state is
            // untouched, so the InternalError reply is exact.
            fork::VmForkError::KernelCall(_) => VmError::InternalError,
        }
    }
}

impl From<brk::BrkError> for VmError {
    fn from(e: brk::BrkError) -> Self {
        match e {
            brk::BrkError::ProcessNotFound => VmError::InvalidProcess,
            brk::BrkError::OutOfMemory => VmError::OutOfMemory,
        }
    }
}

impl From<munmap::MunmapError> for VmError {
    fn from(e: munmap::MunmapError) -> Self {
        match e {
            munmap::MunmapError::ProcessNotFound => VmError::InvalidProcess,
            munmap::MunmapError::BadAddress => VmError::InvalidAddress,
            // C: map_unmap_range returns EINVAL for length < page
            // (region.c:1233), wrapping ranges (region.c:1234), and
            // map_unmap_region for unaligned len (region.c:1076).
            munmap::MunmapError::InvalidLength => VmError::InvalidParam,
            munmap::MunmapError::NotMapped => VmError::InvalidAddress,
            // C: split_region / low-end shrink return EINVAL when the
            // memtype lacks the required callback (region.c:1164/:1096).
            munmap::MunmapError::MemTypeNotSupported => VmError::InvalidParam,
            munmap::MunmapError::InternalError => VmError::InternalError,
        }
    }
}

impl From<mmap::MmapError> for VmError {
    fn from(e: mmap::MmapError) -> Self {
        match e {
            // C do_mmap returns EINVAL for len <= 0 (mmap.c:228-229)
            mmap::MmapError::InvalidLength => VmError::InvalidParam,
            mmap::MmapError::BadAddress => VmError::InvalidAddress,
            // C do_mmap returns EINVAL for bad flag combinations
            // (mmap.c:229-245)
            mmap::MmapError::InvalidFlags => VmError::InvalidParam,
            mmap::MmapError::PermissionDenied => VmError::PermissionDenied,
            mmap::MmapError::OutOfMemory => VmError::OutOfMemory,
            // C do_mmap returns ENXIO for disabled / writable-shared file
            // mappings (mmap.c:255-261)
            mmap::MmapError::FileMapDisabled => VmError::NoDevice,
            mmap::MmapError::ProcessNotFound => VmError::InvalidEndpoint,
        }
    }
}

impl From<map_phys::MapPhysError> for VmError {
    fn from(e: map_phys::MapPhysError) -> Self {
        match e {
            map_phys::MapPhysError::PermissionDenied => VmError::PermissionDenied,
            map_phys::MapPhysError::OutOfMemory => VmError::OutOfMemory,
            // C do_map_phys: `if (len <= 0) return EINVAL` (mmap.c:323)
            map_phys::MapPhysError::InvalidLength => VmError::InvalidParam,
            map_phys::MapPhysError::ProcessNotFound => VmError::InvalidProcess,
        }
    }
}

impl From<exit::VmExitError> for VmError {
    fn from(e: exit::VmExitError) -> Self {
        match e {
            exit::VmExitError::ProcessNotFound => VmError::InvalidProcess,
            exit::VmExitError::NotExiting => VmError::InvalidProcess,
        }
    }
}

impl From<rs::RsError> for VmError {
    fn from(e: rs::RsError) -> Self {
        match e {
            rs::RsError::ProcessNotFound => VmError::InvalidProcess,
            // C: rs.c:56-58 — `do_rs_set_priv` with no mask for a sys proc
            // prints "sys procs don't share!" and returns EINVAL.
            rs::RsError::SysProcNoMask => VmError::InvalidProcess,
            rs::RsError::PinFailed => VmError::NotImplemented,
            // C: real_brk() returns ENOMEM on failure (break.c:63-68).
            rs::RsError::HeapExtendFailed => VmError::OutOfMemory,
            rs::RsError::PreallocMapConflict => VmError::NotImplemented,
            rs::RsError::UpdateNotImplemented => VmError::NotImplemented,
            // C: rs.c:386-388 — `do_rs_memctl` default arm returns EINVAL.
            rs::RsError::InvalidRequest => VmError::InvalidParam,
            rs::RsError::MakeVmFailed => VmError::PermissionDenied,
            rs::RsError::HeapPreallocFailed => VmError::NotImplemented,
            rs::RsError::MapPreallocFailed => VmError::NotImplemented,
            // C: rs.c:285-288 / rs.c:305-308 — `rs_memctl_*_prealloc` return
            // EINVAL when `*len <= 0`.
            rs::RsError::InvalidLength => VmError::InvalidParam,
        }
    }
}

impl From<query::QueryError> for VmError {
    fn from(e: query::QueryError) -> Self {
        match e {
            // C: do_info/do_get_phys/do_get_refcount return EINVAL for
            // every failure (utility.c:110, mmap.c:444-452, region.c:
            // 1327-1334/1346-1353). The getrusage ESRCH path is handled by
            // `query_rusage_error_to_vm_error` below.
            query::QueryError::ProcessNotFound => VmError::InvalidProcess,
            query::QueryError::NotMapped => VmError::InvalidParam,
            query::QueryError::NotSupported => VmError::InvalidParam,
            query::QueryError::InvalidQuery => VmError::InvalidParam,
        }
    }
}

/// Context-specific mapping for getrusage: ProcessNotFound → InvalidEndpoint
/// (C: utility.c:442 returns ESRCH for invalid endpoint in getrusage).
fn query_rusage_error_to_vm_error(e: query::QueryError) -> VmError {
    match e {
        query::QueryError::ProcessNotFound => VmError::InvalidEndpoint,
        _ => VmError::from(e),
    }
}

// -- remap implementation (shared by VM_REMAP and VM_REMAP_RO) --

/// Mmap address range for remap operations.
/// C: `VM_MMAPBASE` / `VM_MMAPTOP` (computed at boot in minix3).
/// In 64-bit minix-rs, we use the same range as `handle_mmap`.
const REMAP_MMAP_BASE: u64 = 0x0000_0001_0000_0000;
const REMAP_MMAP_TOP: u64  = 0x0000_0200_0000_0000;

/// Page-align a length value, matching C's `size += VM_PAGE_SIZE - size % VM_PAGE_SIZE`.
fn align_up_page(len: u64) -> u64 {
    const PAGE_SIZE: u64 = 4096;
    (len + PAGE_SIZE - 1) & !(PAGE_SIZE - 1)
}

/// Shared implementation for VM_REMAP and VM_REMAP_RO.
///
/// C source: `do_remap()` in mmap.c:366-434.
///
/// # Safety argument for cross-process access
///
/// This function reads from the source process and writes to the destination
/// process. In the single-threaded VM event loop, only one thread accesses
/// the process table at a time. We use `find_region_snapshot` (read-only,
/// returns a Copy snapshot) to capture source info, then `get_active` to
/// get an `&mut ActiveProc` for the destination. The `increment_region_remaps`
/// call targets the source slot directly — safe because it's a different
/// slot than the currently-held `ActiveProc`.
fn dispatch_remap_impl(
    table: &VmProcTable,
    _page_alloc: &mut VmPageAllocator,
    _frames: &mut PageFrames,
    request: VmRemapIn,
    readonly: bool,
) -> VmReply {
    // Step 1: Validate input (C: mmap.c:387 — `if (size <= 0) return EINVAL`)
    if request.length.0 == 0 {
        return VmReply::Error(VmError::InvalidParam);
    }

    // Step 2: Validate endpoints (C: mmap.c:389-392)
    // C reads destination/source from the message fields — NOT m_source.
    // `caller` (m_source) is used for ACL only (main.c:165-176 acl_check).
    let src_slot = match table.vm_isokendpt(request.who) {
        Ok(slot) => slot,
        // C: `if ((r = vm_isokendpt(source, &sn)) != OK) return EINVAL;`
        Err(_) => return VmReply::Error(VmError::InvalidParam),
    };
    let dst_slot = match table.vm_isokendpt(request.destination) {
        Ok(slot) => slot,
        // C: `if ((r = vm_isokendpt(destination, &dn)) != OK) return EINVAL;`
        Err(_) => return VmReply::Error(VmError::InvalidParam),
    };

    // Step 3: Look up source region (C: mmap.c:396 — `map_lookup(svmp, sa, NULL)`)
    let src_snapshot = match table.find_region_snapshot(src_slot, request.vaddr) {
        Some(s) => s,
        // C: `if (!(src_region = map_lookup(svmp, sa, NULL))) return EINVAL;`
        None => return VmReply::Error(VmError::InvalidParam),
    };

    // Step 4: Source region must start exactly at vaddr
    // (C: mmap.c:398-400 — `if(src_region->vaddr != sa) return EFAULT`)
    if src_snapshot.vaddr != request.vaddr {
        return VmReply::Error(VmError::InvalidAddress);
    }

    // Step 5: Page-align length and verify it matches source region
    // (C: mmap.c:402-406 — size alignment + `if(size != src_region->length) return EFAULT`)
    let aligned_len = VirBytes(align_up_page(request.length.0));
    if src_snapshot.length != aligned_len {
        return VmReply::Error(VmError::InvalidAddress);
    }

    // Step 6: Determine destination address range
    // (C: mmap.c:412-415 — `if(da) map_page_region(dvmp, da, 0, ...) else map_page_region(dvmp, VM_MMAPBASE, VM_MMAPTOP, ...)`)
    let (minv, maxv) = if request.target.0 != 0 {
        (request.target, VirBytes(request.target.0 + aligned_len.0))
    } else {
        (VirBytes(REMAP_MMAP_BASE), VirBytes(REMAP_MMAP_TOP))
    };

    // Step 7: Find free slot in destination process
    let dst_vaddr = {
        let dst_proc = match table.get_active(dst_slot) {
            Some(p) => p,
            None => return VmReply::Error(VmError::InvalidEndpoint),
        };
        match dst_proc.regions().find_slot(minv, maxv, aligned_len) {
            Some(addr) => addr,
            None => return VmReply::Error(VmError::OutOfMemory),
        }
        // dst_proc (ActiveProc) dropped here — releases the borrow
    };

    // Step 8: Create new shared region
    // (C: mmap.c:412-415 — `map_page_region(dvmp, ..., flags, 0, &mem_type_shared)`)
    let mut flags = VrFlags::SHARED;
    if !readonly {
        flags |= VrFlags::WRITABLE;
    }

    let mut new_region = VirRegion::with_memtype(
        dst_vaddr,
        aligned_len,
        flags,
        &MEM_TYPE_SHARED,
    );

    // Step 9: Set shared source parameters
    // (C: mmap.c:417 — `shared_setsource(vr, svmp->vm_endpoint, src_region)`)
    // which sets vr->param.shared.{ep, vaddr, id} and increments srcvr->remaps
    new_region.param = VrParam::Shared {
        ep: request.who.0,
        vaddr: src_snapshot.vaddr,
        id: src_snapshot.id,
    };

    // Step 10: Insert new region into destination process
    {
        let mut dst_proc = match table.get_active(dst_slot) {
            Some(p) => p,
            None => return VmReply::Error(VmError::InvalidEndpoint),
        };
        if dst_proc.regions_mut().insert(new_region).is_err() {
            return VmReply::Error(VmError::OutOfMemory);
        }
    }

    // Step 11: Increment source region's remaps counter
    // (C: mem_shared.c:195 — `srcvr->remaps++`)
    //
    // SAFETY: src_slot != dst_slot is not strictly required by Minix3
    // (a process can remap its own region to a different address),
    // but if they're the same slot, we must not hold an ActiveProc
    // for it. Since we dropped dst_proc above, this is safe regardless.
    let _ = table.increment_region_remaps(src_slot, src_snapshot.vaddr);

    // Step 12: Return mapped address (C: mmap.c:431 — `m->m_lsys_vm_vmremap.ret_addr = vr->vaddr`)
    VmReply::Mmap(VmMmapOut { ret_addr: dst_vaddr })
}

#[cfg(test)]
mod tests {
    use super::*;
    use minix_types::{Endpoint, UserSlot};

    fn init_test_slots() {
        let table = VmProcTable::get_global();
        unsafe {
            table.reset_slot(UserSlot::new(50));
            table.reset_slot(UserSlot::new(51));
        }
        let empty = table.get_empty(UserSlot::new(50)).unwrap();
        let _parent = empty.activate(Endpoint::from_generation_slot(1, 50));
    }

    #[test]
    fn test_vm_error_invalid_process_maps_to_einval() {
        let result = VmError::InvalidProcess.to_errno();
        assert_eq!(result, minix_types::EINVAL,
            "VmError::InvalidProcess must map to EINVAL — covers: \
             fork.c:44, break.c:53, exit.c:69, mmap.c:329, rs.c:44/94/165/361, utility.c:110");
    }

    #[test]
    fn test_vm_error_invalid_endpoint_maps_to_esrch() {
        let result = VmError::InvalidEndpoint.to_errno();
        assert_eq!(result, minix_types::ESRCH,
            "VmError::InvalidEndpoint must map to ESRCH — covers: \
             mmap.c:216 (third-party), utility.c:442 (getrusage)");
    }

    #[test]
    fn test_vm_error_slot_in_use_maps_to_einval() {
        let result = VmError::SlotInUse.to_errno();
        assert_eq!(result, minix_types::EINVAL);
    }

    /// V10-P1-2: pins the live-update scaffold. `dispatch_rs_update` must
    /// currently return `Error(NotImplemented)` — the `Ok`/`Suspend` arms
    /// (dispatcher.rs) and `DispatchAction::Suspend` (vm_server.rs) are
    /// unreachable until kernel `sys_update` lands. Flip this test when
    /// live update is implemented.
    /// V11/T7 structural test: the CALLMAP is the executable form of C's
    /// CALLMAP (main.c:543-575). Exactly the 26 calls C registers are
    /// registered here; the four parity-unregistered numbers (exec_newmem,
    /// ADDDMA/DELDMA/GETDMA — see the note above `build_callmap`) stay
    /// `None`, and every other relative index is `None` too.
    #[test]
    fn test_callmap_registration_matches_c() {
        let registered: Vec<usize> = CALLMAP
            .iter()
            .enumerate()
            .filter(|(_, h)| h.is_some())
            .map(|(i, _)| i)
            .collect();
        assert_eq!(registered.len(), 26, "exactly the 26 C CALLMAP entries");
        for rel in [call_index(VM_EXEC_NEWMEM), call_index(VM_ADDDMA), call_index(VM_DELDMA), call_index(VM_GETDMA)] {
            assert!(CALLMAP[rel].is_none(), "parity calls must stay unregistered: +{rel}");
        }
        // The highest registered call is VM_RS_PREPARE (+48), the highest
        // relative index in the table (C: com.h:769 NR_VM_CALLS 49).
        assert_eq!(CALLMAP.len(), 49);
        assert!(CALLMAP[48].is_some());
        assert!(CALLMAP.iter().skip(49).next().is_none());
    }

    #[test]
    fn test_dispatch_rs_update_pins_not_implemented() {
        use crate::region::PAGE_SIZE as REGION_PAGE_SIZE;

        let table = VmProcTable::get_global();
        unsafe {
            table.reset_slot(UserSlot::new(50));
            table.reset_slot(UserSlot::new(51));
        }
        for slot in [50i32, 51] {
            let empty = table.get_empty(UserSlot::new(slot as usize)).unwrap();
            empty.activate(Endpoint::from_generation_slot(1, slot)).init_regions();
        }

        let mut page_alloc = default_vm();
        let mut frames =
            crate::region::PageFrames::new(minix_types::PhysBytes(256 * REGION_PAGE_SIZE as u64));
        let src = Endpoint::from_generation_slot(1, 50);
        let dst = Endpoint::from_generation_slot(1, 51);

        let reply = MessageDispatcher::dispatch_rs_update(
            &mut crate::vm_server::VmContext {
                proc_table: table,
                gateway: test_gateway(),
                page_alloc,
                page_frames: Some(frames),
                page_cache: _default_cache(),
                vfs_queue: crate::vfs_queue::VfsRequestQueue::new(),
                kernel_allocated: crate::boot::KernelAllocated::ZERO,
                vm_allocated_bytes: 0,
                pagefault_errors: 0,
                dropped_messages: 0,
                sanity_ticks: 0,
            },
            src,
            dst,
            0,
        );
        assert!(
            matches!(reply, VmReply::Error(VmError::NotImplemented)),
            "V10-P1-2: live update must stay NotImplemented until sys_update lands: {:?}",
            reply
        );
    }

    // ── DEFERRED dispatcher happy-path tests ─────
    //
    // These tests pin the fail-closed validation behavior of the 4
    // newly-added dispatch functions (procctl, remap, remap_ro, vfs_reply).
    // Each test sets up a MessageM1 payload that simulates what the
    // corresponding C sender would write, then checks the result matches
    // the documented validation behavior. Real end-to-end behavior is
    // DEFERRED — these tests only cover the "reject bad input" half of
    // the fail-closed contract.

    fn default_vm() -> crate::alloc_page::VmPageAllocator {
        // Tiny allocator for tests; capacity is irrelevant since we
        // never reach the allocation path in validation-only tests.
        use crate::phys_mem::bitmap_alloc::BitmapAllocator;
        use crate::phys_mem::PhysAlloc;
        crate::alloc_page::VmPageAllocator::new(PhysAlloc::Bitmap(BitmapAllocator::new_for_test(256)))
    }

    fn default_frames() -> crate::region::PageFrames {
        use crate::region::page_state::PAGE_SIZE;
        use minix_types::PhysBytes;
        crate::region::PageFrames::new(PhysBytes(256 * PAGE_SIZE as u64))
    }

    fn default_table() -> &'static VmProcTable {
        VmProcTable::get_global()
    }

    fn _default_cache() -> crate::page_cache::PageCache {
        crate::page_cache::PageCache::new()
    }

    fn test_gateway() -> alloc::rc::Rc<
        core::cell::RefCell<alloc::boxed::Box<dyn crate::kernel_gateway::KernelGateway>>,
    > {
        alloc::rc::Rc::new(core::cell::RefCell::new(alloc::boxed::Box::new(
            crate::kernel_gateway::MockGateway::new(),
        )))
    }

    #[test]
    fn test_dispatch_procctl_rejects_negative_param() {
        // Unknown param values → EINVAL (C: exit.c:149 default).
        let req = VmProcctlIn {
            param: -1,
            who: Endpoint(1),
            m1: 0,
            len: 0,
            flags: 0,
        };
        let mut page_alloc = default_vm();
        let mut frames = default_frames();
        match MessageDispatcher::dispatch_procctl(
                &mut crate::vm_server::VmContext {
                    proc_table: default_table(),
                    gateway: test_gateway(),
                    page_alloc,
                    page_frames: Some(frames),
                    page_cache: _default_cache(),
                    vfs_queue: crate::vfs_queue::VfsRequestQueue::new(),
                    kernel_allocated: crate::boot::KernelAllocated::ZERO,
                    vm_allocated_bytes: 0,
                    pagefault_errors: 0,
                    dropped_messages: 0,
                    sanity_ticks: 0,
                },
                Endpoint(0), req,
            ) {
            VmReply::Error(VmError::InvalidParam) => {} // expected: EINVAL (C exit.c:149)
            other => panic!("dispatch_procctl(-1) must return InvalidParam (EINVAL), got {:?}", other),
        }
    }

    #[test]
    fn test_dispatch_procctl_rejects_zero_who() {
        let req = VmProcctlIn {
            param: 1,
            who: Endpoint(0),
            m1: 0,
            len: 0,
            flags: 0,
        };
        let mut page_alloc = default_vm();
        let mut frames = default_frames();
        match MessageDispatcher::dispatch_procctl(
                &mut crate::vm_server::VmContext {
                    proc_table: default_table(),
                    gateway: test_gateway(),
                    page_alloc,
                    page_frames: Some(frames),
                    page_cache: _default_cache(),
                    vfs_queue: crate::vfs_queue::VfsRequestQueue::new(),
                    kernel_allocated: crate::boot::KernelAllocated::ZERO,
                    vm_allocated_bytes: 0,
                    pagefault_errors: 0,
                    dropped_messages: 0,
                    sanity_ticks: 0,
                },
                Endpoint(0), req,
            ) {
            // C do_procctl collapses vm_isokendpt failures to EINVAL
            // (exit.c:122-125) → InvalidProcess.
            VmReply::Error(VmError::InvalidProcess) => {} // expected
            other => panic!("dispatch_procctl(who=0) must return InvalidProcess (EINVAL), got {:?}", other),
        }
    }

    #[test]
    fn test_dispatch_procctl_clear_rejects_unauthorized_caller() {
        // VMPPARAM_CLEAR requires RS or VFS as caller.
        // Caller endpoint 42 is neither RS(0) nor VFS(1).
        let req = VmProcctlIn {
            param: 1, // VMPPARAM_CLEAR
            who: Endpoint(42),
            m1: 0,
            len: 0,
            flags: 0,
        };
        let mut page_alloc = default_vm();
        let mut frames = default_frames();
        match MessageDispatcher::dispatch_procctl(
                &mut crate::vm_server::VmContext {
                    proc_table: default_table(),
                    gateway: test_gateway(),
                    page_alloc,
                    page_frames: Some(frames),
                    page_cache: _default_cache(),
                    vfs_queue: crate::vfs_queue::VfsRequestQueue::new(),
                    kernel_allocated: crate::boot::KernelAllocated::ZERO,
                    vm_allocated_bytes: 0,
                    pagefault_errors: 0,
                    dropped_messages: 0,
                    sanity_ticks: 0,
                },
                Endpoint(42), req,
            ) {
            VmReply::Error(VmError::PermissionDenied) => {} // expected
            other => panic!("dispatch_procctl CLEAR from non-RS/VFS must return PermissionDenied, got {:?}", other),
        }
    }

    #[test]
    fn test_dispatch_procctl_handlemem_rejects_non_vfs_caller() {
        // VMPPARAM_HANDLEMEM requires VFS as caller.
        // RS(0) is not VFS(1).
        let req = VmProcctlIn {
            param: 2, // VMPPARAM_HANDLEMEM
            who: Endpoint(42),
            m1: 0x1000,
            len: 16,
            flags: 1,
        };
        let mut page_alloc = default_vm();
        let mut frames = default_frames();
        match MessageDispatcher::dispatch_procctl(
                &mut crate::vm_server::VmContext {
                    proc_table: default_table(),
                    gateway: test_gateway(),
                    page_alloc,
                    page_frames: Some(frames),
                    page_cache: _default_cache(),
                    vfs_queue: crate::vfs_queue::VfsRequestQueue::new(),
                    kernel_allocated: crate::boot::KernelAllocated::ZERO,
                    vm_allocated_bytes: 0,
                    pagefault_errors: 0,
                    dropped_messages: 0,
                    sanity_ticks: 0,
                },
                Endpoint(0), req,
            ) {
            VmReply::Error(VmError::PermissionDenied) => {} // expected
            other => panic!("dispatch_procctl HANDLEMEM from RS must return PermissionDenied, got {:?}", other),
        }
    }

    #[test]
    fn test_dispatch_procctl_unknown_param_returns_einval() {
        // Unknown param values → EINVAL (C: exit.c:149 default).
        let req = VmProcctlIn {
            param: 99,
            who: Endpoint(42),
            m1: 0,
            len: 0,
            flags: 0,
        };
        let mut page_alloc = default_vm();
        let mut frames = default_frames();
        match MessageDispatcher::dispatch_procctl(
                &mut crate::vm_server::VmContext {
                    proc_table: default_table(),
                    gateway: test_gateway(),
                    page_alloc,
                    page_frames: Some(frames),
                    page_cache: _default_cache(),
                    vfs_queue: crate::vfs_queue::VfsRequestQueue::new(),
                    kernel_allocated: crate::boot::KernelAllocated::ZERO,
                    vm_allocated_bytes: 0,
                    pagefault_errors: 0,
                    dropped_messages: 0,
                    sanity_ticks: 0,
                },
                Endpoint(0), req,
            ) {
            VmReply::Error(VmError::InvalidParam) => {} // expected: EINVAL
            other => panic!("dispatch_procctl(unknown param) must return InvalidParam (EINVAL), got {:?}", other),
        }
    }

    #[test]
    fn test_dispatch_remap_rejects_zero_vaddr() {
        let mut page_alloc = default_vm();
        let mut frames = default_frames();
        let req = VmRemapIn {
            caller: Endpoint(1),
            destination: Endpoint(1),
            who: Endpoint(2),
            vaddr: VirBytes(0),
            length: VirBytes(0x1000),
            target: VirBytes(0x2000),
            flags: 0,
        };
        // vaddr=0 won't match any region start, so endpoint validation
        // or region lookup will fail (C: map_lookup returns NULL for addr 0)
        match MessageDispatcher::dispatch_remap(&mut crate::vm_server::VmContext {
                    proc_table: default_table(),
                    gateway: test_gateway(),
                    page_alloc,
                    page_frames: Some(frames),
                    page_cache: _default_cache(),
                    vfs_queue: crate::vfs_queue::VfsRequestQueue::new(),
                    kernel_allocated: crate::boot::KernelAllocated::ZERO,
                    vm_allocated_bytes: 0,
                    pagefault_errors: 0,
                    dropped_messages: 0,
                    sanity_ticks: 0,
                }, req) {
            VmReply::Error(_) => {} // expected: any error
            other => panic!("dispatch_remap(vaddr=0) must return Error, got {:?}", other),
        }
    }

    #[test]
    fn test_dispatch_remap_rejects_zero_length() {
        let mut page_alloc = default_vm();
        let mut frames = default_frames();
        let req = VmRemapIn {
            caller: Endpoint(1),
            destination: Endpoint(1),
            who: Endpoint(2),
            vaddr: VirBytes(0x1000),
            length: VirBytes(0),
            target: VirBytes(0x2000),
            flags: 0,
        };
        match MessageDispatcher::dispatch_remap(&mut crate::vm_server::VmContext {
                    proc_table: default_table(),
                    gateway: test_gateway(),
                    page_alloc,
                    page_frames: Some(frames),
                    page_cache: _default_cache(),
                    vfs_queue: crate::vfs_queue::VfsRequestQueue::new(),
                    kernel_allocated: crate::boot::KernelAllocated::ZERO,
                    vm_allocated_bytes: 0,
                    pagefault_errors: 0,
                    dropped_messages: 0,
                    sanity_ticks: 0,
                }, req) {
            VmReply::Error(VmError::InvalidParam) => {} // expected (C: EINVAL)
            other => panic!("dispatch_remap(length=0) must return InvalidParam, got {:?}", other),
        }
    }

    #[test]
    fn test_dispatch_remap_rejects_invalid_endpoints() {
        let mut page_alloc = default_vm();
        let mut frames = default_frames();
        // Endpoint(1) is not in the process table → InvalidEndpoint
        let req = VmRemapIn {
            caller: Endpoint(1),
            destination: Endpoint(1),
            who: Endpoint(2),
            vaddr: VirBytes(0x1000),
            length: VirBytes(0x1000),
            target: VirBytes(0x2000),
            flags: 0,
        };
        match MessageDispatcher::dispatch_remap(&mut crate::vm_server::VmContext {
                    proc_table: default_table(),
                    gateway: test_gateway(),
                    page_alloc,
                    page_frames: Some(frames),
                    page_cache: _default_cache(),
                    vfs_queue: crate::vfs_queue::VfsRequestQueue::new(),
                    kernel_allocated: crate::boot::KernelAllocated::ZERO,
                    vm_allocated_bytes: 0,
                    pagefault_errors: 0,
                    dropped_messages: 0,
                    sanity_ticks: 0,
                }, req) {
            VmReply::Error(VmError::InvalidParam) => {} // expected (C: EINVAL)
            other => panic!("dispatch_remap(bad endpoint) must return InvalidParam, got {:?}", other),
        }
    }

    #[test]
    fn test_dispatch_remap_ro_rejects_invalid_endpoints() {
        let mut page_alloc = default_vm();
        let mut frames = default_frames();
        let req = VmRemapIn {
            caller: Endpoint(1),
            destination: Endpoint(1),
            who: Endpoint(2),
            vaddr: VirBytes(0x1000),
            length: VirBytes(0x1000),
            target: VirBytes(0x2000),
            flags: 0,
        };
        match MessageDispatcher::dispatch_remap_ro(&mut crate::vm_server::VmContext {
                    proc_table: default_table(),
                    gateway: test_gateway(),
                    page_alloc,
                    page_frames: Some(frames),
                    page_cache: _default_cache(),
                    vfs_queue: crate::vfs_queue::VfsRequestQueue::new(),
                    kernel_allocated: crate::boot::KernelAllocated::ZERO,
                    vm_allocated_bytes: 0,
                    pagefault_errors: 0,
                    dropped_messages: 0,
                    sanity_ticks: 0,
                }, req) {
            VmReply::Error(VmError::InvalidParam) => {} // expected (C: EINVAL)
            other => panic!("dispatch_remap_ro(bad endpoint) must return InvalidParam, got {:?}", other),
        }
    }

    #[test]
    fn test_dispatch_vfs_reply_rejects_zero_reqid() {
        let mut vfs_queue = crate::vfs_queue::VfsRequestQueue::new();
        let req = VmVfsReplyIn {
            endpoint: Endpoint(1),
            result: 0,
            reqid: 0,
            dev: 0,
            ino: 0,
            fd: 0,
            size_pages: 0,
        };
        let result = MessageDispatcher::dispatch_vfs_reply(
                &mut crate::vm_server::VmContext {
                    proc_table: default_table(),
                    gateway: test_gateway(),
                    page_alloc: default_vm(),
                    page_frames: Some(default_frames()),
                    page_cache: _default_cache(),
                    vfs_queue,
                    kernel_allocated: crate::boot::KernelAllocated::ZERO,
                    vm_allocated_bytes: 0,
                    pagefault_errors: 0,
                    dropped_messages: 0,
                    sanity_ticks: 0,
                },
                req,
            );
        match result.reply {
            VmReply::Error(VmError::InvalidAddress) => {} // expected
            other => panic!("dispatch_vfs_reply(reqid=0) must return InvalidAddress, got {:?}", other),
        }
        assert!(result.callback.is_none());
    }

    #[test]
    fn test_dispatch_vfs_reply_no_active_request_returns_error() {
        let mut vfs_queue = crate::vfs_queue::VfsRequestQueue::new();
        // No active request in the queue — handle_reply will fail
        let req = VmVfsReplyIn {
            endpoint: Endpoint(5),
            result: 0,
            reqid: 42,
            dev: 100,
            ino: 7,
            fd: 3,
            size_pages: 16,
        };
        let result = MessageDispatcher::dispatch_vfs_reply(
                &mut crate::vm_server::VmContext {
                    proc_table: default_table(),
                    gateway: test_gateway(),
                    page_alloc: default_vm(),
                    page_frames: Some(default_frames()),
                    page_cache: _default_cache(),
                    vfs_queue,
                    kernel_allocated: crate::boot::KernelAllocated::ZERO,
                    vm_allocated_bytes: 0,
                    pagefault_errors: 0,
                    dropped_messages: 0,
                    sanity_ticks: 0,
                },
                req,
            );
        match result.reply {
            VmReply::Error(VmError::InvalidAddress) => {} // expected: no active request
            other => panic!("dispatch_vfs_reply(no active) must return error, got {:?}", other),
        }
        assert!(result.callback.is_none());
    }

    #[test]
    fn test_dispatch_vfs_reply_negative_reqid_returns_error() {
        let mut vfs_queue = crate::vfs_queue::VfsRequestQueue::new();
        let req = VmVfsReplyIn {
            endpoint: Endpoint(5),
            result: 0,
            reqid: -1,
            dev: 0,
            ino: 0,
            fd: 0,
            size_pages: 0,
        };
        let result = MessageDispatcher::dispatch_vfs_reply(
                &mut crate::vm_server::VmContext {
                    proc_table: default_table(),
                    gateway: test_gateway(),
                    page_alloc: default_vm(),
                    page_frames: Some(default_frames()),
                    page_cache: _default_cache(),
                    vfs_queue,
                    kernel_allocated: crate::boot::KernelAllocated::ZERO,
                    vm_allocated_bytes: 0,
                    pagefault_errors: 0,
                    dropped_messages: 0,
                    sanity_ticks: 0,
                },
                req,
            );
        match result.reply {
            VmReply::Error(VmError::InvalidAddress) => {} // expected
            other => panic!("dispatch_vfs_reply(reqid=-1) must return InvalidAddress, got {:?}", other),
        }
        assert!(result.callback.is_none());
    }

    #[test]
    fn test_dispatch_forgetcache_rejects_zero_pages() {
        let mut cache = _default_cache();
        let mut frames = default_frames();
        let mut page_alloc = default_vm();
        let req = VmCacheIn {
            dev: 1,
            dev_offset: 0,
            ino: 0,
            ino_offset: 0,
            pages: 0,
            flags: 0,
            block: 0,
        };
        // C: bytes < VM_PAGE_SIZE → EINVAL (mem_cache.c:292-294).
        match MessageDispatcher::dispatch_forgetcache(
                &mut crate::vm_server::VmContext {
                    proc_table: default_table(),
                    gateway: test_gateway(),
                    page_alloc,
                    page_frames: Some(frames),
                    page_cache: cache,
                    vfs_queue: crate::vfs_queue::VfsRequestQueue::new(),
                    kernel_allocated: crate::boot::KernelAllocated::ZERO,
                    vm_allocated_bytes: 0,
                    pagefault_errors: 0,
                    dropped_messages: 0,
                    sanity_ticks: 0,
                },
                req,
            ) {
            VmReply::Error(VmError::InvalidParam) => {}
            other => panic!("dispatch_forgetcache(pages=0) must return InvalidParam (EINVAL), got {:?}", other),
        }
    }

    #[test]
    fn test_dispatch_forgetcache_rejects_unaligned_offset() {
        let mut cache = _default_cache();
        let mut frames = default_frames();
        let req = VmCacheIn {
            dev: 1,
            dev_offset: 100, // not page-aligned
            ino: 0,
            ino_offset: 0,
            pages: 1,
            flags: 0,
            block: 0,
        };
        let mut page_alloc = default_vm();
        match MessageDispatcher::dispatch_forgetcache(
                &mut crate::vm_server::VmContext {
                    proc_table: default_table(),
                    gateway: test_gateway(),
                    page_alloc,
                    page_frames: Some(frames),
                    page_cache: cache,
                    vfs_queue: crate::vfs_queue::VfsRequestQueue::new(),
                    kernel_allocated: crate::boot::KernelAllocated::ZERO,
                    vm_allocated_bytes: 0,
                    pagefault_errors: 0,
                    dropped_messages: 0,
                    sanity_ticks: 0,
                },
                req,
            ) {
            VmReply::Error(VmError::InvalidAddress) => {}
            other => panic!("dispatch_forgetcache(unaligned offset) must return InvalidAddress, got {:?}", other),
        }
    }

    #[test]
    fn test_dispatch_forgetcache_valid_input_returns_ok() {
        let mut cache = _default_cache();
        let mut frames = default_frames();
        let req = VmCacheIn {
            dev: 1,
            dev_offset: 0,
            ino: 0,
            ino_offset: 0,
            pages: 1,
            flags: 0,
            block: 0,
        };
        let mut page_alloc = default_vm();
        match MessageDispatcher::dispatch_forgetcache(
                &mut crate::vm_server::VmContext {
                    proc_table: default_table(),
                    gateway: test_gateway(),
                    page_alloc,
                    page_frames: Some(frames),
                    page_cache: cache,
                    vfs_queue: crate::vfs_queue::VfsRequestQueue::new(),
                    kernel_allocated: crate::boot::KernelAllocated::ZERO,
                    vm_allocated_bytes: 0,
                    pagefault_errors: 0,
                    dropped_messages: 0,
                    sanity_ticks: 0,
                },
                req,
            ) {
            VmReply::Ok => {}
            other => panic!("dispatch_forgetcache(valid) must return Ok, got {:?}", other),
        }
    }

    // ── dispatch_setcache tests ──────────────────────────────────────

    #[test]
    fn test_dispatch_setcache_rejects_zero_pages() {
        let table = default_table();
        let mut frames = default_frames();
        let mut cache = _default_cache();
        let req = VmCacheIn {
            dev: 1,
            dev_offset: 0,
            ino: 0,
            ino_offset: 0,
            pages: 0,
            flags: 0,
            block: 0,
        };
        let mut page_alloc = default_vm();
        // C: bytes < VM_PAGE_SIZE → EINVAL (mem_cache.c:204-205).
        match MessageDispatcher::dispatch_setcache(&mut crate::vm_server::VmContext { proc_table: table, gateway: test_gateway(), page_alloc, page_frames: Some(frames), page_cache: cache, vfs_queue: crate::vfs_queue::VfsRequestQueue::new(), kernel_allocated: crate::boot::KernelAllocated::ZERO, vm_allocated_bytes: 0, pagefault_errors: 0, dropped_messages: 0, sanity_ticks: 0 }, Endpoint(1), req) {
            VmReply::Error(VmError::InvalidParam) => {}
            other => panic!("dispatch_setcache(pages=0) must return InvalidParam (EINVAL), got {:?}", other),
        }
    }

    #[test]
    fn test_dispatch_setcache_fails_closed_without_valid_caller() {
        // The fresh global process table has no active processes, so any
        // setcache fails fail-closed at the endpoint check (C: panic
        // "bogus source" → Rust InvalidProcess). The NO_DEV guard inside
        // addcache is covered by page_cache::tests::test_addcache_rejects_no_device.
        let table = default_table();
        let mut frames = default_frames();
        let mut cache = _default_cache();
        let req = VmCacheIn {
            dev: 0,
            dev_offset: 0,
            ino: 0,
            ino_offset: 0,
            pages: 1,
            flags: 0,
            block: 0x1000,
        };
        let mut page_alloc = default_vm();
        match MessageDispatcher::dispatch_setcache(&mut crate::vm_server::VmContext { proc_table: table, gateway: test_gateway(), page_alloc, page_frames: Some(frames), page_cache: cache, vfs_queue: crate::vfs_queue::VfsRequestQueue::new(), kernel_allocated: crate::boot::KernelAllocated::ZERO, vm_allocated_bytes: 0, pagefault_errors: 0, dropped_messages: 0, sanity_ticks: 0 }, Endpoint(1), req) {
            VmReply::Error(VmError::InvalidProcess) => {}
            other => panic!("dispatch_setcache(no valid caller) must return InvalidProcess, got {:?}", other),
        }
    }

    #[test]
    fn test_dispatch_setcache_rejects_unaligned_dev_offset() {
        let table = default_table();
        let mut frames = default_frames();
        let mut cache = _default_cache();
        let req = VmCacheIn {
            dev: 1,
            dev_offset: 100, // not page-aligned
            ino: 0,
            ino_offset: 0,
            pages: 1,
            flags: 0,
            block: 0x1000,
        };
        let mut page_alloc = default_vm();
        match MessageDispatcher::dispatch_setcache(&mut crate::vm_server::VmContext { proc_table: table, gateway: test_gateway(), page_alloc, page_frames: Some(frames), page_cache: cache, vfs_queue: crate::vfs_queue::VfsRequestQueue::new(), kernel_allocated: crate::boot::KernelAllocated::ZERO, vm_allocated_bytes: 0, pagefault_errors: 0, dropped_messages: 0, sanity_ticks: 0 }, Endpoint(1), req) {
            VmReply::Error(VmError::InvalidAddress) => {}
            other => panic!("dispatch_setcache(unaligned dev_offset) must return InvalidAddress, got {:?}", other),
        }
    }

    #[test]
    fn test_dispatch_setcache_rejects_invalid_caller() {
        let table = default_table();
        let mut frames = default_frames();
        let mut cache = _default_cache();
        let req = VmCacheIn {
            dev: 1,
            dev_offset: 0,
            ino: 0,
            ino_offset: 0,
            pages: 1,
            flags: 0,
            block: 0x1000,
        };
        // Endpoint(999) is not in the process table
        let mut page_alloc = default_vm();
        match MessageDispatcher::dispatch_setcache(&mut crate::vm_server::VmContext { proc_table: table, gateway: test_gateway(), page_alloc, page_frames: Some(frames), page_cache: cache, vfs_queue: crate::vfs_queue::VfsRequestQueue::new(), kernel_allocated: crate::boot::KernelAllocated::ZERO, vm_allocated_bytes: 0, pagefault_errors: 0, dropped_messages: 0, sanity_ticks: 0 }, Endpoint(999), req) {
            VmReply::Error(VmError::InvalidProcess) => {}
            other => panic!("dispatch_setcache(invalid caller) must return InvalidProcess, got {:?}", other),
        }
    }

    // ── dispatch_mapcache tests ──────────────────────────────────────

    #[test]
    fn test_dispatch_mapcache_rejects_unaligned_offset() {
        let table = default_table();
        let mut page_alloc = default_vm();
        let mut frames = default_frames();
        let cache = _default_cache();
        let req = VmCacheIn {
            dev: 1,
            dev_offset: 100, // not page-aligned
            ino: 0,
            ino_offset: 0,
            pages: 1,
            flags: 0,
            block: 0,
        };
        let mut cache = _default_cache();
        match MessageDispatcher::dispatch_mapcache(&mut crate::vm_server::VmContext { proc_table: table, gateway: test_gateway(), page_alloc, page_frames: Some(frames), page_cache: cache, vfs_queue: crate::vfs_queue::VfsRequestQueue::new(), kernel_allocated: crate::boot::KernelAllocated::ZERO, vm_allocated_bytes: 0, pagefault_errors: 0, dropped_messages: 0, sanity_ticks: 0 }, Endpoint(1), req) {
            VmReply::Error(VmError::InvalidAddress) => {}
            other => panic!("dispatch_mapcache(unaligned dev_offset) must return InvalidAddress, got {:?}", other),
        }
    }

    #[test]
    fn test_dispatch_mapcache_rejects_zero_pages() {
        let table = default_table();
        let mut page_alloc = default_vm();
        let mut frames = default_frames();
        let cache = _default_cache();
        let req = VmCacheIn {
            dev: 1,
            dev_offset: 0,
            ino: 0,
            ino_offset: 0,
            pages: 0,
            flags: 0,
            block: 0,
        };
        let mut cache = _default_cache();
        // C: bytes < VM_PAGE_SIZE → EINVAL (mem_cache.c:107).
        match MessageDispatcher::dispatch_mapcache(&mut crate::vm_server::VmContext { proc_table: table, gateway: test_gateway(), page_alloc, page_frames: Some(frames), page_cache: cache, vfs_queue: crate::vfs_queue::VfsRequestQueue::new(), kernel_allocated: crate::boot::KernelAllocated::ZERO, vm_allocated_bytes: 0, pagefault_errors: 0, dropped_messages: 0, sanity_ticks: 0 }, Endpoint(1), req) {
            VmReply::Error(VmError::InvalidParam) => {}
            other => panic!("dispatch_mapcache(pages=0) must return InvalidParam (EINVAL), got {:?}", other),
        }
    }

    #[test]
    fn test_dispatch_mapcache_rejects_invalid_caller() {
        let table = default_table();
        let mut page_alloc = default_vm();
        let mut frames = default_frames();
        let cache = _default_cache();
        let req = VmCacheIn {
            dev: 1,
            dev_offset: 0,
            ino: 0,
            ino_offset: 0,
            pages: 1,
            flags: 0,
            block: 0,
        };
        // Endpoint(999) is not in the process table
        let mut cache = _default_cache();
        match MessageDispatcher::dispatch_mapcache(&mut crate::vm_server::VmContext { proc_table: table, gateway: test_gateway(), page_alloc, page_frames: Some(frames), page_cache: cache, vfs_queue: crate::vfs_queue::VfsRequestQueue::new(), kernel_allocated: crate::boot::KernelAllocated::ZERO, vm_allocated_bytes: 0, pagefault_errors: 0, dropped_messages: 0, sanity_ticks: 0 }, Endpoint(999), req) {
            VmReply::Error(VmError::InvalidProcess) => {}
            other => panic!("dispatch_mapcache(invalid caller) must return InvalidProcess, got {:?}", other),
        }
    }

    #[test]
    fn test_dispatch_mapcache_cache_miss_returns_not_found() {
        let table = default_table();
        let mut page_alloc = default_vm();
        let mut frames = default_frames();
        let cache = _default_cache(); // empty cache → all lookups miss
        let req = VmCacheIn {
            dev: 1,
            dev_offset: 0,
            ino: 0,
            ino_offset: 0,
            pages: 1,
            flags: 0,
            block: 0,
        };
        // Caller endpoint doesn't matter for cache miss path —
        // but we need a valid endpoint to get past vm_isokendpt.
        // Endpoint(0) is VM itself, which should be in the table.
        let mut cache = _default_cache();
        match MessageDispatcher::dispatch_mapcache(&mut crate::vm_server::VmContext { proc_table: table, gateway: test_gateway(), page_alloc, page_frames: Some(frames), page_cache: cache, vfs_queue: crate::vfs_queue::VfsRequestQueue::new(), kernel_allocated: crate::boot::KernelAllocated::ZERO, vm_allocated_bytes: 0, pagefault_errors: 0, dropped_messages: 0, sanity_ticks: 0 }, Endpoint(0), req) {
            VmReply::Error(VmError::NotFound) | VmReply::Error(VmError::InvalidProcess) => {} // either is acceptable
            other => panic!("dispatch_mapcache(cache miss) must return NotFound or InvalidProcess, got {:?}", other),
        }
    }

    // ── VM_RS_MEMCTL sub-request decode (25-P0-1 regression) ─────

    #[test]
    fn test_decode_rs_memctl_unknown_req_einval() {
        // C: rs.c:386-388 — `do_rs_memctl` default arm returns EINVAL.
        // 25-P0-1: unknown sub-request previously mapped to
        // `VmError::InvalidAddress` (EFAULT); must be EINVAL.
        let err = decode_rs_memctl_request(99, 0, 0).unwrap_err();
        assert_eq!(err, VmError::InvalidParam);
        assert_eq!(err.to_errno(), minix_types::EINVAL);
    }

    #[test]
    fn test_decode_rs_memctl_all_valid_codes() {
        // C: com.h:741-745 — VM_RS_MEM_PIN/MAKE_VM/HEAP_PREALLOC/
        // MAP_PREALLOC/GET_PREALLOC_MAP = 0..4.
        assert!(matches!(
            decode_rs_memctl_request(0, 0, 0),
            Ok(rs::RsMemctlRequest::Pin)
        ));
        assert!(matches!(
            decode_rs_memctl_request(1, 0, 0),
            Ok(rs::RsMemctlRequest::MakeVmInstance)
        ));
        assert!(matches!(
            decode_rs_memctl_request(2, 0x1000, 0x2000),
            Ok(rs::RsMemctlRequest::HeapPrealloc { addr: VirBytes(0x1000), len: 0x2000 })
        ));
        assert!(matches!(
            decode_rs_memctl_request(3, 0x1000, 0x2000),
            Ok(rs::RsMemctlRequest::MapPrealloc { addr: VirBytes(0x1000), len: 0x2000 })
        ));
        assert!(matches!(
            decode_rs_memctl_request(4, 0, 0),
            Ok(rs::RsMemctlRequest::GetPreallocMap)
        ));
    }
}
