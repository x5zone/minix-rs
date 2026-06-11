//! Fork syscall implementation (PFN index model).
//!
//! Uses PageSlot Copy semantics and PageFrames refcount for CoW.
//!
//! Rollback model mirrors Minix3:
//! - `fork_region`: single-region rollback on `ev_reference` failure (decrement refcounts)
//! - `fork_regions`: multi-region rollback on `fork_region` failure (free all previously
//!   copied regions, equivalent to Minix3's `map_free_proc`)
//! - `do_fork`: top-level orchestration, frees page table on `fork_regions` failure
//!   (equivalent to Minix3's `pt_free(&vmc->vm_pt)`)

use minix_types::{VirBytes, Endpoint, UserSlot};
use crate::region::{VirRegion, VrFlags, PageFrames, PfnAllocator, PfnAllocError, PAGE_SIZE};
use crate::memtype::{MemType, MemTypeError, MEM_TYPE_ANON};
use crate::cow_exec_pf::cow_resolve_core;
use crate::vmproc::VmProcTable;
use alloc::boxed::Box;
use alloc::vec::Vec;

/// Fork a single VirRegion: share physical pages (refcount++) and set child
/// region read-only. Corresponds to Minix3 `map_copy_region()` (region.c).
///
/// On `ev_reference` failure, rolls back all previously incremented refcounts
/// and returns `VmForkError`.
pub(crate) fn fork_region(
    src: &VirRegion,
    frames: &mut PageFrames,
) -> Result<Box<VirRegion>, VmForkError> {
    let mut dst = VirRegion::new(src.vaddr, src.length, src.flags);
    dst.parent_slot = src.parent_slot;
    dst.def_memtype = src.def_memtype;
    dst.remaps = src.remaps;
    dst.id = src.id;
    dst.param = src.param.clone();

    if let Some(mt) = src.def_memtype {
        mt.ev_copy(src, &mut dst)?;
    }

    if let crate::region::VrParam::File { fdref_id: Some(id), .. } = dst.param {
        crate::fdref::FdRefTable::get_global().ref_entry(id);
    }

    // Track refcount increments for rollback on error.
    let mut refcounted_pfns: Vec<u32> = Vec::new();

    for (i, slot_opt) in src.physblocks.iter().enumerate() {
        if let Some(slot) = slot_opt {
            if slot.is_mapped() {
                if let Some(state) = frames.get_mut(slot.pfn) {
                    state.refcount = state.refcount.saturating_add(1);
                    refcounted_pfns.push(slot.pfn);
                }
                if let Some(mt) = slot.memtype {
                    if let Err(e) = mt.ev_reference(frames, *slot) {
                        // Rollback: decrement refcount for all pages that were incremented.
                        for pfn in &refcounted_pfns {
                            if let Some(state) = frames.get_mut(*pfn) {
                                if state.refcount > 0 {
                                    state.refcount -= 1;
                                }
                            }
                        }
                        return Err(VmForkError::from(e));
                    }
                }
            }
            dst.physblocks[i] = Some(*slot);
        }
    }

    dst.set_writable(false);

    Ok(Box::new(dst))
}

pub(crate) fn fork_regions(
    src_regions: &[&VirRegion],
    frames: &mut PageFrames,
) -> Result<Vec<Box<VirRegion>>, VmForkError> {
    let mut dst_regions = Vec::with_capacity(src_regions.len());
    for src in src_regions {
        match fork_region(src, frames) {
            Ok(dst) => dst_regions.push(dst),
            Err(e) => {
                free_forked_regions(&mut dst_regions, frames);
                return Err(e);
            }
        }
    }
    Ok(dst_regions)
}

/// Free all forked regions by decrementing refcounts and calling `ev_unreference`.
/// Corresponds to Minix3's `map_free_proc()` → `map_free()` → `map_subfree()` →
/// `pb_unreferenced()`.
fn free_forked_regions(regions: &mut [Box<VirRegion>], frames: &mut PageFrames) {
    for region in regions.iter() {
        for slot_opt in region.physblocks.iter() {
            if let Some(slot) = slot_opt {
                if slot.is_mapped() {
                    if let Some(mt) = slot.memtype {
                        mt.ev_unreference(frames, slot.pfn);
                    }
                    if let Some(state) = frames.get_mut(slot.pfn) {
                        if state.refcount > 0 {
                            state.refcount -= 1;
                        }
                    }
                }
            }
        }
    }
}

