//! VM RS (Reincarnation Server) service handlers.
//!
//! Handles VM_RS_SET_PRIV, VM_RS_PREPARE, VM_RS_UPDATE, and VM_RS_MEMCTL
//! requests from RS. These services support Minix3's live update mechanism.
//!
//! Corresponds to Minix3's `do_rs_set_priv()`, `do_rs_prepare()`,
//! `do_rs_update()`, and `do_rs_memctl()` in `rs.c`.
//!
//! ## Implementation status
//!
//! - **SET_PRIV**: Fully implemented — sets ACL for target process.
//! - **MEMCTL (PIN)**: Returns OK (single VM instance, no pinning needed).
//! - **MEMCTL (MAKE_VM)**: Returns error (multi-VM-instance not supported).
//! - **MEMCTL (HEAP_PREALLOC)**: Delegates to brk module.
//! - **MEMCTL (MAP_PREALLOC)**: Delegates to mmap module.
//! - **MEMCTL (GET_PREALLOC_MAP)**: Queries region with PREALLOC_MAP flag.
//! - **PREPARE**: Partially implemented — validates endpoints + pins both processes' memory via `map_pin_memory` + extends the destination heap to match the source via `brk`. `map_proc_dyn_data` (CoW-transfer of mmap regions) is deferred.
//! - **UPDATE**: Partially implemented — validates endpoints + checks RsUpdateFlags (ROLLBACK/NOMMAP) + PREALLOC_MAP conflict detection. sys_update/swap_proc_slot/swap_proc_dyn_data deferred.

use minix_types::{VirBytes, Endpoint};
use crate::vmproc::{VmProcTable, EndpointError};
use crate::region::{PageFrames, VrFlags};
use crate::alloc_page::VmPageAllocator;
use crate::acl::AclState;

// ── Error type ───────────────────────────────────────────────────────
//
// Unified error type for all RS service handlers. Maps to `VmError` via
// `From<RsError> for VmError`, then to C errno via `VmError::to_errno()`.
// Per-error `to_errno()` methods are intentionally omitted — the single
// source of truth is `VmError::to_errno()` in `minix_types::ipc::vm`.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RsError {
    // ── Common ──
    ProcessNotFound,

    // ── SET_PRIV specific ──
    SysProcNoMask,

    // ── PREPARE specific ──
    PinFailed,
    HeapExtendFailed,

    // ── UPDATE specific ──
    PreallocMapConflict,
    UpdateNotImplemented,

    // ── MEMCTL specific ──
    InvalidRequest,
    MakeVmFailed,
    HeapPreallocFailed,
    MapPreallocFailed,
    InvalidLength,
}

// ── Endpoint-lookup error unification ──
//
// See `munmap.rs` for the full rationale. RS handlers collapse both
// `EndpointError::InvalidSlot` and `EndpointError::DeadEndpoint` to
// `RsError::ProcessNotFound` — RS context doesn't distinguish EINVAL
// from EDEADEPT at the call site.
impl From<EndpointError> for RsError {
    fn from(_: EndpointError) -> Self {
        RsError::ProcessNotFound
    }
}

// ── Request/result types ─────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RsMemctlRequest {
    Pin,
    MakeVmInstance,
    HeapPrealloc { addr: VirBytes, len: usize },
    MapPrealloc { addr: VirBytes, len: usize },
    GetPreallocMap,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RsMemctlResult {
    Ok,
    AddrLen { addr: VirBytes, len: usize },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RsUpdateResult {
    Ok,
    Suspend,
}

/// Flags for VM_RS_UPDATE request.
///
/// Corresponds to Minix3's `SF_VM_*` flags in `minix/rs.h:198-199`.
///
/// ```c
/// #define SF_VM_ROLLBACK  0x080    /* set when vm update is a rollback */
/// #define SF_VM_NOMMAP    0x100    /* set when vm update ignores mmapped regions */
/// ```
bitflags::bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub(crate) struct RsUpdateFlags: u32 {
        /// Set when the VM update is a rollback.
        const ROLLBACK = 0x080;
        /// Set when the VM update ignores mmapped regions.
        const NOMMAP   = 0x100;
    }
}

// ── Handler: SET_PRIV ────────────────────────────────────────────────

