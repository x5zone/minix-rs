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
    VirBytes,
    VmForkIn, VmBrkIn, VmMunmapIn, VmUnmapPhysIn, VmShmUnmapIn,
    VmExitIn, VmWillexitIn, VmPagefaultIn, VmExecNewmemIn,
    VmMmapIn, VmMapPhysIn, VmVfsMmapIn, VmCacheIn,
    VmForkOut, VmBrkOut, VmMmapOut, VmMapPhysOut,
    VmReply, VmError,
};
use crate::vmproc::VmProcTable;
use crate::alloc_page::VmPageAllocator;
use crate::region::PageFrames;
use crate::page_cache::{PageCache, CacheKey};
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

pub(crate) struct MessageDispatcher;

impl MessageDispatcher {
    // -- fork --

    /// Dispatch VM_FORK request.
    ///
    /// Corresponds to Minix3 `do_fork()` in fork.c.
    /// C returns EINVAL on vm_isokendpt failure (fork.c:44).
    pub(crate) fn dispatch_fork(
        table: &VmProcTable,
        _page_alloc: &mut VmPageAllocator,
        frames: &mut PageFrames,
        request: VmForkIn,
    ) -> VmReply {
        match fork::do_fork(table, frames, request.parent_endpoint, request.child_slot) {
            Ok(child_endpoint) => VmReply::Fork(VmForkOut { child_endpoint }),
            Err(e) => VmReply::Error(fork_error_to_vm_error(e)),
        }
    }

    // -- brk --

    /// Dispatch VM_BRK request. C: `do_brk()` in break.c.
    pub(crate) fn dispatch_brk(
        table: &VmProcTable,
        page_alloc: &mut VmPageAllocator,
        frames: &mut PageFrames,
        request: VmBrkIn,
    ) -> VmReply {
        let req = brk::BrkRequest {
            endpoint: request.endpoint,
            new_brk_addr: request.new_addr,
        };
        match brk::handle_brk(table, page_alloc, frames, &req) {
            Ok(response) => VmReply::Brk(VmBrkOut { new_addr: response.new_brk_addr }),
            Err(e) => VmReply::Error(brk_error_to_vm_error(e)),
        }
    }

    // -- munmap --

    /// Dispatch VM_MUNMAP request. C: `do_munmap()` in mmap.c.
    pub(crate) fn dispatch_munmap(
        table: &VmProcTable,
        page_alloc: &mut VmPageAllocator,
        frames: &mut PageFrames,
        request: VmMunmapIn,
    ) -> VmReply {
        let req = munmap::MunmapRequest {
            endpoint: request.endpoint,
            addr: request.addr,
            length: request.length,
            lookup_region_length: false,
        };
        match munmap::handle_munmap(table, page_alloc, frames, &req) {
            Ok(()) => VmReply::Munmap,
            Err(e) => VmReply::Error(munmap_error_to_vm_error(e)),
        }
    }

    // -- unmap_phys --
    // VM_UNMAP_PHYS: unmap a VR_DIRECT region. Length is the full region length.
    pub(crate) fn dispatch_unmap_phys(
        table: &VmProcTable,
        page_alloc: &mut VmPageAllocator,
        frames: &mut PageFrames,
        request: VmUnmapPhysIn,
    ) -> VmReply {
        let req = munmap::MunmapRequest {
            endpoint: request.target,
            addr: request.vaddr,
            length: VirBytes(0),
            lookup_region_length: true,
        };
        match munmap::handle_munmap(table, page_alloc, frames, &req) {
            Ok(()) => VmReply::Munmap,
            Err(e) => VmReply::Error(munmap_error_to_vm_error(e)),
        }
    }

    // -- shm_unmap --
    // VM_SHM_UNMAP: unmap a shared memory region. Length is the full region length.
    pub(crate) fn dispatch_shm_unmap(
        table: &VmProcTable,
        page_alloc: &mut VmPageAllocator,
        frames: &mut PageFrames,
        request: VmShmUnmapIn,
    ) -> VmReply {
        let req = munmap::MunmapRequest {
            endpoint: request.forwhom,
            addr: request.addr,
            length: VirBytes(0),
            lookup_region_length: true,
        };
        match munmap::handle_munmap(table, page_alloc, frames, &req) {
            Ok(()) => VmReply::Munmap,
            Err(e) => VmReply::Error(munmap_error_to_vm_error(e)),
        }
    }

