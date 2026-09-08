//! Memory region management module.

extern crate alloc;

pub(crate) mod vir_region;
pub(crate) mod page_state;
pub(crate) mod region_map;

pub(crate) use vir_region::{VirRegion, VrFlags, VrParam};
pub(crate) use page_state::{PageFrames, PageSlot, PageFlags, PAGE_SIZE, PfnAllocator, PfnAllocError};
// Re-export PageAllocFlags from phys_mem to avoid duplication
pub(crate) use region_map::RegionMap;

use crate::alloc_page::VmPageAllocator;
use crate::memtype::MemType;
use crate::pagetable::Paging;
use minix_types::{Endpoint, UserSlot, VirBytes};

/// Free all pages in a region, unmapping them from the page table and
/// releasing physical frames.
///
/// In test builds, callers pass `page_table = None` (the zeroed test stub
/// table has no real storage behind it), so only the physical-frame release
/// runs; unmap is exercised once a test-injectable Paging implementation
/// lands (02-stage-vm todo V11-P2-1).
pub(crate) fn free_region_pages(
    mut region: VirRegion,
    page_table: Option<&mut crate::pagetable::PageTable>,
    frames: &mut PageFrames,
    page_alloc: &mut VmPageAllocator,
    vfs_queue: &mut crate::vfs_queue::VfsRequestQueue,
    owner: Endpoint,
) {
    let page_count = (region.length.0 / PAGE_SIZE) as usize;
    if let Some(pt) = page_table {
        for i in 0..page_count {
            let vaddr = VirBytes(region.vaddr.0 + (i as u64) * PAGE_SIZE);
            let _ = pt.unmap(vaddr);
        }
    }

    let fdref_id = if let VrParam::File { fdref_id: Some(id), .. } = region.param {
        Some(id)
    } else {
        None
    };

    if let Some(mt) = region.def_memtype {
        mt.ev_delete(&mut region);
    }

    let pending = region.free_range(frames, VirBytes(0), region.length);
    for (pfn, mt) in pending {
        mt.ev_unreference(frames, pfn);
        page_alloc.free_pfn(pfn);
    }

    if let Some(id) = fdref_id {
        let pending_close = crate::fdref::FdRefTable::get_global().deref_entry(id);
        if let Some(close) = pending_close {
            // Minix3 sends an async VFS request to close the borrowed fd in
            // this branch (C fdref.c:150 — `vfs_request(VMVFSREQ_FDCLOSE, …)`
            // with no callback: a failed close is VFS's diagnostic, not
            // VM's). V11/T10: the close is enqueued into the VfsRequestQueue;
            // the send half (VFS_VMCALL wire) is edge E-VFSWIRE — the vfs
            // server's message-level decode is still being built by the
            // 09/13-stage workflows.
            let vreq = crate::vfs_queue::VfsRequest {
                request_type: crate::vfs_queue::VfsRequestType::FdClose,
                req_id: 0, // assigned by the queue
                caller_endpoint: owner,
                fd: close.fd,
                offset: 0,
                length: 0,
                callback: None,
                state: None,
            };
            if vfs_queue.request(vreq).is_err() {
                // Queue full → the close is lost and the fd leaks in VFS.
                // Fail-closed drop + audit (C panics on SLABALLOC failure;
                // this codebase never panics at the IPC boundary, V9-P0-1).
                audit_log!(
                    "[VM VFS] fdclose queue full — fd {} close dropped (fd leak)",
                    close.fd
                );
            }
        }
    }
}

/// Why a shared-remap release can be refused — one variant per C `getsrc`
/// failure branch (mem_shared.c:62-98), so the audit log can say exactly
/// which invariant broke instead of a bare errno.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RegionRemapsError {
    /// Source endpoint no longer resolves to an in-use process.
    SourceProcessGone,
    /// Source process exists but its region table was never initialized.
    RegionsNotInitialized,
    /// No region lives at the recorded source vaddr.
    SourceRegionMissing,
    /// The region at the vaddr is no longer anon typed (re-typed, or the
    /// original source was freed and a non-anon mapping took the address).
    SourceNotAnon,
    /// Region id mismatch — the original source was freed and its vaddr
    /// recycled by a different region (why region ids are unique,
    /// C `region.c:445`).
    IdMismatch,
    /// More shared remaps were deleted than were made (C
    /// `assert(src_region->remaps > 0)`, demoted to a checked error —
    /// no panics at the IPC boundary, V9-P0-1).
    Underflow,
}

