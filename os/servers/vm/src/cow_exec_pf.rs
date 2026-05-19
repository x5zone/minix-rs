//! Copy-on-Write and page fault handling (方案三：PFN 索引模型).
//!
//! Uses PageFrames + PageSlot for CoW resolution and page fault dispatch.

use minix_types::VirBytes;
use crate::region::{VirRegion, PageFrames, PageSlot, PfnAllocator, PfnAllocError, PAGE_SIZE};
use crate::memtype::{MemType, PagefaultResult, MemTypeError, MEM_TYPE_ANON};
use crate::vmproc::ActiveProc;

pub(crate) fn handle_pagefault(
    proc: &ActiveProc<'_>,
    region: &mut VirRegion,
    frames: &mut PageFrames,
    alloc: &mut dyn PfnAllocator,
    fault_addr: VirBytes,
    write: bool,
) -> Result<PagefaultAction, CowError> {
    let offset = VirBytes(fault_addr.0 - region.vaddr.0);

    let memtype = region.def_memtype
        .ok_or(CowError::NoMemType)?;

    let result = memtype.ev_pagefault(proc, region, frames, offset, write)?;

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

    if let Some((pfn, mt)) = pending {
        mt.ev_unreference(frames, pfn);
        alloc.free_pfn(pfn);
    }

    Ok(new_pfn)
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

fn copy_page_content(frames: &PageFrames, src_pfn: u32, dst_pfn: u32) {
    let _ = (frames, src_pfn, dst_pfn);
    // TODO: Copy page content from src_pfn to dst_pfn via Direct Map.
    // Equivalent to Minix3's sys_abscopy(ph->ph->phys, new_page, VM_PAGE_SIZE).
    // Requires vm_phys_to_virt() to convert physical addresses to virtual
    // addresses, then core::ptr::copy_nonoverlapping to copy PAGE_SIZE bytes.
}

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
}
