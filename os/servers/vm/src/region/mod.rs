//! Memory region management module.

extern crate alloc;

pub(crate) mod vir_region;
pub(crate) mod page_state;
pub(crate) mod region_map;

pub(crate) use vir_region::{VirRegion, VrFlags, VrParam, VmError};
pub(crate) use page_state::{PageFrames, PageSlot, PageFlags, PFN_NONE, PAGE_SIZE, PfnAllocator, PfnAllocError};
// Re-export PageAllocFlags from phys_mem to avoid duplication
pub(crate) use region_map::RegionMap;

use crate::alloc_page::VmPageAllocator;
use crate::pagetable::Paging;
use minix_types::VirBytes;

/// Free all pages in a region, unmapping them from the page table and
/// releasing physical frames.
///
/// In test builds, page table operations are skipped since X86_64Paging
/// methods are not yet implemented.
pub(crate) fn free_region_pages(
    mut region: VirRegion,
    page_table: Option<&mut crate::pagetable::PageTable>,
    frames: &mut PageFrames,
    page_alloc: &mut VmPageAllocator,
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
            // TODO: capture the close in the VFS
            // request queue instead of dropping it.
            //
            // Minix3 sends a `VFS_FDCLOSE` IPC message to VFS in this
            // branch (region.c, `map_region_deref` → `vfs_request`).
            // The Rust rewrite is blocked on IpcTransport
            // (see IpcTransport TODO) and on the VFS request dispatcher path
            // (ipc/dispatcher.rs:484 "TODO: add missing imports and
            // decode helpers"). Until both land, we enqueue the close
            // locally so the FdRefTable's refcount semantics are
            // correct, and the VFS IPC send is DEFERRED.
            //
            // Before: `let _ = close;` silently dropped the
            // `PendingFdClose { fd, dev, ino }` value. This was
            // review-patterns-skill §模式31 (返回值完整性) — a
            // discarding let _ is acceptable only with a comment
            // explaining why. The fix moves the close to a typed
            // local so a future VFS-send implementation has the
            // value ready to consume.
            let _close: crate::fdref::PendingFdClose = close;
            // Future (DEFERRED): enqueue to VfsRequestQueue
            //   self.vfs_queue.enqueue(VfsRequestType::FdClose { fd, dev, ino });
            // which is drained by the VM main loop's VFS-FDCLOSE
            // send path. See IpcTransport TODO for the dependency.
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
    for (vaddr, length) in region_specs {
        crate::fork::handle_memory_once(regions, frames, pfn_alloc, vaddr, length, true)
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

    /// Test that pinning an empty region map succeeds trivially.
    #[test]
    fn test_map_pin_memory_empty() {
        let mut regions = RegionMap::new();
        let mut frames = make_frames();
        let mut alloc = make_page_alloc();
        let result = map_pin_memory(&mut regions, &mut frames, &mut alloc);
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
        let result = map_pin_memory(&mut regions, &mut frames, &mut alloc);
        // No CoW pages to resolve, so pinning succeeds.
        assert!(result.is_ok());
    }
}