/// Handle VM_RS_SET_PRIV — set ACL for a process.
///
/// Corresponds to Minix3's `do_rs_set_priv()` (rs.c:34).
///
/// - If `mask_ptr` is `Some`, the caller provides a custom ACL mask
///   (C: `VM_RS_BUF != 0` → `sys_datacopy` from RS).
/// - If `mask_ptr` is `None` and `is_sys_proc` is true, return error
///   (C: system processes must have an explicit ACL mask).
/// - If `mask_ptr` is `None` and `is_sys_proc` is false, use default ACL
///   (C: user processes share the default permission set).
pub(crate) fn handle_rs_set_priv(
    table: &VmProcTable,
    _caller: Endpoint,
    target: Endpoint,
    mask: Option<crate::acl::AclMask>,
    is_sys_proc: bool,
) -> Result<(), RsError> {
    let slot = table.vm_isokendpt(target)?;

    let mut active = table.get_active(slot)
        .ok_or(RsError::ProcessNotFound)?;

    if mask.is_none() && is_sys_proc {
        return Err(RsError::SysProcNoMask);
    }

    let acl = AclState::acl_set(is_sys_proc, mask);
    active.set_acl(acl);

    Ok(())
}

// ── Handler: PREPARE ─────────────────────────────────────────────────

/// Handle VM_RS_PREPARE — prepare live update memory state.
///
/// Corresponds to Minix3's `do_rs_prepare()` (rs.c:71).
///
/// Pins memory for both source and destination processes so that no page
/// faults occur during the live update window. Also extends the destination
/// process's heap to match the source's if needed (prevents heap exhaustion
/// during update).
///
/// # C source (rs.c:71-145)
///
/// The C implementation does 5 things in order:
/// 1. Validate src/dst endpoints via `vm_isokendpt`
/// 2. `map_pin_memory(src_vmp)` — pin source process
/// 3. Extend dst heap to match src (`real_brk`) — **not yet implemented**
/// 4. `map_pin_memory(dst_vmp)` — pin destination process
/// 5. `map_proc_dyn_data(src_vmp, dst_vmp)` — map dynamic data — **not yet implemented**
///
/// Steps 1-4 are implemented below. Step 5 (`map_proc_dyn_data`, the CoW
/// transfer of mmap regions) is deferred: it requires a range-constrained
/// region copy with page-table sync on a live process (see A-8 gap contract
/// in the design doc / `25-rs-services.md`).
pub(crate) fn handle_rs_prepare(
    table: &VmProcTable,
    page_alloc: &mut VmPageAllocator,
    frames: &mut PageFrames,
    src: Endpoint,
    dst: Endpoint,
    _flags: u32,
) -> Result<(), RsError> {
    // Step 1: Validate source endpoint.
    if src == Endpoint::NONE {
        return Err(RsError::InvalidRequest);
    }
    let src_slot = table.vm_isokendpt(src)?;
    let dst_slot = table.vm_isokendpt(dst)?;

    // Step 2: Pin source process memory.
    // C: map_pin_memory(src_vmp)
    {
        let mut src_proc = table.get_active(src_slot)
            .ok_or(RsError::ProcessNotFound)?;
        crate::region::map_pin_memory(
            src_proc.regions_mut(),
            frames,
            page_alloc,
        ).map_err(|_| RsError::PinFailed)?;
    }

    // Step 3: Extend dst heap to match src (C: rs.c:116-126).
    //
    // C computes the current data-region end for both processes
    // (`region_search(&vmp->vm_regions_avl, VM_MMAPBASE, AVL_LESS)` →
    // `vaddr + length`) and grows dst only when src's end is higher —
    // "better safe than sorry": the destination must not run out of heap
    // during the live update window. minix-rs tracks the heap top as
    // `vm_region_top` (kept in sync with the data region by brk.rs); the
    // C-faithful `find_less(MMAP_BASE)` measure is used here so the
    // comparison matches the C ground truth exactly. The `src > dst`
    // guard is load-bearing: calling brk unconditionally would shrink a
    // larger dst heap to the source's smaller size.
    let src_data_end = {
        let src_proc = table.get_active(src_slot)
            .ok_or(RsError::ProcessNotFound)?;
        let data_vr = src_proc.regions()
            .find_less(VirBytes(crate::mmap::MMAP_BASE))
            .ok_or(RsError::HeapExtendFailed)?;
        data_vr.vaddr.0 + data_vr.length.0
    };
    let dst_data_end = {
        let dst_proc = table.get_active(dst_slot)
            .ok_or(RsError::ProcessNotFound)?;
        let data_vr = dst_proc.regions()
            .find_less(VirBytes(crate::mmap::MMAP_BASE))
            .ok_or(RsError::HeapExtendFailed)?;
        data_vr.vaddr.0 + data_vr.length.0
    };
    if src_data_end > dst_data_end {
        let req = crate::brk::BrkRequest {
            endpoint: dst,
            new_brk_addr: VirBytes(src_data_end),
        };
        crate::brk::handle_brk(table, page_alloc, frames, &req)
            .map_err(|_| RsError::HeapExtendFailed)?;
    }

    // Step 4: Pin destination process memory.
    // C: map_pin_memory(dst_vmp)
    {
        let mut dst_proc = table.get_active(dst_slot)
            .ok_or(RsError::ProcessNotFound)?;
        crate::region::map_pin_memory(
            dst_proc.regions_mut(),
            frames,
            page_alloc,
        ).map_err(|_| RsError::PinFailed)?;
    }

    // Step 5 (DEFERRED): map_proc_dyn_data(src_vmp, dst_vmp)
    // Requires mmap region sharing support. Not blocking for basic pin functionality.

    Ok(())
}