/// Validate and decrement the source region's `remaps` inside one process's
/// region map — the map-level core behind both release paths. The checks
/// mirror C `getsrc`'s tail (mem_shared.c:84-98): the region at `addr` must
/// exist, still be anon typed, and still carry `expected_id`; the `remaps`
/// underflow check replaces C's `assert`.
pub(crate) fn decrement_remaps_in(
    regions: &mut RegionMap,
    addr: VirBytes,
    expected_id: i32,
) -> Result<(), RegionRemapsError> {
    let Some(region) = regions.get_mut(&addr) else {
        return Err(RegionRemapsError::SourceRegionMissing);
    };
    // Shared remaps are only ever taken of anon regions, so a re-typed
    // survivor at the same vaddr is a mismatch, not a decrement target.
    if region.def_memtype.map(|m| m.name()) != Some(crate::memtype::MEM_TYPE_ANON.name()) {
        return Err(RegionRemapsError::SourceNotAnon);
    }
    if region.id != expected_id {
        return Err(RegionRemapsError::IdMismatch);
    }
    if region.remaps <= 0 {
        return Err(RegionRemapsError::Underflow);
    }
    region.remaps -= 1;
    Ok(())
}

/// Release one deleted shared-remap region's claim on its source: re-validate
/// the source identity (C `getsrc`) and decrement the source's `remaps`
/// (C `shared_delete`, mem_shared.c:110-123).
///
/// Why this lives beside the deletion funnels instead of in
/// `MemType::ev_delete`: the funnels hold an `ActiveProc`/`ExitingProc` — an
/// exclusive `&mut VmProc` — for the slot being torn down, so a memtype hook
/// has no legal mutable path into the table cell. The source may even be the
/// very slot being torn down (a process can remap its own region); that case
/// goes through `own_regions`, the doomed process's own map, while only
/// genuinely cross-process sources reach the table — always from a slot
/// other than the held handle's, which is exactly what
/// `VmProcTable::decrement_region_remaps`' safety contract demands.
///
/// `remaps` feeds two observable behaviors — the source stays writable while
/// a remap holds (`anon_writable`, mem_anon.c:105-113) and GET_REF reports
/// `1 + remaps` (mem_shared.c:207-209) — which is why a missed decrement is
/// semantic drift and not just stale bookkeeping (G-V12-7: before this
/// existed, every shm unmap inflated the source's refcount report forever).
pub(crate) fn release_shared_remap(
    param: &VrParam,
    own_slot: UserSlot,
    own_regions: &mut RegionMap,
    table: &crate::vmproc::VmProcTable,
) {
    let VrParam::Shared { ep, vaddr, id } = *param else {
        return;
    };
    if ep == 0 || vaddr.0 == 0 {
        // C getsrc: "shared region has not defined source region."
        audit_log!("[VM shared] release: source not defined (ep={ep})");
        return;
    }
    let Ok(src_slot) = table.vm_isokendpt(Endpoint(ep)) else {
        // C getsrc: "shared memory with missing source process."
        audit_log!("[VM shared] release: source process gone (ep={ep})");
        return;
    };
    let outcome = if src_slot == own_slot {
        decrement_remaps_in(own_regions, vaddr, id)
    } else {
        table.decrement_region_remaps(src_slot, vaddr, id)
    };
    if let Err(e) = outcome {
        audit_log!("[VM shared] release: source remaps left untouched: {e:?}");
        // The audit macro compiles out without the `vm_acl_audit` feature;
        // touch the binding so the no-audit build stays warning-free.
        let _ = &e;
    }
}

