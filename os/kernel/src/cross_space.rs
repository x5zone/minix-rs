//! Cross-address-space runtime: data_copy_vmcheck and CopyResult.
//!
//! # Minix3 C Source Mapping
//!
//! - `sys_datacopy` is a **user-space library macro** (`syslib.h:129`) that expands to
//!   `sys_vircopy(p1, v1, p2, v2, len, 0)`. There is **no** `SYS_DATACOPY` kernel call
//!   number and **no** `do_datacopy.c` file in Minix3.
//! - The kernel-side handler for `SYS_VIRCOPY`/`SYS_PHYSCOPY` is `do_copy()` in
//!   `kernel/system/do_copy.c`.
//! - `data_copy_vmcheck()` is defined in `arch/i386/memory.c:690` and
//!   `arch/earm/memory.c:595` (architecture-specific implementations).
//!
//! # ARCHITECTURE NOTE
//!
//! `dispatch_datacopy` is currently **dead code**: `syscall.rs` has no `Datacopy` variant
//! and does not route any kernel call to this function. In Minix3, `sys_datacopy` is a
//! user-space convenience macro that sends `SYS_VIRCOPY`, which is already handled by
//! `dispatch_vircopy` in `syscall_copy.rs`. This module's `data_copy_vmcheck` function
//! is the kernel-internal helper used by SIGSEND, GETINFO, etc., and should be wired
//! into those dispatch paths when they are implemented.
//!
//! # Design Decisions (23-cross-space-runtime.md §3)
//!
//! - **D2**: `CopyResult` enum distinguishes Ok/Fault/VmSuspend
//! - **D3**: Direct Map for 64-bit address space (whole-segment mapping)
//! - **D4**: Stack allocation for small copies, kernel heap for large

use minix_types::{Endpoint, Message, MessageM1};

use crate::proc::KProcess;
use crate::syscall::KcallResult;

// ── Minix3 error codes ──

const OK: i32 = 0;
const EFAULT: i32 = 14;
const EINVAL: i32 = 22;
const EDEADSRCDST: i32 = 29; // Minix3: src or dst endpoint dead/invalid

// ── SELF ──

const SELF: i32 = -2;

// ── CopyResult ──

/// Result of a cross-address-space copy operation.
///
/// Design decision D2: enum replaces C's multiple return code paths.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CopyResult {
    /// Copy completed successfully. C: return OK
    Ok,
    /// Copy failed: bad address or access violation. C: return EFAULT
    Fault,
    /// Copy suspended: process is waiting for VM to handle page fault.
    /// C: VMSUSPEND — sets RTS_VMSUSPEND on the caller.
    VmSuspend,
}

impl CopyResult {
    /// Convert to KcallResult for system call return.
    pub fn to_kcall_result(&self) -> KcallResult {
        match self {
            CopyResult::Ok => KcallResult::Ok(OK),
            CopyResult::Fault => KcallResult::Ok(EFAULT),
            CopyResult::VmSuspend => KcallResult::Ok(EFAULT), // VMSUSPEND handled separately
        }
    }
}

// ── Helper ──

fn msg_m1(msg: &Message) -> MessageM1 {
    // SAFETY: `m_type` has been validated by the caller to select the M1
    // format. All union variants share the same size and `#[repr(C)]`
    // layout, so reading a different variant is sound.
    unsafe { msg.m_u.m_m1 }
}

// ── Dispatch function ──

