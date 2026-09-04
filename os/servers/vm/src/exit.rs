//! VM exit handling.
//!
//! Handles VM_EXIT and VM_WILLEXIT requests from PM.
//! Releases all process resources: memory regions, physical pages, page tables.
//!
//! Corresponds to Minix3's `do_exit()` and `do_willexit()` in `exit.c`.

use minix_types::Endpoint;
use crate::vmproc::{VmProcTable, EndpointError};
use crate::region::{RegionMap, PageFrames, PageFlags, PfnAllocator};
use crate::alloc_page::VmPageAllocator;

/// Errors from VM_EXIT / VM_WILLEXIT operations.
///
/// All variants map to `VmError` via `From<VmExitError> for VmError`,
/// then to C errno via `VmError::to_errno()`. Per-error `to_errno()`
/// method is intentionally omitted — the single source of truth is
/// `VmError::to_errno()` in `minix_types::ipc::vm`.
///
/// Note: C `do_exit()` returns EINVAL for both error cases (exit.c:68,74).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum VmExitError {
    ProcessNotFound,
    NotExiting,
}

// ── Endpoint-lookup error unification ──
//
// See `munmap.rs` for the full rationale. Exit handlers collapse both
// EndpointError variants to VmExitError::ProcessNotFound. C `do_exit()`
// returns EINVAL for both — we mirror that.
impl From<EndpointError> for VmExitError {
    fn from(_: EndpointError) -> Self {
        VmExitError::ProcessNotFound
    }
}

/// Handle VM_EXIT — release process resources.
///
/// Corresponds to Minix3's `do_exit()` (exit.c:60).
/// Prerequisite: VM_WILLEXIT must have been called first (enforced by typestate).
pub(crate) fn handle_vm_exit(
    table: &VmProcTable,
    page_alloc: &mut VmPageAllocator,
    frames: &mut PageFrames,
    endpoint: Endpoint,
) -> Result<(), VmExitError> {
    let slot = table.vm_isokendpt(endpoint)?;

    let mut exiting = table.get_exiting(slot)
        .ok_or(VmExitError::NotExiting)?;

    free_process_phys(exiting.regions_mut(), frames, page_alloc);

    // SAFETY: Single-threaded VM ensures no concurrent access to this slot.
    // reap() restores the VmProc slot to vacant state (empty typestate).
    unsafe { exiting.reap(); }

    Ok(())
}

/// Handle VM_WILLEXIT — pre-notification that process will exit.
///
/// Corresponds to Minix3's `do_willexit()` (exit.c:100).
/// Sets EXITING flag so subsequent memory allocation requests are rejected.
pub(crate) fn handle_vm_willexit(
    table: &VmProcTable,
    endpoint: Endpoint,
) -> Result<(), VmExitError> {
    let slot = table.vm_isokendpt(endpoint)?;

    let active = table.get_active(slot)
        .ok_or(VmExitError::ProcessNotFound)?;

    let _exiting = active.mark_exiting();

    Ok(())
}