    // -- mmap --

    /// Dispatch VM_MMAP request. C: `do_mmap()` in mmap.c.
    /// May return `VmReply::Suspend` for file-backed mappings.
    pub(crate) fn dispatch_mmap(
        table: &VmProcTable,
        page_alloc: &mut VmPageAllocator,
        frames: &mut PageFrames,
        request: VmMmapIn,
    ) -> VmReply {
        match mmap::handle_mmap(table, page_alloc, frames, &request) {
            Ok(mmap::MmapResult::Complete(response)) => VmReply::Mmap(VmMmapOut { ret_addr: response.mapped_addr }),
            Ok(mmap::MmapResult::Suspended) => VmReply::Suspend,
            Err(e) => VmReply::Error(mmap_error_to_vm_error(e)),
        }
    }

    // -- vfs_mmap (synchronous VFS-initiated path) --
    pub(crate) fn dispatch_vfs_mmap(
        table: &VmProcTable,
        page_alloc: &mut VmPageAllocator,
        frames: &mut PageFrames,
        request: VmVfsMmapIn,
    ) -> VmReply {
        match mmap::handle_vfs_mmap(table, page_alloc, frames, &request) {
            Ok(mmap::MmapResult::Complete(response)) => VmReply::VfsMmap(VmMmapOut { ret_addr: response.mapped_addr }),
            Ok(mmap::MmapResult::Suspended) => VmReply::Suspend,
            Err(e) => VmReply::Error(mmap_error_to_vm_error(e)),
        }
    }

    // -- map_phys --

    /// Dispatch VM_MAP_PHYS request. C: `do_map_phys()` in mmap.c.
    pub(crate) fn dispatch_map_phys(
        table: &VmProcTable,
        page_alloc: &mut VmPageAllocator,
        frames: &mut PageFrames,
        request: VmMapPhysIn,
    ) -> VmReply {
        match map_phys::handle_map_phys(table, page_alloc, frames, request.caller, request.target, request.phys_addr, request.length) {
            Ok(virt_addr) => VmReply::MapPhys(VmMapPhysOut { virt_addr }),
            Err(e) => VmReply::Error(map_phys_error_to_vm_error(e)),
        }
    }

    // -- cache (mapcache / setcache / forgetcache / clearcache) --

    /// Dispatch VM_MAPCACHEPAGE request.
    /// Corresponds to Minix3 `do_mapcache()` (mem_cache.c:95).
    /// Maps cached blocks into the caller's address space.
    pub(crate) fn dispatch_mapcache(
        table: &VmProcTable,
        page_alloc: &mut VmPageAllocator,
        frames: &mut PageFrames,
        cache: &mut PageCache,
        request: VmCacheIn,
    ) -> VmReply {
        let _ = (table, page_alloc, frames, cache, request);
        VmReply::Error(VmError::NotImplemented)
    }

    /// Dispatch VM_SETCACHEPAGE request.
    /// Corresponds to Minix3 `do_setcache()` (mem_cache.c:196).
    /// Registers anonymous memory pages as cache blocks.
    pub(crate) fn dispatch_setcache(
        table: &VmProcTable,
        frames: &mut PageFrames,
        cache: &mut PageCache,
        request: VmCacheIn,
    ) -> VmReply {
        let _ = (table, frames, cache, request);
        VmReply::Error(VmError::NotImplemented)
    }

    /// Dispatch VM_FORGETCACHEPAGE request.
    /// Corresponds to Minix3 `do_forgetcache()` (mem_cache.c:283).
    /// Invalidates cached pages for a given device offset range.
    pub(crate) fn dispatch_forgetcache(
        cache: &mut PageCache,
        frames: &mut PageFrames,
        request: VmCacheIn,
    ) -> VmReply {
        let bytes = request.pages as u64 * 4096;
        for offset in (0..bytes).step_by(4096) {
            cache.remove(
                &CacheKey::ByDevice { dev: request.dev, offset: request.dev_offset + offset },
                frames,
            );
        }
        VmReply::Ok
    }