/// Top-level fork orchestration. Corresponds to Minix3's `do_fork()`.
///
/// Error handling mirrors Minix3:
/// - Validation failure → return error, no side effects
/// - `pt_new` failure → return `NoMemory`, no side effects
/// - `fork_regions` failure → free page table + free copied regions, return `NoMemory`
/// - `sys_fork` failure → panic (irrecoverable: kernel has created the child process)
pub(crate) fn do_fork(
    table: &VmProcTable,
    frames: &mut PageFrames,
    parent_endpoint: Endpoint,
    child_slot: UserSlot,
) -> Result<Endpoint, VmForkError> {
    let parent_slot = table
        .vm_isokendpt(parent_endpoint)
        .map_err(|_| VmForkError::InvalidEndpoint)?;

    let parent = table
        .get_active(parent_slot)
        .ok_or(VmForkError::InvalidSlot)?;

    assert_ne!(parent_slot, child_slot, "parent and child must occupy different slots");

    let empty = table
        .get_empty(child_slot)
        .ok_or(VmForkError::SlotInUse)?;

    let mut child = empty.activate_relaxed(Endpoint::NONE);

    child.init_from_fork(
        Endpoint::NONE,
        parent.total(),
        parent.total_max(),
        parent.region_top(),
    );
    child.copy_acl_from(&parent);

    child.init_page_table().map_err(|_| VmForkError::NoMemory)?;
    child.init_regions();

    let parent_regions: alloc::vec::Vec<&VirRegion> = parent.regions().iter().collect();
    let dst_regions = match fork_regions(&parent_regions, frames) {
        Ok(r) => r,
        Err(e) => {
            // SAFETY: page table was initialized by init_page_table() above,
            // and fork_regions failure means no regions were copied into it,
            // so there are no dangling references.
            unsafe { child.free_page_table(); }
            return Err(e);
        }
    };

    for region in dst_regions {
        child.regions_mut().insert(*region);
    }

    // SAFETY: child regions were just copied from parent with shared pages.
    // The page table is freshly created and contains only the pages mapped
    // during fork_region. No other thread can access this data (single-threaded
    // VM event loop model).
    unsafe { child.setup_cow_for_all_regions(frames); }
    // SAFETY: page table was just populated by setup_cow_for_all_regions.
    // All mappings point to valid physical frames with correct refcounts.
    // Single-threaded VM ensures no concurrent modification.
    unsafe { child.write_page_table_mappings(frames); }

    let child_endpoint = sys_fork(parent.endpoint(), child.slot());
    child.set_endpoint(child_endpoint);

    child.bind_page_table()
        .expect("pt_bind failed after sys_fork — irrecoverable, kernel already created child");

    // TODO: handle_memory_once — notify kernel of initial memory mapping
    // for the child process. Corresponds to Minix3's
    //   handle_memory_once(vmc, msgaddr, PFF_VMINHIBIT, ...)

    Ok(child_endpoint)
}

/// Notify kernel to create child process scheduling entity.
/// Corresponds to Minix3's `sys_fork()`.
///
/// Returns the child's new endpoint assigned by the kernel.
/// On failure, panics — like Minix3, this is irrecoverable because
/// the kernel may have already created the child process.
#[cfg(test)]
fn sys_fork(_parent_endpoint: Endpoint, child_slot: UserSlot) -> Endpoint {
    Endpoint::from_generation_slot(1, child_slot.get() as i32)
}

#[cfg(not(test))]
fn sys_fork(_parent_endpoint: Endpoint, _child_slot: UserSlot) -> Endpoint {
    todo!("sys_fork: send SYS_FORK message to kernel and receive child endpoint")
}

