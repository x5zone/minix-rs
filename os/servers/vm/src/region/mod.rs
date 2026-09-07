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
use crate::pagetable::Paging;
use minix_types::{Endpoint, VirBytes};

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
}