    /// Dispatch VM_CLEARCACHE request.
    /// Corresponds to Minix3 `do_clearcache()` (mem_cache.c:315).
    /// Invalidates all cached pages for a given device.
    pub(crate) fn dispatch_clearcache(
        cache: &mut PageCache,
        frames: &mut PageFrames,
        request: VmCacheIn,
    ) -> VmReply {
        cache.clear_by_dev(request.dev, frames);
        VmReply::Ok
    }

    // -- exit --

    /// Dispatch VM_EXIT request. C: `do_vm_exit()` in exit.c.
    pub(crate) fn dispatch_exit(
        table: &VmProcTable,
        page_alloc: &mut VmPageAllocator,
        frames: &mut PageFrames,
        request: VmExitIn,
    ) -> VmReply {
        match exit::handle_vm_exit(table, page_alloc, frames, request.endpoint) {
            Ok(()) => VmReply::Exit,
            Err(e) => VmReply::Error(exit_error_to_vm_error(e)),
        }
    }

    // -- willexit --
    pub(crate) fn dispatch_willexit(
        table: &VmProcTable,
        request: VmWillexitIn,
    ) -> VmReply {
        match exit::handle_vm_willexit(table, request.endpoint) {
            Ok(()) => VmReply::Willexit,
            Err(e) => VmReply::Error(exit_error_to_vm_error(e)),
        }
    }

    // -- pagefault --
    pub(crate) fn dispatch_pagefault(
        table: &VmProcTable,
        page_alloc: &mut VmPageAllocator,
        frames: &mut PageFrames,
        request: VmPagefaultIn,
    ) -> VmReply {
        let _ = (table, page_alloc, frames, request);
        VmReply::Error(VmError::NotImplemented)
    }

    // -- exec_newmem --
    pub(crate) fn dispatch_exec_newmem(
        table: &VmProcTable,
        page_alloc: &mut VmPageAllocator,
        frames: &mut PageFrames,
        request: VmExecNewmemIn,
    ) -> VmReply {
        let _ = (table, page_alloc, frames, request);
        VmReply::Error(VmError::NotImplemented)
    }

    // -- rs_set_priv --
    pub(crate) fn dispatch_rs_set_priv(
        table: &VmProcTable,
        caller: minix_types::Endpoint,
        target: minix_types::Endpoint,
        mask: Option<crate::acl::AclMask>,
        is_sys_proc: bool,
    ) -> VmReply {
        match rs::handle_rs_set_priv(table, caller, target, mask, is_sys_proc) {
            Ok(()) => VmReply::Ok,
            Err(e) => VmReply::Error(rs_set_priv_error_to_vm_error(e)),
        }
    }

    // -- rs_prepare --
    pub(crate) fn dispatch_rs_prepare(
        table: &VmProcTable,
        page_alloc: &mut VmPageAllocator,
        frames: &mut PageFrames,
        src: minix_types::Endpoint,
        dst: minix_types::Endpoint,
        flags: u32,
    ) -> VmReply {
        match rs::handle_rs_prepare(table, page_alloc, frames, src, dst, flags) {
            Ok(()) => VmReply::Ok,
            Err(e) => VmReply::Error(rs_prepare_error_to_vm_error(e)),
        }
    }

    // -- rs_update --
    pub(crate) fn dispatch_rs_update(
        table: &VmProcTable,
        page_alloc: &mut VmPageAllocator,
        frames: &mut PageFrames,
        src: minix_types::Endpoint,
        dst: minix_types::Endpoint,
        flags: u32,
    ) -> VmReply {
        match rs::handle_rs_update(table, page_alloc, frames, src, dst, flags) {
            Ok(rs::RsUpdateResult::Ok) => VmReply::Ok,
            Ok(rs::RsUpdateResult::Suspend) => VmReply::Suspend,
            Err(e) => VmReply::Error(rs_update_error_to_vm_error(e)),
        }
    }

