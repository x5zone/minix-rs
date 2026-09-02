//! Cross-process memory copy system calls.
//!
//! Implements SYS_VIRCOPY, SYS_PHYSCOPY, SYS_SAFECOPYFROM, SYS_SAFECOPYTO,
//! SYS_VSAFECOPY, SYS_UMAP, SYS_UMAP_REMOTE, SYS_VUMAP, SYS_MEMSET,
//! and SYS_SAFEMEMSET.
//!
//! # Minix3 C Source Mapping
//!
//! - `do_copy.c` — SYS_VIRCOPY, SYS_PHYSCOPY
//! - `do_safecopy.c` — SYS_SAFECOPYFROM, SYS_SAFECOPYTO, SYS_VSAFECOPY
//! - `do_umap.c` — SYS_UMAP
//! - `do_umap_remote.c` — SYS_UMAP_REMOTE
//! - `do_vumap.c` — SYS_VUMAP
//! - `do_memset.c` — SYS_MEMSET
//! - `do_safememset.c` — SYS_SAFEMEMSET
//!
//! # Design Decisions (18-syscall-copy.md §3)
//!
//! - **D1**: Direct Map replaces createpde + lin_lin_copy
//! - **D3**: `GrantVerifyResult` struct for verify_grant output
//! - **D4**: Loop + depth counter for indirect grant chains
//! - **D8**: Merged do_umap/do_umap_remote into single dispatch_umap
//! - **D9**: `Option<SoftFaultInfo>` for CPF_TRY scenarios

use minix_types::{
    Endpoint, Message, PhysBytes, VirBytes,
    MessLsysKrnSysCopy, MessLsysKrnSysUmap, MessLsysKernSafecopy,
    MessLsysKrnSysMemset, MessSysSafememset, MessLsysKrnSysVumap, MessLsysKernVsafecopy,
};

use crate::proc::KProcess;
use crate::proc_table::ProcessTable;
use crate::kpriv::PrivTable;
use crate::syscall::{KcallResult, Syscall};
use crate::vm::{
    AddressRef, CrossSpaceResult, VmCopyError, cross_space_copy,
    lookup_in_table, lookup_range_in_table,
};
use crate::grant::{
    CpFlags, SoftFaultInfo, VerifyGrantOutcome, verify_grant,
};
use minix_arch::DirectMapArch;

// =========================================================================
// Cross-process copy dispatch — implementation status.
//
// # Infrastructure (all architectures)
//
// - `minix_arch::CurrentPteWalk::walk` (arch crate) — trait-dispatched
//   PTE walk via Direct Map, returns `Option<(PhysBytes, PageFlags)>`.
//   - x86_64: 4-level walk (PML4 → PDPT → PD → PT)
//   - aarch64: 4-level walk (L0 → L1 → L2 → L3)
//   - riscv64: 3-level Sv39 walk (L2 → L1 → L0)
// - `cross_space::data_copy_vmcheck` (cross_space.rs) — full cross-process
//   copy using `vm::cross_space_copy` + PTE walk + Direct Map + VMSUSPEND.
// - `cross_space::memset_vmcheck` (cross_space.rs) — cross-process memset
//   with VMSUSPEND handling, mirroring `data_copy_vmcheck`.
// - `vm::lookup_in_table` / `vm::lookup_range_in_table` (vm.rs) — PTE walk
//   entry points, trait-dispatched via `CurrentPteWalk`.
// - `grant::verify_grant` (grant.rs) — grant verification API with
//   `VerifyGrantOutcome` (Ok/Err/Suspended) + `GrantVerifyResult`.
//
// # All dispatches COMPLETED
//
// 1. `dispatch_copy` — VIRCOPY/PHYSCOPY: `data_copy_vmcheck` (normal) +
//    `cross_space_copy` (CP_FLAG_TRY → EFAULT on fault).
// 2. `dispatch_memset` — MEMSET: `memset_vmcheck` (VmSuspend on fault).
// 3. `dispatch_safecopy_from/to` — `verify_grant` + `data_copy_vmcheck`.
// 4. `dispatch_vsafecopy` — `verify_grant` + vector copy loop.
// 5. `dispatch_umap` / `dispatch_umap_remote` — `verify_grant` +
//    `lookup_in_table` / `lookup_range_in_table` + message writeback.
// 6. `dispatch_vumap` — vector `lookup_range_in_table` + per-element
//    `verify_grant` + message writeback.
// 7. `dispatch_safememset` — `verify_grant` + `memset_vmcheck`.
// =========================================================================

// ── Minix3 error codes ──
// Centralized in `crate::errno` to prevent value drift (FIX-01: R-02/R-09/R-18).
// Previously ELOOP=40 here (should be 62).
use crate::errno::*;

// ── Copy flags ──

/// Try-copy flag for SYS_VIRCOPY. C: `CP_FLAG_TRY` — do_copy.c
const CP_FLAG_TRY: u32 = 0x01;

// ── Segment types ──
// C: const.h:59-68 — segment encoding: type in high byte, index in low byte

/// Segment type mask. C: `SEGMENT_TYPE` — const.h:59
const SEGMENT_TYPE_MASK: i32 = 0xFF00;
/// Segment index mask. C: `SEGMENT_INDEX` — const.h:60
const SEGMENT_INDEX_MASK: i32 = 0x00FF;

/// Physical segment flag. C: `PHYS_SEG` — const.h:62
#[allow(dead_code)] // segment type constant; not yet wired to all call sites
const PHYS_SEG: i32 = 0x0400;
/// Local VM segment type (requires VM lookup). C: `LOCAL_VM_SEG` — const.h:64
const LOCAL_VM_SEG: i32 = 0x1000;
/// Memory grant segment index. C: `MEM_GRANT` — const.h:65
const MEM_GRANT: i32 = 3;
/// Virtual address segment index. C: `VIR_ADDR` — const.h:66
const VIR_ADDR: i32 = 1;
/// VM data segment = LOCAL_VM_SEG | VIR_ADDR. C: `VM_D` — const.h:67
#[allow(dead_code)] // segment type constant; not yet wired to all call sites
const VM_D: i32 = LOCAL_VM_SEG | VIR_ADDR;
/// VM grant segment = LOCAL_VM_SEG | MEM_GRANT. C: `VM_GRANT` — const.h:68
#[allow(dead_code)] // segment type constant; not yet wired to all call sites
const VM_GRANT: i32 = LOCAL_VM_SEG | MEM_GRANT;

// ── Constants ──

/// Maximum indirect grant chain depth. C: `MAX_INDIRECT_DEPTH` — do_safecopy.c:21
#[allow(dead_code)] // grant indirect chain depth limit; not yet wired to all call sites
const MAX_INDIRECT_DEPTH: usize = 5;

/// Maximum VSAFECOPY vector elements. C: `SCPVEC_NR`
const SCPVEC_NR: usize = 64;

/// Maximum VUMAP vector elements. C: `MAPVEC_NR`
const MAPVEC_NR: usize = 64;

/// SELF endpoint sentinel. C: `SELF` — endpoint.h
const SELF: i32 = -2;

/// NONE endpoint sentinel. C: `NONE` — endpoint.h
const NONE: i32 = -1;
/// ANY endpoint sentinel. C: `ANY` — endpoint.h
const ANY: i32 = -3;

// ── Safecopy access direction ──

/// Safecopy access direction.
///
/// C: `access` parameter to `safecopy()` — do_safecopy.c:279
///
/// `Read` = CPF_READ (copy from granter to grantee),
/// `Write` = CPF_WRITE (copy from grantee to granter).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SafecopyAccess {
    /// Read from granter to grantee. C: `CPF_READ`
    Read,
    /// Write from grantee to granter. C: `CPF_WRITE`
    Write,
}

impl SafecopyAccess {
    /// Convert to `CpFlags` for `verify_grant`.
    pub fn to_flags(self) -> CpFlags {
        match self {
            SafecopyAccess::Read => CpFlags::READ,
            SafecopyAccess::Write => CpFlags::WRITE,
        }
    }
}

// ── Helper: access typed message payloads (FIX-08: R-04) ──
//
// All helpers use `Message::debug_check_m_type_any()` to verify `m_type`
// before the `unsafe` union access. In debug builds, a mismatch panics —
// catching dispatch table bugs. In release builds, the check is a no-op.

/// Extract SYS_VIRCOPY/SYS_PHYSCOPY payload. C: `m_lsys_krn_sys_copy`
fn msg_copy(msg: &Message) -> MessLsysKrnSysCopy {
    msg.debug_check_m_type_any(&[Syscall::Vircopy as i32, Syscall::Physcopy as i32]);
    // SAFETY: `m_type` verified above (debug) / guaranteed by dispatch (release).
    unsafe { msg.m_u.m_lsys_krn_sys_copy }
}

/// Extract SYS_UMAP/UMAP_REMOTE payload. C: `m_lsys_krn_sys_umap`
fn msg_umap(msg: &Message) -> MessLsysKrnSysUmap {
    msg.debug_check_m_type_any(&[Syscall::Umap as i32, Syscall::UmapRemote as i32]);
    // SAFETY: `m_type` verified above (debug) / guaranteed by dispatch (release).
    unsafe { msg.m_u.m_lsys_krn_sys_umap }
}

/// Extract SYS_SAFECOPYFROM/TO payload. C: `m_lsys_kern_safecopy`
fn msg_safecopy(msg: &Message) -> MessLsysKernSafecopy {
    msg.debug_check_m_type_any(&[Syscall::SafecopyFrom as i32, Syscall::SafecopyTo as i32]);
    // SAFETY: `m_type` verified above (debug) / guaranteed by dispatch (release).
    unsafe { msg.m_u.m_lsys_kern_safecopy }
}

/// Extract SYS_MEMSET payload. C: `m_lsys_krn_sys_memset`
fn msg_memset(msg: &Message) -> MessLsysKrnSysMemset {
    msg.debug_check_m_type_any(&[Syscall::Memset as i32]);
    // SAFETY: `m_type` verified above (debug) / guaranteed by dispatch (release).
    unsafe { msg.m_u.m_lsys_krn_sys_memset }
}

/// Extract SYS_SAFEMEMSET payload.
fn msg_safememset(msg: &Message) -> MessSysSafememset {
    msg.debug_check_m_type_any(&[Syscall::Safememset as i32]);
    // SAFETY: `m_type` verified above (debug) / guaranteed by dispatch (release).
    unsafe { msg.m_u.m_sys_safememset }
}

/// Extract SYS_VUMAP payload. C: `m_lsys_krn_sys_vumap`
fn msg_vumap(msg: &Message) -> MessLsysKrnSysVumap {
    msg.debug_check_m_type_any(&[Syscall::Vumap as i32]);
    // SAFETY: `m_type` verified above (debug) / guaranteed by dispatch (release).
    unsafe { msg.m_u.m_lsys_krn_sys_vumap }
}

/// Extract SYS_VSAFECOPY payload. C: `m_lsys_kern_vsafecopy`
fn msg_vsafecopy(msg: &Message) -> MessLsysKernVsafecopy {
    msg.debug_check_m_type_any(&[Syscall::Vsafecopy as i32]);
    // SAFETY: `m_type` verified above (debug) / guaranteed by dispatch (release).
    unsafe { msg.m_u.m_lsys_kern_vsafecopy }
}

// ── Dispatch functions ──

/// Dispatch SYS_VIRCOPY / SYS_PHYSCOPY.
///
/// C: `do_copy()` — do_copy.c
///
/// Copy data using virtual or physical addressing.
/// Both calls share the same handler; permissions differ.
pub fn dispatch_vircopy(
    caller: &mut KProcess,
    msg: &Message,
    proc_table: &crate::proc_table::ProcessTable,
) -> KcallResult {
    dispatch_copy(caller, msg, proc_table)
}

/// Dispatch SYS_PHYSCOPY.
///
/// C: `do_copy()` — do_copy.c (same handler as VIRCOPY)
pub fn dispatch_physcopy(
    caller: &mut KProcess,
    msg: &Message,
    proc_table: &crate::proc_table::ProcessTable,
) -> KcallResult {
    dispatch_copy(caller, msg, proc_table)
}

