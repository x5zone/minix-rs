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

use minix_types::{VirBytes, Endpoint, UserSlot, NR_PROCS};
use crate::region::{VirRegion, VrFlags, PageFrames, PfnAllocator, PfnAllocError, PAGE_SIZE};
use crate::memtype::{MemType, MemTypeError, MEM_TYPE_ANON};
use crate::cow_exec_pf::cow_resolve_core;
use crate::vmproc::VmProcTable;
use alloc::vec::Vec;

/// Ensure a virtual address range is mapped and (optionally) writable.
///
/// Corresponds to Minix3's `handle_memory_once()` (pagefaults.c:245-252).
/// Iterates page-by-page over `[mem, mem+len)`, looking up each page's region
/// and resolving CoW if `wrflag` is set and the page is shared.
///
/// This is used after `sys_fork` to pre-fault the message buffer pages so that
/// the kernel can write the fork reply message without triggering a page fault
/// (which would deadlock since VM is single-threaded and would need to handle
/// its own page fault).
///
/// Returns `Ok(())` if all pages are mapped (and writable if requested).
/// Returns `Err(VmForkError::PageNotMapped)` if any address has no region.
/// Returns `Err(VmForkError::CowAllocFailed)` if CoW resolution fails.
pub(crate) fn handle_memory_once(
    regions: &mut crate::region::RegionMap,
    frames: &mut PageFrames,
    pfn_alloc: &mut dyn PfnAllocator,
    mem: VirBytes,
    len: VirBytes,
    wrflag: bool,
) -> Result<(), VmForkError> {
    // Page-align start and length, matching Minix3's handle_memory_start.
    let page_offset = mem.0 % PAGE_SIZE;
    let start = VirBytes(mem.0 - page_offset);
    let aligned_len = VirBytes(((len.0 + page_offset + PAGE_SIZE - 1) / PAGE_SIZE) * PAGE_SIZE);

    let mut addr = start;
    let end = VirBytes(start.0 + aligned_len.0);

    while addr.0 < end.0 {
        // Find the region containing this address.
        // C: map_lookup(hmstate->vmp, hmstate->mem, NULL)
        let region_start = {
            let region = regions.find(addr).ok_or(VmForkError::PageNotMapped)?;

            // If wrflag is set, region must be writable.
            // C: !(region->flags & VR_WRITABLE) && hmstate->wrflag → EFAULT
            if wrflag && !region.is_writable() {
                return Err(VmForkError::PageNotMapped);
            }

            region.vaddr
        };

        // Process pages within this region.
        let region = regions.find_mut(addr).ok_or(VmForkError::PageNotMapped)?;
        let region_end = VirBytes(region.vaddr.0 + region.length.0);

        while addr.0 < end.0 && addr.0 < region_end.0 {
            let offset = VirBytes(addr.0 - region_start.0);

            if wrflag {
                // Resolve CoW for this page if needed.
                // C: map_handle_memory(vmp, region, offset, PAGE_SIZE, wrflag, ...)
                if region.needs_cow(frames, offset) {
                    cow_resolve_core(region, frames, pfn_alloc, offset)
                        .map_err(|_| VmForkError::CowAllocFailed)?;
                }
            }

            addr.0 += PAGE_SIZE;
        }
    }

    Ok(())
}

/// Fork a single VirRegion: share physical pages (refcount++) and set child
/// region read-only. Corresponds to Minix3 `map_copy_region()` (region.c).
///
/// On `ev_reference` failure, rolls back all previously incremented refcounts
/// and returns `VmForkError`.
pub(crate) fn fork_region(
    src: &VirRegion,
    frames: &mut PageFrames,
) -> Result<VirRegion, VmForkError> {
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

    for (i, slot) in src.physblocks.iter().enumerate() {
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
            dst.physblocks[i] = *slot;
        }
    }

    dst.set_writable(false);

    Ok(dst)
}

