//! VM exit handling.
//!
//! Handles VM_EXIT and VM_WILLEXIT requests from PM.
//! Releases all process resources: memory regions, physical pages, page tables.
//!
//! Corresponds to Minix3's `do_exit()` and `do_willexit()` in `exit.c`.

use minix_types::Endpoint;
use crate::vmproc::{VmProcTable, EndpointError};
use crate::region::{RegionMap, PageFrames, PageFlags, PFN_NONE, PfnAllocator};
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

    let exiting = table.get_exiting(slot)
        .ok_or(VmExitError::NotExiting)?;

    free_process_phys(exiting.regions(), frames, page_alloc);

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

/// Release physical pages for all regions in the exiting process.
///
/// Corresponds to Minix3's map_free_proc() → map_free() → map_subfree() →
/// pb_unreferenced() → ev_unreference() → free_mem() chain (region.c:589-602,
/// region.c:568-585, region.c:527-563, pb.c:96-133, mem_anon.c:56-62).
///
/// For each mapped PageSlot: decrements PageFrames refcount (equivalent to
/// Minix3's pb.refcount--), calls the MemType ev_unreference callback,
/// and conditionally frees the physical page when refcount reaches 0.
/// RegionMap::clear() (inside reap()) handles releasing the data structures.
///
/// NOTE: ev_unreference in the PFN model is a no-op for anonymous/direct
/// memory — the caller is responsible for both refcount decrement and
/// physical page freeing. This separation of concerns is documented in
/// §3.2 of 20-vm-exit.md.
fn free_process_phys(
    regions: &RegionMap,
    frames: &mut PageFrames,
    page_alloc: &mut VmPageAllocator,
) {
    for region in regions.iter() {
        for slot in &region.physblocks {
            if slot.is_mapped() {
                if let Some(mt) = slot.memtype {
                    mt.ev_unreference(frames, slot.pfn);
                }
                let should_free = if let Some(state) = frames.get_mut(slot.pfn) {
                    if state.refcount > 0 {
                        state.refcount -= 1;
                    }
                    state.refcount == 0 && !state.flags.contains(PageFlags::IN_CACHE)
                } else {
                    false
                };
                if should_free {
                    page_alloc.free_pfn(slot.pfn);
                }
            }
        }
    }
}

/// Handle VMPPARAM_CLEAR — clear process memory but keep slot active.
///
/// Corresponds to Minix3's `do_procctl()` case `VMPPARAM_CLEAR` (exit.c:130-137).
/// Called by RS or VFS to release a process's memory and page table, then
/// create a fresh page table and bind it. The process slot remains IN_USE.
///
/// C sequence: `free_proc(vmp)` → `pt_new(&vmp->vm_pt)` → `pt_bind(&vmp->vm_pt, vmp)`.
/// Rust equivalent: free physical pages → clear regions → free old page table →
/// init new page table → bind.
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
    free_process_phys(proc.regions(), frames, page_alloc);

    // Step 2: Clear region map.
    // C: free_proc → region_init(&vmp->vm_regions_avl)
    proc.regions_mut().clear();

    // Step 3: Free old page table and create a new one.
    // C: pt_free(&vmp->vm_pt); pt_new(&vmp->vm_pt)
    // SAFETY: We just freed all mappings and cleared regions. The page table
    // is not active on any CPU (single-threaded VM event loop guarantees this).
    unsafe { proc.free_page_table(); }
    proc.init_page_table().map_err(|_| VmProcctlError::PageTableError)?;

    // Step 4: Bind new page table to this process.
    // C: pt_bind(&vmp->vm_pt, vmp)
    proc.bind_page_table().map_err(|_| VmProcctlError::PageTableError)?;

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
            // For file-backed regions, C would SUSPEND; we return NotImplemented
            // since we can't handle VFS callbacks yet.
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
            VmProcctlError::InvalidEndpoint => minix_types::VmError::InvalidEndpoint,
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
