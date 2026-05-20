//! Fork syscall implementation (方案三：PFN 索引模型).
//!
//! Uses PageSlot Copy semantics and PageFrames refcount for CoW.

use minix_types::VirBytes;
use crate::region::{VirRegion, VrFlags, PageFrames, PfnAllocator, PfnAllocError, PAGE_SIZE};
use crate::memtype::{MemType, MemTypeError, MEM_TYPE_ANON};
use crate::cow_exec_pf::cow_resolve_core;
use alloc::boxed::Box;
use alloc::vec::Vec;

pub(crate) fn fork_region(
    src: &VirRegion,
    frames: &mut PageFrames,
) -> Result<Box<VirRegion>, ForkError> {
    let mut dst = VirRegion::new(src.vaddr, src.length, src.flags);
    dst.parent_slot = src.parent_slot;
    dst.def_memtype = src.def_memtype;
    dst.remaps = src.remaps;
    dst.id = src.id;
    dst.param = src.param.clone();

    if let Some(mt) = src.def_memtype {
        mt.ev_copy(src, &mut dst)?;
    }

    for (i, slot_opt) in src.physblocks.iter().enumerate() {
        if let Some(slot) = slot_opt {
            if slot.is_mapped() {
                if let Some(state) = frames.get_mut(slot.pfn) {
                    state.refcount = state.refcount.saturating_add(1);
                }
                if let Some(mt) = slot.memtype {
                    mt.ev_reference(frames, *slot)?;
                }
            }
            dst.physblocks[i] = Some(*slot);
        }
    }

    dst.set_writable(false);

    Ok(Box::new(dst))
}

pub(crate) fn fork_regions(
    src_regions: &[Box<VirRegion>],
    frames: &mut PageFrames,
) -> Result<Vec<Box<VirRegion>>, ForkError> {
    let mut dst_regions = Vec::with_capacity(src_regions.len());
    for src in src_regions {
        let dst = fork_region(src, frames)?;
        dst_regions.push(dst);
    }
    Ok(dst_regions)
}

/// Resolve CoW for a single page within a region (fork helper).
///
/// Thin wrapper around `cow_resolve_core` that maps `CowCoreError` to
/// `ForkError`.
pub(crate) fn cow_copy_page(
    region: &mut VirRegion,
    frames: &mut PageFrames,
    alloc: &mut dyn PfnAllocator,
    offset: VirBytes,
) -> Result<(), ForkError> {
    cow_resolve_core(region, frames, alloc, offset)
        .map(|_| ())
        .map_err(|e| match e {
            crate::cow_exec_pf::CowCoreError::NoMemory => ForkError::NoMemory,
            crate::cow_exec_pf::CowCoreError::PageNotMapped => ForkError::PageNotMapped,
        })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ForkError {
    NoMemory,
    PageNotMapped,
    MemType(MemTypeError),
}

impl From<MemTypeError> for ForkError {
    fn from(e: MemTypeError) -> Self {
        Self::MemType(e)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use minix_types::PhysBytes;

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
    fn test_fork_region_basic() {
        let mut frames = make_frames(8);
        let mut alloc = TestAlloc { next: 0 };
        let mut src = VirRegion::new(VirBytes(0x1000), VirBytes(0x4000), VrFlags::empty());
        src.set_writable(true);
        src.def_memtype = Some(&MEM_TYPE_ANON);

        let pfn0 = alloc.alloc_pfn().unwrap();
        let pfn1 = alloc.alloc_pfn().unwrap();
        src.map_page(&mut frames, VirBytes(0x0000), pfn0, &MEM_TYPE_ANON);
        src.map_page(&mut frames, VirBytes(0x1000), pfn1, &MEM_TYPE_ANON);

        let dst = fork_region(&src, &mut frames).unwrap();

        assert_eq!(dst.vaddr, src.vaddr);
        assert_eq!(dst.length, src.length);
        assert!(!dst.is_writable());

        assert_eq!(frames.get(pfn0).unwrap().refcount, 2);
        assert_eq!(frames.get(pfn1).unwrap().refcount, 2);
    }

    #[test]
    fn test_cow_copy_page() {
        let mut frames = make_frames(8);
        let mut alloc = TestAlloc { next: 0 };
        let mut region = VirRegion::new(VirBytes(0x1000), VirBytes(0x4000), VrFlags::empty());
        region.def_memtype = Some(&MEM_TYPE_ANON);

        let pfn = alloc.alloc_pfn().unwrap();
        region.map_page(&mut frames, VirBytes(0x0000), pfn, &MEM_TYPE_ANON);

        frames.get_mut(pfn).unwrap().refcount = 2;

        cow_copy_page(&mut region, &mut frames, &mut alloc, VirBytes(0x0000)).unwrap();

        assert_eq!(frames.get(pfn).unwrap().refcount, 1);

        let new_slot = region.get_slot(VirBytes(0x0000)).unwrap();
        assert_ne!(new_slot.pfn, pfn);
        assert_eq!(frames.get(new_slot.pfn).unwrap().refcount, 1);
    }

    #[test]
    fn test_cow_copy_page_no_sharing() {
        let mut frames = make_frames(8);
        let mut alloc = TestAlloc { next: 0 };
        let mut region = VirRegion::new(VirBytes(0x1000), VirBytes(0x4000), VrFlags::empty());
        region.def_memtype = Some(&MEM_TYPE_ANON);

        let pfn = alloc.alloc_pfn().unwrap();
        region.map_page(&mut frames, VirBytes(0x0000), pfn, &MEM_TYPE_ANON);

        cow_copy_page(&mut region, &mut frames, &mut alloc, VirBytes(0x0000)).unwrap();

        let slot = region.get_slot(VirBytes(0x0000)).unwrap();
        assert_eq!(slot.pfn, pfn);
    }
}