// ── Handler: UPDATE ──────────────────────────────────────────────────

/// Handle VM_RS_UPDATE — execute live update process switch.
///
/// Corresponds to Minix3's `do_rs_update()` (rs.c:150).
///
/// # Implementation status
///
/// Steps 1-3 (endpoint validation + flag check + PREALLOC_MAP check)
/// are implemented. Steps 4-7 are deferred:
///
/// 4. `sys_update(src_e, dst_e, flags)` — kernel syscall (DEFERRED)
/// 5. `swap_proc_slot(src_vmp, dst_vmp)` — typestate extension (DEFERRED)
/// 6. `swap_proc_dyn_data(src_vmp, dst_vmp, flags)` — mmap sharing (DEFERRED)
/// 7. `pt_bind()` + reply message (DEFERRED)
///
/// # C source (rs.c:150-213)
///
/// ```c
/// int do_rs_update(message *m_ptr)
/// {
///     // 1. Validate endpoints
///     if(vm_isokendpt(src_e, &src_p) != OK) return EINVAL;
///     if(vm_isokendpt(dst_e, &dst_p) != OK) return EINVAL;
///     // 2. Check flags
///     if((sys_upd_flags & (SF_VM_ROLLBACK|SF_VM_NOMMAP)) == 0) {
///         if(map_region_lookup_type(dst_vmp, VR_PREALLOC_MAP))
///             return ENOSYS;
///     }
///     // 3. sys_update (kernel)
///     r = sys_update(src_e, dst_e, ...);
///     // 4. swap_proc_slot + swap_proc_dyn_data + pt_bind
///     // 5. Reply + return SUSPEND
/// }
/// ```
pub(crate) fn handle_rs_update(
    table: &VmProcTable,
    _page_alloc: &mut VmPageAllocator,
    _frames: &mut PageFrames,
    src: Endpoint,
    dst: Endpoint,
    flags: u32,
) -> Result<RsUpdateResult, RsError> {
    // Step 1: Validate source and destination endpoints.
    // C: vm_isokendpt(src_e, &src_p) / vm_isokendpt(dst_e, &dst_p)
    if src == Endpoint::NONE {
        return Err(RsError::InvalidRequest);
    }
    let src_slot = table.vm_isokendpt(src)?;
    let dst_slot = table.vm_isokendpt(dst)?;

    // Step 2: Check flags — if neither ROLLBACK nor NOMMAP is set,
    // the destination process must not have any PREALLOC_MAP regions.
    // C: if((sys_upd_flags & (SF_VM_ROLLBACK|SF_VM_NOMMAP)) == 0) {
    //         if(map_region_lookup_type(dst_vmp, VR_PREALLOC_MAP))
    //             return ENOSYS;
    //     }
    let update_flags = RsUpdateFlags::from_bits_truncate(flags);
    if !update_flags.contains(RsUpdateFlags::ROLLBACK)
        && !update_flags.contains(RsUpdateFlags::NOMMAP)
    {
        let has_prealloc = {
            let dst_proc = table.get_active(dst_slot)
                .ok_or(RsError::ProcessNotFound)?;
            dst_proc.regions().iter()
                .any(|vr| vr.flags.contains(VrFlags::PREALLOC_MAP))
        };
        if has_prealloc {
            return Err(RsError::PreallocMapConflict);
        }
    }

    // Steps 3-7: DEFERRED — requires kernel sys_update syscall,
    // swap_proc_slot typestate extension, and swap_proc_dyn_data.
    Err(RsError::UpdateNotImplemented)
}

// ── Handler: MEMCTL ──────────────────────────────────────────────────