/// Release physical pages for all regions of a process.
///
/// Corresponds to Minix3's map_free_proc() → map_free() → map_subfree() →
/// pb_unreferenced() → ev_unreference() → free_mem() chain (region.c:589-602,
/// region.c:568-585, region.c:527-563, pb.c:96-133, mem_anon.c:56-62),
/// plus the per-region ev_delete / fdref_deref step (C map_free →
/// `if(region->def_memtype->ev_delete) ev_delete(region)` → for file regions
/// mappedfile_delete → fdref_deref, mem_file.c:280-287).
///
/// For each mapped PageSlot: decrements PageFrames refcount (equivalent to
/// Minix3's pb.refcount--), calls the MemType ev_unreference callback,
/// and conditionally frees the physical page when refcount reaches 0.
/// RegionMap::clear() (inside reap()) handles releasing the data structures.
///
/// NOTE: ev_unreference in the PFN model is a no-op for anonymous/direct
/// memory — the caller is responsible for both refcount decrement and
/// physical page freeing. This separation of concerns is documented in
/// §3.2 of 22-vm-exit.md.
fn free_process_phys(
    regions: &mut RegionMap,
    frames: &mut PageFrames,
    page_alloc: &mut VmPageAllocator,
) {
    for region in regions.iter_mut() {
        // Capture the fdref id before ev_delete clears it, mirroring the
        // munmap path (`free_region_pages`, region/mod.rs): fdref balance is
        // restored per region regardless of memtype.
        let fdref_id = if let crate::region::VrParam::File { fdref_id: Some(id), .. } = &region.param {
            Some(*id)
        } else {
            None
        };

        // C: map_free → if(region->def_memtype->ev_delete) ev_delete(region)
        // (region.c:578-580). File regions reset inited/fdref_id here.
        if let Some(mt) = region.def_memtype {
            mt.ev_delete(&mut *region);
        }

        // C: map_subfree → pb_unreferenced per mapped page (region.c:527-563).
        for slot in &region.physblocks {
            if let Some(pfn) = slot.pfn() {
                if let Some(mt) = slot.memtype() {
                    mt.ev_unreference(frames, pfn);
                }
                let should_free = if let Some(state) = frames.get_mut(pfn) {
                    if state.refcount > 0 {
                        state.refcount -= 1;
                    }
                    state.refcount == 0 && !state.flags.contains(PageFlags::IN_CACHE)
                } else {
                    false
                };
                if should_free {
                    page_alloc.free_pfn(pfn);
                }
            }
        }

        // C: mappedfile_delete → fdref_deref (mem_file.c:280-287). The
        // VFS_FDCLOSE send is DEFERRED (doc 23) — the pending close is
        // captured locally so the fdref refcount semantics stay correct.
        if let Some(id) = fdref_id
            && let Some(close) = crate::fdref::FdRefTable::get_global().deref_entry(id)
        {
            let _close: crate::fdref::PendingFdClose = close;
        }
    }
}

/// Handle VMPPARAM_CLEAR — clear process memory but keep slot active.
///
/// Corresponds to Minix3's `do_procctl()` case `VMPPARAM_CLEAR` (exit.c:130-137).
/// Called by RS or VFS to release a process's memory and page table, then
/// create a fresh page table. The process slot remains IN_USE.
///
/// C sequence: `free_proc(vmp)` → `pt_new(&vmp->vm_pt)` → `pt_bind(&vmp->vm_pt, vmp)`.
/// Rust equivalent: free physical pages → clear regions → free old page table →
/// init new page table. C's `pt_bind` step has no counterpart call here —
/// see the Step 4 note below.
///
/// # Caller permission
/// Only RS_PROC_NR and VFS_PROC_NR may call this (C: exit.c:131-132).
/// The caller must check `msg.m_source` before calling this function.
pub(crate) fn handle_procctl_clear(
    table: &VmProcTable,
    page_alloc: &mut VmPageAllocator,
    frames: &mut PageFrames,
    endpoint: Endpoint,
) -> Result<(), VmProcctlError> {
    let slot = table.vm_isokendpt(endpoint).map_err(|_| VmProcctlError::InvalidEndpoint)?;

    let mut proc = table.get_active(slot)
        .ok_or(VmProcctlError::ProcessNotFound)?;

    // Step 1: Free physical pages for all regions.
    // C: free_proc(vmp) → map_free_proc(vmp)
    free_process_phys(proc.regions_mut(), frames, page_alloc);

    // Step 2: Clear region map + reset usage stats.
    // C: free_proc → region_init(&vmp->vm_regions_avl) +
    //    vm_region_top = 0 + reset_vm_rusage(vmp) (exit.c:39-43)
    proc.regions_mut().clear();
    proc.set_region_top(minix_types::VirBytes::new(0));
    proc.reset_rusage();

    // Step 3: Free old page table and create a new one.
    // C: pt_free(&vmp->vm_pt); pt_new(&vmp->vm_pt)
    // SAFETY: We just freed all mappings and cleared regions. The page table
    // is not active on any CPU (single-threaded VM event loop guarantees this).
    unsafe { proc.free_page_table(); }
    proc.init_page_table().map_err(|_| VmProcctlError::PageTableError)?;

    // C's sequence ends with `pt_bind(&vmp->vm_pt, vmp)` (exit.c:137), whose
    // substance is the `sys_vmctl_set_addrspace` root notification
    // (pagetable.c:1421) plus i386 pagedir_mappings bookkeeping that Direct
    // Map eliminates. The Rust arch-layer bind helper was a validated no-op
    // and has been removed (07-paging_init_design D8-④); kernel-side root
    // (re-)registration is carried by the VMCTL SetAddrSpace channel.

    Ok(())
}