/// Shared implementation for VIRCOPY and PHYSCOPY.
///
/// C: do_copy.c:22-90
fn dispatch_copy(
    caller: &mut KProcess,
    msg: &Message,
    proc_table: &crate::proc_table::ProcessTable,
) -> KcallResult {
    let m = msg_copy(msg);

    // C: do_copy.c:51-52 — extract source and destination
    let mut src_endpt = m.src_endpt;
    let mut dst_endpt = m.dst_endpt;
    let src_addr = m.src_addr;
    let dst_addr = m.dst_addr;
    let nr_bytes = m.nr_bytes;
    let flags = m.flags as u32;

    // C: do_copy.c:64-65 — SELF replacement
    if src_endpt == SELF {
        src_endpt = caller.p_endpoint.0;
    }
    if dst_endpt == SELF {
        dst_endpt = caller.p_endpoint.0;
    }

    // C: do_copy.c:66-71 — endpoint validation via isokendpt
    // C: for(i=_SRC_; i<=_DST_; i++) { if(!isokendpt(vir_addr[i].proc_nr_e, &p)) return EINVAL; }
    // isokendpt checks: 1) proc_nr in range, 2) slot occupied, 3) generation matches.
    // ProcessTable::endpoint_to_nr() performs all three checks.
    if src_endpt != NONE
        && proc_table.endpoint_to_nr(Endpoint(src_endpt)).is_none() {
            return KcallResult::Ok(EINVAL);
        }
    if dst_endpt != NONE
        && proc_table.endpoint_to_nr(Endpoint(dst_endpt)).is_none() {
            return KcallResult::Ok(EINVAL);
        }

    // C: do_copy.c:77 — overflow check: src_addr + nr_bytes must not wrap.
    // On 64-bit, vir_bytes == phys_bytes; the check is still needed to
    // prevent `copy_nonoverlapping` from wrapping around.
    if nr_bytes > 0
        && (src_addr.checked_add(nr_bytes).is_none()
            || dst_addr.checked_add(nr_bytes).is_none())
        {
            return KcallResult::Ok(E2BIG);
        }

    // Build AddressRef: NONE → Physical (raw physical address), else → Process.
    // C: do_copy.c:66 — `proc_addr[i] = NULL` when `proc_nr_e == NONE`,
    // meaning the address is physical.
    let src = if src_endpt == NONE {
        AddressRef::Physical(PhysBytes(src_addr))
    } else {
        AddressRef::Process {
            endpoint: Endpoint(src_endpt),
            offset: VirBytes(src_addr),
        }
    };
    let dst = if dst_endpt == NONE {
        AddressRef::Physical(PhysBytes(dst_addr))
    } else {
        AddressRef::Process {
            endpoint: Endpoint(dst_endpt),
            offset: VirBytes(dst_addr),
        }
    };

    // Build proc_cr3 closure: resolves Endpoint → page-table root (CR3/TTBR0).
    // Reads caller's fields BEFORE the mutable borrow to avoid aliasing —
    // the closure captures `caller_cr3` and `caller_endpt` by value (both Copy).
    let caller_endpt = caller.p_endpoint;
    let caller_cr3 = caller.p_seg.phys_root;
    let proc_cr3 = |endpt: Endpoint| {
        if endpt == caller_endpt {
            Some(caller_cr3)
        } else {
            proc_table
                .endpoint_to_nr(endpt)
                .and_then(|nr| proc_table.get(nr))
                .map(|p| p.p_seg.phys_root)
        }
    };

    // C: do_copy.c:80-85 — CP_FLAG_TRY handling.
    // Try-copy mode (VFS-only): calls virtual_copy_f directly (no vmcheck
    // wrapper), returns EFAULT on page fault instead of VMSUSPEND.
    // C: `if (flags & CP_FLAG_TRY) return virtual_copy_f(...) == VMSUSPEND ? EFAULT : OK;`
    let try_mode = flags & CP_FLAG_TRY != 0;

    if try_mode {
        // Call cross_space_copy directly — no VMSUSPEND side effect.
        let result = cross_space_copy::<minix_arch::CurrentDirectMap>(
            &src, &dst, nr_bytes as usize, &proc_cr3,
        );
        return match result {
            CrossSpaceResult::Completed(Ok(())) => KcallResult::Ok(OK),
            CrossSpaceResult::Completed(Err(_)) => KcallResult::Ok(EFAULT),
            // CP_FLAG_TRY: VMSUSPEND → EFAULT (no suspend).
            CrossSpaceResult::Suspended(_) => KcallResult::Ok(EFAULT),
        };
    }

    // C: do_copy.c:86-89 — normal copy with VM check.
    // virtual_copy_vmcheck(caller, &vir_addr[_SRC_], &vir_addr[_DST_], bytes)
    let result = crate::cross_space::data_copy_vmcheck(
        caller,
        src,
        dst,
        nr_bytes as usize,
        proc_cr3,
    );

    match result {
        CrossSpaceResult::Completed(Ok(())) => KcallResult::Ok(OK),
        CrossSpaceResult::Completed(Err(VmCopyError::UnknownEndpoint)) => KcallResult::Ok(EINVAL),
        CrossSpaceResult::Completed(Err(_)) => KcallResult::Ok(EFAULT),
        CrossSpaceResult::Suspended(_) => KcallResult::VmSuspend,
    }
}

/// Dispatch SYS_SAFECOPYFROM.
///
/// C: `do_safecopy_from()` — do_safecopy.c:388-394
///
/// Copy data from a granter to the caller using grant-based access control.
/// This is a thin wrapper over `safecopy_common_impl` with `access = CpFlags::READ`.
pub fn dispatch_safecopy_from(
    caller: &mut KProcess,
    msg: &Message,
    proc_table: &ProcessTable,
    priv_table: &PrivTable,
) -> KcallResult {
    safecopy_common_impl(caller, msg, proc_table, priv_table, CpFlags::READ)
}

/// Dispatch SYS_SAFECOPYTO.
/// C: `do_safecopy_to()` — do_safecopy.c:377-383
///
/// Copy data from the caller to a granter using grant-based access control.
/// This is a thin wrapper over `safecopy_common_impl` with `access = CpFlags::WRITE`.
pub fn dispatch_safecopy_to(
    caller: &mut KProcess,
    msg: &Message,
    proc_table: &ProcessTable,
    priv_table: &PrivTable,
) -> KcallResult {
    safecopy_common_impl(caller, msg, proc_table, priv_table, CpFlags::WRITE)
}

/// Shared implementation for SAFECOPYFROM (READ) and SAFECOPYTO (WRITE).
///
/// C: `safecopy()` — do_safecopy.c:271-372 (the static `safecopy` helper
/// that both `do_safecopy_from` and `do_safecopy_to` delegate to).
///
/// # Flow
///
/// 1. Validate granter/grantee endpoints (do_safecopy.c:290-293).
/// 2. Call `verify_grant` to resolve the grant and get the effective
///    granter + virtual offset (do_safecopy.c:305-312).
/// 3. Build src/dst `AddressRef` based on access direction:
///    - READ: src = effective_granter@v_offset, dst = caller@addr
///    - WRITE: src = caller@addr, dst = effective_granter@v_offset
/// 4. If `CPF_TRY`: use `cross_space_copy` directly (no VMSUSPEND — returns
///    EFAULT on fault, writes soft-fault marker).
/// 5. Else: use `data_copy_vmcheck` (VMSUSPEND on fault for VM resolution).
fn safecopy_common_impl(
    caller: &mut KProcess,
    msg: &Message,
    proc_table: &ProcessTable,
    priv_table: &PrivTable,
    access: CpFlags,
) -> KcallResult {
    let m = msg_safecopy(msg);
    // C: do_safecopy.c:377-394 — extract parameters
    let granter_ep = m.from_to;
    let grant_id = m.grant_id;
    let bytes = m.bytes;
    let g_offset = m.offset;
    let addr = m.address;

    // C: do_safecopy.c:290-293 — endpoint validation.
    // "nonsense processes" — both granter and grantee (caller) must be valid.
    if granter_ep == NONE || caller.p_endpoint.0 == NONE {
        return KcallResult::Ok(EFAULT);
    }

    // C: do_safecopy.c:305-306 — verify_grant() is called by safecopy().
    // Before that, we need the granter to exist (analogous to
    // endpoint_lookup in C's do_safecopy).
    if proc_table.endpoint_to_nr(Endpoint(granter_ep)).is_none() {
        return KcallResult::Ok(EINVAL);
    }

    // C: cp_grant_id_t is an i32 with -1 (INVALID_GRANT) as the sentinel.
    // A negative grant_id is invalid. verify_grant also checks this, but
    // we return EINVAL early to match C's do_safecopy flow.
    if grant_id < 0 {
        return KcallResult::Ok(EINVAL);
    }

    // Build proc_cr3 closure: resolves Endpoint → page-table root.
    // Captures caller fields by value (both Copy) to avoid aliasing
    // with the &mut caller borrow.
    let caller_endpt = caller.p_endpoint;
    let caller_cr3 = caller.p_seg.phys_root;
    let proc_cr3 = |endpt: Endpoint| {
        if endpt == caller_endpt {
            Some(caller_cr3)
        } else {
            proc_table
                .endpoint_to_nr(endpt)
                .and_then(|nr| proc_table.get(nr))
                .map(|p| p.p_seg.phys_root)
        }
    };

    // C: do_safecopy.c:305-312 — verify_grant(granter, grantee, grantid,
    // bytes, access, g_offset, &v_offset, &new_granter, &sfinfo).
    let grantee = caller.p_endpoint;
    let outcome = verify_grant(
        caller,
        Endpoint(granter_ep),
        grantee,
        grant_id,
        bytes,
        access,
        g_offset,
        proc_table,
        priv_table,
        &proc_cr3,
    );

    let result = match outcome {
        VerifyGrantOutcome::Ok(r) => r,
        VerifyGrantOutcome::Err(e) => return KcallResult::Ok(e),
        // verify_grant already set RTS_VMREQUEST on caller via
        // data_copy_vmcheck's suspend path.
        VerifyGrantOutcome::Suspended(_) => return KcallResult::VmSuspend,
    };

    // C: do_safecopy.c:317 — granter = new_granter (effective granter).
    let effective_granter = result.effective_granter;
    let v_offset = result.offset;

    // C: do_safecopy.c:320-333 — build src/dst based on access direction.
    let (src, dst) = if access.contains(CpFlags::READ) {
        // READ: copy from granter@v_offset to caller@addr
        (
            AddressRef::Process { endpoint: effective_granter, offset: v_offset },
            AddressRef::Process { endpoint: grantee, offset: VirBytes(addr) },
        )
    } else {
        // WRITE: copy from caller@addr to granter@v_offset
        (
            AddressRef::Process { endpoint: grantee, offset: VirBytes(addr) },
            AddressRef::Process { endpoint: effective_granter, offset: v_offset },
        )
    };

    // C: do_safecopy.c:336-370 — CPF_TRY soft-fault path.
    if let Some(ref sfinfo) = result.sfinfo {
        // C: virtual_copy (no vmcheck) — EFAULT on fault, no VMSUSPEND.
        let copy_result = cross_space_copy::<minix_arch::CurrentDirectMap>(
            &src, &dst, bytes as usize, &proc_cr3,
        );
        match copy_result {
            CrossSpaceResult::Completed(Ok(())) => return KcallResult::Ok(OK),
            CrossSpaceResult::Completed(Err(_)) => {
                write_soft_fault_marker(caller, sfinfo, &proc_cr3);
                return KcallResult::Ok(EFAULT);
            }
            CrossSpaceResult::Suspended(_) => {
                // CPF_TRY: VMSUSPEND → EFAULT (no suspend).
                write_soft_fault_marker(caller, sfinfo, &proc_cr3);
                return KcallResult::Ok(EFAULT);
            }
        }
    }

    // C: do_safecopy.c:371 — virtual_copy_vmcheck(caller, &v_src, &v_dst, bytes).
    let result = crate::cross_space::data_copy_vmcheck(
        caller,
        src,
        dst,
        bytes as usize,
        proc_cr3,
    );

    match result {
        CrossSpaceResult::Completed(Ok(())) => KcallResult::Ok(OK),
        CrossSpaceResult::Completed(Err(VmCopyError::UnknownEndpoint)) => KcallResult::Ok(EINVAL),
        CrossSpaceResult::Completed(Err(_)) => KcallResult::Ok(EFAULT),
        CrossSpaceResult::Suspended(_) => KcallResult::VmSuspend,
    }
}

/// Write the soft-fault marker for CPF_TRY grants.
///
/// C: do_safecopy.c:355-365 — `data_copy(KERNEL, &sfinfo.value, sfinfo.endpt,
/// sfinfo.addr, sizeof(sfinfo.value))`.
///
/// Writes the grant ID (with sequence number) into the grant entry's
/// `cp_faulted` field to mark that a soft fault occurred. Failure is
/// logged but does not affect the EFAULT return to the caller.
fn write_soft_fault_marker(
    caller: &mut KProcess,
    sfinfo: &SoftFaultInfo,
    proc_cr3: &dyn Fn(Endpoint) -> Option<PhysBytes>,
) {
    // C: sfinfo.addr points to the cp_faulted field in the granter's
    // grant table entry. We write sfinfo.value (the grant ID) there.
    {
        let marker = sfinfo.value;
        let dst = AddressRef::Process {
            endpoint: sfinfo.endpoint,
            offset: VirBytes(sfinfo.addr),
        };
        // Use a kernel stack variable's physical address as the source.
        // C: data_copy(KERNEL, (vir_bytes)&sfinfo.value, sfinfo.endpt,
        //     sfinfo.addr, sizeof(sfinfo.value))
        let marker_phys = minix_arch::CurrentDirectMap::virt_to_phys(VirBytes(
            &marker as *const i32 as u64,
        ));
        let src = AddressRef::Physical(marker_phys);
        let _ = crate::cross_space::data_copy_vmcheck(
            caller,
            src,
            dst,
            core::mem::size_of::<i32>(),
            proc_cr3,
        );
    };
}