    // -- rs_memctl --
    pub(crate) fn dispatch_rs_memctl(
        table: &VmProcTable,
        page_alloc: &mut VmPageAllocator,
        frames: &mut PageFrames,
        target: minix_types::Endpoint,
        request: rs::RsMemctlRequest,
    ) -> VmReply {
        match rs::handle_rs_memctl(table, page_alloc, frames, target, request) {
            Ok(rs::RsMemctlResult::Ok) => VmReply::Ok,
            Ok(rs::RsMemctlResult::AddrLen { addr, len }) => VmReply::RsMemctlAddrLen { addr, len },
            Err(e) => VmReply::Error(rs_memctl_error_to_vm_error(e)),
        }
    }

    // -- get_phys --
    pub(crate) fn dispatch_get_phys(
        table: &VmProcTable,
        target: minix_types::Endpoint,
        addr: VirBytes,
    ) -> VmReply {
        match query::handle_get_phys(table, target, addr) {
            Ok(phys) => VmReply::GetPhys { phys_addr: phys },
            Err(e) => VmReply::Error(query_error_to_vm_error(e)),
        }
    }

    // -- get_refcount --
    pub(crate) fn dispatch_get_refcount(
        table: &VmProcTable,
        frames: &PageFrames,
        target: minix_types::Endpoint,
        addr: VirBytes,
    ) -> VmReply {
        match query::handle_get_refcount(table, frames, target, addr) {
            Ok(cnt) => VmReply::GetRefcount { count: cnt },
            Err(e) => VmReply::Error(query_error_to_vm_error(e)),
        }
    }

    // -- info --
    pub(crate) fn dispatch_info(
        table: &VmProcTable,
        page_alloc: &VmPageAllocator,
        q: query::InfoQuery,
    ) -> VmReply {
        match query::handle_info(table, page_alloc, q) {
            Ok(query::InfoResult::Stats(s)) => VmReply::InfoStats {
                page_size: s.page_size,
                total_pages: s.total_pages,
                free_pages: s.free_pages,
                largest_contiguous: s.largest_contiguous,
            },
            Ok(query::InfoResult::Usage(u)) => VmReply::InfoUsage {
                total: u.total,
                shared: u.shared,
                text: u.text,
                data: u.data,
                stack: u.stack,
            },
            Ok(query::InfoResult::Region { regions, count, next }) => VmReply::InfoRegion {
                regions: regions.map(|r| minix_types::VmRegionInfo {
                    vaddr: r.vaddr,
                    length: r.length,
                    flags: r.flags as u32,
                }),
                count,
                next,
            },
            Err(e) => VmReply::Error(query_error_to_vm_error(e)),
        }
    }