/// Handle VMPPARAM_HANDLEMEM — resolve memory for a process region.
///
/// Corresponds to Minix3's `do_procctl()` case `VMPPARAM_HANDLEMEM`
/// (exit.c:139-148). Called by VFS to ensure memory pages are mapped
/// for a given address range (wrflag=1 means writable).
///
/// C: `handle_memory_start(vmp, msg->VMPCTL_M1, msg->VMPCTL_LEN,
///     msg->VMPCTL_FLAGS, VFS_PROC_NR, VFS_PROC_NR, transid, 1)`.
///
/// In the current Rust implementation, `handle_memory_once` resolves
/// CoW pages synchronously. File-backed mappings that would SUSPEND
/// in C (waiting for VFS to provide the page) are not yet supported
/// and return `VmProcctlError::NotImplemented`.
///
/// # Caller permission
/// Only VFS_PROC_NR may call this (C: exit.c:140-141).
pub(crate) fn handle_procctl_handlemem(
    table: &VmProcTable,
    page_alloc: &mut VmPageAllocator,
    frames: &mut PageFrames,
    endpoint: Endpoint,
    mem: u64,
    len: i32,
    wrflag: i32,
) -> Result<VmProcctlHandlememResult, VmProcctlError> {
    let slot = table.vm_isokendpt(endpoint).map_err(|_| VmProcctlError::InvalidEndpoint)?;

    let mut proc = table.get_active(slot)
        .ok_or(VmProcctlError::ProcessNotFound)?;

    if len <= 0 {
        return Err(VmProcctlError::InvalidAddress);
    }

    // Design D4 (22-design.v1.md): file-backed HANDLEMEM requires VFS to
    // provide pages — C SUSPENDs and continues via VM_VFS_REPLY
    // (exit.c:144-148). The VFS callback machinery is not wired yet
    // (backlog B1, doc 23) — fail explicitly with NotImplemented instead of
    // silently succeeding. The check covers the region at the range start;
    // mixed anon/file ranges are handled when the full SUSPEND/VM_VFS_REPLY
    // path lands (doc 23).
    if let Some(region) = proc.regions().find(minix_types::VirBytes(mem))
        && matches!(region.param, crate::region::VrParam::File { .. })
    {
        return Err(VmProcctlError::NotImplemented);
    }

    let mem = minix_types::VirBytes(mem);
    let length = minix_types::VirBytes(len as u64);
    let writable = wrflag != 0;

    // Use handle_memory_once to resolve CoW / ensure pages are mapped.
    // C: handle_memory_start → handle_memory_step → map_handle_memory
    let result = crate::fork::handle_memory_once(
        proc.regions_mut(),
        frames,
        page_alloc,
        mem,
        length,
        writable,
    );

    match result {
        Ok(()) => {
            // C returns SUSPEND for the async path, but our synchronous
            // handle_memory_once completes immediately for anonymous memory.
            // File-backed regions were already rejected with NotImplemented
            // above (C would SUSPEND waiting for VFS-provided pages).
            //
            // However, handle_memory_once already succeeds for anonymous pages,
            // which is the common case for VMPPARAM_HANDLEMEM (VFS is asking
            // us to make pages writable for exec, which is always anonymous).
            Ok(VmProcctlHandlememResult::Completed)
        }
        Err(crate::fork::VmForkError::PageNotMapped) => Err(VmProcctlError::PageNotMapped),
        Err(crate::fork::VmForkError::CowAllocFailed) => Err(VmProcctlError::OutOfMemory),
        Err(_) => Err(VmProcctlError::InternalError),
    }
}

/// Result of VMPPARAM_HANDLEMEM.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum VmProcctlHandlememResult {
    /// Memory was resolved synchronously (anonymous pages).
    /// C would return SUSPEND for the async path; we complete immediately.
    Completed,
}

/// Errors from VM_PROCCTL operations.
///
/// Maps to `VmError` via `From<VmProcctlError> for VmError`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum VmProcctlError {
    InvalidEndpoint,
    ProcessNotFound,
    // V10-P2-1 (DEFERRED): no constructor yet — kept for the errno-mapping
    // surface (V10-P2-3) and the CLEAR/HANDLEMEM permission checks.
    #[allow(dead_code)]
    PermissionDenied,
    InvalidAddress,
    PageNotMapped,
    OutOfMemory,
    PageTableError,
    InternalError,
    NotImplemented,
}