/// Dispatch SYS_DATACOPY.
///
/// **DEAD CODE**: Minix3 has no `SYS_DATACOPY` kernel call number. `sys_datacopy`
/// is a user-space macro (`syslib.h:129`) that expands to `sys_vircopy(..., 0)`,
/// which sends `SYS_VIRCOPY`. The kernel-side handler is `do_copy()` in
/// `kernel/system/do_copy.c`, already dispatched via `dispatch_vircopy` in
/// `syscall_copy.rs`.
///
/// This function exists as a placeholder for potential future use if a
/// separate `SYS_DATACOPY` call number is introduced.
pub fn dispatch_datacopy(caller: &mut KProcess, msg: &Message) -> KcallResult {
    let m1 = msg_m1(msg);
    // C: sys_datacopy is a macro (syslib.h:129), not a kernel call.
    // The actual kernel handler is do_copy() in do_copy.c for SYS_VIRCOPY.
    //
    // Note: this dispatch function uses the M1 message format for field
    // extraction. The real C handler (do_copy) uses the
    // `m_lsys_krn_sys_copy` format, but since this function is dead code
    // (no SYS_DATACOPY call number exists in Minix3), the M1 format
    // serves as a placeholder. The endpoint validation and SELF
    // replacement logic mirror do_copy.c:51-58.
    //
    // The Direct Map body of `data_copy_vmcheck` is DEFERRED (Direct
    // Map PTE walk + vmcheck not yet implemented).
    let mut src_endpt = m1.m1i1;
    let mut dst_endpt = m1.m1i2;
    let src_addr = m1.m1p1;
    let dst_addr = m1.m1p2;
    let bytes = m1.m1p1; // Placeholder: M1 format used here (dead code).
                         // The real C handler uses m_lsys_krn_sys_copy.nr_bytes,
                         // not m1.m1p1. This placeholder is adequate since
                         // no caller routes here.

    // SELF replacement (same as do_copy.c logic)
    if src_endpt == SELF {
        src_endpt = caller.p_endpoint.0;
    }
    if dst_endpt == SELF {
        dst_endpt = caller.p_endpoint.0;
    }

    // Endpoint validation. C uses `isokendpt()` which checks:
    //   1. Endpoint is in the valid slot range
    //   2. The slot is in IN_USE state
    //   3. The endpoint's generation matches
    // We can only do the slot-range check here without a global
    // ProcessTable; the full validation is performed by the actual
    // `dispatch_vircopy` path (see syscall_copy.rs:152-209).
    const NR_PROCS: i32 = 256; // minix-types endpoint max
    if src_endpt < 0 || src_endpt >= NR_PROCS || dst_endpt < 0 || dst_endpt >= NR_PROCS {
        return KcallResult::Ok(EDEADSRCDST);
    }

    // Call data_copy_vmcheck (arch/i386/memory.c:690). The body is
    // DEFERRED (see data_copy_vmcheck's own TODO); we pass the
    // extracted values through so a future Direct Map impl has the
    // contract already in place.
    let result = data_copy_vmcheck(
        caller,
        Endpoint(src_endpt),
        src_addr,
        Endpoint(dst_endpt),
        dst_addr,
        bytes as usize,
    );

    result.to_kcall_result()
}

// ── Kernel-internal cross-process copy ──

/// Kernel-internal cross-process copy with VM check.
///
/// C: `data_copy_vmcheck()` — arch/i386/memory.c:690, arch/earm/memory.c:595
///
/// This is the core function used by SIGSEND, DATACOPY, and other
/// operations that need to copy data across address spaces.
///
/// **Critical**: If this returns `CopyResult::VmSuspend`, the caller
/// MUST NOT modify process registers (e.g., in SIGSEND), because
/// the process will be resumed and the copy retried.
pub fn data_copy_vmcheck(
    caller: &mut KProcess,
    src_endpt: Endpoint,
    _src_addr: u64,
    dst_endpt: Endpoint,
    _dst_addr: u64,
    _bytes: usize,
) -> CopyResult {
    // C: data_copy_vmcheck — SELF replacement (already done by callers)
    // C: virtual_copy_vmcheck → Direct Map copy
    // On 64-bit: use Direct Map to access physical memory directly
    // On page fault: set RTS_VMSUSPEND, return VmSuspend

    // TODO: implement with Direct Map
    // For now, return Ok as placeholder
    let _ = (caller, src_endpt, dst_endpt);
    CopyResult::Ok
}

// ── Tests ──

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_copy_result_to_kcall() {
        assert!(matches!(CopyResult::Ok.to_kcall_result(), KcallResult::Ok(0)));
        assert!(matches!(CopyResult::Fault.to_kcall_result(), KcallResult::Ok(14)));
    }

    #[test]
    fn test_copy_result_variants() {
        assert_eq!(CopyResult::Ok, CopyResult::Ok);
        assert_ne!(CopyResult::Ok, CopyResult::Fault);
        assert_ne!(CopyResult::Fault, CopyResult::VmSuspend);
    }
}