/// Resolve CoW for a single page within a region (fork helper).
///
/// Thin wrapper around `cow_resolve_core` that maps `CowCoreError` to
/// `VmForkError`.
pub(crate) fn cow_copy_page(
    region: &mut VirRegion,
    frames: &mut PageFrames,
    alloc: &mut dyn PfnAllocator,
    offset: VirBytes,
) -> Result<(), VmForkError> {
    cow_resolve_core(region, frames, alloc, offset)
        .map(|_| ())
        .map_err(|e| match e {
            crate::cow_exec_pf::CowCoreError::NoMemory => VmForkError::NoMemory,
            crate::cow_exec_pf::CowCoreError::PageNotMapped => VmForkError::PageNotMapped,
        })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum VmForkError {
    InvalidEndpoint,
    InvalidSlot,
    SlotInUse,
    NoMemory,
    PageNotMapped,
    MemType(MemTypeError),
}

impl From<MemTypeError> for VmForkError {
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

    #[test]
    fn test_fork_rollback_on_ev_reference_error() {
        use crate::memtype::{MemType, PagefaultResult, MemTypeError};

        struct FailOnRefMemType;
        impl MemType for FailOnRefMemType {
            fn name(&self) -> &'static str { "fail-on-ref" }
            fn ev_pagefault(&self, _proc_endpoint: Endpoint, _region: &mut VirRegion,
                _frames: &mut PageFrames, _offset: VirBytes, _write: bool,
            ) -> Result<PagefaultResult, MemTypeError> { Ok(PagefaultResult::Handled) }
            fn ev_unreference(&self, _frames: &mut PageFrames, _pfn: u32) {}
            fn ev_reference(&self, _frames: &mut PageFrames, _slot: crate::region::PageSlot,
            ) -> Result<(), MemTypeError> {
                Err(MemTypeError::NotSupported)
            }
            fn writable(&self, _frames: &PageFrames, _slot: crate::region::PageSlot,
                _region: &VirRegion,
            ) -> bool { false }
        }

        static FAIL_MT: FailOnRefMemType = FailOnRefMemType;

        let mut frames = make_frames(8);
        let mut alloc = TestAlloc { next: 0 };
        let mut src = VirRegion::new(VirBytes(0x1000), VirBytes(0x4000), VrFlags::empty());
        src.set_writable(true);
        src.def_memtype = Some(&FAIL_MT);

        let pfn0 = alloc.alloc_pfn().unwrap();
        let pfn1 = alloc.alloc_pfn().unwrap();
        src.map_page(&mut frames, VirBytes(0x0000), pfn0, &FAIL_MT);
        src.map_page(&mut frames, VirBytes(0x1000), pfn1, &FAIL_MT);

        assert_eq!(frames.get(pfn0).unwrap().refcount, 1);
        assert_eq!(frames.get(pfn1).unwrap().refcount, 1);

        let result = fork_region(&src, &mut frames);
        assert!(result.is_err());

        assert_eq!(frames.get(pfn0).unwrap().refcount, 1,
            "refcount should be rolled back after ev_reference failure");
        assert_eq!(frames.get(pfn1).unwrap().refcount, 1,
            "refcount should be rolled back after ev_reference failure");
    }

    #[test]
    fn test_fork_regions_rollback_on_failure() {
        use crate::memtype::{MemType, PagefaultResult, MemTypeError};

        struct FailOnSecondRefMemType;
        impl MemType for FailOnSecondRefMemType {
            fn name(&self) -> &'static str { "fail-on-2nd-ref" }
            fn ev_pagefault(&self, _proc_endpoint: Endpoint, _region: &mut VirRegion,
                _frames: &mut PageFrames, _offset: VirBytes, _write: bool,
            ) -> Result<PagefaultResult, MemTypeError> { Ok(PagefaultResult::Handled) }
            fn ev_unreference(&self, _frames: &mut PageFrames, _pfn: u32) {}
            fn ev_reference(&self, _frames: &mut PageFrames, _slot: crate::region::PageSlot,
            ) -> Result<(), MemTypeError> {
                static COUNT: core::sync::atomic::AtomicUsize =
                    core::sync::atomic::AtomicUsize::new(0);
                let n = COUNT.fetch_add(1, core::sync::atomic::Ordering::SeqCst);
                if n >= 1 {
                    return Err(MemTypeError::NotSupported);
                }
                Ok(())
            }
            fn writable(&self, _frames: &PageFrames, _slot: crate::region::PageSlot,
                _region: &VirRegion,
            ) -> bool { false }
        }

        static FAIL2_MT: FailOnSecondRefMemType = FailOnSecondRefMemType;

        let mut frames = make_frames(16);
        let mut alloc = TestAlloc { next: 0 };

        let mut src0 = VirRegion::new(VirBytes(0x1000), VirBytes(0x2000), VrFlags::empty());
        src0.set_writable(true);
        src0.def_memtype = Some(&FAIL2_MT);
        let pfn0 = alloc.alloc_pfn().unwrap();
        let pfn1 = alloc.alloc_pfn().unwrap();
        src0.map_page(&mut frames, VirBytes(0x0000), pfn0, &FAIL2_MT);
        src0.map_page(&mut frames, VirBytes(0x1000), pfn1, &FAIL2_MT);

        let mut src1 = VirRegion::new(VirBytes(0x5000), VirBytes(0x2000), VrFlags::empty());
        src1.set_writable(true);
        src1.def_memtype = Some(&FAIL2_MT);
        let pfn2 = alloc.alloc_pfn().unwrap();
        let pfn3 = alloc.alloc_pfn().unwrap();
        src1.map_page(&mut frames, VirBytes(0x0000), pfn2, &FAIL2_MT);
        src1.map_page(&mut frames, VirBytes(0x1000), pfn3, &FAIL2_MT);

        let src_regions: Vec<Box<VirRegion>> = vec![Box::new(src0), Box::new(src1)];
        let src_refs: Vec<&VirRegion> = src_regions.iter().map(|b| b.as_ref()).collect();

        assert_eq!(frames.get(pfn0).unwrap().refcount, 1);
        assert_eq!(frames.get(pfn1).unwrap().refcount, 1);

        let result = fork_regions(&src_refs, &mut frames);
        assert!(result.is_err());

        assert_eq!(frames.get(pfn0).unwrap().refcount, 1,
            "refcount for first region page should be rolled back after fork_regions failure");
        assert_eq!(frames.get(pfn1).unwrap().refcount, 1,
            "refcount for first region page should be rolled back after fork_regions failure");
    }
}