/// Dispatch SYS_VSAFECOPY.
///
/// C: `do_vsafecopy()` — do_safecopy.c:399-447
///
/// Perform a vector of safecopy operations.
///
/// # Flow
///
/// 1. Validate caller endpoint + vec_size bounds (do_safecopy.c:407-415).
/// 2. Copy `vscp_vec[]` from caller's user space via `data_copy_vmcheck`
///    (do_safecopy.c:417-419).
/// 3. Per-element loop (do_safecopy.c:422-444):
///    - `v_from == SELF` → WRITE, granter = v_to
///    - `v_to == SELF` → READ, granter = v_from
///    - else → EINVAL
///    - Call `safecopy_common_impl` for each element.
///
/// # Resume semantics
///
/// If any element suspends (VmSuspend), the whole vsafecopy is retried
/// after VM resolves the fault. The vec is re-copied from user space on
/// retry, matching C's behavior (static `vec[]` is re-populated by
/// `virtual_copy_vmcheck`).
pub fn dispatch_vsafecopy(
    caller: &mut KProcess,
    msg: &Message,
    proc_table: &ProcessTable,
    priv_table: &PrivTable,
) -> KcallResult {
    let m = msg_vsafecopy(msg);
    // C: do_safecopy.c:407-415 — extract vector parameters
    let vec_addr = m.vec_addr;
    let vec_size = m.vec_size;

    // C: do_safecopy.c:408 — `assert(src.proc_nr_e != NONE)`.
    // We replace the panic with an EFAULT return so a misbehaving caller
    // gets an explicit error rather than a kernel panic.
    if caller.p_endpoint.0 == NONE {
        return KcallResult::Ok(EFAULT);
    }

    // C: do_safecopy.c:414-415 — `els = vec_size; bytes = els * sizeof(...)`.
    if vec_size <= 0 {
        return KcallResult::Ok(EINVAL);
    }
    if vec_size > MAX_VSCPVEC {
        return KcallResult::Ok(EINVAL);
    }

    let els = vec_size as usize;
    let bytes = match els.checked_mul(core::mem::size_of::<VscpVec>()) {
        Some(b) => b,
        None => return KcallResult::Ok(EINVAL),
    };

    // C: do_safecopy.c:401 — `static struct vscp_vec vec[SCPVEC_NR]`.
    // Rust: stack-allocated array (no static needed — the vec is re-copied
    // on resume). SCPVEC_NR × sizeof(VscpVec) = 64 × 40 = 2560 bytes,
    // well within kernel stack limits.
    let mut vec: [VscpVec; SCPVEC_NR] = [VscpVec {
        v_from: 0, v_to: 0, v_gid: 0, v_bytes: 0, v_offset: 0, v_addr: 0,
    }; SCPVEC_NR];

    // C: do_safecopy.c:417-419 — virtual_copy_vmcheck to copy the vector.
    // src = caller@vec_addr, dst = KERNEL@vec.
    let caller_endpt = caller.p_endpoint;
    let caller_cr3 = caller.p_seg.phys_root;
    let proc_cr3 = |endpt: Endpoint| {
        if endpt == caller_endpt {
            Some(caller_cr3)
        } else {
            proc_table
                .endpoint_to_nr(endpt)
                .and_then(|nr| proc_table.get(nr))
                .map(|p| p.p_seg.phys_root)
        }
    };

    let vec_dst_phys = minix_arch::CurrentDirectMap::virt_to_phys(VirBytes(
        vec.as_mut_ptr() as u64,
    ));
    let copy_result = crate::cross_space::data_copy_vmcheck(
        caller,
        AddressRef::Process {
            endpoint: caller_endpt,
            offset: VirBytes(vec_addr),
        },
        AddressRef::Physical(vec_dst_phys),
        bytes,
        proc_cr3,
    );
    match copy_result {
        CrossSpaceResult::Completed(Ok(())) => {} // proceed to loop
        CrossSpaceResult::Completed(Err(VmCopyError::UnknownEndpoint)) => {
            return KcallResult::Ok(EINVAL);
        }
        CrossSpaceResult::Completed(Err(_)) => return KcallResult::Ok(EFAULT),
        CrossSpaceResult::Suspended(_) => return KcallResult::VmSuspend,
    }

    // C: do_safecopy.c:422-444 — per-element safecopy loop.
    for elem in vec.iter().take(els) {
        // C: do_safecopy.c:425-435 — determine access direction + granter.
        let (access, granter) = if elem.v_from == SELF {
            (CpFlags::WRITE, elem.v_to)
        } else if elem.v_to == SELF {
            (CpFlags::READ, elem.v_from)
        } else {
            // C: printf + return EINVAL
            return KcallResult::Ok(EINVAL);
        };

        // Build a synthetic SAFECOPY message for this element.
        // C: safecopy(caller, granter, caller->p_endpoint, vec[i].v_gid,
        //            vec[i].v_bytes, vec[i].v_offset, vec[i].v_addr, access)
        let mut elem_msg = Message::default();
        elem_msg.m_u.m_lsys_kern_safecopy.from_to = granter;
        elem_msg.m_u.m_lsys_kern_safecopy.grant_id = elem.v_gid;
        elem_msg.m_u.m_lsys_kern_safecopy.bytes = elem.v_bytes;
        elem_msg.m_u.m_lsys_kern_safecopy.offset = elem.v_offset;
        elem_msg.m_u.m_lsys_kern_safecopy.address = elem.v_addr;

        let result = safecopy_common_impl(
            caller, &elem_msg, proc_table, priv_table, access,
        );
        match result {
            KcallResult::Ok(OK) => continue,
            KcallResult::Ok(e) => return KcallResult::Ok(e),
            KcallResult::VmSuspend => return KcallResult::VmSuspend,
            _ => return result,
        }
    }

    KcallResult::Ok(OK)
}

/// Maximum number of `vscp_vec` elements accepted by vsafecopy.
///
/// C: `SCPVEC_NR` — const.h:48. Fixed at compile time; we mirror the
/// C static array bound.
pub const MAX_VSCPVEC: i32 = SCPVEC_NR as i32;

/// Vector safecopy element.
///
/// C: `struct vscp_vec` — safecopy.h:55-71
///
/// Each element describes one copy operation within a vsafecopy call.
/// `v_from` and `v_to` are endpoints; one of them must equal `SELF`
/// (the calling process) — the other side is the granter.
#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct VscpVec {
    /// Source endpoint. C: `v_from`
    pub v_from: i32,
    /// Destination endpoint. C: `v_to`
    pub v_to: i32,
    /// Grant ID. C: `v_gid`
    pub v_gid: i32,
    /// Bytes to copy. C: `v_bytes`
    pub v_bytes: u64,
    /// Offset within the grant. C: `v_offset`
    pub v_offset: u64,
    /// Virtual address in the SELF side. C: `v_addr`
    pub v_addr: u64,
}

/// Dispatch SYS_UMAP.
///
/// C: `do_umap()` — do_umap.c
///
/// Map virtual address to physical address. Subset of UMAP_REMOTE:
/// only allows mapping own address space and grants where caller is grantee.
///
/// Design decision D8: merged with UMAP_REMOTE logic.
pub fn dispatch_umap(
    caller: &mut KProcess,
    msg: &mut Message,
    proc_table: &ProcessTable,
    priv_table: &PrivTable,
) -> KcallResult {
    let m = msg_umap(msg);
    // C: do_umap.c:25-36 — security check
    let seg_index = m.segment & SEGMENT_INDEX_MASK;
    let endpt = m.src_endpt;

    // C: do_umap.c:34 — only MEM_GRANT with SELF or own grants allowed
    if seg_index != MEM_GRANT && endpt != SELF {
        return KcallResult::Ok(EPERM);
    }

    // C: do_umap.c:35-36 — set dst_endpt = SELF and delegate
    dispatch_umap_remote_impl(caller, msg, SELF, proc_table, priv_table)
}

/// Dispatch SYS_UMAP_REMOTE.
///
/// C: `do_umap_remote()` — do_umap_remote.c
///
/// Map virtual address to physical address for any process, with grantee check.
pub fn dispatch_umap_remote(
    caller: &mut KProcess,
    msg: &mut Message,
    proc_table: &ProcessTable,
    priv_table: &PrivTable,
) -> KcallResult {
    let m = msg_umap(msg);
    let grantee = m.dst_endpt;
    dispatch_umap_remote_impl(caller, msg, grantee, proc_table, priv_table)
}

/// Shared implementation for UMAP and UMAP_REMOTE.
///
/// C: do_umap_remote.c:26-120
///
/// # Flow
///
/// 1. Validate source endpoint (SELF → caller; else `endpoint_to_nr`).
/// 2. Validate grantee endpoint (do_umap_remote.c:47-55).
/// 3. For `LOCAL_VM_SEG + MEM_GRANT`: call `verify_grant` with
///    `access = CpFlags::empty()` (resolve-only, no direction check) to
///    get the effective granter + virtual offset, then fall through to
///    VIR_ADDR lookup.
/// 4. For `LOCAL_VM_SEG + VIR_ADDR`: call `lookup_in_table` to resolve
///    VA → PA in the target process's page table.
/// 5. If `vm_running`: call `lookup_range_in_table` for contiguity check
///    (must cover all `count` bytes).
/// 6. Write `phys_addr` to `m_krn_lsys_sys_umap.dst_addr` (reply field).
fn dispatch_umap_remote_impl(
    caller: &mut KProcess,
    msg: &mut Message,
    grantee: i32,
    proc_table: &ProcessTable,
    priv_table: &PrivTable,
) -> KcallResult {
    let m = msg_umap(msg);
    let seg_type = m.segment & SEGMENT_TYPE_MASK;
    let seg_index = m.segment & SEGMENT_INDEX_MASK;
    let endpt = m.src_endpt;
    let offset = m.src_addr;
    let count = m.nr_bytes;

    // C: do_umap_remote.c:39-45 — endpoint validation + SELF replacement
    let target_endpoint = if endpt == SELF {
        caller.p_endpoint
    } else {
        let ep = Endpoint(endpt);
        if proc_table.endpoint_to_nr(ep).is_none() {
            return KcallResult::Ok(EINVAL);
        }
        ep
    };

    // C: do_umap_remote.c:47-55 — grantee validation
    // R-18 (2026-08-13): Two distinct EINVAL checks (invalid grantee + non-grant
    // segment type) intentionally kept separate for readability. Allowed per
    // clippy::if_same_then_else.
    #[allow(clippy::if_same_then_else)]
    let grantee_endpoint = if grantee == SELF {
        caller.p_endpoint
    } else if grantee == NONE || grantee == ANY {
        return KcallResult::Ok(EINVAL);
    } else if seg_index != MEM_GRANT {
        return KcallResult::Ok(EINVAL);
    } else {
        let ep = Endpoint(grantee);
        if proc_table.endpoint_to_nr(ep).is_none() {
            return KcallResult::Ok(EINVAL);
        }
        ep
    };

    // C: do_umap_remote.c:57-104 — segment type dispatch
    if seg_type != LOCAL_VM_SEG {
        return KcallResult::Ok(EINVAL);
    }

    // Resolve grant (if MEM_GRANT) or use raw offset (if VIR_ADDR).
    // C: do_umap_remote.c:60-82.
    let (lookup_endpt, lookup_addr) = if seg_index == MEM_GRANT {
        // C: verify_grant(targetpr->p_endpoint, grantee, grant, count,
        //                  0, 0, &newoffset, &newep, NULL)
        // access = 0 → resolve-only, no READ/WRITE permission check.
        let caller_endpt = caller.p_endpoint;
        let caller_cr3 = caller.p_seg.phys_root;
        let proc_cr3 = |endpt: Endpoint| {
            if endpt == caller_endpt {
                Some(caller_cr3)
            } else {
                proc_table
                    .endpoint_to_nr(endpt)
                    .and_then(|nr| proc_table.get(nr))
                    .map(|p| p.p_seg.phys_root)
            }
        };

        let grant_id = offset as i32;
        let outcome = verify_grant(
            caller,
            target_endpoint,
            grantee_endpoint,
            grant_id,
            count as u64,
            CpFlags::empty(),
            0,
            proc_table,
            priv_table,
            &proc_cr3,
        );

        let result = match outcome {
            VerifyGrantOutcome::Ok(r) => r,
            VerifyGrantOutcome::Err(_) => return KcallResult::Ok(EFAULT),
            VerifyGrantOutcome::Suspended(_) => return KcallResult::VmSuspend,
        };

        // C: do_umap_remote.c:73-76 — validate effective granter endpoint.
        if proc_table
            .endpoint_to_nr(result.effective_granter)
            .is_none()
        {
            return KcallResult::Ok(EFAULT);
        }
        (result.effective_granter, result.offset.0)
    } else if seg_index == VIR_ADDR {
        (target_endpoint, offset)
    } else {
        // C: do_umap_remote.c:86-89 — bogus seg_index → EFAULT.
        return KcallResult::Ok(EFAULT);
    };

    // C: do_umap_remote.c:94-97 — vm_lookup(targetpr, lin_addr, &phys_addr, NULL).
    let cr3 = match proc_table
        .endpoint_to_nr(lookup_endpt)
        .and_then(|nr| proc_table.get(nr))
    {
        Some(p) => p.p_seg.phys_root,
        None => return KcallResult::Ok(EFAULT),
    };

    let phys_addr = match lookup_in_table::<minix_arch::CurrentDirectMap>(
        cr3,
        VirBytes(lookup_addr),
    ) {
        Some((pa, _)) => pa,
        None => return KcallResult::Ok(EFAULT),
    };

    // C: do_umap_remote.c:98-99 — panic on zero physical address.
    // We return EFAULT instead of panicking (safer for the kernel).
    if phys_addr.0 == 0 {
        return KcallResult::Ok(EFAULT);
    }

    // C: do_umap_remote.c:106-109 — contiguity check via vm_lookup_range.
    // Only checked when vm_running (VM has taken over memory management).
    if crate::vm_running() {
        let count_usize = if count < 0 { 0 } else { count as usize };
        match lookup_range_in_table::<minix_arch::CurrentDirectMap>(
            cr3,
            VirBytes(lookup_addr),
            count_usize,
        ) {
            Some((_, chunk)) if chunk == count_usize => { /* contiguous */ }
            _ => return KcallResult::Ok(EFAULT),
        }
    }

    // C: do_umap_remote.c:111 — m_ptr->m_krn_lsys_sys_umap.dst_addr = phys_addr.
    // SAFETY: m_type == SYS_UMAP || SYS_UMAP_REMOTE guarantees the reply
    // variant m_krn_lsys_sys_umap can be written (same union, different view).
    msg.m_u.m_krn_lsys_sys_umap.dst_addr = phys_addr.0;

    KcallResult::Ok(OK)
}