/// Pin all memory regions of a process, ensuring every page is physically
/// mapped and writable.
///
/// Corresponds to Minix3's `map_pin_memory()` (region.c:779).
///
/// This is used during RS (Reincarnation Server) live update: before
/// swapping process slots, all of the process's memory must be resident
/// so that no page faults can occur during the update window.
///
/// # Implementation
///
/// Collects (vaddr, length) pairs from all regions first, then processes
/// each region via `handle_memory_once` with `wrflag=true`. The two-phase
/// approach avoids holding a mutable reference to `RegionMap` while
/// iterating and calling `handle_memory_once` (which also needs `&mut RegionMap`).
///
/// # Errors
///
/// Returns `PinMemoryError::PageNotMapped` if any region's pages cannot
/// be resolved (e.g., CoW allocation failure). In C, this panics
/// (`panic("map_pin_memory: map_handle_memory failed")`); the Rust
/// version uses `Result` for recoverable error propagation.
///
/// # C source (region.c:779-795)
///
/// ```c
/// int map_pin_memory(struct vmproc *vmp)
/// {
///     struct vir_region *vr;
///     int r;
///     region_iter iter;
///     region_start_iter_least(&vmp->vm_regions_avl, &iter);
///     pt_assert(&vmp->vm_pt);
///     while((vr = region_get_iter(&iter))) {
///         r = map_handle_memory(vmp, vr, 0, vr->length, 1, NULL, 0, 0);
///         if(r != OK) {
///             panic("map_pin_memory: map_handle_memory failed: %d", r);
///         }
///         region_incr_iter(&iter);
///     }
///     pt_assert(&vmp->vm_pt);
///     return OK;
/// }
/// ```
pub(crate) fn map_pin_memory(
    regions: &mut RegionMap,
    frames: &mut PageFrames,
    pfn_alloc: &mut dyn PfnAllocator,
    pt: &mut crate::pagetable::PageTable,
) -> Result<(), PinMemoryError> {
    // Phase 1: Collect region (vaddr, length) pairs.
    // We must collect first because handle_memory_once needs &mut RegionMap,
    // and we can't hold an iterator reference while also mutating.
    let region_specs: alloc::vec::Vec<(VirBytes, VirBytes)> = regions
        .iter()
        .map(|r| (r.vaddr, r.length))
        .collect();

    // Phase 2: Process each region with wrflag=true.
    // C: map_handle_memory(vmp, vr, 0, vr->length, 1 /* wrflag */, NULL, 0, 0)
    // G-V12-8: CoW resolution inside also writes the PTEs via `pt`.
    for (vaddr, length) in region_specs {
        crate::fork::handle_memory_once(regions, frames, pfn_alloc, vaddr, length, true, pt)
            .map_err(|_| PinMemoryError::PageNotMapped)?;
    }

    Ok(())
}

