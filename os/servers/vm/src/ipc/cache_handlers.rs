//! Cache-request handlers — the Rust `mem_cache.c` family.
//!
//! V12-P2-1: the four `do_mapcache`/`do_setcache`/`do_forgetcache`/
//! `do_clearcache` counterparts moved out of `dispatcher.rs` (which keeps
//! decode → call → encode only). C houses them in `mem_cache.c`, the cache
//! domain module; the Rust handlers live beside their dispatcher siblings
//! rather than inside `page_cache.rs` because they consume the whole
//! [`crate::vm_server::VmContext`] (process table + frames + allocator +
//! cache) and putting IPC handlers in the data-structure module would
//! invert the layering (page_cache → vm_server).
//!
//! They are inherent methods of
//! [`crate::ipc::dispatcher::MessageDispatcher`], so the CALLMAP table and
//! the tests resolve exactly as before the move.

use crate::ipc::dispatcher::MessageDispatcher;
use crate::region::{PageFrames, VrFlags, VirRegion};
use crate::vm_server::VmContext;
use crate::vmproc::VmProcTable;
use crate::page_cache::{VMC_NO_INODE, VMSF_ONCE};
use minix_types::{Endpoint, VmCacheIn, VirBytes, VmError, VmReply};

impl MessageDispatcher {
    /// Dispatch VM_MAPCACHEPAGE request.
    /// Corresponds to Minix3 `do_mapcache()` (mem_cache.c:95-193).
    /// Maps cached blocks into the caller's (the FS's) address space.
    ///
    /// # C semantics
    ///
    /// 1. Validate alignment of `dev_off`/`ino_off` → EFAULT (mem_cache.c:108-110).
    /// 2. `vm_isokendpt(msg->m_source)` → get caller.
    /// 3. `bytes < VM_PAGE_SIZE` → EINVAL (mem_cache.c:116).
    /// 4. `map_page_region(caller, VM_MMAPBASE, VM_MMAPTOP, bytes,
    ///    VR_ANON|VR_WRITABLE, 0, &mem_type_cache)` → allocate a fresh
    ///    cache-memtype region (mem_cache.c:131-134).
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

        // Step 1: alignment (C: mem_cache.c:108-110 → EFAULT).
        if !request.dev_offset.is_multiple_of(PAGE_SIZE) || !request.ino_offset.is_multiple_of(PAGE_SIZE) {
            return VmReply::Error(VmError::InvalidAddress);
        }

        // Step 2: bytes < VM_PAGE_SIZE → EINVAL (C: mem_cache.c:116).
        // Ordering note: C checks vm_isokendpt first and panics on a bogus
        // source; Rust validates the request before resolving the caller
        // (fail-closed), so a malformed request is rejected even when the
        // caller is unresolvable.
        let bytes = request.pages as u64 * PAGE_SIZE;
        if bytes < PAGE_SIZE {
            return VmReply::Error(VmError::InvalidParam);
        }

        // Step 3: caller endpoint (C: vm_isokendpt, mem_cache.c:113-114).
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
        // the whole region on miss / one-shot (C: mem_cache.c:144-167).
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

}