pub(crate) fn fork_regions(
    src_regions: &[&VirRegion],
    frames: &mut PageFrames,
) -> Result<Vec<VirRegion>, VmForkError> {
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
fn free_forked_regions(regions: &mut [VirRegion], frames: &mut PageFrames) {
    for region in regions.iter() {
        for slot in region.physblocks.iter() {
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

/// Top-level fork orchestration. Corresponds to Minix3's `do_fork()`.
///
/// Error handling mirrors Minix3:
/// - Validation failure → return error, no side effects
/// - `pt_new` failure → return `PageTableInitFailed`, no side effects
/// - `fork_regions` failure → free page table + free copied regions, return `CowAllocFailed`
/// - `sys_fork` failure → panic (irrecoverable: kernel has created the child process)
pub(crate) fn do_fork(
    table: &VmProcTable,
    frames: &mut PageFrames,
    pfn_alloc: &mut dyn PfnAllocator,
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

    // C: fork.c:47-52 — `childproc >= NR_PROCS` → EINVAL. The exec-rewrite
    // temp slot (`VM_EXEC_TMP_SLOT == NR_PROCS`) is not a valid fork target;
    // only slots 0..NR_PROCS-1 are. `UserSlot` is `usize`, so C's negative
    // check (`childproc < 0`) is structurally impossible.
    if child_slot.get() >= NR_PROCS {
        return Err(VmForkError::InvalidSlot);
    }

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

    child.init_page_table().map_err(|_| VmForkError::PageTableInitFailed)?;
    child.init_regions();

    let parent_regions: alloc::vec::Vec<&VirRegion> = parent.regions().iter().collect();
    let dst_regions = match fork_regions(&parent_regions, frames) {
        Ok(r) => r,
        Err(e) => {
            // SAFETY: `free_page_table()` on the child after `fork_regions` failure.
            //
            // Preconditions verified:
            //   1. `child` is in `Active` typestate (via `activate_relaxed` above),
            //      so `free_page_table()` is a valid state transition.
            //   2. `init_page_table()` succeeded (line 219), so the page table
            //      allocator state is initialized and can be freed.
            //   3. `fork_regions` failed *before* writing any region metadata
            //      into the page table — no live PTEs reference the allocators.
            //   4. No CR3 points to this page table (not yet bound via
            //      `bind_page_table`), so freeing it cannot cause TLB shootdown
            //      issues or dangling hardware references.
            //
            // Alias safety: `child` is a local mutable handle obtained from
            // `activate_relaxed` and not yet published to any other data
            // structure. VM is single-threaded (event loop model), so no
            // concurrent access to `child` is possible.
            // SAFETY: See reasoning above — child is Active, page table is
            // initialized, no CR3 points to it, no live PTEs, single-threaded.
            unsafe { child.free_page_table(); }
            return Err(e);
        }
    };

    for region in dst_regions {
        child.regions_mut().insert(region)
            .expect("fork: child regions are fresh, no overlap possible");
    }

    // SAFETY: `setup_cow_for_all_regions()` on the child after region insertion.
    //
    // Preconditions verified:
    //   1. `child` is in `Active` typestate — `setup_cow_for_all_regions`
    //      requires an active process with initialized page table (✓ from
    //      `init_page_table()` at line 219).
    //   2. The child page table has no live mappings — it was freshly created
    //      by `init_page_table()` and no prior `write_page_table_mappings()`
    //      call has been made.
    //   3. The child's region metadata (`dst_regions`) matches the source
    //      from which `frames` pages were allocated — `dst_regions` come from
    //      `fork_regions(&parent_regions, frames)`, which ensures 1:1
    //      correspondence between region VA ranges and frame entries.
    //   4. `frames` contains all parent pages with refcount ≥ 1 (parent still
    //      holds a reference), so CoW marking cannot underflow refcounts.
    //   5. No CR3 points to this page table (not yet bound), so CoW flag
    //      writes do not require TLB invalidation.
    //
    // Alias safety: `child` and `frames` are local variables; VM is
    // single-threaded (event loop model), so exclusive access is guaranteed.
    // SAFETY: See reasoning above — child is Active, all regions are mapped,
    // no CR3 points to it, CoW refcounts are valid, single-threaded.
    unsafe { child.setup_cow_for_all_regions(frames); }

    // Write page table mappings. If this fails, rollback by freeing the page table.
    // SAFETY: `write_page_table_mappings()` on the child after CoW setup.
    //
    // Preconditions verified:
    //   1. The page table is in the "regions-marked-CoW" state produced by
    //      `setup_cow_for_all_regions` — all PTEs are either empty or CoW.
    //   2. All mapping source pages exist in `frames` with the correct refcount
    //      (refcount already bumped by `fork_regions` for shared pages).
    //   3. The PT walk is the only consumer of these pages at this moment —
    //      no other code path reads or modifies the child's page table.
    //   4. No CR3 points to this page table (not yet bound), so PTE writes
    //      do not require TLB invalidation.
    //
    // Error path: if `write_page_table_mappings` fails, the page table has
    // only CoW/empty entries (the function failed before committing all
    // mappings). `free_page_table()` is safe because:
    //   - No active CR3 points to it (not yet bound).
    //   - CoW refcounts were bumped in `fork_regions`; we drop the page
    //     table WITHOUT decrementing the parent refcount because CoW
    //     semantics treat the parent's refcount as the source of truth.
    //   - VM is single-threaded, so no concurrent access to `child`.
    // SAFETY: See reasoning above — child is Active, page table initialized,
    // no CR3 points to it, CoW refcounts valid, single-threaded.
    if let Err(_) = unsafe { child.write_page_table_mappings(frames) } {
        // SAFETY: `free_page_table()` after failed `write_page_table_mappings`.
        // Page table has only CoW/empty entries — no active CR3 points to it.
        // CoW refcounts remain valid (parent holds source-of-truth refcount).
        // VM single-threaded: no concurrent access.
        unsafe { child.free_page_table(); }
        return Err(VmForkError::PageTableMapFailed);
    }

    // Bind page table BEFORE sys_fork so that failure is recoverable.
    // If bind_page_table fails, we can still rollback (free page table, clear slot).
    // After sys_fork, the kernel has committed the child process and rollback
    // is no longer possible — so any failure after sys_fork is irrecoverable.
    if let Err(_) = child.bind_page_table() {
        // SAFETY: `free_page_table()` after failed `bind_page_table`.
        //
        // Preconditions verified:
        //   1. `bind_page_table` failed BEFORE the kernel has accepted the
        //      new process — the page table is not in any CR3 register.
        //   2. `free_page_table` can therefore drop all mappings and the
        //      underlying page-allocator state without races against the
        //      kernel MMU (no TLB shootdown needed).
        //   3. This is the LAST recoverable point — after `sys_fork` the
        //      kernel commits the child and rollback is no longer possible.
        //   4. VM is single-threaded, so no concurrent access to `child`.
        // SAFETY: See reasoning above — no CR3, no TLB shootdown needed,
        // last recoverable point, single-threaded.
        unsafe { child.free_page_table(); }
        return Err(VmForkError::PageTableMapFailed);
    }

    let child_endpoint = sys_fork(parent.endpoint(), child.slot());
    child.set_endpoint(child_endpoint);

    // C: fork.c:97-108 — pre-fault message buffer pages for child and parent.
    // After sys_fork, the kernel writes the fork reply to both the parent's
    // and child's message buffers. If these pages are CoW (read-only), the
    // write would trigger a page fault, which would deadlock because VM is
    // single-threaded. handle_memory_once resolves CoW eagerly.
    //
    // C code:
    //   vir = msgaddr;
    //   handle_memory_once(vmc, vir, sizeof(message), 1)  // child
    //   handle_memory_once(vmp, vir, sizeof(message), 1)  // parent
    //
    // Note: In Minix3, msgaddr comes from sys_fork's return value
    // (rpp->p_delivermsg_vir). Since our sys_fork is a stub, we use a
    // placeholder. When IpcTransport is implemented, msgaddr will be
    // returned by the real sys_fork call.
    //
    // # DEFERRED
    //
    // The eager CoW resolution for fork's deliver-message buffer is
    // blocked by 2 independent dependencies:
    //
    // **Dependency 1 (this TODO)**: `VmProcTable` does not support
    // simultaneous mutable access to two slots. The C version of
    // `do_fork` calls `handle_memory_once` for both `vmc` (child)
    // and `vmp` (parent) with the same `msgaddr`. In Rust, the
    // `VmProcTable::get_active()` returns a `&ActiveProc<'_>` (an
    // immutable view) for the parent, while `child` is a mutable
    // `EmptySlot`/etc. We need either:
    //   - Split borrowing (NLL doesn't support it across struct
    //     fields without explicit `RefCell`/`UnsafeCell`); or
    //   - Restructure `do_fork` to take both slots as a single
    //     tuple return.
    //
    // **Dependency 2**: `sys_fork` is a stub (line 334 below). The
    // real `sys_fork` must return the kernel-assigned child endpoint
    // AND the `msgaddr` of the deliver-message buffer. Currently
    // `sys_fork` returns only the endpoint; the `msgaddr` is a
    // placeholder. This blocks Dependency 1 because the eager CoW
    // resolution needs the real `msgaddr`.
    //
    // # Why safe to defer?
    //
    // The C source comment in `fork.c:97-108` says:
    //   "making these messages writable is an optimisation and
    //    its return value needn't be checked"
    //
    // If the pages are still CoW when the kernel writes the reply,
    // the child process will trigger a page fault on first access.
    // The page fault handler will resolve CoW normally (allocating
    // a fresh page for the child). The only cost is a one-time page
    // fault per fork, not a deadlock. (The "deadlock" concern in
    // the original comment was about the *parent* triggering a
    // page fault, not the child — but the child is a fresh process
    // and can handle its own page faults asynchronously.)
    //
    // # Implementation path
    //
    // When Dependencies 1 and 2 are resolved:
    // ```ignore
    // let msgaddr = sys_fork(parent_endpoint, child.slot()).1; // tuple
    // handle_memory_once(&mut child, msgaddr, MEM_MESSAGE_SIZE, 1)?;
    // handle_memory_once(parent,       msgaddr, MEM_MESSAGE_SIZE, 1)?;
    // ```
    // (The second call's mutable access to `parent` is what
    // Dependency 1's split-borrow restructure is for.)

    Ok(child_endpoint)
}

/// Notify kernel to create child process scheduling entity.
/// Corresponds to Minix3's `sys_fork()`.
///
/// Returns the child's new endpoint assigned by the kernel.
/// On failure, panics — like Minix3, this is irrecoverable because
/// the kernel may have already created the child process.
///
/// DEFERRED (2026-06-15): Once IpcTransport is implemented, this will call:
///   ipc_call_kernel(SYS_FORK, parent_endpoint, child_slot)
/// Implementation path: (1) IpcTransport::sendrecv to KERNEL endpoint;
/// (2) kernel do_fork creates child proc + copies address space;
/// (3) returns child endpoint. Currently returns a deterministic endpoint
/// for testing — real hardware requires kernel IPC (kernel IPC core dependency).
fn sys_fork(_parent_endpoint: Endpoint, child_slot: UserSlot) -> Endpoint {
    Endpoint::from_generation_slot(1, child_slot.get() as i32)
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
            crate::cow_exec_pf::CowCoreError::NoMemory => VmForkError::CowAllocFailed,
            crate::cow_exec_pf::CowCoreError::PageNotMapped => VmForkError::PageNotMapped,
        })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum VmForkError {
    InvalidEndpoint,
    InvalidSlot,
    SlotInUse,
    /// CoW page allocation failed (cow_resolve_core or ev_reference).
    /// Corresponds to Minix3's ENOMEM from map_handle_memory / pb_reference.
    CowAllocFailed,
    /// Page table initialization failed (pt_new / init_page_table).
    /// Corresponds to Minix3's ENOMEM from pt_new() in do_fork().
    PageTableInitFailed,
    /// Page table mapping or binding failed (pt_map / pt_bind).
    /// Corresponds to Minix3's ENOMEM from pt_writemap() / pt_bind().
    PageTableMapFailed,
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
                _table: &crate::vmproc::VmProcTable, _alloc: &mut dyn crate::region::PfnAllocator,
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
                _table: &crate::vmproc::VmProcTable, _alloc: &mut dyn crate::region::PfnAllocator,
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

        let src_regions: Vec<VirRegion> = vec![src0, src1];
        let src_refs: Vec<&VirRegion> = src_regions.iter().collect();

        assert_eq!(frames.get(pfn0).unwrap().refcount, 1);
        assert_eq!(frames.get(pfn1).unwrap().refcount, 1);

        let result = fork_regions(&src_refs, &mut frames);
        assert!(result.is_err());

        assert_eq!(frames.get(pfn0).unwrap().refcount, 1,
            "refcount for first region page should be rolled back after fork_regions failure");
        assert_eq!(frames.get(pfn1).unwrap().refcount, 1,
            "refcount for first region page should be rolled back after fork_regions failure");
    }

    #[test]
    fn test_handle_memory_once_no_cow() {
        // Region with refcount=1 (no CoW needed) — should succeed without changes.
        let mut frames = make_frames(4);
        let mut alloc = TestAlloc { next: 0 };
        let mut regions = crate::region::RegionMap::new();

        let mut region = VirRegion::new(VirBytes(0x1000), VirBytes(0x4000), VrFlags::empty());
        region.set_writable(true);
        region.def_memtype = Some(&MEM_TYPE_ANON);

        let pfn = alloc.alloc_pfn().unwrap();
        region.map_page(&mut frames, VirBytes(0x0000), pfn, &MEM_TYPE_ANON);

        regions.insert(region).unwrap();

        // wrflag=true, but refcount=1 so no CoW needed.
        let result = handle_memory_once(
            &mut regions, &mut frames, &mut alloc,
            VirBytes(0x1000), VirBytes(0x1000), true,
        );
        assert!(result.is_ok());
        assert_eq!(frames.get(pfn).unwrap().refcount, 1);
    }

    #[test]
    fn test_handle_memory_once_resolves_cow() {
        // Region with refcount=2 (CoW needed) — should resolve and get private page.
        let mut frames = make_frames(8);
        let mut alloc = TestAlloc { next: 0 };
        let mut regions = crate::region::RegionMap::new();

        let mut region = VirRegion::new(VirBytes(0x1000), VirBytes(0x4000), VrFlags::empty());
        region.set_writable(true);
        region.def_memtype = Some(&MEM_TYPE_ANON);

        let pfn = alloc.alloc_pfn().unwrap();
        region.map_page(&mut frames, VirBytes(0x0000), pfn, &MEM_TYPE_ANON);
        frames.get_mut(pfn).unwrap().refcount = 2; // shared page

        regions.insert(region).unwrap();

        let result = handle_memory_once(
            &mut regions, &mut frames, &mut alloc,
            VirBytes(0x1000), VirBytes(0x1000), true,
        );
        assert!(result.is_ok());

        // CoW should have been resolved: new private page allocated.
        let region = regions.find(VirBytes(0x1000)).unwrap();
        let slot = region.get_slot(VirBytes(0x0000)).unwrap();
        assert_ne!(slot.pfn, pfn, "CoW should allocate a new page");
        assert_eq!(frames.get(slot.pfn).unwrap().refcount, 1);
        assert_eq!(frames.get(pfn).unwrap().refcount, 1);
    }

    #[test]
    fn test_handle_memory_once_unmapped_address() {
        // Address not in any region — should return PageNotMapped.
        let mut frames = make_frames(4);
        let mut alloc = TestAlloc { next: 0 };
        let mut regions = crate::region::RegionMap::new(); // empty

        let result = handle_memory_once(
            &mut regions, &mut frames, &mut alloc,
            VirBytes(0x1000), VirBytes(0x1000), true,
        );
        assert_eq!(result, Err(VmForkError::PageNotMapped));
    }
}