/// Dispatch SYS_VUMAP.
///
/// C: `do_vumap()` — do_vumap.c
///
/// Map a vector of grants or virtual addresses to physical addresses.
/// Used by drivers for DMA setup.
///
/// # Flow
///
/// 1. Validate caller endpoint + vcount/pmax bounds + access flags.
/// 2. Copy `vvec[]` from caller's user space via `data_copy_vmcheck`.
/// 3. Per-element loop (do_vumap.c:73-118):
///    - `source != SELF` → `verify_grant` to resolve grant → vir_addr + granter
///    - `source == SELF` → use `vv_addr + offset` directly
///    - Inner while: `lookup_range_in_table` → fill `pvec[pcount]`
///    - On `chunk == 0`: `CPF_READ` → EFAULT; else `vm_check_range` (suspend)
/// 4. Copy `pvec[]` back to caller via `data_copy_vmcheck`.
/// 5. Write `pcount` to `m_krn_lsys_sys_vumap.pcount`.
pub fn dispatch_vumap(
    caller: &mut KProcess,
    msg: &mut Message,
    proc_table: &ProcessTable,
    priv_table: &PrivTable,
) -> KcallResult {
    let m = msg_vumap(msg);
    // C: do_vumap.c:39-46 — extract parameters
    let source = m.endpt;
    let vaddr = m.vaddr;
    let vcount = m.vcount;
    let mut offset = m.offset;
    let access_raw = m.access;
    let paddr = m.paddr;
    let pmax = m.pmax;

    // C: do_vumap.c:37 — caller must have a valid endpoint.
    let caller_endpt = caller.p_endpoint;
    if caller_endpt.0 == NONE {
        return KcallResult::Ok(EFAULT);
    }

    // C: do_vumap.c:48-52 — vcount/pmax bounds + MAPVEC_NR cap.
    if vcount <= 0 || pmax <= 0 {
        return KcallResult::Ok(EINVAL);
    }
    let vcount = if (vcount as usize) > MAPVEC_NR { MAPVEC_NR as i32 } else { vcount };
    let pmax = if (pmax as usize) > MAPVEC_NR { MAPVEC_NR as i32 } else { pmax };

    // C: do_vumap.c:54-60 — access flag translation.
    let access = match access_raw {
        VUA_READ => CpFlags::READ,
        VUA_WRITE => CpFlags::WRITE,
        x if x == (VUA_READ | VUA_WRITE) => CpFlags::READ | CpFlags::WRITE,
        _ => return KcallResult::Ok(EINVAL),
    };

    // C: do_vumap.c:79 — if source != SELF, the granter must exist.
    // C does this lazily inside verify_grant; we pre-check for early EINVAL.
    if source != SELF
        && source != NONE
        && proc_table.endpoint_to_nr(Endpoint(source)).is_none()
    {
        return KcallResult::Ok(EINVAL);
    }

    // C: do_vumap.c:63-66 — data_copy of vvec[] from caller to KERNEL.
    // C: `size = vcount * sizeof(vvec[0])`.
    let vcount_usize = vcount as usize;
    let pmax_usize = pmax as usize;
    let vvec_bytes = vcount_usize * core::mem::size_of::<VumapVir>();

    // Stack-allocated input + output vectors.
    // MAPVEC_NR × sizeof(VumapVir) = 64 × 16 = 1024 bytes (well within
    // kernel stack limits). Same for VumapPhys.
    let mut vvec: [VumapVir; MAPVEC_NR] = [VumapVir::ZERO; MAPVEC_NR];
    let mut pvec: [VumapPhys; MAPVEC_NR] = [VumapPhys::ZERO; MAPVEC_NR];

    let caller_cr3 = caller.p_seg.phys_root;
    let proc_cr3 = |endpt: Endpoint| {
        if endpt == caller_endpt {
            Some(caller_cr3)
        } else {
            proc_table
                .endpoint_to_nr(endpt)
                .and_then(|nr| proc_table.get(nr))
                .map(|p| p.p_seg.phys_root)
        }
    };

    // Copy vvec from caller's user space.
    // C: data_copy(endpt, vaddr, KERNEL, (vir_bytes) vvec, size).
    let vvec_dst_phys = minix_arch::CurrentDirectMap::virt_to_phys(VirBytes(
        vvec.as_mut_ptr() as u64,
    ));
    let vvec_copy = crate::cross_space::data_copy_vmcheck(
        caller,
        AddressRef::Process {
            endpoint: caller_endpt,
            offset: VirBytes(vaddr),
        },
        AddressRef::Physical(vvec_dst_phys),
        vvec_bytes,
        proc_cr3,
    );
    match vvec_copy {
        CrossSpaceResult::Completed(Ok(())) => {}
        CrossSpaceResult::Completed(Err(VmCopyError::UnknownEndpoint)) => {
            return KcallResult::Ok(EINVAL);
        }
        CrossSpaceResult::Completed(Err(_)) => return KcallResult::Ok(EFAULT),
        CrossSpaceResult::Suspended(_) => return KcallResult::VmSuspend,
    }

    // C: do_vumap.c:68 — pcount = 0.
    let mut pcount: usize = 0;

    // C: do_vumap.c:73-118 — per-element loop.
    for vv in vvec.iter().take(vcount_usize) {
        if pcount >= pmax_usize {
            break;
        }

        let mut size = vv.vv_size as usize;

        // C: do_vumap.c:75-77 — if size <= offset → EINVAL.
        if size <= offset as usize {
            return KcallResult::Ok(EINVAL);
        }
        size -= offset as usize;

        // C: do_vumap.c:79-87 — resolve grant or use direct address.
        let (mut vir_addr, granter) = if source != SELF {
            let outcome = verify_grant(
                caller,
                Endpoint(source),
                caller_endpt,
                vv.vv_grant(),
                size as u64,
                access,
                offset,
                proc_table,
                priv_table,
                &proc_cr3,
            );

            let result = match outcome {
                VerifyGrantOutcome::Ok(r) => r,
                VerifyGrantOutcome::Err(e) => return KcallResult::Ok(e),
                VerifyGrantOutcome::Suspended(_) => return KcallResult::VmSuspend,
            };
            (result.offset.0, result.effective_granter)
        } else {
            // C: do_vumap.c:84-86 — vir_addr = vvec[i].vv_addr + offset.
            (vv.vv_addr() + offset, caller_endpt)
        };

        // C: do_vumap.c:89-90 — resolve granter to proc pointer.
        let granter_cr3 = match proc_table
            .endpoint_to_nr(granter)
            .and_then(|nr| proc_table.get(nr))
        {
            Some(p) => p.p_seg.phys_root,
            None => return KcallResult::Ok(EFAULT),
        };

        // C: do_vumap.c:93-115 — inner while: split into physical chunks.
        while size > 0 && pcount < pmax_usize {
            let chunk_result = lookup_range_in_table::<minix_arch::CurrentDirectMap>(
                granter_cr3,
                VirBytes(vir_addr),
                size,
            );

            match chunk_result {
                Some((phys_addr, chunk)) if chunk > 0 => {
                    pvec[pcount] = VumapPhys {
                        vp_addr: phys_addr.0,
                        vp_size: chunk as u64,
                    };
                    pcount += 1;
                    vir_addr += chunk as u64;
                    size -= chunk;
                }
                _ => {
                    // C: do_vumap.c:96-107 — chunk == 0 (unmapped page).
                    if access.contains(CpFlags::READ) {
                        // Read access requires the page to be present.
                        return KcallResult::Ok(EFAULT);
                    }
                    // Write access: ask VM to allocate the page.
                    // C: return vm_check_range(caller, procp, vir_addr, size, 1).
                    // In Rust, we suspend_for_vm — the call is retried after
                    // VM resolves the fault (kernel_call_resume).
                    use crate::vm::{VmSuspendType, VmCheckParams};
                    let check_params = VmCheckParams {
                        start: VirBytes(vir_addr),
                        length: VirBytes(size as u64),
                        write_flag: true,
                    };
                    caller.suspend_for_vm(
                        VmSuspendType::KernelCall,
                        granter,
                        check_params,
                        None,
                    );
                    return KcallResult::VmSuspend;
                }
            }
        }

        // C: do_vumap.c:117 — offset = 0 (only first entry uses offset).
        offset = 0;
    }

    // C: do_vumap.c:121 — assert(pcount > 0).
    if pcount == 0 {
        return KcallResult::Ok(EFAULT);
    }

    // C: do_vumap.c:123-125 — data_copy_vmcheck of pvec[] from KERNEL to caller.
    let pvec_bytes = pcount * core::mem::size_of::<VumapPhys>();
    let pvec_src_phys = minix_arch::CurrentDirectMap::virt_to_phys(VirBytes(
        pvec.as_ptr() as u64,
    ));
    let pvec_copy = crate::cross_space::data_copy_vmcheck(
        caller,
        AddressRef::Physical(pvec_src_phys),
        AddressRef::Process {
            endpoint: caller_endpt,
            offset: VirBytes(paddr),
        },
        pvec_bytes,
        proc_cr3,
    );

    match pvec_copy {
        CrossSpaceResult::Completed(Ok(())) => {
            // C: do_vumap.c:128 — m_ptr->m_krn_lsys_sys_vumap.pcount = pcount.
            // SAFETY: m_type == SYS_VUMAP guarantees the reply variant
            // m_krn_lsys_sys_vumap can be written (same union, different view).
            msg.m_u.m_krn_lsys_sys_vumap.pcount = pcount as i32;
            KcallResult::Ok(OK)
        }
        CrossSpaceResult::Completed(Err(VmCopyError::UnknownEndpoint)) => {
            KcallResult::Ok(EINVAL)
        }
        CrossSpaceResult::Completed(Err(_)) => KcallResult::Ok(EFAULT),
        CrossSpaceResult::Suspended(_) => KcallResult::VmSuspend,
    }
}

/// VUMAP input vector element.
///
/// C: `struct vumap_vir` — type.h:40-46
///
/// Union of grant ID (for non-SELF source) or virtual address (for SELF).
/// On 64-bit, the union is 8 bytes (u64-aligned), followed by `vv_size`.
#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct VumapVir {
    /// Union of `cp_grant_id_t u_grant` (i32) or `vir_bytes u_addr` (u64).
    /// Stored as u64; use `vv_grant()` / `vv_addr()` accessors.
    pub vv_u: u64,
    /// Size in bytes. C: `vv_size`
    pub vv_size: u64,
}

impl VumapVir {
    /// Zeroed element (for array initialization).
    pub const ZERO: Self = Self { vv_u: 0, vv_size: 0 };

    /// Interpret `vv_u` as a grant ID. C: `vv_grant` (= `vv_u.u_grant`).
    pub fn vv_grant(&self) -> i32 {
        self.vv_u as i32
    }

    /// Interpret `vv_u` as a virtual address. C: `vv_addr` (= `vv_u.u_addr`).
    pub fn vv_addr(&self) -> u64 {
        self.vv_u
    }
}

/// VUMAP output vector element.
///
/// C: `struct vumap_phys` — type.h:50-53
#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct VumapPhys {
    /// Physical address. C: `vp_addr`
    pub vp_addr: u64,
    /// Size in bytes. C: `vp_size`
    pub vp_size: u64,
}

impl VumapPhys {
    /// Zeroed element (for array initialization).
    pub const ZERO: Self = Self { vp_addr: 0, vp_size: 0 };
}

/// VUMAP access flags.
///
/// C: `VUA_READ` / `VUA_WRITE` from `minix/ipc.h`.
/// Bits are independent, allowing `VUA_READ | VUA_WRITE`.
pub const VUA_READ: i32 = 0x01;
pub const VUA_WRITE: i32 = 0x02;

/// Dispatch SYS_MEMSET.
///
/// C: `do_memset()` — do_memset.c → `vm_memset()` — memory.c:526-577
///
/// Write a pattern into the specified memory in a process's address space.
/// Uses `cross_space::memset_vmcheck` (Direct Map + PTE walk + VMSUSPEND).
///
/// - `process == NONE` → physical address memset (no PTE walk needed)
/// - `process != NONE` → virtual address in target process (PTE walk)
/// - On page fault → `KcallResult::VmSuspend` (caller suspended via
///   `suspend_for_vm`, VM handles fault, `kernel_call_resume` retries)
pub fn dispatch_memset(
    caller: &mut KProcess,
    msg: &Message,
    proc_table: &ProcessTable,
) -> KcallResult {
    let m = msg_memset(msg);
    // C: do_memset.c:19-25 — extract parameters
    let process = m.process;
    let base = m.base;
    let pattern = m.pattern;
    let count = m.count;

    // C: vm_memset:536-537 — check_resumed_caller() precondition.
    if caller.p_endpoint.0 == NONE {
        return KcallResult::Ok(EFAULT);
    }

    // C: vm_memset:540-541 — endpoint_lookup for the target process.
    if process != NONE
        && proc_table.endpoint_to_nr(Endpoint(process)).is_none() {
            return KcallResult::Ok(ESRCH);
        }

    // C: vm_memset:543 — pattern & 0xFF (truncate to single byte).
    let pattern_byte = (pattern & 0xFF) as u8;

    // Zero-byte memset is a no-op (C vm_memset:553 while(left > 0) 自然处理 count=0).
    if count == 0 {
        return KcallResult::Ok(OK);
    }

    // Overflow check: base + count must not wrap.
    if base.checked_add(count).is_none() {
        return KcallResult::Ok(E2BIG);
    }

    // Build AddressRef: NONE → Physical, else → Process.
    let dst = if process == NONE {
        AddressRef::Physical(PhysBytes(base))
    } else {
        AddressRef::Process {
            endpoint: Endpoint(process),
            offset: VirBytes(base),
        }
    };

    // Build proc_cr3 closure (same pattern as dispatch_copy).
    let caller_endpt = caller.p_endpoint;
    let caller_cr3 = caller.p_seg.phys_root;
    let proc_cr3 = |endpt: Endpoint| {
        if endpt == caller_endpt {
            Some(caller_cr3)
        } else {
            proc_table
                .endpoint_to_nr(endpt)
                .and_then(|nr| proc_table.get(nr))
                .map(|p| p.p_seg.phys_root)
        }
    };

    // C: vm_memset:553-580 — memset via Direct Map + PTE walk.
    let result = crate::cross_space::memset_vmcheck(
        caller,
        dst,
        pattern_byte,
        count as usize,
        proc_cr3,
    );

    match result {
        CrossSpaceResult::Completed(Ok(())) => KcallResult::Ok(OK),
        CrossSpaceResult::Completed(Err(VmCopyError::UnknownEndpoint)) => KcallResult::Ok(EINVAL),
        CrossSpaceResult::Completed(Err(_)) => KcallResult::Ok(EFAULT),
        CrossSpaceResult::Suspended(_) => KcallResult::VmSuspend,
    }
}