impl From<VmProcctlError> for minix_types::VmError {
    fn from(e: VmProcctlError) -> Self {
        match e {
            // C `do_procctl()` collapses both vm_isokendpt failure modes to
            // EINVAL (exit.c:122-125). `InvalidProcess` maps to EINVAL.
            VmProcctlError::InvalidEndpoint => minix_types::VmError::InvalidProcess,
            VmProcctlError::ProcessNotFound => minix_types::VmError::InvalidProcess,
            VmProcctlError::PermissionDenied => minix_types::VmError::PermissionDenied,
            VmProcctlError::InvalidAddress => minix_types::VmError::InvalidAddress,
            VmProcctlError::PageNotMapped => minix_types::VmError::PageNotMapped,
            VmProcctlError::OutOfMemory => minix_types::VmError::OutOfMemory,
            VmProcctlError::PageTableError => minix_types::VmError::PageTableError,
            VmProcctlError::InternalError => minix_types::VmError::InternalError,
            VmProcctlError::NotImplemented => minix_types::VmError::NotImplemented,
        }
    }
}

impl From<EndpointError> for VmProcctlError {
    fn from(_: EndpointError) -> Self {
        VmProcctlError::InvalidEndpoint
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vmproc::VmProcTable;
    use minix_types::{Endpoint, UserSlot};
    use crate::phys_mem::{BitmapAllocator, PhysAlloc};
    use crate::region::PageFrames;
    use minix_types::PhysBytes;
    use crate::region::PAGE_SIZE as REGION_PAGE_SIZE;

    fn init_test_process(slot: UserSlot) -> Endpoint {
        let table = VmProcTable::get_global();
        // SAFETY: test-only cleanup. Single-threaded, no concurrent access.
        unsafe {
            table.reset_slot(slot);
        }
        let empty = table.get_empty(slot).unwrap();
        let ep = Endpoint::from_generation_slot(1, slot.get() as i32);
        let mut active = empty.activate(ep);
        // Skip init_page_table() — exit tests don't need page table access,
        // and init_page_table() accesses mock physical memory causing SIGSEGV.
        active.init_regions();
        ep
    }

    fn make_frames() -> PageFrames {
        PageFrames::new(PhysBytes(256 * REGION_PAGE_SIZE as u64))
    }

    fn make_page_alloc() -> VmPageAllocator {
        VmPageAllocator::new(PhysAlloc::Bitmap(BitmapAllocator::new_for_test(256)))
    }

    #[test]
    fn test_exit_error_to_errno() {
        use minix_types::{VmError, EINVAL};
        assert_eq!(VmError::from(VmExitError::ProcessNotFound).to_errno(), EINVAL);
        assert_eq!(VmError::from(VmExitError::NotExiting).to_errno(), EINVAL);
    }

    #[test]
    fn test_procctl_error_to_errno() {
        use minix_types::{VmError, EINVAL, EPERM, EFAULT};
        // C do_procctl collapses vm_isokendpt failures to EINVAL
        // (exit.c:122-125).
        assert_eq!(VmError::from(VmProcctlError::InvalidEndpoint).to_errno(), EINVAL);
        assert_eq!(VmError::from(VmProcctlError::ProcessNotFound).to_errno(), EINVAL);
        // C: exit.c:131-132/:141-142 — unauthorized callers → EPERM.
        assert_eq!(VmError::from(VmProcctlError::PermissionDenied).to_errno(), EPERM);
        // HANDLEMEM guard: len<=0 → InvalidAddress (EFAULT).
        assert_eq!(VmError::from(VmProcctlError::InvalidAddress).to_errno(), EFAULT);
    }

    #[test]
    fn test_procctl_handlemem_rejects_non_positive_len() {
        let table = VmProcTable::get_global();
        let mut page_alloc = make_page_alloc();
        let mut frames = make_frames();
        let slot = UserSlot::new(53);
        let ep = init_test_process(slot);

        // len <= 0 is rejected before any region lookup (defensive
        // hardening — C would SUSPEND and reply OK for len==0 via the
        // empty handle_memory_step loop; see 22-vm-exit.md §3 D5).
        let result = handle_procctl_handlemem(
            table, &mut page_alloc, &mut frames, ep, 0x1000, 0, 1,
        );
        assert!(matches!(result, Err(VmProcctlError::InvalidAddress)));
    }

    #[test]
    fn test_procctl_handlemem_process_not_found() {
        let table = VmProcTable::get_global();
        let mut page_alloc = make_page_alloc();
        let mut frames = make_frames();

        let result = handle_procctl_handlemem(
            table, &mut page_alloc, &mut frames, Endpoint::NONE, 0x1000, 16, 1,
        );
        assert!(matches!(result, Err(VmProcctlError::InvalidEndpoint)));
    }

    #[test]
    fn test_procctl_clear_process_not_found() {
        let table = VmProcTable::get_global();
        let mut page_alloc = make_page_alloc();
        let mut frames = make_frames();

        // Invalid endpoint → EINVAL (C exit.c:122-125). Does not reach the
        // page-table path (which needs real paging, B4 backlog).
        let result = handle_procctl_clear(
            table, &mut page_alloc, &mut frames, Endpoint::NONE,
        );
        assert!(matches!(result, Err(VmProcctlError::InvalidEndpoint)));
    }

    #[test]
    fn test_procctl_handlemem_file_backed_not_implemented() {
        let table = VmProcTable::get_global();
        let mut page_alloc = make_page_alloc();
        let mut frames = make_frames();
        let slot = UserSlot::new(54);
        let ep = init_test_process(slot);

        // Insert a file-backed region (VrParam::File) at 0x1000.
        let mut region = crate::region::VirRegion::new(
            minix_types::VirBytes(0x1000),
            minix_types::VirBytes(0x1000),
            crate::region::VrFlags::WRITABLE,
        );
        region.param = crate::region::VrParam::File {
            inited: false,
            fdref_id: None,
            offset: 0,
            clearend: 0,
        };
        let mut active = table.get_active(slot).unwrap();
        active.regions_mut().insert(region).unwrap();

        // File-backed HANDLEMEM needs VFS-provided pages (C: SUSPEND +
        // VM_VFS_REPLY, exit.c:144-148); not wired yet → NotImplemented
        // (ENOSYS). Design D4 (22-design.v1.md).
        let result = handle_procctl_handlemem(
            table, &mut page_alloc, &mut frames, ep, 0x1000, 16, 1,
        );
        assert!(matches!(result, Err(VmProcctlError::NotImplemented)));
    }

    #[test]
    fn test_exit_process_not_found() {
        let table = VmProcTable::get_global();
        let mut page_alloc = make_page_alloc();
        let mut frames = make_frames();

        let result = handle_vm_exit(table, &mut page_alloc, &mut frames, Endpoint::NONE);
        assert!(matches!(result, Err(VmExitError::ProcessNotFound)));
    }

    #[test]
    fn test_exit_without_willexit_fails() {
        let table = VmProcTable::get_global();
        let mut page_alloc = make_page_alloc();
        let mut frames = make_frames();
        let slot = UserSlot::new(50);
        let ep = init_test_process(slot);

        let result = handle_vm_exit(table, &mut page_alloc, &mut frames, ep);
        assert!(matches!(result, Err(VmExitError::NotExiting)));
    }

    #[test]
    fn test_willexit_then_exit_succeeds() {
        let table = VmProcTable::get_global();
        let mut page_alloc = make_page_alloc();
        let mut frames = make_frames();
        let slot = UserSlot::new(51);
        let ep = init_test_process(slot);

        handle_vm_willexit(table, ep).unwrap();
        let result = handle_vm_exit(table, &mut page_alloc, &mut frames, ep);
        assert!(result.is_ok());
    }

    #[test]
    fn test_exit_slot_reusable() {
        let table = VmProcTable::get_global();
        let mut page_alloc = make_page_alloc();
        let mut frames = make_frames();
        let slot = UserSlot::new(52);
        let ep = init_test_process(slot);

        handle_vm_willexit(table, ep).unwrap();
        handle_vm_exit(table, &mut page_alloc, &mut frames, ep).unwrap();

        let empty = table.get_empty(slot);
        assert!(empty.is_some(), "slot should be empty after exit");
    }
}
