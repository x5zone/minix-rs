//! Copy-on-Write and page fault handling (PFN index model).
//!
//! Uses PageFrames + PageSlot for CoW resolution and page fault dispatch.

use minix_types::{Endpoint, PhysBytes, VirBytes};
use crate::region::{VirRegion, PageFrames, PageSlot, PfnAllocator, PfnAllocError, PAGE_SIZE};
use crate::memtype::{MemType, PagefaultResult, MemTypeError, MEM_TYPE_ANON};
use crate::phys_mem::AlignedPhysBytes;
use crate::direct_map::vm_phys_to_virt;

/// VM page fault handler entry point.
///
/// Dispatches to the region's `MemType::ev_pagefault`, then acts on the
/// returned `PagefaultResult`: allocate a new page, resolve CoW, or report
/// an access violation.
pub(crate) fn handle_pagefault(
    proc_endpoint: Endpoint,
    region: &mut VirRegion,
    frames: &mut PageFrames,
    alloc: &mut dyn PfnAllocator,
    fault_addr: VirBytes,
    write: bool,
) -> Result<PagefaultAction, CowError> {
    let offset = VirBytes(fault_addr.0 - region.vaddr.0);

    let memtype = region.def_memtype
        .ok_or(CowError::NoMemType)?;

    let result = memtype.ev_pagefault(proc_endpoint, region, frames, offset, write)?;

    match result {
        PagefaultResult::Handled => Ok(PagefaultAction::Handled),
        PagefaultResult::NeedNewPage => {
            alloc_and_map(region, frames, alloc, offset, memtype)?;
            Ok(PagefaultAction::MappedNewPage)
        }
        PagefaultResult::NeedCow => {
            cow_resolve(region, frames, alloc, offset)?;
            Ok(PagefaultAction::CowResolved)
        }
        PagefaultResult::NeedVfsIo => {
            Ok(PagefaultAction::Suspended)
        }
        PagefaultResult::AccessViolation => {
            Ok(PagefaultAction::AccessViolation)
        }
    }
}

pub(crate) fn alloc_and_map(
    region: &mut VirRegion,
    frames: &mut PageFrames,
    alloc: &mut dyn PfnAllocator,
    offset: VirBytes,
    memtype: &'static dyn MemType,
) -> Result<u32, CowError> {
    let pfn = alloc.alloc_pfn()
        .map_err(|_| CowError::NoMemory)?;

    // NOTE: If map_page could fail in the future, we would need to roll back:
    //   alloc.free_pfn(pfn);
    // Currently map_page is infallible (just sets slot + increments refcount).
    region.map_page(frames, offset, pfn, memtype);

    Ok(pfn)
}

pub(crate) fn cow_resolve(
    region: &mut VirRegion,
    frames: &mut PageFrames,
    alloc: &mut dyn PfnAllocator,
    offset: VirBytes,
) -> Result<u32, CowError> {
    cow_resolve_core(region, frames, alloc, offset).map_err(Into::into)
}

/// Core CoW resolution: allocate a new physical page, copy content from the
/// shared page, unmap the old slot and map the new one as `MEM_TYPE_ANON`.
///
/// If `refcount <= 1` the page is already private and no copy is needed.
pub(crate) fn cow_resolve_core(
    region: &mut VirRegion,
    frames: &mut PageFrames,
    alloc: &mut dyn PfnAllocator,
    offset: VirBytes,
) -> Result<u32, CowCoreError> {
    let slot = region.get_slot(offset)
        .ok_or(CowCoreError::PageNotMapped)?;

    if !slot.is_mapped() {
        return Err(CowCoreError::PageNotMapped);
    }

    let old_pfn = slot.pfn;
    let refcount = frames.get(old_pfn)
        .map(|s| s.refcount)
        .unwrap_or(0);

    if refcount <= 1 {
        return Ok(old_pfn);
    }

    let new_pfn = alloc.alloc_pfn()
        .map_err(|_| CowCoreError::NoMemory)?;

    copy_page_content(frames, old_pfn, new_pfn);

    let pending = region.unmap_page(frames, offset);
    region.map_page(frames, offset, new_pfn, &MEM_TYPE_ANON);

    // If the old page's refcount dropped to 0 and it's not cached, unmap_page
    // returns (pfn, memtype) so the caller can notify the memtype (ev_unreference)
    // and free the physical page. This matches Minix3's pb_unreferenced() path
    // where a shared page's last reference is released.
    if let Some((pfn, mt)) = pending {
        mt.ev_unreference(frames, pfn);
        alloc.free_pfn(pfn);
    }

    #[cfg(debug_assertions)]
    verify_cow_consistency(frames, old_pfn, new_pfn, region, offset);

    Ok(new_pfn)
}