/// Dispatch SYS_SAFEMEMSET.
///
/// C: `do_safememset()` — do_safememset.c
///
/// Write a pattern into granted memory. Verifies CPF_WRITE permission first,
/// then calls `memset_vmcheck` on the resolved address.
///
/// # Flow
///
/// 1. Validate dst_endpt + caller endpoints (do_safememset.c:36-37).
/// 2. Check dst process has a grant table (do_safememset.c:42-45).
/// 3. Call `verify_grant(CPF_WRITE)` to resolve the address
///    (do_safememset.c:48-49).
/// 4. Call `memset_vmcheck` on the resolved offset + effective granter
///    (do_safememset.c:56).
pub fn dispatch_safememset(
    caller: &mut KProcess,
    msg: &Message,
    proc_table: &ProcessTable,
    priv_table: &PrivTable,
) -> KcallResult {
    let m = msg_safememset(msg);
    // C: do_safememset.c:24-29 — extract parameters
    let dst_endpt = m.dst_endpt;     // SMS_DST
    let grant_id = m.grant_id;       // SMS_GID
    let offset = m.offset;           // SMS_OFFSET
    let pattern = m.pattern;         // SMS_PATTERN
    let bytes = m.bytes;             // SMS_BYTES

    // C: do_safememset.c:36-37 — endpoint validation (NONE check)
    if dst_endpt == NONE || caller.p_endpoint.0 == NONE {
        return KcallResult::Ok(EFAULT);
    }

    // C: do_safememset.c:39-40 — endpoint_lookup
    let dst_nr = match proc_table.endpoint_to_nr(Endpoint(dst_endpt)) {
        Some(nr) => nr,
        None => return KcallResult::Ok(EINVAL),
    };

    // C: do_safememset.c:42-45 — privilege + grant table check.
    let dst_proc = proc_table.get(dst_nr).expect("endpoint_to_nr succeeded");
    let priv_id = dst_proc.priv_id;
    let has_grant_table = priv_id
        .and_then(|pid| priv_table.get(pid))
        .map(|priv_| priv_.runtime.s_grant_table != 0)
        .unwrap_or(false);
    if !has_grant_table {
        return KcallResult::Ok(EINVAL);
    }

    // Build proc_cr3 closure.
    let caller_endpt = caller.p_endpoint;
    let caller_cr3 = caller.p_seg.phys_root;
    let proc_cr3 = |endpt: Endpoint| {
        if endpt == caller_endpt {
            Some(caller_cr3)
        } else {
            proc_table
                .endpoint_to_nr(endpt)
                .and_then(|nr| proc_table.get(nr))
                .map(|p| p.p_seg.phys_root)
        }
    };

    // C: do_safememset.c:48-49 — verify_grant(CPF_WRITE).
    // C: offset/bytes are `long` in the message (m2_l1/m2_l2) but
    // vir_bytes/size_t are unsigned; cast to u64 for verify_grant.
    let grantee = caller.p_endpoint;
    let outcome = verify_grant(
        caller,
        Endpoint(dst_endpt),
        grantee,
        grant_id,
        bytes as u64,
        CpFlags::WRITE,
        offset as u64,
        proc_table,
        priv_table,
        &proc_cr3,
    );

    let result = match outcome {
        VerifyGrantOutcome::Ok(r) => r,
        VerifyGrantOutcome::Err(e) => return KcallResult::Ok(e),
        VerifyGrantOutcome::Suspended(_) => return KcallResult::VmSuspend,
    };

    // C: do_safememset.c:56 — vm_memset(caller, new_granter, v_offset,
    // pattern, bytes). Uses memset_vmcheck for Direct Map + VMSUSPEND.
    let pattern_byte = (pattern & 0xFF) as u8;
    let dst = AddressRef::Process {
        endpoint: result.effective_granter,
        offset: result.offset,
    };

    let memset_result = crate::cross_space::memset_vmcheck(
        caller,
        dst,
        pattern_byte,
        bytes as usize,
        proc_cr3,
    );

    match memset_result {
        CrossSpaceResult::Completed(Ok(())) => KcallResult::Ok(OK),
        CrossSpaceResult::Completed(Err(VmCopyError::UnknownEndpoint)) => KcallResult::Ok(EINVAL),
        CrossSpaceResult::Completed(Err(_)) => KcallResult::Ok(EFAULT),
        CrossSpaceResult::Suspended(_) => KcallResult::VmSuspend,
    }
}

// ── Tests ──

#[cfg(test)]
mod tests {
    use super::*;
    use crate::proc::ProcNr;

    #[test]
    fn test_safecopy_access_flags() {
        assert_eq!(SafecopyAccess::Read.to_flags(), CpFlags::READ);
        assert_eq!(SafecopyAccess::Write.to_flags(), CpFlags::WRITE);
    }

    #[test]
    fn test_umap_security_check() {
        // UMAP only allows MEM_GRANT with SELF or own grants
        // seg_index != MEM_GRANT && endpt != SELF → EPERM
        // This is tested via the dispatch function
    }

    #[test]
    fn test_grant_constants_match_c() {
        // C: safecopies.h:64-75 — correct CPF_* values
        assert_eq!(CpFlags::READ.bits(), 0x000001);
        assert_eq!(CpFlags::WRITE.bits(), 0x000002);
        assert_eq!(CpFlags::TRY.bits(), 0x000010);
        assert_eq!(CpFlags::USED.bits(), 0x000100);
        assert_eq!(CpFlags::DIRECT.bits(), 0x000200);
        assert_eq!(CpFlags::INDIRECT.bits(), 0x000400);
        assert_eq!(CpFlags::MAGIC.bits(), 0x000800);
        assert_eq!(CpFlags::VALID.bits(), 0x001000);
    }

    #[test]
    fn test_max_indirect_depth() {
        assert_eq!(MAX_INDIRECT_DEPTH, 5);
    }

    #[test]
    fn test_segment_constants_match_c() {
        // C: const.h:59-68
        assert_eq!(SEGMENT_TYPE_MASK, 0xFF00, "SEGMENT_TYPE");
        assert_eq!(SEGMENT_INDEX_MASK, 0x00FF, "SEGMENT_INDEX");
        assert_eq!(PHYS_SEG, 0x0400, "PHYS_SEG");
        assert_eq!(LOCAL_VM_SEG, 0x1000, "LOCAL_VM_SEG");
        assert_eq!(MEM_GRANT, 3, "MEM_GRANT");
        assert_eq!(VIR_ADDR, 1, "VIR_ADDR");
        assert_eq!(VM_D, 0x1001, "VM_D = LOCAL_VM_SEG | VIR_ADDR");
        assert_eq!(VM_GRANT, 0x1003, "VM_GRANT = LOCAL_VM_SEG | MEM_GRANT");
    }

    #[test]
    fn test_mess_lsys_krn_sys_copy_layout() {
        use core::mem::{size_of, align_of};
        // Verify the struct fits within the 56-byte message payload
        // C: sizeof(message) - sizeof(m_source) - sizeof(m_type) = 64 - 4 - 4 = 56
        assert!(size_of::<MessLsysKrnSysCopy>() <= 56,
            "MessLsysKrnSysCopy size {} exceeds 56-byte payload", size_of::<MessLsysKrnSysCopy>());
        assert_eq!(align_of::<MessLsysKrnSysCopy>(), 8,
            "MessLsysKrnSysCopy alignment must be 8");
    }

    #[test]
    fn test_mess_lsys_krn_sys_umap_layout() {
        use core::mem::size_of;
        assert!(size_of::<MessLsysKrnSysUmap>() <= 56,
            "MessLsysKrnSysUmap size {} exceeds 56-byte payload", size_of::<MessLsysKrnSysUmap>());
    }

    #[test]
    fn test_dispatch_copy_rejects_invalid_src_endpoint() {
        // C: do_copy.c:67 — if(!isokendpt(vir_addr[i].proc_nr_e, &p)) return EINVAL
        use crate::proc::KProcess;
        use minix_types::Endpoint;
        use minix_types::Message;

        let proc_table = crate::test_helpers::test_proc_table();
        let mut caller = KProcess::new(ProcNr(0), Endpoint(0));
        let mut msg = Message::default();
        msg.m_type = Syscall::Vircopy as i32;
        // src_endpt = 9999 is out of range / no matching process
        msg.m_u.m_lsys_krn_sys_copy.src_endpt = 9999;
        msg.m_u.m_lsys_krn_sys_copy.dst_endpt = NONE; // NONE is allowed
        msg.m_u.m_lsys_krn_sys_copy.src_addr = 0;
        msg.m_u.m_lsys_krn_sys_copy.dst_addr = 0;
        msg.m_u.m_lsys_krn_sys_copy.nr_bytes = 0;
        msg.m_u.m_lsys_krn_sys_copy.flags = 0;
        let result = dispatch_vircopy(&mut caller, &msg, &proc_table);
        assert_eq!(result, KcallResult::Ok(EINVAL));
    }

    #[test]
    fn test_dispatch_copy_rejects_invalid_dst_endpoint() {
        // C: do_copy.c:67 — same check for destination
        use crate::proc::KProcess;
        use minix_types::Endpoint;
        use minix_types::Message;

        let proc_table = crate::test_helpers::test_proc_table();
        let mut caller = KProcess::new(ProcNr(0), Endpoint(0));
        let mut msg = Message::default();
        msg.m_type = Syscall::Vircopy as i32;
        msg.m_u.m_lsys_krn_sys_copy.src_endpt = NONE; // NONE is allowed
        msg.m_u.m_lsys_krn_sys_copy.dst_endpt = 9999;
        msg.m_u.m_lsys_krn_sys_copy.src_addr = 0;
        msg.m_u.m_lsys_krn_sys_copy.dst_addr = 0;
        msg.m_u.m_lsys_krn_sys_copy.nr_bytes = 0;
        msg.m_u.m_lsys_krn_sys_copy.flags = 0;
        let result = dispatch_vircopy(&mut caller, &msg, &proc_table);
        assert_eq!(result, KcallResult::Ok(EINVAL));
    }

    #[test]
    fn test_dispatch_copy_accepts_none_endpoint() {
        // C: do_copy.c:66 — if(vir_addr[i].proc_nr_e != NONE) { isokendpt... }
        // NONE endpoint skips validation (physical address copy).
        use crate::proc::KProcess;
        use minix_types::Endpoint;
        use minix_types::Message;

        let proc_table = crate::test_helpers::test_proc_table();
        let mut caller = KProcess::new(ProcNr(0), Endpoint(0));
        let mut msg = Message::default();
        msg.m_type = Syscall::Vircopy as i32;
        msg.m_u.m_lsys_krn_sys_copy.src_endpt = NONE;
        msg.m_u.m_lsys_krn_sys_copy.dst_endpt = NONE;
        msg.m_u.m_lsys_krn_sys_copy.src_addr = 0;
        msg.m_u.m_lsys_krn_sys_copy.dst_addr = 0;
        msg.m_u.m_lsys_krn_sys_copy.nr_bytes = 0;
        msg.m_u.m_lsys_krn_sys_copy.flags = 0;
        // Should NOT return EINVAL — NONE endpoints are valid (physical copy)
        let result = dispatch_vircopy(&mut caller, &msg, &proc_table);
        assert_ne!(result, KcallResult::Ok(EINVAL));
    }

    #[test]
    fn test_dispatch_copy_self_replacement_and_valid_endpoint() {
        // C: do_copy.c:64-65 — SELF → caller endpoint, then isokendpt
        use crate::proc::KProcess;
        use minix_types::Endpoint;
        use minix_types::Message;
        use crate::proc::RtsFlagsBits;

        let mut proc_table = crate::test_helpers::test_proc_table();
        // Activate a slot so endpoint_to_nr can find it
        let target_nr = 0;
        let target_ep = Endpoint::from_generation_slot(1, target_nr);
        if let Some(target) = proc_table.get_mut(ProcNr(target_nr)) {
            target.p_endpoint = target_ep;
            target.p_rts_flags.clear(RtsFlagsBits::SLOT_FREE);
        }

        let mut caller = KProcess::new(ProcNr(0), target_ep);
        let mut msg = Message::default();
        msg.m_type = Syscall::Vircopy as i32;
        msg.m_u.m_lsys_krn_sys_copy.src_endpt = SELF;
        msg.m_u.m_lsys_krn_sys_copy.dst_endpt = NONE;
        msg.m_u.m_lsys_krn_sys_copy.src_addr = 0;
        msg.m_u.m_lsys_krn_sys_copy.dst_addr = 0;
        msg.m_u.m_lsys_krn_sys_copy.nr_bytes = 0;
        msg.m_u.m_lsys_krn_sys_copy.flags = 0;
        // SELF should resolve to caller's endpoint, which is valid
        let result = dispatch_vircopy(&mut caller, &msg, &proc_table);
        assert_ne!(result, KcallResult::Ok(EINVAL));
    }

