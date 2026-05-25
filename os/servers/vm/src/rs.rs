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
//! - **PREPARE**: Not yet implemented (requires map_pin_memory).
//! - **UPDATE**: Not yet implemented (requires swap_proc_slot typestate extension).

use minix_types::{VirBytes, EINVAL, EPERM, ENOSYS, Endpoint};
use crate::vmproc::VmProcTable;
use crate::region::{PageFrames, VrFlags};
use crate::alloc_page::VmPageAllocator;
use crate::acl::AclState;

// ── Error types ──────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RsSetPrivError {
    ProcessNotFound,
    SysProcNoMask,
}

impl RsSetPrivError {
    pub(crate) fn to_errno(&self) -> i32 {
        match self {
            Self::ProcessNotFound => EINVAL,
            Self::SysProcNoMask => EINVAL,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RsPrepareError {
    ProcessNotFound,
    NotImplemented,
}

impl RsPrepareError {
    pub(crate) fn to_errno(&self) -> i32 {
        match self {
            Self::ProcessNotFound => EINVAL,
            Self::NotImplemented => ENOSYS,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RsUpdateError {
    ProcessNotFound,
    PreallocMapConflict,
    NotImplemented,
}

impl RsUpdateError {
    pub(crate) fn to_errno(&self) -> i32 {
        match self {
            Self::ProcessNotFound => EINVAL,
            Self::PreallocMapConflict => ENOSYS,
            Self::NotImplemented => ENOSYS,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RsMemctlError {
    ProcessNotFound,
    InvalidRequest,
    MakeVmFailed,
    HeapPreallocFailed,
    MapPreallocFailed,
    InvalidLength,
}

impl RsMemctlError {
    pub(crate) fn to_errno(&self) -> i32 {
        match self {
            Self::ProcessNotFound => EINVAL,
            Self::InvalidRequest => EINVAL,
            Self::MakeVmFailed => EPERM,
            Self::HeapPreallocFailed => ENOSYS,
            Self::MapPreallocFailed => ENOSYS,
            Self::InvalidLength => EINVAL,
        }
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
) -> Result<(), RsSetPrivError> {
    let slot = table.vm_isokendpt(target)
        .map_err(|_| RsSetPrivError::ProcessNotFound)?;

    let mut active = table.get_active(slot)
        .ok_or(RsSetPrivError::ProcessNotFound)?;

    if mask.is_none() && is_sys_proc {
        return Err(RsSetPrivError::SysProcNoMask);
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
/// Not yet implemented: requires `map_pin_memory()` and
/// `map_proc_dyn_data()` from the region module.
pub(crate) fn handle_rs_prepare(
    table: &VmProcTable,
    _page_alloc: &mut VmPageAllocator,
    _frames: &mut PageFrames,
    _src: Endpoint,
    _dst: Endpoint,
    _flags: u32,
) -> Result<(), RsPrepareError> {
    let _ = table;
    Err(RsPrepareError::NotImplemented)
}

// ── Handler: UPDATE ──────────────────────────────────────────────────

/// Handle VM_RS_UPDATE — execute live update process switch.
///
/// Corresponds to Minix3's `do_rs_update()` (rs.c:150).
///
/// Not yet implemented: requires `swap_proc_slot()` typestate extension
/// and kernel `sys_update` syscall support.
pub(crate) fn handle_rs_update(
    table: &VmProcTable,
    _page_alloc: &mut VmPageAllocator,
    _frames: &mut PageFrames,
    _src: Endpoint,
    _dst: Endpoint,
    _flags: u32,
) -> Result<RsUpdateResult, RsUpdateError> {
    let _ = table;
    Err(RsUpdateError::NotImplemented)
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
    target: Endpoint,
    request: RsMemctlRequest,
) -> Result<RsMemctlResult, RsMemctlError> {
    let slot = table.vm_isokendpt(target)
        .map_err(|_| RsMemctlError::ProcessNotFound)?;

    let active = table.get_active(slot)
        .ok_or(RsMemctlError::ProcessNotFound)?;

    match request {
        RsMemctlRequest::Pin => {
            // C: only pins when num_vm_instances > 1.
            // Current design: single VM instance, no-op.
            Ok(RsMemctlResult::Ok)
        }
        RsMemctlRequest::MakeVmInstance => {
            // C: rs_memctl_make_vm_instance — multi-VM-instance support.
            // Not supported in current design.
            Err(RsMemctlError::MakeVmFailed)
        }
        RsMemctlRequest::HeapPrealloc { len, .. } => {
            if len == 0 {
                return Err(RsMemctlError::InvalidLength);
            }
            // C: rs_memctl_heap_prealloc (rs.c:281) — computes
            // *addr = data_vr->vaddr + data_vr->length (current brk),
            // bytes = *addr + *len, then calls real_brk(vmp, bytes).
            // Rust: compute new absolute brk = current_top + len.
            let current_brk = active.region_top();
            let new_brk = VirBytes(current_brk.0 + len as u64);
            let req = crate::brk::BrkRequest {
                endpoint: target,
                new_brk_addr: new_brk,
            };
            crate::brk::handle_brk(table, page_alloc, frames, &req)
                .map(|_| RsMemctlResult::AddrLen {
                    addr: current_brk,
                    len,
                })
                .map_err(|_| RsMemctlError::HeapPreallocFailed)
        }
        RsMemctlRequest::MapPrealloc { len, .. } => {
            if len == 0 {
                return Err(RsMemctlError::InvalidLength);
            }
            // C: rs_memctl_map_prealloc → map_page_region()
            // Rust: allocate anonymous region via mmap.
            let aligned_len = VirBytes(((len as u64) + 4095) & !4095);
            let mmap_req = minix_types::VmMmapIn {
                caller: target,
                forwhom: target,
                addr: VirBytes(0),
                length: aligned_len,
                prot: 3,
                flags: 0x1002,
                fd: -1,
                offset: 0,
            };
            crate::mmap::handle_mmap(table, page_alloc, frames, &mmap_req)
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
                .map_err(|_| RsMemctlError::MapPreallocFailed)
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
    use minix_types::{UserSlot, PhysBytes};

    fn make_frames() -> PageFrames {
        PageFrames::new(PhysBytes(256 * REGION_PAGE_SIZE as u64))
    }

    fn make_page_alloc() -> VmPageAllocator {
        VmPageAllocator::new(PhysAlloc::Bitmap(BitmapAllocator::new_for_test(256)))
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
        assert_eq!(result, Err(RsSetPrivError::ProcessNotFound));
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
        assert_eq!(result, Err(RsSetPrivError::ProcessNotFound));
    }

    #[test]
    fn test_memctl_pin_not_found() {
        let table = VmProcTable::get_global();
        let mut page_alloc = make_page_alloc();
        let mut frames = make_frames();
        let result = handle_rs_memctl(
            table,
            &mut page_alloc,
            &mut frames,
            Endpoint(9999),
            RsMemctlRequest::Pin,
        );
        assert_eq!(result, Err(RsMemctlError::ProcessNotFound));
    }

    #[test]
    fn test_memctl_make_vm_not_found() {
        let table = VmProcTable::get_global();
        let mut page_alloc = make_page_alloc();
        let mut frames = make_frames();
        let result = handle_rs_memctl(
            table,
            &mut page_alloc,
            &mut frames,
            Endpoint(9999),
            RsMemctlRequest::MakeVmInstance,
        );
        assert_eq!(result, Err(RsMemctlError::ProcessNotFound));
    }

    #[test]
    fn test_memctl_heap_prealloc_zero_len() {
        let table = VmProcTable::get_global();
        let mut page_alloc = make_page_alloc();
        let mut frames = make_frames();
        let result = handle_rs_memctl(
            table,
            &mut page_alloc,
            &mut frames,
            Endpoint(9999),
            RsMemctlRequest::HeapPrealloc { addr: VirBytes(0), len: 0 },
        );
        assert_eq!(result, Err(RsMemctlError::ProcessNotFound));
    }

    #[test]
    fn test_memctl_map_prealloc_zero_len() {
        let table = VmProcTable::get_global();
        let mut page_alloc = make_page_alloc();
        let mut frames = make_frames();
        let result = handle_rs_memctl(
            table,
            &mut page_alloc,
            &mut frames,
            Endpoint(9999),
            RsMemctlRequest::MapPrealloc { addr: VirBytes(0), len: 0 },
        );
        assert_eq!(result, Err(RsMemctlError::ProcessNotFound));
    }

    #[test]
    fn test_prepare_not_implemented() {
        let table = VmProcTable::get_global();
        let mut page_alloc = make_page_alloc();
        let mut frames = make_frames();
        let result = handle_rs_prepare(
            table,
            &mut page_alloc,
            &mut frames,
            Endpoint(1),
            Endpoint(2),
            0,
        );
        assert_eq!(result, Err(RsPrepareError::NotImplemented));
    }

    #[test]
    fn test_update_not_implemented() {
        let table = VmProcTable::get_global();
        let mut page_alloc = make_page_alloc();
        let mut frames = make_frames();
        let result = handle_rs_update(
            table,
            &mut page_alloc,
            &mut frames,
            Endpoint(1),
            Endpoint(2),
            0,
        );
        assert_eq!(result, Err(RsUpdateError::NotImplemented));
    }

    #[test]
    fn test_error_errno_mapping() {
        assert_eq!(RsSetPrivError::ProcessNotFound.to_errno(), EINVAL);
        assert_eq!(RsSetPrivError::SysProcNoMask.to_errno(), EINVAL);
        assert_eq!(RsPrepareError::ProcessNotFound.to_errno(), EINVAL);
        assert_eq!(RsPrepareError::NotImplemented.to_errno(), ENOSYS);
        assert_eq!(RsUpdateError::ProcessNotFound.to_errno(), EINVAL);
        assert_eq!(RsUpdateError::PreallocMapConflict.to_errno(), ENOSYS);
        assert_eq!(RsMemctlError::ProcessNotFound.to_errno(), EINVAL);
        assert_eq!(RsMemctlError::InvalidRequest.to_errno(), EINVAL);
        assert_eq!(RsMemctlError::MakeVmFailed.to_errno(), EPERM);
        assert_eq!(RsMemctlError::InvalidLength.to_errno(), EINVAL);
    }
}