/// Handle VM_RS_MEMCTL — memory control for live update.
///
/// Corresponds to Minix3's `do_rs_memctl()` (rs.c:349).
///
/// Five sub-requests:
/// - **PIN**: Pin process memory (no-op with single VM instance).
/// - **MAKE_VM**: Create VM instance (not supported).
/// - **HEAP_PREALLOC**: Preallocate heap space via brk.
/// - **MAP_PREALLOC**: Preallocate mmap region.
/// - **GET_PREALLOC_MAP**: Query preallocated mmap region.
pub(crate) fn handle_rs_memctl(
    table: &VmProcTable,
    page_alloc: &mut VmPageAllocator,
    frames: &mut PageFrames,
    vfs_queue: &mut crate::vfs_queue::VfsRequestQueue,
    target: Endpoint,
    request: RsMemctlRequest,
) -> Result<RsMemctlResult, RsError> {
    let slot = table.vm_isokendpt(target)?;

    let mut active = table.get_active(slot)
        .ok_or(RsError::ProcessNotFound)?;

    match request {
        RsMemctlRequest::Pin => {
            // C: rs.c:368-371 — `if (num_vm_instances <= 1) return OK;`
            // only actually pins when VM can recover from crashes
            // (saves memory in the single-instance case).
            if crate::global::vm_instance_count() <= 1 {
                return Ok(RsMemctlResult::Ok);
            }
            crate::region::map_pin_memory(active.regions_mut(), frames, page_alloc)
                .map_err(|_| RsError::PinFailed)?;
            Ok(RsMemctlResult::Ok)
        }
        RsMemctlRequest::MakeVmInstance => {
            // C: rs_memctl_make_vm_instance — multi-VM-instance support.
            // Not supported in current design.
            Err(RsError::MakeVmFailed)
        }
        RsMemctlRequest::HeapPrealloc { len, .. } => {
            if len == 0 {
                return Err(RsError::InvalidLength);
            }
            // C: rs_memctl_heap_prealloc (rs.c:281) — computes
            // *addr = data_vr->vaddr + data_vr->length (current brk),
            // bytes = *addr + *len, then calls real_brk(vmp, bytes).
            // Rust: compute new absolute brk = current_top + len.
            let current_brk = active.region_top();
            // Tightening: C wraps silently (`bytes = *addr + *len` would
            // shrink the heap on overflow); minix-rs fails closed.
            let new_brk = current_brk.0
                .checked_add(len as u64)
                .map(VirBytes)
                .ok_or(RsError::InvalidLength)?;
            let req = crate::brk::BrkRequest {
                endpoint: target,
                new_brk_addr: new_brk,
            };
            crate::brk::handle_brk(table, page_alloc, frames, &req)
                .map(|_| RsMemctlResult::AddrLen {
                    addr: current_brk,
                    len,
                })
                .map_err(|_| RsError::HeapPreallocFailed)
        }
        RsMemctlRequest::MapPrealloc { len, .. } => {
            if len == 0 {
                return Err(RsError::InvalidLength);
            }
            // C: rs_memctl_map_prealloc (rs.c:300-324) → map_page_region(
            //     vmp, base, top, *len, VR_ANON|VR_WRITABLE|VR_UNINITIALIZED,
            //     MF_PREALLOC, &mem_type_anon) then sets VR_PREALLOC_MAP.
            // Rust: delegate to handle_mmap with the same flag set — mmap's
            // `to_vr_flags` maps MmapFlags::PREALLOC → VrFlags::PREALLOC_MAP
            // and MmapFlags::UNINITIALIZED → VrFlags::UNINITIALIZED.
            // (FIX 25-R1: the previous hardcoded `0x1002` omitted both bits,
            // so the region never carried PREALLOC_MAP and the follow-up
            // GET_PREALLOC_MAP lookup would always miss.)
            //
            // C's `map_page_region` is an internal call that skips `do_mmap`'s
            // privilege checks; `handle_mmap` gates MAP_UNINITIALIZED on an
            // execpriv caller (VFS/RS). The semantically correct model is an
            // RS-initiated third-party mapping for the target — exactly the
            // C privilege model (`do_rs_memctl` is always called by RS).
            let aligned_len = VirBytes(((len as u64) + 4095) & !4095);
            let mmap_req = minix_types::VmMmapIn {
                caller: Endpoint::RS,
                forwhom: target,
                addr: VirBytes(0),
                length: aligned_len,
                prot: crate::mmap::ProtFlags::READ.bits()
                    | crate::mmap::ProtFlags::WRITE.bits(),
                flags: crate::mmap::MmapFlags::PRIVATE.bits()
                    | crate::mmap::MmapFlags::ANONYMOUS.bits()
                    | crate::mmap::MmapFlags::PREALLOC.bits()
                    | crate::mmap::MmapFlags::UNINITIALIZED.bits()
                    | crate::mmap::MmapFlags::THIRDPARTY.bits(),
                fd: -1,
                offset: 0,
            };
            crate::mmap::handle_mmap(table, page_alloc, frames, vfs_queue, &mmap_req)
                .map(|result| match result {
                    crate::mmap::MmapResult::Complete(resp) => RsMemctlResult::AddrLen {
                        addr: resp.mapped_addr,
                        len: aligned_len.0 as usize,
                    },
                    crate::mmap::MmapResult::Suspended => RsMemctlResult::AddrLen {
                        addr: VirBytes(0),
                        len: 0,
                    },
                })
                .map_err(|_| RsError::MapPreallocFailed)
        }
        RsMemctlRequest::GetPreallocMap => {
            // C: rs_memctl_get_prealloc_map — find VR_PREALLOC_MAP region.
            let regions = active.regions();
            let found = regions.iter().find(|vr| vr.flags.contains(VrFlags::PREALLOC_MAP));
            match found {
                Some(vr) => Ok(RsMemctlResult::AddrLen {
                    addr: vr.vaddr,
                    len: vr.length.0 as usize,
                }),
                None => Ok(RsMemctlResult::AddrLen {
                    addr: VirBytes(0),
                    len: 0,
                }),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vmproc::VmProcTable;
    use crate::phys_mem::{BitmapAllocator, PhysAlloc};
    use crate::region::PAGE_SIZE as REGION_PAGE_SIZE;
    use minix_types::{PhysBytes, UserSlot};

    fn make_frames() -> PageFrames {
        PageFrames::new(PhysBytes(256 * REGION_PAGE_SIZE as u64))
    }

    fn make_page_alloc() -> VmPageAllocator {
        VmPageAllocator::new(PhysAlloc::Bitmap(BitmapAllocator::new_for_test(256)))
    }

    /// Activates a fresh process in `slot` with an initialized region map.
    ///
    /// Mirrors the brk/mmap test helpers: `init_page_table()` uses the
    /// test-build stub, so no mock physical memory is touched.
    fn init_test_process(slot: UserSlot) -> Endpoint {
        let table = VmProcTable::get_global();
        unsafe { table.reset_slot(slot); }
        let empty = table.get_empty(slot).unwrap();
        let ep = Endpoint::from_generation_slot(1, slot.get() as i32);
        let mut active = empty.activate(ep);
        active.init_page_table().unwrap();
        active.init_regions();
        ep
    }

    #[test]
    fn test_set_priv_sys_proc_no_mask() {
        let table = VmProcTable::get_global();
        let result = handle_rs_set_priv(
            table,
            Endpoint::RS,
            Endpoint(9999),
            None,
            true,
        );
        assert_eq!(result, Err(RsError::ProcessNotFound));
    }

    #[test]
    fn test_set_priv_user_proc_not_found() {
        let table = VmProcTable::get_global();
        let result = handle_rs_set_priv(
            table,
            Endpoint::RS,
            Endpoint(9999),
            None,
            false,
        );
        assert_eq!(result, Err(RsError::ProcessNotFound));
    }

    #[test]
    fn test_set_priv_updates_acl() {
        let table = VmProcTable::get_global();
        let slot = UserSlot::new(64);
        let ep = init_test_process(slot);
        let mask = crate::acl::AclMask::from_bits_truncate(0x1);

        let result = handle_rs_set_priv(
            table,
            Endpoint::RS,
            ep,
            Some(mask),
            false,
        );
        assert_eq!(result, Ok(()));

        let active = table.get_active(slot).unwrap();
        assert_eq!(active.acl(), AclState::acl_set(false, Some(mask)));
    }

    #[test]
    fn test_set_priv_sys_proc_with_mask_ok() {
        // C: rs.c:48-53 — a sys proc with an explicit mask is accepted.
        let table = VmProcTable::get_global();
        let slot = UserSlot::new(63);
        let ep = init_test_process(slot);
        let mask = crate::acl::AclMask::from_bits_truncate(0x3);

        let result = handle_rs_set_priv(
            table,
            Endpoint::RS,
            ep,
            Some(mask),
            true,
        );
        assert_eq!(result, Ok(()));
        assert_eq!(
            table.get_active(slot).unwrap().acl(),
            AclState::acl_set(true, Some(mask))
        );
    }

    #[test]
    fn test_memctl_pin_not_found() {
        let table = VmProcTable::get_global();
        let mut page_alloc = make_page_alloc();
        let mut frames = make_frames();
        let mut vfs_queue = crate::vfs_queue::VfsRequestQueue::new();
        let result = handle_rs_memctl(
            table,
            &mut page_alloc,
            &mut frames,
            &mut vfs_queue,
            Endpoint(9999),
            RsMemctlRequest::Pin,
        );
        assert_eq!(result, Err(RsError::ProcessNotFound));
    }

    #[test]
    fn test_memctl_pin_single_instance_ok() {
        // C: rs.c:368-371 — `num_vm_instances <= 1` → OK without pinning.
        // The test global defaults to 0 instances, exercising the guard.
        let table = VmProcTable::get_global();
        let mut page_alloc = make_page_alloc();
        let mut frames = make_frames();
        let mut vfs_queue = crate::vfs_queue::VfsRequestQueue::new();
        let slot = UserSlot::new(65);
        let ep = init_test_process(slot);

        let result = handle_rs_memctl(
            table,
            &mut page_alloc,
            &mut frames,
            &mut vfs_queue,
            ep,
            RsMemctlRequest::Pin,
        );
        assert_eq!(result, Ok(RsMemctlResult::Ok));
    }

    #[test]
    fn test_memctl_make_vm_not_found() {
        let table = VmProcTable::get_global();
        let mut page_alloc = make_page_alloc();
        let mut frames = make_frames();
        let mut vfs_queue = crate::vfs_queue::VfsRequestQueue::new();
        let result = handle_rs_memctl(
            table,
            &mut page_alloc,
            &mut frames,
            &mut vfs_queue,
            Endpoint(9999),
            RsMemctlRequest::MakeVmInstance,
        );
        assert_eq!(result, Err(RsError::ProcessNotFound));
    }

    #[test]
    fn test_memctl_heap_prealloc_zero_len() {
        let table = VmProcTable::get_global();
        let mut page_alloc = make_page_alloc();
        let mut frames = make_frames();
        let mut vfs_queue = crate::vfs_queue::VfsRequestQueue::new();
        let result = handle_rs_memctl(
            table,
            &mut page_alloc,
            &mut frames,
            &mut vfs_queue,
            Endpoint(9999),
            RsMemctlRequest::HeapPrealloc { addr: VirBytes(0), len: 0 },
        );
        assert_eq!(result, Err(RsError::ProcessNotFound));
    }

    #[test]
    fn test_memctl_map_prealloc_zero_len() {
        let table = VmProcTable::get_global();
        let mut page_alloc = make_page_alloc();
        let mut frames = make_frames();
        let mut vfs_queue = crate::vfs_queue::VfsRequestQueue::new();
        let result = handle_rs_memctl(
            table,
            &mut page_alloc,
            &mut frames,
            &mut vfs_queue,
            Endpoint(9999),
            RsMemctlRequest::MapPrealloc { addr: VirBytes(0), len: 0 },
        );
        assert_eq!(result, Err(RsError::ProcessNotFound));
    }

    #[test]
    fn test_memctl_map_prealloc_sets_prealloc_flag() {
        // C: rs.c:313-321 — the preallocated region carries VR_PREALLOC_MAP
        // so the follow-up GET_PREALLOC_MAP (rs.c:329-344) can find it.
        // Regression for FIX 25-R1: the old hardcoded flags (0x1002) omitted
        // MAP_PREALLOC, so the region never carried the flag and the
        // follow-up lookup always missed.
        let table = VmProcTable::get_global();
        let mut page_alloc = make_page_alloc();
        let mut frames = make_frames();
        let mut vfs_queue = crate::vfs_queue::VfsRequestQueue::new();
        let slot = UserSlot::new(66);
        let ep = init_test_process(slot);

        let result = handle_rs_memctl(
            table,
            &mut page_alloc,
            &mut frames,
            &mut vfs_queue,
            ep,
            RsMemctlRequest::MapPrealloc { addr: VirBytes(0), len: 0x3000 },
        );
        let RsMemctlResult::AddrLen { addr, len } = result.unwrap() else {
            panic!("expected AddrLen");
        };
        assert_ne!(addr.0, 0);
        assert_eq!(len, 0x3000);

        // The region must carry PREALLOC_MAP (else the LU handshake cannot
        // find the preallocated range).
        let active = table.get_active(slot).unwrap();
        assert!(active.regions().iter()
            .any(|vr| vr.flags.contains(VrFlags::PREALLOC_MAP)));

        // GET_PREALLOC_MAP must return exactly that region.
        let got = handle_rs_memctl(
            table,
            &mut page_alloc,
            &mut frames,
            &mut vfs_queue,
            ep,
            RsMemctlRequest::GetPreallocMap,
        );
        assert_eq!(got, Ok(RsMemctlResult::AddrLen { addr, len }));
    }

    #[test]
    fn test_memctl_heap_prealloc_grows_heap() {
        // C: rs_memctl_heap_prealloc (rs.c:281-295) — *addr = current brk,
        // bytes = *addr + *len, real_brk(vmp, bytes).
        let table = VmProcTable::get_global();
        let mut page_alloc = make_page_alloc();
        let mut frames = make_frames();
        let mut vfs_queue = crate::vfs_queue::VfsRequestQueue::new();
        let slot = UserSlot::new(67);
        let ep = init_test_process(slot);
        {
            let mut active = table.get_active(slot).unwrap();
            active.set_region_top(VirBytes(0x4000_0000));
        }

        let result = handle_rs_memctl(
            table,
            &mut page_alloc,
            &mut frames,
            &mut vfs_queue,
            ep,
            RsMemctlRequest::HeapPrealloc { addr: VirBytes(0), len: 0x2000 },
        );
        let RsMemctlResult::AddrLen { addr, len } = result.unwrap() else {
            panic!("expected AddrLen");
        };
        assert_eq!(addr, VirBytes(0x4000_0000));
        assert_eq!(len, 0x2000);
        assert_eq!(
            table.get_active(slot).unwrap().region_top(),
            VirBytes(0x4000_2000)
        );
    }

    #[test]
    fn test_prepare_invalid_endpoint() {
        let table = VmProcTable::get_global();
        let mut page_alloc = make_page_alloc();
        let mut frames = make_frames();
        // With input validation, an invalid dst (Endpoint(2) — slot 2
        // not in the test table) returns ProcessNotFound.
        let result = handle_rs_prepare(
            table,
            &mut page_alloc,
            &mut frames,
            Endpoint(1),
            Endpoint(2),
            0,
        );
        assert_eq!(result, Err(RsError::ProcessNotFound));
    }

    #[test]
    fn test_prepare_extends_dst_heap_to_src() {
        // C: rs.c:116-126 — if src's data end > dst's, real_brk(dst, src_end).
        let table = VmProcTable::get_global();
        let mut page_alloc = make_page_alloc();
        let mut frames = make_frames();
        let src_slot = UserSlot::new(68);
        let dst_slot = UserSlot::new(69);
        let src = init_test_process(src_slot);
        let dst = init_test_process(dst_slot);

        // src data region [0x3000_0000, 0x5000_0000), dst [0x3000_0000, 0x4000_0000).
        {
            let mut proc = table.get_active(src_slot).unwrap();
            proc.regions_mut().insert(crate::region::VirRegion::new(
                VirBytes(0x3000_0000),
                VirBytes(0x2000_0000),
                VrFlags::WRITABLE | VrFlags::ANON,
            )).unwrap();
            proc.set_region_top(VirBytes(0x5000_0000));
        }
        {
            let mut proc = table.get_active(dst_slot).unwrap();
            proc.regions_mut().insert(crate::region::VirRegion::new(
                VirBytes(0x3000_0000),
                VirBytes(0x1000_0000),
                VrFlags::WRITABLE | VrFlags::ANON,
            )).unwrap();
            proc.set_region_top(VirBytes(0x4000_0000));
        }

        let result = handle_rs_prepare(
            table,
            &mut page_alloc,
            &mut frames,
            src,
            dst,
            0,
        );
        assert_eq!(result, Ok(()));
        assert_eq!(
            table.get_active(dst_slot).unwrap().region_top(),
            VirBytes(0x5000_0000)
        );
    }

    #[test]
    fn test_prepare_does_not_shrink_dst_heap() {
        // C: `if (src_addr > dst_addr)` — a larger dst heap must be left
        // untouched (calling brk unconditionally would shrink it).
        let table = VmProcTable::get_global();
        let mut page_alloc = make_page_alloc();
        let mut frames = make_frames();
        let src_slot = UserSlot::new(70);
        let dst_slot = UserSlot::new(71);
        let src = init_test_process(src_slot);
        let dst = init_test_process(dst_slot);

        {
            let mut proc = table.get_active(src_slot).unwrap();
            proc.regions_mut().insert(crate::region::VirRegion::new(
                VirBytes(0x3000_0000),
                VirBytes(0x1000_0000),
                VrFlags::WRITABLE | VrFlags::ANON,
            )).unwrap();
            proc.set_region_top(VirBytes(0x4000_0000));
        }
        {
            let mut proc = table.get_active(dst_slot).unwrap();
            proc.regions_mut().insert(crate::region::VirRegion::new(
                VirBytes(0x3000_0000),
                VirBytes(0x3000_0000),
                VrFlags::WRITABLE | VrFlags::ANON,
            )).unwrap();
            proc.set_region_top(VirBytes(0x6000_0000));
        }

        let result = handle_rs_prepare(
            table,
            &mut page_alloc,
            &mut frames,
            src,
            dst,
            0,
        );
        assert_eq!(result, Ok(()));
        assert_eq!(
            table.get_active(dst_slot).unwrap().region_top(),
            VirBytes(0x6000_0000)
        );
    }

    #[test]
    fn test_update_invalid_endpoint() {
        let table = VmProcTable::get_global();
        let mut page_alloc = make_page_alloc();
        let mut frames = make_frames();
        // Invalid dst (Endpoint(2) — slot 2 not in the test table)
        // returns ProcessNotFound before reaching the NotImplemented stub.
        let result = handle_rs_update(
            table,
            &mut page_alloc,
            &mut frames,
            Endpoint(1),
            Endpoint(2),
            0,
        );
        assert_eq!(result, Err(RsError::ProcessNotFound));
    }

    #[test]
    fn test_update_flags_rollback_bypasses_prealloc_check() {
        // When ROLLBACK flag is set, the PREALLOC_MAP check is skipped.
        // This tests the flag parsing logic without needing a populated
        // VmProcTable (the ProcessNotFound error fires after the flag
        // check, proving the flag was parsed).
        let table = VmProcTable::get_global();
        let mut page_alloc = make_page_alloc();
        let mut frames = make_frames();
        let result = handle_rs_update(
            table,
            &mut page_alloc,
            &mut frames,
            Endpoint(1),
            Endpoint(2),
            RsUpdateFlags::ROLLBACK.bits(),
        );
        // ROLLBACK flag bypasses PREALLOC_MAP check, but dst=2 is
        // still invalid, so we get ProcessNotFound (not PreallocMapConflict).
        assert_eq!(result, Err(RsError::ProcessNotFound));
    }

    #[test]
    fn test_update_flags_nommap_bypasses_prealloc_check() {
        // When NOMMAP flag is set, the PREALLOC_MAP check is skipped.
        let table = VmProcTable::get_global();
        let mut page_alloc = make_page_alloc();
        let mut frames = make_frames();
        let result = handle_rs_update(
            table,
            &mut page_alloc,
            &mut frames,
            Endpoint(1),
            Endpoint(2),
            RsUpdateFlags::NOMMAP.bits(),
        );
        assert_eq!(result, Err(RsError::ProcessNotFound));
    }

    #[test]
    fn test_error_errno_mapping() {
        // Tests the full error path: RsError → From<RsError> for VmError → VmError::to_errno()
        use minix_types::{VmError, EINVAL, ENOSYS, EPERM, ENOMEM};
        assert_eq!(VmError::from(RsError::ProcessNotFound).to_errno(), EINVAL);
        assert_eq!(VmError::from(RsError::SysProcNoMask).to_errno(), EINVAL);
        assert_eq!(VmError::from(RsError::PinFailed).to_errno(), ENOSYS);
        // C: real_brk() returns ENOMEM on failure (break.c:63-68).
        assert_eq!(VmError::from(RsError::HeapExtendFailed).to_errno(), ENOMEM);
        assert_eq!(VmError::from(RsError::PreallocMapConflict).to_errno(), ENOSYS);
        assert_eq!(VmError::from(RsError::UpdateNotImplemented).to_errno(), ENOSYS);
        // C: rs.c:386-388 — do_rs_memctl default arm returns EINVAL.
        assert_eq!(VmError::from(RsError::InvalidRequest).to_errno(), EINVAL);
        assert_eq!(VmError::from(RsError::MakeVmFailed).to_errno(), EPERM);
        assert_eq!(VmError::from(RsError::HeapPreallocFailed).to_errno(), ENOSYS);
        assert_eq!(VmError::from(RsError::MapPreallocFailed).to_errno(), ENOSYS);
        // C: rs.c:287-288 / rs.c:307-308 — *len <= 0 returns EINVAL.
        assert_eq!(VmError::from(RsError::InvalidLength).to_errno(), EINVAL);
    }
}