    #[test]
    fn test_dispatch_copy_overflow_returns_e2big() {
        // C: do_copy.c:77 — src_addr + nr_bytes overflow → E2BIG.
        let proc_table = crate::test_helpers::test_proc_table();
        let mut caller = KProcess::new(ProcNr(0), Endpoint(0));
        let mut msg = Message::default();
        msg.m_type = Syscall::Vircopy as i32;
        msg.m_u.m_lsys_krn_sys_copy.src_endpt = NONE;
        msg.m_u.m_lsys_krn_sys_copy.dst_endpt = NONE;
        msg.m_u.m_lsys_krn_sys_copy.src_addr = u64::MAX;
        msg.m_u.m_lsys_krn_sys_copy.dst_addr = 0;
        msg.m_u.m_lsys_krn_sys_copy.nr_bytes = 1;
        msg.m_u.m_lsys_krn_sys_copy.flags = 0;
        let result = dispatch_vircopy(&mut caller, &msg, &proc_table);
        assert_eq!(result, KcallResult::Ok(E2BIG));
    }

    #[test]
    fn test_dispatch_copy_try_flag_returns_efault_on_fault() {
        // C: do_copy.c:80-85 — CP_FLAG_TRY: VMSUSPEND → EFAULT.
        // With a Process endpoint whose page table is empty (phys_root=0),
        // the PTE walk fails → Suspended. In try mode, this becomes EFAULT
        // instead of VmSuspend.
        use crate::proc::RtsFlagsBits;
        let mut proc_table = crate::test_helpers::test_proc_table();
        let target_ep = Endpoint::from_generation_slot(1, 0);
        if let Some(p) = proc_table.get_mut(ProcNr(0)) {
            p.p_endpoint = target_ep;
            p.p_rts_flags.clear(RtsFlagsBits::SLOT_FREE);
        }
        let mut caller = KProcess::new(ProcNr(0), target_ep);
        let mut msg = Message::default();
        msg.m_type = Syscall::Vircopy as i32;
        msg.m_u.m_lsys_krn_sys_copy.src_endpt = SELF;
        msg.m_u.m_lsys_krn_sys_copy.dst_endpt = NONE;
        msg.m_u.m_lsys_krn_sys_copy.src_addr = 0x1000;
        msg.m_u.m_lsys_krn_sys_copy.dst_addr = 0;
        msg.m_u.m_lsys_krn_sys_copy.nr_bytes = 0x100;
        msg.m_u.m_lsys_krn_sys_copy.flags = CP_FLAG_TRY as i32;
        let result = dispatch_vircopy(&mut caller, &msg, &proc_table);
        assert_eq!(result, KcallResult::Ok(EFAULT));
    }

    // ── dispatch_umap_remote tests ──────────────────────────────────

    /// Helper: build a MessLsysKrnSysUmap message with the given parameters.
    fn build_umap_msg(
        src_endpt: i32,
        segment: i32,
        src_addr: u64,
        dst_endpt: i32,
        nr_bytes: i32,
    ) -> Message {
        let mut msg = Message::default();
        msg.m_type = Syscall::UmapRemote as i32;
        // SAFETY: All fields are simple integer types; `#[repr(C)]` layout
        // is already verified by the `test_mess_lsys_krn_sys_umap_layout`
        // test below.
        msg.m_u.m_lsys_krn_sys_umap.src_endpt = src_endpt;
        msg.m_u.m_lsys_krn_sys_umap.segment = segment;
        msg.m_u.m_lsys_krn_sys_umap.src_addr = src_addr;
        msg.m_u.m_lsys_krn_sys_umap.dst_endpt = dst_endpt;
        msg.m_u.m_lsys_krn_sys_umap.nr_bytes = nr_bytes;
        msg
    }

    /// Helper: build a proc_table with a user process activated at a given slot.
    fn make_umap_proc_table() -> (crate::test_helpers::TestProcTable, Endpoint, Endpoint) {
        use crate::proc::RtsFlagsBits;
        let mut proc_table = crate::test_helpers::test_proc_table();
        let caller_ep = Endpoint(100);
        let target_ep = Endpoint(101);
        // Activate caller at slot 0 (RS_USER slot 0 is USER).
        if let Some(p) = proc_table.get_mut(ProcNr(0)) {
            p.p_endpoint = caller_ep;
            p.p_rts_flags.clear(RtsFlagsBits::SLOT_FREE);
        }
        // Activate target at slot 1.
        if let Some(p) = proc_table.get_mut(ProcNr(1)) {
            p.p_endpoint = target_ep;
            p.p_rts_flags.clear(RtsFlagsBits::SLOT_FREE);
        }
        (proc_table, caller_ep, target_ep)
    }

    #[test]
    fn test_dispatch_umap_remote_self_endpoint_valid() {
        // C: do_umap_remote.c:40-41 — endpt == SELF → caller's endpoint
        let (proc_table, _caller_ep, _target_ep) = make_umap_proc_table();
        let mut caller = KProcess::new(ProcNr(0), Endpoint(100));
        // src_endpt = SELF, segment = LOCAL_VM_SEG | VIR_ADDR.
        let mut msg = build_umap_msg(SELF, LOCAL_VM_SEG | VIR_ADDR, 0x1000, SELF, 0x100i32);
        let priv_table = crate::test_helpers::test_priv_table();
        let result = dispatch_umap_remote(&mut caller, &mut msg, &proc_table, &priv_table);
        // vm_lookup via lookup_in_table returns None in test env (MockPteWalk) → EFAULT
        assert_eq!(result, KcallResult::Ok(EFAULT));
    }

    #[test]
    fn test_dispatch_umap_remote_invalid_src_endpoint() {
        let (proc_table, _, _) = make_umap_proc_table();
        let mut caller = KProcess::new(ProcNr(0), Endpoint(100));
        // src_endpt = 9999 is invalid.
        let mut msg = build_umap_msg(9999, LOCAL_VM_SEG | VIR_ADDR, 0x1000, SELF, 0x100_i32);
        let priv_table = crate::test_helpers::test_priv_table();
        let result = dispatch_umap_remote(&mut caller, &mut msg, &proc_table, &priv_table);
        assert_eq!(result, KcallResult::Ok(EINVAL));
    }

    #[test]
    fn test_dispatch_umap_remote_valid_src_endpoint() {
        let (proc_table, _, target_ep) = make_umap_proc_table();
        let mut caller = KProcess::new(ProcNr(0), Endpoint(100));
        // src_endpt = valid target, segment = VM_D, grantee = SELF.
        let mut msg = build_umap_msg(
            target_ep.0,
            LOCAL_VM_SEG | VIR_ADDR,
            0x1000,
            SELF,
            0x100_i32,
        );
        let priv_table = crate::test_helpers::test_priv_table();
        let result = dispatch_umap_remote(&mut caller, &mut msg, &proc_table, &priv_table);
        // vm_lookup via lookup_in_table returns None in test env (MockPteWalk) → EFAULT
        assert_eq!(result, KcallResult::Ok(EFAULT));
    }

    #[test]
    fn test_dispatch_umap_remote_grantee_none_rejected() {
        let (proc_table, _, _) = make_umap_proc_table();
        let mut caller = KProcess::new(ProcNr(0), Endpoint(100));
        // grantee = NONE is not valid for UMAP_REMOTE.
        let mut msg = build_umap_msg(SELF, LOCAL_VM_SEG | VIR_ADDR, 0x1000, NONE, 0x100_i32);
        let priv_table = crate::test_helpers::test_priv_table();
        let result = dispatch_umap_remote(&mut caller, &mut msg, &proc_table, &priv_table);
        assert_eq!(result, KcallResult::Ok(EINVAL));
    }

    #[test]
    fn test_dispatch_umap_remote_grantee_any_rejected() {
        let (proc_table, _, _) = make_umap_proc_table();
        let mut caller = KProcess::new(ProcNr(0), Endpoint(100));
        // grantee = ANY is not valid for UMAP_REMOTE.
        let mut msg = build_umap_msg(SELF, LOCAL_VM_SEG | VIR_ADDR, 0x1000, ANY, 0x100_i32);
        let priv_table = crate::test_helpers::test_priv_table();
        let result = dispatch_umap_remote(&mut caller, &mut msg, &proc_table, &priv_table);
        assert_eq!(result, KcallResult::Ok(EINVAL));
    }

    #[test]
    fn test_dispatch_umap_remote_grantee_invalid_endpoint_rejected() {
        let (proc_table, _, _) = make_umap_proc_table();
        let mut caller = KProcess::new(ProcNr(0), Endpoint(100));
        // grantee = 9999 is not in proc_table; seg_index must be MEM_GRANT for
        // a non-SELF grantee to be valid. Here seg_index = VIR_ADDR → EINVAL.
        let mut msg = build_umap_msg(SELF, LOCAL_VM_SEG | VIR_ADDR, 0x1000, 9999, 0x100_i32);
        let priv_table = crate::test_helpers::test_priv_table();
        let result = dispatch_umap_remote(&mut caller, &mut msg, &proc_table, &priv_table);
        assert_eq!(result, KcallResult::Ok(EINVAL));
    }

    #[test]
    fn test_dispatch_umap_remote_grantee_valid_for_grant_segment() {
        let (proc_table, _, target_ep) = make_umap_proc_table();
        let mut caller = KProcess::new(ProcNr(0), Endpoint(100));
        // grantee = valid target, segment = VM_GRANT (LOCAL_VM_SEG | MEM_GRANT).
        let mut msg = build_umap_msg(
            SELF,
            LOCAL_VM_SEG | MEM_GRANT,
            0x1000,
            target_ep.0,
            0x100_i32,
        );
        let priv_table = crate::test_helpers::test_priv_table();
        let result = dispatch_umap_remote(&mut caller, &mut msg, &proc_table, &priv_table);
        // verify_grant fails (no grant table in test) → EFAULT
        assert_eq!(result, KcallResult::Ok(EFAULT));
    }

    #[test]
    fn test_dispatch_umap_remote_invalid_segment_type() {
        let (proc_table, _, _) = make_umap_proc_table();
        let mut caller = KProcess::new(ProcNr(0), Endpoint(100));
        // segment type 0x9999 is unknown → EINVAL.
        let mut msg = build_umap_msg(SELF, 0x9999, 0x1000, SELF, 0x100_i32);
        let priv_table = crate::test_helpers::test_priv_table();
        let result = dispatch_umap_remote(&mut caller, &mut msg, &proc_table, &priv_table);
        assert_eq!(result, KcallResult::Ok(EINVAL));
    }

    #[test]
    fn test_dispatch_umap_remote_bogus_seg_index_for_vm_seg() {
        let (proc_table, _, _) = make_umap_proc_table();
        let mut caller = KProcess::new(ProcNr(0), Endpoint(100));
        // segment = LOCAL_VM_SEG | 0x99 (bogus index) → EFAULT.
        let mut msg = build_umap_msg(SELF, LOCAL_VM_SEG | 0x99, 0x1000, SELF, 0x100_i32);
        let priv_table = crate::test_helpers::test_priv_table();
        let result = dispatch_umap_remote(&mut caller, &mut msg, &proc_table, &priv_table);
        assert_eq!(result, KcallResult::Ok(EFAULT));
    }

    #[test]
    fn test_dispatch_umap_rejects_non_self_non_grant() {
        // C: do_umap.c:34 — seg_index != MEM_GRANT && endpt != SELF → EPERM
        let (proc_table, _, target_ep) = make_umap_proc_table();
        let mut caller = KProcess::new(ProcNr(0), Endpoint(100));
        // src_endpt = target (not SELF), seg_index = VIR_ADDR (not MEM_GRANT) → EPERM.
        let mut msg = build_umap_msg(
            target_ep.0,
            LOCAL_VM_SEG | VIR_ADDR,
            0x1000,
            SELF,
            0x100_i32,
        );
        let priv_table = crate::test_helpers::test_priv_table();
        let result = dispatch_umap(&mut caller, &mut msg, &proc_table, &priv_table);
        assert_eq!(result, KcallResult::Ok(EPERM));
    }

    // ── dispatch_safememset tests ───────────────────────────────────

    /// Helper: build a MessSysSafememset message.
    fn build_safememset_msg(
        dst_endpt: i32,
        grant_id: i32,
        offset: i64,
        pattern: i32,
        bytes: i64,
    ) -> Message {
        let mut msg = Message::default();
        msg.m_type = Syscall::Safememset as i32;
        msg.m_u.m_sys_safememset.dst_endpt = dst_endpt;
        msg.m_u.m_sys_safememset.grant_id = grant_id;
        msg.m_u.m_sys_safememset.offset = offset;
        msg.m_u.m_sys_safememset.pattern = pattern;
        msg.m_u.m_sys_safememset.bytes = bytes;
        msg
    }

    #[test]
    fn test_dispatch_safememset_rejects_none_dst() {
        // C: do_safememset.c:36-37 — dst_endpt == NONE → EFAULT
        let proc_table = crate::test_helpers::test_proc_table();
        let priv_table = crate::test_helpers::test_priv_table();
        let mut caller = KProcess::new(ProcNr(0), Endpoint(100));
        let msg = build_safememset_msg(NONE, 0, 0, 0, 0);
        let result = dispatch_safememset(&mut caller, &msg, &proc_table, &priv_table);
        assert_eq!(result, KcallResult::Ok(EFAULT));
    }