/// Error type for `map_pin_memory`.
///
/// C source panics on failure; Rust returns `Result` for error propagation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PinMemoryError {
    /// A page could not be mapped or CoW resolution failed.
    /// C: `panic("map_pin_memory: map_handle_memory failed: %d", r)`
    PageNotMapped,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::phys_mem::{PhysAlloc, BitmapAllocator};

    fn make_frames() -> PageFrames {
        PageFrames::new(minix_types::PhysBytes(256 * PAGE_SIZE as u64))
    }

    fn make_page_alloc() -> VmPageAllocator {
        VmPageAllocator::new(PhysAlloc::Bitmap(BitmapAllocator::new_for_test(256)))
    }

    /// V11/T10: freeing a file-backed region whose fdref reaches zero must
    /// enqueue an FdClose request into the VfsRequestQueue (the send half
    /// is edge E-VFSWIRE — the vfs server's VFS_VMCALL message decode).
    #[test]
    fn test_free_region_pages_enqueues_fdclose() {
        let mut frames = make_frames();
        let mut page_alloc = make_page_alloc();
        let table = crate::fdref::FdRefTable::get_global();
        let id = table.create(9, 0xAA, 0xBB);
        table.ref_entry(id);

        let mut region = VirRegion::with_memtype(
            VirBytes(0x1000),
            VirBytes(0x1000),
            VrFlags::WRITABLE,
            &crate::memtype::MEM_TYPE_MAPPED_FILE,
        );
        region.param = VrParam::File {
            inited: true,
            fdref_id: Some(id),
            offset: 0,
            clearend: 0,
        };

        let mut queue = crate::vfs_queue::VfsRequestQueue::new();
        free_region_pages(
            region,
            None,
            &mut frames,
            &mut page_alloc,
            &mut queue,
            Endpoint(42),
        );

        // fdref fully deref'd …
        assert!(table.get(id).is_none());
        // … and the FdClose request is queued for the main loop's send.
        let (ty, fd, ep) = queue.active_fd_close().expect("fdclose must be enqueued");
        assert_eq!(ty, crate::vfs_queue::VfsRequestType::FdClose);
        assert_eq!(fd, 9);
        assert_eq!(ep, Endpoint(42));
    }

    /// V11/T21 (V11-P2-1): with SimPaging as the test `PageTable`,
    /// `free_region_pages` can pass `Some(pt)` and exercise REAL unmap
    /// dispatch — the mapped page leaves the simulated page table and its
    /// frame refcount drops. This is the test-side proof that the
    /// cfg(test) `PageTable` alias swap works end to end.
    #[test]
    fn test_free_region_pages_sim_paging_unmaps() {
        use crate::pagetable::{PageTable, Paging};

        let mut frames = make_frames();
        let mut page_alloc = make_page_alloc();
        let pfn = page_alloc.alloc_pfn().expect("alloc in test");
        let mut region = VirRegion::with_memtype(
            VirBytes(0x1000),
            VirBytes(0x1000),
            VrFlags::WRITABLE | VrFlags::ANON,
            &crate::memtype::MEM_TYPE_ANON,
        );
        region.map_page(&mut frames, VirBytes(0), pfn, &crate::memtype::MEM_TYPE_ANON);

        let mut pt = <PageTable as Paging>::new().expect("sim paging new");
        let vaddr = VirBytes(0x1000);
        pt.map(vaddr, minix_types::PhysBytes(pfn as u64 * 0x1000),
               crate::pagetable::PageFlags::WRITABLE)
            .expect("sim map");
        assert!(pt.query(vaddr).is_some(), "mapping must be live pre-free");

        let mut queue = crate::vfs_queue::VfsRequestQueue::new();
        free_region_pages(
            region,
            Some(&mut pt),
            &mut frames,
            &mut page_alloc,
            &mut queue,
            Endpoint(42),
        );

        // Real unmap happened through the trait: the virtual page is gone…
        assert!(pt.query(vaddr).is_none(), "unmap must clear the sim entry");
        // …and the physical frame was released (refcount 1 → 0).
        assert_eq!(frames.get(pfn).unwrap().refcount, 0);
    }

    /// Test that pinning an empty region map succeeds trivially.
    #[test]
    fn test_map_pin_memory_empty() {
        let mut regions = RegionMap::new();
        let mut frames = make_frames();
        let mut alloc = make_page_alloc();
        let mut pt = <crate::pagetable::PageTable as crate::pagetable::Paging>::new().unwrap();
        let result = map_pin_memory(&mut regions, &mut frames, &mut alloc, &mut pt);
        assert!(result.is_ok());
    }

    /// Test that pinning a region with no CoW pages succeeds.
    /// `map_pin_memory` resolves CoW pages; if there are none to resolve,
    /// it simply returns Ok — the region is already "pinned".
    #[test]
    fn test_map_pin_memory_non_cow_region() {
        let mut regions = RegionMap::new();
        let region = VirRegion::new(VirBytes(0x1000_0000), VirBytes(PAGE_SIZE), VrFlags::WRITABLE);
        regions.insert(region).unwrap();

        let mut frames = make_frames();
        let mut alloc = make_page_alloc();
        let mut pt = <crate::pagetable::PageTable as crate::pagetable::Paging>::new().unwrap();
        let result = map_pin_memory(&mut regions, &mut frames, &mut alloc, &mut pt);
        // No CoW pages to resolve, so pinning succeeds.
        assert!(result.is_ok());
    }

    // --- G-V12-7: shared-remap release (`release_shared_remap`) — C
    // `shared_delete`'s source `remaps--` with its `getsrc` validation. ---

    /// Spin up an in-use slot with an initialized region map, mirroring the
    /// dispatcher tests' harness (`init_test_slots`).
    fn init_process(slot: UserSlot) -> Endpoint {
        let table = crate::vmproc::VmProcTable::get_global();
        unsafe { table.reset_slot(slot); }
        let empty = table.get_empty(slot).unwrap();
        let ep = Endpoint::from_generation_slot(1, slot.get() as i32);
        let mut active = empty.activate(ep);
        active.init_regions();
        active.endpoint()
    }

    fn install_source_region(slot: UserSlot, remaps: i32, def_memtype: &'static dyn MemType)
        -> (VirBytes, i32)
    {
        let table = crate::vmproc::VmProcTable::get_global();
        let mut proc = table.get_active(slot).expect("source process active");
        let mut src = VirRegion::new(VirBytes(0x2000_0000), VirBytes(PAGE_SIZE as u64), VrFlags::WRITABLE);
        src.def_memtype = Some(def_memtype);
        src.remaps = remaps;
        let vaddr = src.vaddr;
        let id = src.id;
        proc.regions_mut().insert(src).unwrap();
        (vaddr, id)
    }

    fn source_remaps(slot: UserSlot, vaddr: VirBytes) -> Option<i32> {
        let table = crate::vmproc::VmProcTable::get_global();
        let mut proc = table.get_active(slot)?;
        proc.regions_mut().get_mut(&vaddr).map(|r| r.remaps)
    }

    /// Cross-process release: the deleter's own map is NOT the source's, so
    /// the decrement must land in the source process's table entry.
    #[test]
    fn test_release_shared_remap_cross_process() {
        let src_slot = UserSlot::new(80);
        let dst_slot = UserSlot::new(81);
        let src_ep = init_process(src_slot);
        init_process(dst_slot);
        let (vaddr, id) = install_source_region(src_slot, 2, &crate::memtype::MEM_TYPE_ANON);

        let mut deleter_map = RegionMap::new();
        release_shared_remap(
            &VrParam::Shared { ep: src_ep.0, vaddr, id },
            dst_slot,
            &mut deleter_map,
            crate::vmproc::VmProcTable::get_global(),
        );

        assert_eq!(source_remaps(src_slot, vaddr), Some(1),
            "cross-process release must decrement the source's remaps");
    }

    /// Self-remap release: the source lives in the deleting process's own
    /// map — the table cell is off-limits (an ActiveProc for that slot would
    /// alias it), so the decrement goes through `own_regions`.
    #[test]
    fn test_release_shared_remap_self_uses_own_map() {
        let slot = UserSlot::new(82);
        let ep = init_process(slot);
        let (vaddr, id) = install_source_region(slot, 1, &crate::memtype::MEM_TYPE_ANON);

        let table = crate::vmproc::VmProcTable::get_global();
        let mut proc = table.get_active(slot).expect("deleter active");
        release_shared_remap(
            &VrParam::Shared { ep: ep.0, vaddr, id },
            slot,
            proc.regions_mut(),
            table,
        );

        assert_eq!(source_remaps(slot, vaddr), Some(0),
            "self-remap release must decrement through the process's own map");
    }

    /// Each `getsrc` mismatch refuses the decrement and leaves the source
    /// untouched (C `shared_delete` returns early the same way) — wrong id,
    /// missing vaddr, re-typed source, exhausted remaps, undefined source.
    #[test]
    fn test_release_shared_remap_rejects_mismatches() {
        let src_slot = UserSlot::new(83);
        let dst_slot = UserSlot::new(84);
        let src_ep = init_process(src_slot);
        init_process(dst_slot);
        let (vaddr, id) = install_source_region(src_slot, 2, &crate::memtype::MEM_TYPE_ANON);
        let mut deleter_map = RegionMap::new();
        let table = crate::vmproc::VmProcTable::get_global();

        // (a) id mismatch — the source was freed and the vaddr recycled.
        release_shared_remap(
            &VrParam::Shared { ep: src_ep.0, vaddr, id: id + 1 },
            dst_slot, &mut deleter_map, table,
        );
        assert_eq!(source_remaps(src_slot, vaddr), Some(2), "id mismatch must refuse");

        // (b) no region at the recorded vaddr.
        release_shared_remap(
            &VrParam::Shared { ep: src_ep.0, vaddr: VirBytes(0x3000_0000), id },
            dst_slot, &mut deleter_map, table,
        );
        assert_eq!(source_remaps(src_slot, vaddr), Some(2), "missing vaddr must refuse");

        // (c) survivor at the vaddr is no longer anon typed.
        let table2 = crate::vmproc::VmProcTable::get_global();
        {
            let mut proc = table2.get_active(src_slot).unwrap();
            proc.regions_mut().get_mut(&vaddr).unwrap().def_memtype =
                Some(&crate::memtype::MEM_TYPE_DIRECT);
        }
        release_shared_remap(
            &VrParam::Shared { ep: src_ep.0, vaddr, id },
            dst_slot, &mut deleter_map, table,
        );
        assert_eq!(source_remaps(src_slot, vaddr), Some(2), "non-anon source must refuse");

        // (d) remaps already zero — underflow refused.
        {
            let mut proc = table2.get_active(src_slot).unwrap();
            let r = proc.regions_mut().get_mut(&vaddr).unwrap();
            r.def_memtype = Some(&crate::memtype::MEM_TYPE_ANON);
            r.remaps = 0;
        }
        release_shared_remap(
            &VrParam::Shared { ep: src_ep.0, vaddr, id },
            dst_slot, &mut deleter_map, table,
        );
        assert_eq!(source_remaps(src_slot, vaddr), Some(0), "underflow must refuse (no wrap)");

        // (e) undefined source triple — no endpoint lookup at all.
        release_shared_remap(
            &VrParam::Shared { ep: 0, vaddr: VirBytes(0), id },
            dst_slot, &mut deleter_map, table,
        );
        assert_eq!(source_remaps(src_slot, vaddr), Some(0), "undefined source must no-op");
    }
}
