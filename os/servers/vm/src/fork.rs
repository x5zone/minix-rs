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
use crate::region::{VirRegion, PageFrames, PfnAllocator, PAGE_SIZE};
use crate::memtype::MemTypeError;
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
    pt: &mut crate::pagetable::PageTable,
) -> Result<(), VmForkError> {
    // Page-align start and length, matching Minix3's handle_memory_start.
    let page_offset = mem.0 % PAGE_SIZE;
    let start = VirBytes(mem.0 - page_offset);
    let aligned_len = VirBytes((len.0 + page_offset).div_ceil(PAGE_SIZE) * PAGE_SIZE);

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
                // — C 的该路径同样写 PTE（pt_writemap），否则内核恢复进程后
                // 指令重执行二次故障（G-V12-8）。
                if region.needs_cow(frames, offset) {
                    cow_resolve_core(region, frames, pfn_alloc, offset, pt)
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
        if let Some(pfn) = slot.pfn() {
            if let Some(state) = frames.get_mut(pfn) {
                state.refcount = state.refcount.saturating_add(1);
                refcounted_pfns.push(pfn);
            }
            if let Some(mt) = slot.memtype()
                && let Err(e) = mt.ev_reference(frames, *slot) {
                    // Rollback: decrement refcount for all pages that were incremented.
                    for pfn in &refcounted_pfns {
                        if let Some(state) = frames.get_mut(*pfn)
                            && state.refcount > 0 {
                                state.refcount -= 1;
                            }
                    }
                    return Err(VmForkError::from(e));
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
            if let Some(pfn) = slot.pfn() {
                if let Some(mt) = slot.memtype() {
                    mt.ev_unreference(frames, pfn);
                }
                if let Some(state) = frames.get_mut(pfn)
                    && state.refcount > 0 {
                        state.refcount -= 1;
                    }
            }
        }
    }
}

/// Top-level fork orchestration. Corresponds to Minix3's `do_fork()`.
///
/// Error handling mirrors Minix3, with one documented deviation:
/// - Validation failure → return error, no side effects
/// - `pt_new` failure → return `PageTableInitFailed`, no side effects
/// - `fork_regions` failure → free page table + free copied regions, return `CowAllocFailed`
/// - `sys_fork` failure → C panics (post-commit irrecoverable); minix-rs
///   replies fail-closed instead (`KernelCall`). Pre-E1 the trap never
///   executed, so the reply is exact (kernel state untouched); post-E2
///   real post-commit kernel failures get a VM-side review at edge E2
///   sign-off (edge_todo.md).
pub(crate) fn do_fork(
    gateway: &mut dyn crate::kernel_gateway::KernelGateway,
    table: &VmProcTable,
    frames: &mut PageFrames,
    pfn_alloc: &mut dyn PfnAllocator,
    parent_endpoint: Endpoint,
    child_slot: UserSlot,
) -> Result<Endpoint, VmForkError> {
    let parent_slot = table
        .vm_isokendpt(parent_endpoint)
        .map_err(|_| VmForkError::InvalidEndpoint)?;

    let mut parent = table
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
            //      write_page_table_mappings), so freeing it cannot cause TLB shootdown
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
    if unsafe { child.write_page_table_mappings(frames) }.is_err() {
        // SAFETY: `free_page_table()` after failed `write_page_table_mappings`.
        // Page table has only CoW/empty entries — no active CR3 points to it.
        // CoW refcounts remain valid (parent holds source-of-truth refcount).
        // VM single-threaded: no concurrent access.
        unsafe { child.free_page_table(); }
        return Err(VmForkError::PageTableMapFailed);
    }

    // `write_page_table_mappings` failing above is the last recoverable
    // point — after `sys_fork` the kernel has committed the child process
    // and rollback is no longer possible. There is no separate bind step:
    // the kernel-side address-space registration happens in `sys_fork`
    // itself (A1 adoption semantics; Minix3's `pt_bind()` step-5
    // notification is subsumed by the VMCTL SetAddrSpace path).

    // C: fork.c:57-63 — sys_fork commits the child in the kernel and
    // returns its endpoint. minix-rs routes the call through the gateway
    // (kernel_gateway.rs) and maps failure to a fail-closed error reply:
    // pre-E1 the trap stub answers -EIO while the kernel state is
    // untouched, so the error is exact rather than a fabricated endpoint.
    // C: fork.c:57-63 — sys_fork commits the child in the kernel and
    // returns its endpoint plus the deliver-message buffer address
    // (`msgaddr`, the fifth output parameter, fork.c:90). minix-rs routes
    // the call through the gateway: kernel-call failure maps to a
    // fail-closed error reply (pre-E1 the trap stub answers -EIO while
    // kernel state is untouched); the C msgaddr output has no reply field
    // in the minix-rs kernel yet (edge E-FORKMSG), so the gateway hands
    // back `None` for it and the eager-CoW phase below is skipped.
    let (child_endpoint, fork_msgaddr) = gateway
        .sys_fork(parent.endpoint(), child.slot())
        .map_err(VmForkError::KernelCall)?;
    child.set_endpoint(child_endpoint);

    // C: fork.c:100-108 — pre-fault the deliver-message buffer for child
    // and parent (in that order). After sys_fork both sides' buffer pages
    // are CoW-shared (refcount 2, read-only PTEs); the kernel is about to
    // write the fork reply into both, and a write fault here would hit the
    // VM single-threaded event loop. `handle_memory_once(write)` resolves
    // the CoW eagerly, leaving each side a private page.
    //
    // C panics on failure; minix-rs propagates fail-closed (same
    // post-commit posture as the sys_fork error above).
    //
    // Dependency notes (V11/T33): the former "two live views cannot
    // coexist" blocker dissolved when the typestate API gained
    // `activate_relaxed` — `parent` and `child` are views of *different*
    // slots and coexist here exactly as C's `vmp`/`vmc` do. The remaining
    // dependency is the kernel reply field (edge E-FORKMSG); until it
    // lands, `fork_msgaddr == None` skips the phase (the kernel never
    // delivers a fork reply pre-E2 anyway).
    if let Some(msgaddr) = fork_msgaddr {
        let msg_len = VirBytes(core::mem::size_of::<minix_types::Message>() as u64);

        // C fork.c:103 — child first. G-V12-8: the eager CoW must also
        // write each side's PTEs, or the kernel write of the fork reply
        // re-faults on resume.
        let (child_regions, child_pt) = child.mem_parts_mut();
        handle_memory_once(
            child_regions,
            frames,
            pfn_alloc,
            VirBytes(msgaddr),
            msg_len,
            true,
            child_pt,
        )?;

        // C fork.c:105-107 — then the parent.
        let (parent_regions, parent_pt) = parent.mem_parts_mut();
        handle_memory_once(
            parent_regions,
            frames,
            pfn_alloc,
            VirBytes(msgaddr),
            msg_len,
            true,
            parent_pt,
        )?;
    }

    Ok(child_endpoint)
}

/// Resolve CoW for a single page within a region (fork helper).
///
/// Thin wrapper around `cow_resolve_core` that maps `CowCoreError` to
/// `VmForkError`.
// Test-only single-page CoW entry. The fork production path's eager-CoW
// phase (V11/T33) goes through `handle_memory_once`, which resolves CoW
// internally; this wrapper stays as the focused unit-test surface.
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn cow_copy_page(
    region: &mut VirRegion,
    frames: &mut PageFrames,
    alloc: &mut dyn PfnAllocator,
    offset: VirBytes,
    pt: &mut crate::pagetable::PageTable,
) -> Result<(), VmForkError> {
    cow_resolve_core(region, frames, alloc, offset, pt)
        .map(|_| ())
        .map_err(|e| match e {
            crate::cow_exec_pf::CowCoreError::NoMemory => VmForkError::CowAllocFailed,
            crate::cow_exec_pf::CowCoreError::PageNotMapped => VmForkError::PageNotMapped,
            crate::cow_exec_pf::CowCoreError::PageTable(_) => VmForkError::CowAllocFailed,
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
    /// Page table mapping failed (pt_writemap).
    /// Corresponds to Minix3's ENOMEM from pt_writemap().
    PageTableMapFailed,
    /// The kernel-side sys_fork failed through the gateway (V11/T9).
    /// C: do_fork.c panics on this; minix-rs replies fail-closed
    /// (NotImplemented pre-E1 — trap not wired; InternalError for a real
    /// kernel errno). See `do_fork` doc above.
    KernelCall(crate::kernel_gateway::GatewayError),
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
    use crate::region::{VrFlags, PfnAllocError};
    use crate::memtype::MEM_TYPE_ANON;

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

    /// Shared MockGateway handle for the do_fork tests (same idiom as
    /// rs.rs's `update_gateway`).
    fn update_gateway() -> alloc::rc::Rc<core::cell::RefCell<
        crate::kernel_gateway::MockGateway>>
    {
        alloc::rc::Rc::new(core::cell::RefCell::new(
            crate::kernel_gateway::MockGateway::new(),
        ))
    }

    /// Parent setup for do_fork tests: live slot with one writable anon
    /// region (def_memtype ANON) whose first page is mapped (the future
    /// deliver-message buffer). Returns the parent endpoint.
    fn init_fork_parent(
        slot: UserSlot,
        frames: &mut PageFrames,
        alloc: &mut TestAlloc,
    ) -> Endpoint {
        let table = VmProcTable::get_global();
        unsafe { table.reset_slot(slot); }
        let empty = table.get_empty(slot).unwrap();
        let ep = Endpoint::from_generation_slot(1, slot.get() as i32);
        let mut active = empty.activate(ep);
        active.init_page_table().unwrap();
        active.init_regions();
        {
            let mut proc = table.get_active(slot).unwrap();
            let mut region = crate::region::VirRegion::new(
                VirBytes(0x3000_0000),
                VirBytes(0x4000),
                VrFlags::ANON | VrFlags::WRITABLE,
            );
            region.def_memtype = Some(&MEM_TYPE_ANON);
            proc.regions_mut().insert(region).unwrap();
        }
        let pfn = alloc.alloc_pfn().unwrap();
        let mut proc = table.get_active(slot).unwrap();
        let region = proc.regions_mut().find_mut(VirBytes(0x3000_0000)).unwrap();
        region.map_page(frames, VirBytes(0), pfn, &MEM_TYPE_ANON);
        ep
    }

    /// V11/T33: do_fork's eager-CoW phase (C fork.c:100-108). The mock
    /// gateway hands back a msgaddr pointing at the parent's single mapped
    /// page (refcount 2 after the CoW fork); the child phase resolves the
    /// child's copy, after which the parent's refcount is 1 and the parent
    /// phase is a no-op — the exact C end state: each side private.
    #[test]
    fn test_do_fork_eager_cow_resolves_message_pages() {
        let table = VmProcTable::get_global();
        let mut frames = make_frames(16);
        let mut alloc = TestAlloc { next: 5 };
        let (parent_slot, child_slot) = (UserSlot::new(74), UserSlot::new(75));

        let parent_ep = init_fork_parent(parent_slot, &mut frames, &mut alloc);
        assert_ne!(parent_ep, Endpoint::NONE);

        let gateway = update_gateway();
        gateway.borrow_mut().fork_reply = Ok(Endpoint::from_generation_slot(2, 75));
        gateway.borrow_mut().fork_msgaddr = core::cell::Cell::new(Some(0x3000_0000));

        let child_ep = do_fork(
            &mut *gateway.borrow_mut(),
            table,
            &mut frames,
            &mut alloc,
            parent_ep,
            child_slot,
        ).unwrap();

        assert_ne!(child_ep, parent_ep);
        // Parent: kept its original (now private) page.
        let parent_view = table.get_active(parent_slot).unwrap();
        let src_slot = parent_view.regions().iter()
            .find(|vr| vr.vaddr.0 == 0x3000_0000).unwrap()
            .get_slot(VirBytes(0)).and_then(|s| s.pfn()).unwrap();
        assert_eq!(frames.get(src_slot).map(|s| s.refcount), Some(1),
            "parent side resolved to a private page");
        // Child: resolved its own copy (different frame).
        let child_view = table.get_active(child_slot).unwrap();
        let dst_pfn = child_view.regions().iter()
            .find(|vr| vr.vaddr.0 == 0x3000_0000).unwrap()
            .get_slot(VirBytes(0)).and_then(|s| s.pfn())
            .expect("child message page must be mapped after eager CoW");
        assert_ne!(dst_pfn, src_slot, "child got its own copy");
    }

    #[test]
    fn test_do_fork_without_msgaddr_skips_prefault() {
        // Pre-E-FORKMSG shape: gateway hands back no msgaddr → the eager
        // phase is skipped and both sides stay CoW-shared (refcount 2).
        let table = VmProcTable::get_global();
        let mut frames = make_frames(16);
        let mut alloc = TestAlloc { next: 5 };
        let (parent_slot, child_slot) = (UserSlot::new(76), UserSlot::new(77));

        let parent_ep = init_fork_parent(parent_slot, &mut frames, &mut alloc);

        let gateway = update_gateway();
        gateway.borrow_mut().fork_reply = Ok(Endpoint::from_generation_slot(2, 77));
        // fork_msgaddr stays None.

        do_fork(
            &mut *gateway.borrow_mut(),
            table,
            &mut frames,
            &mut alloc,
            parent_ep,
            child_slot,
        ).unwrap();

        let parent_view = table.get_active(parent_slot).unwrap();
        let src_slot = parent_view.regions().iter()
            .find(|vr| vr.vaddr.0 == 0x3000_0000).unwrap()
            .get_slot(VirBytes(0)).and_then(|s| s.pfn()).unwrap();
        assert_eq!(frames.get(src_slot).map(|s| s.refcount), Some(2),
            "CoW-sharing must be untouched without a msgaddr");
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
        let mut pt = <crate::pagetable::PageTable as crate::pagetable::Paging>::new().unwrap();

        let pfn = alloc.alloc_pfn().unwrap();
        region.map_page(&mut frames, VirBytes(0x0000), pfn, &MEM_TYPE_ANON);

        frames.get_mut(pfn).unwrap().refcount = 2;

        cow_copy_page(&mut region, &mut frames, &mut alloc, VirBytes(0x0000), &mut pt).unwrap();

        assert_eq!(frames.get(pfn).unwrap().refcount, 1);

        let new_slot = region.get_slot(VirBytes(0x0000)).unwrap();
        assert_ne!(new_slot.pfn(), Some(pfn));
        assert_eq!(frames.get(new_slot.pfn().unwrap()).unwrap().refcount, 1);
    }

    #[test]
    fn test_cow_copy_page_no_sharing() {
        let mut frames = make_frames(8);
        let mut alloc = TestAlloc { next: 0 };
        // V13-P1-1: the reuse shortcut is gated on is_page_writable, so the
        // no-sharing shape must carry VR_WRITABLE (anon private page) for the
        // same-frame outcome to apply.
        let mut region = VirRegion::new(VirBytes(0x1000), VirBytes(0x4000), VrFlags::WRITABLE);
        region.def_memtype = Some(&MEM_TYPE_ANON);
        let mut pt = <crate::pagetable::PageTable as crate::pagetable::Paging>::new().unwrap();

        let pfn = alloc.alloc_pfn().unwrap();
        region.map_page(&mut frames, VirBytes(0x0000), pfn, &MEM_TYPE_ANON);

        cow_copy_page(&mut region, &mut frames, &mut alloc, VirBytes(0x0000), &mut pt).unwrap();

        let slot = region.get_slot(VirBytes(0x0000)).unwrap();
        assert_eq!(slot.pfn(), Some(pfn));
    }

    #[test]
    fn test_fork_rollback_on_ev_reference_error() {
        use crate::memtype::{MemType, PagefaultResult, MemTypeError};

        struct FailOnRefMemType;
        // SAFETY: test-only static (`FAIL_MT`), single-threaded tests.
        unsafe impl Sync for FailOnRefMemType {}
        impl MemType for FailOnRefMemType {
            fn name(&self) -> &'static str { "fail-on-ref" }
            fn ev_pagefault(&self, _proc_endpoint: Endpoint, _region: &mut VirRegion,
                _frames: &mut PageFrames, _offset: VirBytes, _write: bool,
                _table: &crate::vmproc::VmProcTable, _alloc: &mut dyn crate::region::PfnAllocator,
                _cache: &mut crate::page_cache::PageCache,
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
        // SAFETY: test-only static (`FAIL2_MT`), single-threaded tests.
        unsafe impl Sync for FailOnSecondRefMemType {}
        impl MemType for FailOnSecondRefMemType {
            fn name(&self) -> &'static str { "fail-on-2nd-ref" }
            fn ev_pagefault(&self, _proc_endpoint: Endpoint, _region: &mut VirRegion,
                _frames: &mut PageFrames, _offset: VirBytes, _write: bool,
                _table: &crate::vmproc::VmProcTable, _alloc: &mut dyn crate::region::PfnAllocator,
                _cache: &mut crate::page_cache::PageCache,
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
        let mut pt = <crate::pagetable::PageTable as crate::pagetable::Paging>::new().unwrap();
        let result = handle_memory_once(
            &mut regions, &mut frames, &mut alloc,
            VirBytes(0x1000), VirBytes(0x1000), true, &mut pt,
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

        let mut pt = <crate::pagetable::PageTable as crate::pagetable::Paging>::new().unwrap();
        let result = handle_memory_once(
            &mut regions, &mut frames, &mut alloc,
            VirBytes(0x1000), VirBytes(0x1000), true, &mut pt,
        );
        assert!(result.is_ok());

        // CoW should have been resolved: new private page allocated.
        let region = regions.find(VirBytes(0x1000)).unwrap();
        let slot = region.get_slot(VirBytes(0x0000)).unwrap();
        assert_ne!(slot.pfn(), Some(pfn), "CoW should allocate a new page");
        assert_eq!(frames.get(slot.pfn().unwrap()).unwrap().refcount, 1);
        assert_eq!(frames.get(pfn).unwrap().refcount, 1);
    }

    #[test]
    fn test_handle_memory_once_unmapped_address() {
        // Address not in any region — should return PageNotMapped.
        let mut frames = make_frames(4);
        let mut alloc = TestAlloc { next: 0 };
        let mut regions = crate::region::RegionMap::new(); // empty

        let mut pt = <crate::pagetable::PageTable as crate::pagetable::Paging>::new().unwrap();
        let result = handle_memory_once(
            &mut regions, &mut frames, &mut alloc,
            VirBytes(0x1000), VirBytes(0x1000), true, &mut pt,
        );
        assert_eq!(result, Err(VmForkError::PageNotMapped));
    }
}