    #[test]
    fn test_dispatch_safememset_rejects_invalid_dst_endpoint() {
        // C: do_safememset.c:39-40 — endpoint_lookup fails → EINVAL
        use crate::proc::RtsFlagsBits;
        let mut proc_table = crate::test_helpers::test_proc_table();
        let priv_table = crate::test_helpers::test_priv_table();
        // Activate caller at slot 0 (caller is valid).
        if let Some(p) = proc_table.get_mut(ProcNr(0)) {
            p.p_endpoint = Endpoint(100);
            p.p_rts_flags.clear(RtsFlagsBits::SLOT_FREE);
        }
        let mut caller = KProcess::new(ProcNr(0), Endpoint(100));
        // dst_endpt = 9999 is not in proc_table.
        let msg = build_safememset_msg(9999, 0, 0, 0, 0);
        let result = dispatch_safememset(&mut caller, &msg, &proc_table, &priv_table);
        assert_eq!(result, KcallResult::Ok(EINVAL));
    }

    #[test]
    fn test_dispatch_safememset_rejects_dst_without_grant_table() {
        // C: do_safememset.c:42-45 — priv(dst_p) && s_grant_table == NULL → EINVAL
        use crate::proc::RtsFlagsBits;
        let mut proc_table = crate::test_helpers::test_proc_table();
        let mut priv_table = crate::test_helpers::test_priv_table();
        // Activate caller at slot 0.
        if let Some(p) = proc_table.get_mut(ProcNr(0)) {
            p.p_endpoint = Endpoint(100);
            p.p_rts_flags.clear(RtsFlagsBits::SLOT_FREE);
        }
        // Activate dst at slot 1 with a privilege but NO grant table.
        if let Some(p) = proc_table.get_mut(ProcNr(1)) {
            p.p_endpoint = Endpoint(200);
            p.p_rts_flags.clear(RtsFlagsBits::SLOT_FREE);
            // Assign a static privilege slot for the dst process.
            // assign_static maps proc_nr → priv_id (NR_TASKS + proc_nr).
            let priv_id = priv_table.assign_static(ProcNr(1)).expect("static priv slot 1");
            p.priv_id = Some(priv_id);
        }
        let mut caller = KProcess::new(ProcNr(0), Endpoint(100));
        let msg = build_safememset_msg(200, 0, 0i64, 0, 0i64);
        let result = dispatch_safememset(&mut caller, &msg, &proc_table, &priv_table);
        assert_eq!(result, KcallResult::Ok(EINVAL));
    }

    #[test]
    fn test_dispatch_safememset_valid_setup_suspends_on_grant_read() {
        // C: do_safememset.c:56 — vm_memset(caller, ...) returns OK.
        // verify_grant is now wired: it reads the grant entry from the
        // granter's user space via data_copy_vmcheck. In the test env,
        // MockPteWalk returns None → the copy suspends → VmSuspend.
        use crate::proc::RtsFlagsBits;
        let mut proc_table = crate::test_helpers::test_proc_table();
        let mut priv_table = crate::test_helpers::test_priv_table();
        if let Some(p) = proc_table.get_mut(ProcNr(0)) {
            p.p_endpoint = Endpoint(100);
            p.p_rts_flags.clear(RtsFlagsBits::SLOT_FREE);
        }
        if let Some(p) = proc_table.get_mut(ProcNr(1)) {
            p.p_endpoint = Endpoint(200);
            p.p_rts_flags.clear(RtsFlagsBits::SLOT_FREE);
            let priv_id = priv_table.assign_static(ProcNr(1)).expect("static priv slot 1");
            // Set s_grant_endpoint = granter's endpoint to avoid the
            // temporary-grant-table ENOTREADY path, so verify_grant
            // proceeds to read the grant entry (which suspends in test env).
            if let Some(kpriv) = priv_table.get_mut(priv_id) {
                kpriv.runtime.s_grant_table = 1; // non-zero = "has grant table"
                kpriv.runtime.s_grant_endpoint = Endpoint(200); // matches granter
                kpriv.runtime.s_grant_entries = 16; // allow grant index 0
            }
            p.priv_id = Some(priv_id);
        }
        let mut caller = KProcess::new(ProcNr(0), Endpoint(100));
        let msg = build_safememset_msg(200, 0, 0i64, 0xABi32, 0x100i64);
        let result = dispatch_safememset(&mut caller, &msg, &proc_table, &priv_table);
        // verify_grant reads grant entry via data_copy_vmcheck → MockPteWalk
        // returns None → Suspended → VmSuspend.
        assert_eq!(result, KcallResult::VmSuspend);
    }

    // ── dispatch_safecopy_from tests ───────────────────────────────

    /// Helper: build a MessLsysKernSafecopy message.
    fn build_safecopy_msg(
        from_to: i32,
        grant_id: i32,
        offset: u64,
        address: u64,
        bytes: u64,
    ) -> Message {
        let mut msg = Message::default();
        msg.m_type = Syscall::SafecopyFrom as i32;
        msg.m_u.m_lsys_kern_safecopy.from_to = from_to;
        msg.m_u.m_lsys_kern_safecopy.grant_id = grant_id;
        msg.m_u.m_lsys_kern_safecopy.offset = offset;
        msg.m_u.m_lsys_kern_safecopy.address = address;
        msg.m_u.m_lsys_kern_safecopy.bytes = bytes;
        msg
    }

    #[test]
    fn test_dispatch_safecopy_from_rejects_none_granter() {
        // C: do_safecopy.c:290-293 — granter == NONE || grantee == NONE → EFAULT
        let proc_table = crate::test_helpers::test_proc_table();
        let priv_table = crate::test_helpers::test_priv_table();
        let mut caller = KProcess::new(ProcNr(0), Endpoint(100));
        let msg = build_safecopy_msg(NONE, 0, 0, 0, 0);
        let result = dispatch_safecopy_from(&mut caller, &msg, &proc_table, &priv_table);
        assert_eq!(result, KcallResult::Ok(EFAULT));
    }

    #[test]
    fn test_dispatch_safecopy_from_rejects_invalid_granter() {
        // C: do_safecopy.c:63-67 — verify_grant → endpoint_lookup fails → EINVAL
        let proc_table = crate::test_helpers::test_proc_table();
        let priv_table = crate::test_helpers::test_priv_table();
        let mut caller = KProcess::new(ProcNr(0), Endpoint(100));
        let msg = build_safecopy_msg(9999, 0, 0, 0, 0);
        let result = dispatch_safecopy_from(&mut caller, &msg, &proc_table, &priv_table);
        assert_eq!(result, KcallResult::Ok(EINVAL));
    }

    #[test]
    fn test_dispatch_safecopy_from_rejects_negative_grant_id() {
        // C: cp_grant_id_t is i32; negative IDs are invalid.
        use crate::proc::RtsFlagsBits;
        let mut proc_table = crate::test_helpers::test_proc_table();
        let priv_table = crate::test_helpers::test_priv_table();
        if let Some(p) = proc_table.get_mut(ProcNr(0)) {
            p.p_endpoint = Endpoint(200);
            p.p_rts_flags.clear(RtsFlagsBits::SLOT_FREE);
        }
        let mut caller = KProcess::new(ProcNr(0), Endpoint(100));
        // grant_id = -1 (INVALID_GRANT sentinel) or any negative.
        let msg = build_safecopy_msg(200, -1, 0, 0, 0);
        let result = dispatch_safecopy_from(&mut caller, &msg, &proc_table, &priv_table);
        assert_eq!(result, KcallResult::Ok(EINVAL));
    }

    #[test]
    fn test_dispatch_safecopy_from_no_grant_table_returns_eperm() {
        // Granter exists but has no grant table (s_grant_table == 0).
        // verify_grant returns EPERM (do_safecopy.c:96-101 — HASGRANTTABLE check).
        use crate::proc::RtsFlagsBits;
        let mut proc_table = crate::test_helpers::test_proc_table();
        let priv_table = crate::test_helpers::test_priv_table();
        if let Some(p) = proc_table.get_mut(ProcNr(0)) {
            p.p_endpoint = Endpoint(200);
            p.p_rts_flags.clear(RtsFlagsBits::SLOT_FREE);
        }
        let mut caller = KProcess::new(ProcNr(0), Endpoint(100));
        let msg = build_safecopy_msg(200, 1, 0, 0x1000, 0x100);
        let result = dispatch_safecopy_from(&mut caller, &msg, &proc_table, &priv_table);
        assert_eq!(result, KcallResult::Ok(EPERM));
    }

    // ── dispatch_safecopy_to tests (F-10) ──────────────────────────
    //
    // Mirror the SAFECOPYFROM tests above; both share input validation
    // via safecopy_common_impl. The only difference is the access flag.

    #[test]
    fn test_dispatch_safecopy_to_rejects_none_granter() {
        // C: do_safecopy.c:290-293 — granter == NONE → EFAULT
        let proc_table = crate::test_helpers::test_proc_table();
        let priv_table = crate::test_helpers::test_priv_table();
        let mut caller = KProcess::new(ProcNr(0), Endpoint(100));
        let msg = build_safecopy_msg(NONE, 0, 0, 0, 0);
        let result = dispatch_safecopy_to(&mut caller, &msg, &proc_table, &priv_table);
        assert_eq!(result, KcallResult::Ok(EFAULT));
    }

    #[test]
    fn test_dispatch_safecopy_to_rejects_invalid_granter() {
        // C: do_safecopy.c:63-67 — endpoint_lookup fails → EINVAL
        let proc_table = crate::test_helpers::test_proc_table();
        let priv_table = crate::test_helpers::test_priv_table();
        let mut caller = KProcess::new(ProcNr(0), Endpoint(100));
        let msg = build_safecopy_msg(9999, 0, 0, 0, 0);
        let result = dispatch_safecopy_to(&mut caller, &msg, &proc_table, &priv_table);
        assert_eq!(result, KcallResult::Ok(EINVAL));
    }

    #[test]
    fn test_dispatch_safecopy_to_rejects_negative_grant_id() {
        use crate::proc::RtsFlagsBits;
        let mut proc_table = crate::test_helpers::test_proc_table();
        let priv_table = crate::test_helpers::test_priv_table();
        if let Some(p) = proc_table.get_mut(ProcNr(0)) {
            p.p_endpoint = Endpoint(200);
            p.p_rts_flags.clear(RtsFlagsBits::SLOT_FREE);
        }
        let mut caller = KProcess::new(ProcNr(0), Endpoint(100));
        let msg = build_safecopy_msg(200, -1, 0, 0, 0);
        let result = dispatch_safecopy_to(&mut caller, &msg, &proc_table, &priv_table);
        assert_eq!(result, KcallResult::Ok(EINVAL));
    }

    #[test]
    fn test_dispatch_safecopy_to_no_grant_table_returns_eperm() {
        // Granter exists but has no grant table (s_grant_table == 0).
        // verify_grant returns EPERM (do_safecopy.c:96-101 — HASGRANTTABLE check).
        use crate::proc::RtsFlagsBits;
        let mut proc_table = crate::test_helpers::test_proc_table();
        let priv_table = crate::test_helpers::test_priv_table();
        if let Some(p) = proc_table.get_mut(ProcNr(0)) {
            p.p_endpoint = Endpoint(200);
            p.p_rts_flags.clear(RtsFlagsBits::SLOT_FREE);
        }
        let mut caller = KProcess::new(ProcNr(0), Endpoint(100));
        let msg = build_safecopy_msg(200, 1, 0, 0x1000, 0x100);
        let result = dispatch_safecopy_to(&mut caller, &msg, &proc_table, &priv_table);
        assert_eq!(result, KcallResult::Ok(EPERM));
    }

    // ── dispatch_memset tests ──────────────────────────────────────

    /// Helper: build a MessLsysKrnSysMemset message.
    fn build_memset_msg(base: u64, count: u64, pattern: u64, process: i32) -> Message {
        let mut msg = Message::default();
        msg.m_type = Syscall::Memset as i32;
        msg.m_u.m_lsys_krn_sys_memset.base = base;
        msg.m_u.m_lsys_krn_sys_memset.count = count;
        msg.m_u.m_lsys_krn_sys_memset.pattern = pattern;
        msg.m_u.m_lsys_krn_sys_memset.process = process;
        msg
    }

    #[test]
    fn test_dispatch_memset_rejects_invalid_process() {
        // C: vm_memset:540-541 — process != NONE && !endpoint_lookup → ESRCH
        let proc_table = crate::test_helpers::test_proc_table();
        let mut caller = KProcess::new(ProcNr(0), Endpoint(100));
        // process = 9999 is not in proc_table.
        let msg = build_memset_msg(0x1000, 0x100, 0xAB, 9999);
        let result = dispatch_memset(&mut caller, &msg, &proc_table);
        assert_eq!(result, KcallResult::Ok(ESRCH));
    }

    #[test]
    fn test_dispatch_memset_valid_process_returns_vm_suspend() {
        // C: vm_memset:553-580 — memset in a process with no page table
        // (phys_root = 0) → PTE walk fails → VmSuspend.
        // This is the expected behavior when the target page is not yet
        // faulted in; VM will handle the fault and kernel_call_resume
        // will retry.
        use crate::proc::RtsFlagsBits;
        let mut proc_table = crate::test_helpers::test_proc_table();
        if let Some(p) = proc_table.get_mut(ProcNr(0)) {
            p.p_endpoint = Endpoint(200);
            p.p_rts_flags.clear(RtsFlagsBits::SLOT_FREE);
        }
        let mut caller = KProcess::new(ProcNr(0), Endpoint(100));
        let msg = build_memset_msg(0x1000, 0x100, 0xAB, 200);
        let result = dispatch_memset(&mut caller, &msg, &proc_table);
        assert_eq!(result, KcallResult::VmSuspend);
    }