/// Debug-only CoW consistency verification.
///
/// After CoW resolution, asserts that refcounts and slot mappings are correct:
/// - old_pfn: refcount should be decremented (was shared, now private to other owner)
/// - new_pfn: refcount should be 1 (newly allocated, owned by this region)
/// - region's slot at `offset` should point to new_pfn
#[cfg(debug_assertions)]
fn verify_cow_consistency(
    frames: &PageFrames,
    old_pfn: u32,
    new_pfn: u32,
    region: &VirRegion,
    offset: VirBytes,
) {
    if let Some(old_state) = frames.get(old_pfn) {
        assert!(
            old_state.refcount >= 1,
            "old_pfn {} refcount should be >= 1 after CoW, got {}",
            old_pfn, old_state.refcount
        );
    }

    if let Some(new_state) = frames.get(new_pfn) {
        assert_eq!(
            new_state.refcount, 1,
            "new_pfn {} refcount should be 1 after CoW, got {}",
            new_pfn, new_state.refcount
        );
    }

    if let Some(slot) = region.get_slot(offset) {
        assert!(
            slot.is_mapped(),
            "slot at offset {:?} should be mapped after CoW",
            offset
        );
        assert_eq!(
            slot.pfn, new_pfn,
            "slot at offset {:?} should point to new_pfn {}, got {}",
            offset, new_pfn, slot.pfn
        );
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CowCoreError {
    NoMemory,
    PageNotMapped,
}

impl From<CowCoreError> for CowError {
    fn from(e: CowCoreError) -> Self {
        match e {
            CowCoreError::NoMemory => CowError::NoMemory,
            CowCoreError::PageNotMapped => CowError::PageNotMapped,
        }
    }
}

#[cfg(not(test))]
fn copy_page_content(frames: &PageFrames, src_pfn: u32, dst_pfn: u32) {
    // SAFETY: pfn_to_phys returns a page-aligned physical address (multiple of PAGE_SIZE).
    // AlignedPhysBytes::new_unchecked requires its argument to be page-aligned, which is
    // guaranteed by the PageFrames invariant that all PFNs map to page-aligned addresses.
    let src_phys = AlignedPhysBytes::new_unchecked(frames.pfn_to_phys(src_pfn).0);
    let dst_phys = AlignedPhysBytes::new_unchecked(frames.pfn_to_phys(dst_pfn).0);
    let src_ptr = vm_phys_to_virt(src_phys).0 as *const u8;
    let dst_ptr = vm_phys_to_virt(dst_phys).0 as *mut u8;
    // SAFETY: src_ptr and dst_ptr are valid for reads/writes of PAGE_SIZE bytes.
    // They point to distinct physical pages (src_pfn != dst_pfn guaranteed by caller),
    // so the regions do not overlap. Both pages are mapped and accessible via the
    // direct-mapped region (vm_phys_to_virt).
    unsafe {
        core::ptr::copy_nonoverlapping(src_ptr, dst_ptr, PAGE_SIZE as usize);
    }
}

#[cfg(test)]
fn copy_page_content(_frames: &PageFrames, _src_pfn: u32, _dst_pfn: u32) {
}

/// Resolve CoW for all pages in a region that need it.
///
/// Iterates over every page slot; if `needs_cow` is true, performs
/// `cow_resolve` on that page. Returns the number of pages resolved.
pub(crate) fn cow_resolve_region(
    region: &mut VirRegion,
    frames: &mut PageFrames,
    alloc: &mut dyn PfnAllocator,
) -> Result<usize, CowError> {
    let num_pages = region.physblocks.len();
    let mut resolved = 0;

    for i in 0..num_pages {
        let offset = VirBytes((i as u64) * PAGE_SIZE);
        if region.needs_cow(frames, offset) {
            cow_resolve(region, frames, alloc, offset)?;
            resolved += 1;
        }
    }

    Ok(resolved)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PagefaultAction {
    Handled,
    MappedNewPage,
    CowResolved,
    Suspended,
    AccessViolation,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CowError {
    NoMemory,
    NoMemType,
    PageNotMapped,
    MemType(MemTypeError),
}

impl From<MemTypeError> for CowError {
    fn from(e: MemTypeError) -> Self {
        Self::MemType(e)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use minix_types::PhysBytes;
    use crate::region::VrFlags;

    struct TestAlloc { next: u32 }
    impl PfnAllocator for TestAlloc {
        fn alloc_pfn(&mut self) -> Result<u32, PfnAllocError> {
            let pfn = self.next;
            self.next += 1;
            Ok(pfn)
        }
        fn free_pfn(&mut self, _pfn: u32) {}
    }

    fn make_frames(pages: u32) -> PageFrames {
        PageFrames::new(PhysBytes(pages as u64 * PAGE_SIZE))
    }

    #[test]
    fn test_alloc_and_map() {
        let mut frames = make_frames(8);
        let mut alloc = TestAlloc { next: 0 };
        let mut region = VirRegion::new(VirBytes(0x1000), VirBytes(0x4000), VrFlags::empty());
        region.def_memtype = Some(&MEM_TYPE_ANON);

        let pfn = alloc_and_map(&mut region, &mut frames, &mut alloc, VirBytes(0x0000), &MEM_TYPE_ANON).unwrap();

        let slot = region.get_slot(VirBytes(0x0000)).unwrap();
        assert!(slot.is_mapped());
        assert_eq!(slot.pfn, pfn);
    }

    #[test]
    fn test_cow_resolve() {
        let mut frames = make_frames(8);
        let mut alloc = TestAlloc { next: 0 };
        let mut region = VirRegion::new(VirBytes(0x1000), VirBytes(0x4000), VrFlags::empty());
        region.def_memtype = Some(&MEM_TYPE_ANON);

        let pfn = alloc.alloc_pfn().unwrap();
        region.map_page(&mut frames, VirBytes(0x0000), pfn, &MEM_TYPE_ANON);

        frames.get_mut(pfn).unwrap().refcount = 2;

        let new_pfn = cow_resolve(&mut region, &mut frames, &mut alloc, VirBytes(0x0000)).unwrap();

        assert_ne!(new_pfn, pfn);
        assert_eq!(frames.get(pfn).unwrap().refcount, 1);
        assert_eq!(frames.get(new_pfn).unwrap().refcount, 1);
    }

    #[test]
    fn test_cow_resolve_no_sharing() {
        let mut frames = make_frames(8);
        let mut alloc = TestAlloc { next: 0 };
        let mut region = VirRegion::new(VirBytes(0x1000), VirBytes(0x4000), VrFlags::empty());
        region.def_memtype = Some(&MEM_TYPE_ANON);

        let pfn = alloc.alloc_pfn().unwrap();
        region.map_page(&mut frames, VirBytes(0x0000), pfn, &MEM_TYPE_ANON);

        let result_pfn = cow_resolve(&mut region, &mut frames, &mut alloc, VirBytes(0x0000)).unwrap();
        assert_eq!(result_pfn, pfn);
    }

    #[test]
    fn test_cow_resolve_region() {
        let mut frames = make_frames(8);
        let mut alloc = TestAlloc { next: 0 };
        let mut region = VirRegion::new(VirBytes(0x1000), VirBytes(0x4000), VrFlags::empty());
        region.def_memtype = Some(&MEM_TYPE_ANON);

        let pfn0 = alloc.alloc_pfn().unwrap();
        let pfn1 = alloc.alloc_pfn().unwrap();
        region.map_page(&mut frames, VirBytes(0x0000), pfn0, &MEM_TYPE_ANON);
        region.map_page(&mut frames, VirBytes(0x1000), pfn1, &MEM_TYPE_ANON);

        frames.get_mut(pfn0).unwrap().refcount = 2;
        frames.get_mut(pfn1).unwrap().refcount = 3;

        let resolved = cow_resolve_region(&mut region, &mut frames, &mut alloc).unwrap();
        assert_eq!(resolved, 2);

        assert_eq!(frames.get(pfn0).unwrap().refcount, 1);
        assert_eq!(frames.get(pfn1).unwrap().refcount, 2);
    }

    #[test]
    fn test_cow_resolve_core_refcount_one_fast_path() {
        let mut frames = make_frames(4);
        let mut alloc = TestAlloc { next: 0 };
        let mut region = VirRegion::new(VirBytes(0x1000), VirBytes(0x4000), VrFlags::empty());
        region.def_memtype = Some(&MEM_TYPE_ANON);

        let pfn = alloc.alloc_pfn().unwrap();
        region.map_page(&mut frames, VirBytes(0x0000), pfn, &MEM_TYPE_ANON);
        assert_eq!(frames.get(pfn).unwrap().refcount, 1);

        let result = cow_resolve_core(&mut region, &mut frames, &mut alloc, VirBytes(0x0000)).unwrap();
        assert_eq!(result, pfn);
        assert_eq!(frames.get(pfn).unwrap().refcount, 1);
        let slot = region.get_slot(VirBytes(0x0000)).unwrap();
        assert_eq!(slot.pfn, pfn);
    }
}