    // -- getrusage --
    pub(crate) fn dispatch_getrusage(
        table: &VmProcTable,
        caller: minix_types::Endpoint,
        target: minix_types::Endpoint,
        children: bool,
    ) -> VmReply {
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
    /// Compile-time CALLMAP replacement for C's `vm_calls[c].vmc_func(&msg)` (main.c:163-165).
    ///
    /// Each branch decodes `Message` → per-call `In` type, then delegates to
    /// the existing `dispatch_xxx()` method. Resolved at compile time — no
    /// runtime function pointer table needed.
    ///
    /// Full coverage of ~20 CALLMAP entries from main.c:508-538.
    // TODO: add missing imports and decode helpers; most branches are stubs.
    pub(crate) fn dispatch_by_number(
        call_nr: usize,
        msg: &Message,
        server: &mut crate::VmServer,
    ) -> VmReply {
        let table = VmProcTable::get_global();
        let frames = server.page_frames_mut();
        let page_alloc = server.page_alloc_mut();
        let cache = server.page_cache_mut();

        let vm_rq_base = VM_RQ_BASE as usize;

        match call_nr {
            _c if _c == VM_MMAP as usize - vm_rq_base =>
                Self::dispatch_mmap(table, page_alloc, frames, VmMmapIn::decode(msg)),
            _c if _c == VM_MUNMAP as usize - vm_rq_base =>
                Self::dispatch_munmap(table, page_alloc, frames, VmMunmapIn::decode(msg)),
            _c if _c == VM_MAP_PHYS as usize - vm_rq_base =>
                Self::dispatch_map_phys(table, page_alloc, frames, VmMapPhysIn::decode(msg)),
            _c if _c == VM_EXIT as usize - vm_rq_base =>
                Self::dispatch_exit(table, page_alloc, frames, VmExitIn::decode(msg)),
            _c if _c == VM_FORK as usize - vm_rq_base =>
                Self::dispatch_fork(table, page_alloc, frames, VmForkIn::decode(msg)),
            _c if _c == VM_BRK as usize - vm_rq_base =>
                Self::dispatch_brk(table, page_alloc, frames, VmBrkIn::decode(msg)),
            _c if _c == VM_WILLEXIT as usize - vm_rq_base =>
                Self::dispatch_willexit(table, VmWillexitIn::decode(msg)),
            _c if _c == VM_VFS_MMAP as usize - vm_rq_base =>
                Self::dispatch_vfs_mmap(table, page_alloc, frames, VmVfsMmapIn::decode(msg)),
            _c if _c == VM_MAPCACHEPAGE as usize - vm_rq_base =>
                Self::dispatch_mapcache(table, page_alloc, frames, cache, VmCacheIn::decode(msg)),
            _c if _c == VM_SETCACHEPAGE as usize - vm_rq_base =>
                Self::dispatch_setcache(table, frames, cache, VmCacheIn::decode(msg)),
            _c if _c == VM_FORGETCACHEPAGE as usize - vm_rq_base =>
                Self::dispatch_forgetcache(cache, frames, VmCacheIn::decode(msg)),
            _c if _c == VM_CLEARCACHE as usize - vm_rq_base =>
                Self::dispatch_clearcache(cache, frames, VmCacheIn::decode(msg)),
            // RS calls — TODO: decode helpers needed
            _c if _c == VM_RS_SET_PRIV as usize - vm_rq_base => {
                // TODO: decode_rs_set_priv(msg)
                VmReply::Error(VmError::NotImplemented)
            }
            _c if _c == VM_RS_PREPARE as usize - vm_rq_base => {
                // TODO: decode_rs_prepare(msg)
                VmReply::Error(VmError::NotImplemented)
            }
            _c if _c == VM_RS_UPDATE as usize - vm_rq_base => {
                // TODO: decode_rs_update(msg)
                VmReply::Error(VmError::NotImplemented)
            }
            _c if _c == VM_RS_MEMCTL as usize - vm_rq_base => {
                // TODO: decode_rs_memctl(msg)
                VmReply::Error(VmError::NotImplemented)
            }
            // Generic queries — TODO: decode helpers needed
            _c if _c == VM_GETPHYS as usize - vm_rq_base => {
                // TODO: decode_get_phys(msg)
                VmReply::Error(VmError::NotImplemented)
            }
            _c if _c == VM_GETREF as usize - vm_rq_base => {
                // TODO: decode_get_refcount(msg)
                VmReply::Error(VmError::NotImplemented)
            }
            _c if _c == VM_INFO as usize - vm_rq_base => {
                // TODO: decode_info(msg)
                VmReply::Error(VmError::NotImplemented)
            }
            _c if _c == VM_GETRUSAGE as usize - vm_rq_base => {
                // TODO: decode_getrusage(msg)
                VmReply::Error(VmError::NotImplemented)
            }
            // C has: VM_REMAP, VM_REMAP_RO, VM_PROCCTL, VM_UNMAP_PHYS,
            //        VM_SHM_UNMAP, VM_ADDDMA, VM_DELDMA, VM_GETDMA, VM_VFS_REPLY.
            // These are not yet implemented — return NotImplemented.
            _ => VmReply::Error(VmError::NotImplemented),
        }
    }
}

// ==========================================================================
// Error mapping helpers — per-service error → VmError
// ==========================================================================

fn fork_error_to_vm_error(e: fork::ForkError) -> VmError {
    match e {
        fork::ForkError::InvalidEndpoint => VmError::InvalidProcess,
        fork::ForkError::InvalidSlot => VmError::InvalidProcess,
        fork::ForkError::SlotInUse => VmError::SlotInUse,
        fork::ForkError::NoMemory => VmError::OutOfMemory,
        fork::ForkError::PageNotMapped => VmError::PageNotMapped,
        fork::ForkError::MemType(_) => VmError::MemType,
    }
}

fn brk_error_to_vm_error(e: brk::BrkError) -> VmError {
    match e {
        brk::BrkError::ProcessNotFound => VmError::InvalidProcess,
        brk::BrkError::OutOfMemory => VmError::OutOfMemory,
    }
}

fn munmap_error_to_vm_error(e: munmap::MunmapError) -> VmError {
    match e {
        munmap::MunmapError::ProcessNotFound => VmError::InvalidProcess,
        munmap::MunmapError::BadAddress => VmError::InvalidAddress,
        munmap::MunmapError::InvalidLength => VmError::InvalidAddress,
        munmap::MunmapError::NotMapped => VmError::InvalidAddress,
        munmap::MunmapError::InternalError => VmError::InternalError,
    }
}

fn mmap_error_to_vm_error(e: mmap::MmapError) -> VmError {
    match e {
        mmap::MmapError::InvalidLength => VmError::InvalidAddress,
        mmap::MmapError::BadAddress => VmError::InvalidAddress,
        mmap::MmapError::InvalidFlags => VmError::InvalidAddress,
        mmap::MmapError::PermissionDenied => VmError::PermissionDenied,
        mmap::MmapError::OutOfMemory => VmError::OutOfMemory,
        mmap::MmapError::FileMapDisabled => VmError::NotImplemented,
        mmap::MmapError::ProcessNotFound => VmError::InvalidEndpoint,
    }
}

fn map_phys_error_to_vm_error(e: map_phys::MapPhysError) -> VmError {
    match e {
        map_phys::MapPhysError::PermissionDenied => VmError::PermissionDenied,
        map_phys::MapPhysError::OutOfMemory => VmError::OutOfMemory,
        map_phys::MapPhysError::InvalidLength => VmError::InvalidAddress,
        map_phys::MapPhysError::ProcessNotFound => VmError::InvalidProcess,
    }
}

fn exit_error_to_vm_error(e: exit::VmExitError) -> VmError {
    match e {
        exit::VmExitError::ProcessNotFound => VmError::InvalidProcess,
        exit::VmExitError::NotExiting => VmError::InvalidProcess,
    }
}

fn rs_set_priv_error_to_vm_error(e: rs::RsSetPrivError) -> VmError {
    match e {
        rs::RsSetPrivError::ProcessNotFound => VmError::InvalidProcess,
        rs::RsSetPrivError::SysProcNoMask => VmError::InvalidAddress,
    }
}

fn rs_prepare_error_to_vm_error(e: rs::RsPrepareError) -> VmError {
    match e {
        rs::RsPrepareError::ProcessNotFound => VmError::InvalidProcess,
        rs::RsPrepareError::NotImplemented => VmError::NotImplemented,
    }
}

fn rs_update_error_to_vm_error(e: rs::RsUpdateError) -> VmError {
    match e {
        rs::RsUpdateError::ProcessNotFound => VmError::InvalidProcess,
        rs::RsUpdateError::PreallocMapConflict => VmError::NotImplemented,
        rs::RsUpdateError::NotImplemented => VmError::NotImplemented,
    }
}

fn rs_memctl_error_to_vm_error(e: rs::RsMemctlError) -> VmError {
    match e {
        rs::RsMemctlError::ProcessNotFound => VmError::InvalidProcess,
        rs::RsMemctlError::InvalidRequest => VmError::InvalidAddress,
        rs::RsMemctlError::MakeVmFailed => VmError::PermissionDenied,
        rs::RsMemctlError::HeapPreallocFailed => VmError::NotImplemented,
        rs::RsMemctlError::MapPreallocFailed => VmError::NotImplemented,
        rs::RsMemctlError::InvalidLength => VmError::InvalidAddress,
    }
}

fn query_error_to_vm_error(e: query::QueryError) -> VmError {
    match e {
        query::QueryError::ProcessNotFound => VmError::InvalidProcess,
        query::QueryError::NotMapped => VmError::InvalidAddress,
        query::QueryError::NotSupported => VmError::InvalidAddress,
        query::QueryError::InvalidQuery => VmError::InvalidAddress,
    }
}

fn query_rusage_error_to_vm_error(e: query::QueryError) -> VmError {
    match e {
        query::QueryError::ProcessNotFound => VmError::InvalidEndpoint,
        _ => query_error_to_vm_error(e),
    }
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
}