    #[test]
    fn test_dispatch_memset_physical_zero_bytes_is_noop() {
        // C: vm_memset:553 — count == 0 is a no-op.
        // Physical address with count=0 returns OK without touching memory.
        let proc_table = crate::test_helpers::test_proc_table();
        let mut caller = KProcess::new(ProcNr(0), Endpoint(100));
        let msg = build_memset_msg(0x1000, 0, 0xAB, NONE);
        let result = dispatch_memset(&mut caller, &msg, &proc_table);
        assert_eq!(result, KcallResult::Ok(OK));
    }

    #[test]
    fn test_dispatch_memset_overflow_returns_e2big() {
        // base + count overflow → E2BIG (matches C do_copy.c:77).
        let proc_table = crate::test_helpers::test_proc_table();
        let mut caller = KProcess::new(ProcNr(0), Endpoint(100));
        let msg = build_memset_msg(u64::MAX, 1, 0xAB, NONE);
        let result = dispatch_memset(&mut caller, &msg, &proc_table);
        assert_eq!(result, KcallResult::Ok(E2BIG));
    }

    #[test]
    fn test_dispatch_memset_pattern_truncation_accepted() {
        // C: vm_memset:543 — pattern & 0xFF (fold higher bits away).
        // High-bit patterns are not rejected as invalid; the truncation
        // is internal. Use count=0 to avoid actual memory writes.
        let proc_table = crate::test_helpers::test_proc_table();
        let mut caller = KProcess::new(ProcNr(0), Endpoint(100));
        // pattern = 0xAB_CD_EF_12 — only 0x12 is the actual byte.
        let msg = build_memset_msg(0x1000, 0, 0xABCDEF12, NONE);
        let result = dispatch_memset(&mut caller, &msg, &proc_table);
        assert_eq!(result, KcallResult::Ok(OK));
    }

    // ── dispatch_vsafecopy tests (F-11) ────────────────────────────

    /// Helper: build a MessLsysKernVsafecopy message.
    fn build_vsafecopy_msg(vec_addr: u64, vec_size: i32) -> Message {
        let mut msg = Message::default();
        msg.m_type = Syscall::Vsafecopy as i32;
        msg.m_u.m_lsys_kern_vsafecopy.vec_addr = vec_addr;
        msg.m_u.m_lsys_kern_vsafecopy.vec_size = vec_size;
        msg
    }

    #[test]
    fn test_dispatch_vsafecopy_rejects_none_caller() {
        // C: do_safecopy.c:408 — `assert(src.proc_nr_e != NONE)` → EFAULT
        // (we replace the panic with a graceful EFAULT return).
        let proc_table = crate::test_helpers::test_proc_table();
        let priv_table = crate::test_helpers::test_priv_table();
        let mut caller = KProcess::new(ProcNr(0), Endpoint(NONE));
        let msg = build_vsafecopy_msg(0x1000, 1);
        let result = dispatch_vsafecopy(&mut caller, &msg, &proc_table, &priv_table);
        assert_eq!(result, KcallResult::Ok(EFAULT));
    }

    #[test]
    fn test_dispatch_vsafecopy_rejects_zero_vec_size() {
        // C: do_safecopy.c:414-415 — els = 0 → no-op loop.
        // We reject it explicitly (more defensive than the C no-op).
        let proc_table = crate::test_helpers::test_proc_table();
        let priv_table = crate::test_helpers::test_priv_table();
        let mut caller = KProcess::new(ProcNr(0), Endpoint(100));
        let msg = build_vsafecopy_msg(0x1000, 0);
        let result = dispatch_vsafecopy(&mut caller, &msg, &proc_table, &priv_table);
        assert_eq!(result, KcallResult::Ok(EINVAL));
    }

    #[test]
    fn test_dispatch_vsafecopy_rejects_negative_vec_size() {
        // C: do_safecopy.c:414 — els < 0 → no-op loop (signed i32).
        // We reject it for clarity.
        let proc_table = crate::test_helpers::test_proc_table();
        let priv_table = crate::test_helpers::test_priv_table();
        let mut caller = KProcess::new(ProcNr(0), Endpoint(100));
        let msg = build_vsafecopy_msg(0x1000, -1);
        let result = dispatch_vsafecopy(&mut caller, &msg, &proc_table, &priv_table);
        assert_eq!(result, KcallResult::Ok(EINVAL));
    }

    #[test]
    fn test_dispatch_vsafecopy_rejects_overflow_vec_size() {
        // C: do_safecopy.c:415 — `els * sizeof(vscp_vec)` overflow check.
        // MAX_VSCPVEC + 1 must be rejected.
        let proc_table = crate::test_helpers::test_proc_table();
        let priv_table = crate::test_helpers::test_priv_table();
        let mut caller = KProcess::new(ProcNr(0), Endpoint(100));
        let msg = build_vsafecopy_msg(0x1000, MAX_VSCPVEC + 1);
        let result = dispatch_vsafecopy(&mut caller, &msg, &proc_table, &priv_table);
        assert_eq!(result, KcallResult::Ok(EINVAL));
    }

    #[test]
    fn test_dispatch_vsafecopy_vec_copy_fails_without_real_page_tables() {
        // With a valid caller endpoint and vec_size, the dispatcher
        // proceeds to data_copy_vmcheck to copy the vec from user space.
        // Without real page tables (test mock), the PTE walk fails →
        // EFAULT or VmSuspend (depending on mock PteWalk behavior).
        let proc_table = crate::test_helpers::test_proc_table();
        let priv_table = crate::test_helpers::test_priv_table();
        let mut caller = KProcess::new(ProcNr(0), Endpoint(100));
        let msg = build_vsafecopy_msg(0x1000, 4);
        let result = dispatch_vsafecopy(&mut caller, &msg, &proc_table, &priv_table);
        // The copy fails because the caller has no real page tables.
        // Accept either EFAULT (Completed Err) or VmSuspend (Suspended).
        assert!(
            result == KcallResult::Ok(EFAULT) || result == KcallResult::VmSuspend,
            "expected EFAULT or VmSuspend, got {:?}",
            result
        );
    }

    #[test]
    fn test_vscp_vec_struct_size_matches_c_layout() {
        // C: safecopy.h:55-71 — struct vscp_vec has 6 fields.
        // The Rust repr(C) layout MUST match the C layout exactly,
        // since the kernel will (eventually) copy a user-provided
        // vector directly into a VscpVec array.
        assert_eq!(core::mem::size_of::<VscpVec>(), 40);
    }

    // ── dispatch_vumap tests (F-14) ────────────────────────────────

    /// Helper: build a MessLsysKrnSysVumap message.
    fn build_vumap_msg(
        endpt: i32,
        vaddr: u64,
        vcount: i32,
        paddr: u64,
        pmax: i32,
        access: i32,
        offset: u64,
    ) -> Message {
        let mut msg = Message::default();
        msg.m_type = Syscall::Vumap as i32;
        msg.m_u.m_lsys_krn_sys_vumap.endpt = endpt;
        msg.m_u.m_lsys_krn_sys_vumap.vaddr = vaddr;
        msg.m_u.m_lsys_krn_sys_vumap.vcount = vcount;
        msg.m_u.m_lsys_krn_sys_vumap.paddr = paddr;
        msg.m_u.m_lsys_krn_sys_vumap.pmax = pmax;
        msg.m_u.m_lsys_krn_sys_vumap.access = access;
        msg.m_u.m_lsys_krn_sys_vumap.offset = offset;
        msg
    }

    #[test]
    fn test_dispatch_vumap_rejects_none_caller() {
        // C: do_vumap.c:37 — caller must have a valid endpoint.
        let proc_table = crate::test_helpers::test_proc_table();
        let mut caller = KProcess::new(ProcNr(0), Endpoint(NONE));
        let mut msg = build_vumap_msg(SELF, 0x1000, 4, 0x2000, 4, VUA_READ, 0);
        let priv_table = crate::test_helpers::test_priv_table();
        let result = dispatch_vumap(&mut caller, &mut msg, &proc_table, &priv_table);
        assert_eq!(result, KcallResult::Ok(EFAULT));
    }

    #[test]
    fn test_dispatch_vumap_rejects_zero_vcount() {
        // C: do_vumap.c:48 — vcount <= 0 → EINVAL
        let proc_table = crate::test_helpers::test_proc_table();
        let mut caller = KProcess::new(ProcNr(0), Endpoint(100));
        let mut msg = build_vumap_msg(SELF, 0x1000, 0, 0x2000, 4, VUA_READ, 0);
        let priv_table = crate::test_helpers::test_priv_table();
        let result = dispatch_vumap(&mut caller, &mut msg, &proc_table, &priv_table);
        assert_eq!(result, KcallResult::Ok(EINVAL));
    }

    #[test]
    fn test_dispatch_vumap_rejects_zero_pmax() {
        // C: do_vumap.c:48 — pmax <= 0 → EINVAL
        let proc_table = crate::test_helpers::test_proc_table();
        let mut caller = KProcess::new(ProcNr(0), Endpoint(100));
        let mut msg = build_vumap_msg(SELF, 0x1000, 4, 0x2000, 0, VUA_READ, 0);
        let priv_table = crate::test_helpers::test_priv_table();
        let result = dispatch_vumap(&mut caller, &mut msg, &proc_table, &priv_table);
        assert_eq!(result, KcallResult::Ok(EINVAL));
    }

    #[test]
    fn test_dispatch_vumap_rejects_unknown_access() {
        // C: do_vumap.c:59 — default case in switch → EINVAL.
        // access = 0xFF is neither VUA_READ, VUA_WRITE, nor their OR.
        let proc_table = crate::test_helpers::test_proc_table();
        let mut caller = KProcess::new(ProcNr(0), Endpoint(100));
        let mut msg = build_vumap_msg(SELF, 0x1000, 4, 0x2000, 4, 0xFF, 0);
        let priv_table = crate::test_helpers::test_priv_table();
        let result = dispatch_vumap(&mut caller, &mut msg, &proc_table, &priv_table);
        assert_eq!(result, KcallResult::Ok(EINVAL));
    }

    #[test]
    fn test_dispatch_vumap_rejects_invalid_source_endpoint() {
        // C: do_vumap.c:79 — when source != SELF, the granter must exist.
        use crate::proc::RtsFlagsBits;
        let mut proc_table = crate::test_helpers::test_proc_table();
        if let Some(p) = proc_table.get_mut(ProcNr(0)) {
            p.p_endpoint = Endpoint(200);
            p.p_rts_flags.clear(RtsFlagsBits::SLOT_FREE);
        }
        let mut caller = KProcess::new(ProcNr(0), Endpoint(100));
        // source = 9999 is not in proc_table.
        let mut msg = build_vumap_msg(9999, 0x1000, 4, 0x2000, 4, VUA_READ, 0);
        let priv_table = crate::test_helpers::test_priv_table();
        let result = dispatch_vumap(&mut caller, &mut msg, &proc_table, &priv_table);
        assert_eq!(result, KcallResult::Ok(EINVAL));
    }

    #[test]
    fn test_dispatch_vumap_self_source_suspends_on_copy_fault() {
        // C: do_vumap.c:84-86 — source == SELF bypasses grant verification.
        // data_copy_vmcheck of vvec from caller fails (MockPteWalk returns
        // None) → Suspended → VmSuspend in the test environment.
        let proc_table = crate::test_helpers::test_proc_table();
        let mut caller = KProcess::new(ProcNr(0), Endpoint(100));
        let mut msg = build_vumap_msg(SELF, 0x1000, 4, 0x2000, 4, VUA_READ, 0);
        let priv_table = crate::test_helpers::test_priv_table();
        let result = dispatch_vumap(&mut caller, &mut msg, &proc_table, &priv_table);
        assert_eq!(result, KcallResult::VmSuspend);
    }

    #[test]
    fn test_dispatch_vumap_grant_source_suspends_on_copy_fault() {
        // C: do_vumap.c:79-87 — verify_grant would be called for non-SELF
        // source, but the vvec copy from the caller fails first
        // (MockPteWalk returns None) → Suspended → VmSuspend in the test
        // environment, before grant verification is reached.
        use crate::proc::RtsFlagsBits;
        let mut proc_table = crate::test_helpers::test_proc_table();
        if let Some(p) = proc_table.get_mut(ProcNr(0)) {
            p.p_endpoint = Endpoint(200);
            p.p_rts_flags.clear(RtsFlagsBits::SLOT_FREE);
        }
        let mut caller = KProcess::new(ProcNr(0), Endpoint(100));
        let mut msg = build_vumap_msg(200, 0x1000, 4, 0x2000, 4, VUA_READ | VUA_WRITE, 0);
        let priv_table = crate::test_helpers::test_priv_table();
        let result = dispatch_vumap(&mut caller, &mut msg, &proc_table, &priv_table);
        assert_eq!(result, KcallResult::VmSuspend);
    }

    #[test]
    fn test_dispatch_vumap_clamps_oversize_vcount() {
        // C: do_vumap.c:51 — vcount > MAPVEC_NR is silently clamped.
        // We don't return EINVAL; we accept and clamp (matching C). The
        // clamping/validation passes, but the actual vvec copy suspends in
        // the test environment (MockPteWalk returns None) → VmSuspend.
        let proc_table = crate::test_helpers::test_proc_table();
        let mut caller = KProcess::new(ProcNr(0), Endpoint(100));
        let mut msg = build_vumap_msg(SELF, 0x1000, 1000, 0x2000, 4, VUA_READ, 0);
        let priv_table = crate::test_helpers::test_priv_table();
        let result = dispatch_vumap(&mut caller, &mut msg, &proc_table, &priv_table);
        assert_eq!(result, KcallResult::VmSuspend);
    }
}
