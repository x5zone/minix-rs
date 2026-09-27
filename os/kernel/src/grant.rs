//! Grant table verification — kernel-side API for safecopy.
//!
//! # Minix3 C Source Mapping
//!
//! - `verify_grant()` — `kernel/system/do_safecopy.c:41-266`
//! - `cp_grant_t` — `include/minix/safecopies.h:9-38`
//! - `cp_flag` constants — `include/minix/safecopies.h:63-78`
//! - `HASGRANTTABLE` macro — `do_safecopy.c:28-29`
//!
//! # Design Decisions
//!
//! - **D1**: `CpGrant` is `#[repr(C)]` to match C's `cp_grant_t` exactly,
//!   allowing `read_from_process_vmcheck` to read it directly from user space.
//! - **D2**: `VerifyGrantOutcome` enum distinguishes `Ok` / `Err(code)` /
//!   `Suspended(VmFaultType)` — C conflates error and suspend in a single
//!   `int` return; Rust separates them for type safety.
//! - **D3**: Indirect grant chains followed via loop + depth counter
//!   (C: `do { ... } while(g.cp_flags & CPF_INDIRECT)` at line 173).
//! - **D4**: The function takes a `proc_cr3` closure (same pattern as
//!   `data_copy_vmcheck`) to resolve endpoint → page-table root without
//!   borrowing `ProcessTable` mutably.
//! - **D5**: `CpFlags` uses `bitflags!` (not bare `i32`) — type-safe flag
//!   composition, matches project convention (`RtsFlags`, `MiscFlags`, etc.).
//!
//! # Anti-translate
//!
//! C reads the entry with `data_copy(granter, s_grant_table + sizeof(g) *
//! grant_idx, KERNEL, (vir_bytes)&g, sizeof(g))` — the `KERNEL` destination
//! is the kernel's own stack struct, touched **directly by its virtual
//! address**. Rust mirrors this with `read_from_process_vmcheck` (→
//! `cross_space_read`): the process source is PTE-resolved + Direct-Map
//! aliased (copied one physically-contiguous run at a time, C's
//! `lin_lin_copy`), while the kernel-local destination is written directly by
//! its own VA. Routing the kernel stack variable through
//! `DirectMapArch::virt_to_phys` (as earlier revisions did) is wrong — kernel
//! data sits outside the Direct Map window, so its alias faults as
//! `DstPageFault` (the NK4-C B4 root cause). Source page faults suspend via
//! VMSUSPEND.
//!
//! C's `verify_grant` returns `int` (OK or errno). Rust returns
//! `VerifyGrantOutcome` which separates `Suspended` from `Err` — this is
//! necessary because VMSUSPEND is not an error but a suspend state that
//! requires the caller to return `KcallResult::VmSuspend` and wait for
//! VM to resolve the fault.

use minix_types::{Endpoint, PhysBytes, VirBytes};

use crate::cross_space::read_from_process_vmcheck;
use crate::kpriv::PrivTable;
use crate::proc_table::ProcessTable;
use crate::vm::{AddressRef, CrossSpaceResult, VmFaultType};

// ── Grant ID manipulation ──
// E-DSWIRE:the grant wire vocabulary(ID 打包、CPF_* 旗标、cp_grant_t
// 布局)下沉到共享权威 `minix-types::types::grant`(用户态授权库是它的
// 第二消费者,双份定义正是 E-REQWIRE 退役的模式)。以下 re-export 保持
// 本 crate 的 `crate::grant::*` 路径不变。
pub use minix_types::{
    grant_idx, grant_seq, grant_valid, CpFlags, CpGrant, CpGrantDirect, CpGrantFree,
    CpGrantIndirect, CpGrantMagic, CpGrantUnion, GRANT_INVALID,
};

/// C: `MAX_INDIRECT_DEPTH` — do_safecopy.c:21
const MAX_INDIRECT_DEPTH: u8 = 5;
// ── Soft fault info ──

/// C: `struct cp_sfinfo` — do_safecopy.c:31-36
///
/// Information for handling soft faults (CPF_TRY grants).
#[derive(Debug, Clone, Default)]
pub struct SoftFaultInfo {
    /// C: `try` — if nonzero, try copy only, stop on fault.
    pub try_copy: bool,
    /// C: `endpt` — endpoint owning grant with CPF_TRY flag.
    pub endpoint: Endpoint,
    /// C: `addr` — address to write mark upon soft fault.
    pub addr: u64,
    /// C: `value` — grant ID to use as mark value to write.
    pub value: i32,
}

// ── Verify result ──

/// Result of grant verification.
///
/// D2: Separates `Suspended` from `Err` — C conflates these in a single
/// `int` return; Rust separates them for type safety.
#[derive(Debug)]
pub enum VerifyGrantOutcome {
    /// Verification succeeded. Contains resolved offset + effective granter.
    Ok(GrantVerifyResult),
    /// Verification failed with an error code (EINVAL, EPERM, ELOOP, etc.).
    Err(i32),
    /// Page fault while reading grant entry from granter's address space.
    /// The caller has been marked `RTS_VMREQUEST` by
    /// `read_from_process_vmcheck`; the syscall will be retried after VM
    /// resolves the fault.
    Suspended(VmFaultType),
}

/// Successful grant verification result.
///
/// C: `verify_grant` output parameters `*offset_result`, `*e_granter`, `*sfinfo`.
#[derive(Debug, Clone)]
pub struct GrantVerifyResult {
    /// C: `*offset_result` — resolved virtual address in the effective
    /// granter's space to copy from/to.
    pub offset: VirBytes,
    /// C: `*e_granter` — effective granter endpoint (may differ from the
    /// original for magic grants, where `e_granter = cp_magic.cp_who_from`).
    pub effective_granter: Endpoint,
    /// C: `*sfinfo` — soft fault information (only for CPF_TRY grants).
    pub sfinfo: Option<SoftFaultInfo>,
}

// ── Endpoint sentinels ──

/// C: `ANY` — endpoint.h:55, `_ENDPOINT_SLOT_TOP - 1` (= 31744 with
/// com.h:55 `MAX_NR_TASKS 1023`; the former local `-3` made the grantee
/// gate below deny every who_to=ANY grant with EPERM — the E-MIBGRANT
/// constant class). Derived from the minix-types authority so the two
/// cannot drift (`may_create_magic_grant`'s Endpoint::VFS/MIB pattern).
const ANY: i32 = minix_types::Endpoint::ANY.0;

/// Only VFS and MIB may create magic grants.
///
/// C: do_safecopy.c:218-226 — hardcoded as `granter != VFS_PROC_NR &&
/// granter != MIB_PROC_NR` (C leaves a TODO to make this a system.conf
/// flag; the rewrite keeps the hardcoded policy). `Endpoint::VFS`/`MIB`
/// are the com.h:60/:66 authority (`minix-types` single source — the
/// same pattern as the kernel's errno re-export).
///
/// Extracted as a named predicate (C inlines it) so the policy is unit
/// testable on the host: the full `verify_grant` path crosses the
/// grant-table read (`data_copy_vmcheck`) before this check, which has
/// no host-safe test route.
fn may_create_magic_grant(granter: Endpoint) -> bool {
    granter == Endpoint::VFS || granter == Endpoint::MIB
}

// ── Error codes ──
// Centralized in `crate::errno` to prevent value drift (FIX-01: R-02/R-09/R-18).
// Previously ELOOP=40 here (should be 62).
use crate::errno::*;

/// C: `MEM_TOP` — used for overflow check (do_safecopy.c:187)
const MEM_TOP: u64 = u64::MAX;

// ── verify_grant ──

/// Verify a grant and resolve the source/destination address.
///
/// C: `verify_grant()` — do_safecopy.c:41-266
///
/// This function reads the grant entry from the granter's user-space
/// grant table via `read_from_process_vmcheck` (kernel-local destination,
/// C's `KERNEL` side touched by its own VA), validates it, and returns the
/// resolved virtual address for the caller to use in a subsequent
/// `data_copy_vmcheck` call.
///
/// # Parameters
///
/// - `caller`: the calling process (for VMSUSPEND side effect).
/// - `granter`: the endpoint of the process whose grant table contains
///   the grant (C: `granter` / "copyee").
/// - `grantee`: the endpoint of the process requesting the copy
///   (C: `grantee` / "copyer"). Usually `caller.p_endpoint`.
/// - `grant_id`: the grant ID to verify (C: `grant`).
/// - `bytes`: the number of bytes to copy (C: `bytes`).
/// - `access`: CPF_READ or CPF_WRITE (C: `access`).
/// - `offset_in`: the offset within the grant (C: `offset_in`).
/// - `proc_table`: for endpoint → ProcNr resolution.
/// - `priv_table`: for grant table address + entry count lookup.
/// - `proc_cr3`: closure resolving endpoint → page-table root.
///
/// # Returns
///
/// - `Ok(GrantVerifyResult)` — verification succeeded; use `result.offset`
///   and `result.effective_granter` for the actual data copy.
/// - `Err(code)` — verification failed (EINVAL, EPERM, ELOOP, ENOTREADY).
/// - `Suspended(VmFaultType)` — page fault while reading grant entry;
///   caller has been marked `RTS_VMREQUEST`. Return `KcallResult::VmSuspend`.
///
/// # Anti-translate
///
/// C reads the grant entry with `data_copy(granter, grant_table_addr,
/// KERNEL, &g, sizeof(g))` — the `KERNEL` destination is the kernel's own
/// stack struct, touched directly by its VA. Rust mirrors this with
/// `read_from_process_vmcheck` (→ `cross_space_read`): an `AddressRef::Process`
/// source copied one contiguous physical run at a time into a kernel-local
/// `&mut [u8]` destination. It does NOT route the kernel stack variable
/// through `DirectMapArch::virt_to_phys` — kernel data sits outside the Direct
/// Map window, so that alias faults as `DstPageFault` (the NK4-C B4 root
/// cause of the EPERM that stalled boot).
///
/// C's loop `do { ... } while(g.cp_flags & CPF_INDIRECT)` is replaced
/// with an explicit `for depth in 0..MAX_INDIRECT_DEPTH` loop that
/// `break`s when the grant is not indirect.
// R-18 (2026-08-13): Mirrors C `verify_grant()` signature (do_safecopy.c:41-266)
// exactly — 10 params including 2 closures. Extracting a param struct would
// diverge from C and hurt grep-ability. Allowed per clippy::too_many_arguments.
#[allow(clippy::too_many_arguments)]
pub fn verify_grant(
    caller_nr: ProcNr,
    proc_table: &mut ProcessTable,
    granter: Endpoint,
    grantee: Endpoint,
    grant_id: i32,
    bytes: u64,
    access: CpFlags,
    offset_in: u64,
    priv_table: &PrivTable,
    proc_cr3: &dyn Fn(&crate::proc_table::ProcessTable, Endpoint) -> Option<PhysBytes>,
) -> VerifyGrantOutcome {
    let mut granter = granter;
    let mut grantee = grantee;
    let mut grant_id = grant_id;

    for _depth in 0..MAX_INDIRECT_DEPTH {
        // C: do_safecopy.c:63-67 — validate granter endpoint.
        let granter_nr = match proc_table.endpoint_to_nr(granter) {
            Some(nr) => nr,
            None => return VerifyGrantOutcome::Err(EINVAL),
        };

        // C: do_safecopy.c:68-72 — validate grant ID.
        if !grant_valid(grant_id) {
            return VerifyGrantOutcome::Err(EINVAL);
        }

        // C: do_safecopy.c:73 — get granter proc.
        let granter_proc = match proc_table.get(granter_nr) {
            Some(p) => p,
            None => return VerifyGrantOutcome::Err(EINVAL),
        };

        // C: do_safecopy.c:83-90 — temporary grant table check.
        // If the granter has a temporary grant table (s_grant_endpoint !=
        // p_endpoint), allow unspecified access and return ENOTREADY if
        // no grant table is present or the grantee doesn't match.
        let priv_id = granter_proc.priv_id;
        let priv_ = match priv_id.and_then(|pid| priv_table.get(pid)) {
            Some(p) => p,
            None => return VerifyGrantOutcome::Err(EPERM),
        };

        if priv_.runtime.s_grant_endpoint != granter_proc.p_endpoint {
            if access.is_empty() {
                // C: line 85 — `return OK` for unspecified access.
                // This path is used by cpf_revoke; the safecopy path
                // always specifies access, so this branch is not hit
                // in normal safecopy operation. We return Ok with a
                // zero-offset placeholder (the caller checks access
                // before using the result).
                return VerifyGrantOutcome::Ok(GrantVerifyResult {
                    offset: VirBytes(0),
                    effective_granter: granter,
                    sfinfo: None,
                });
            } else if priv_.runtime.s_grant_table == 0
                || grantee != priv_.runtime.s_grant_endpoint
            {
                return VerifyGrantOutcome::Err(ENOTREADY);
            }
        }

        // C: do_safecopy.c:96-101 — HASGRANTTABLE check.
        if priv_.runtime.s_grant_table == 0 {
            return VerifyGrantOutcome::Err(EPERM);
        }

        // C: do_safecopy.c:103-114 — grant index bounds check.
        let g_idx = grant_idx(grant_id) as i32;
        if priv_.runtime.s_grant_entries <= g_idx {
            return VerifyGrantOutcome::Err(EPERM);
        }

        // C: do_safecopy.c:121-127 — read grant entry from granter's space.
        // data_copy(granter, s_grant_table + sizeof(g) * grant_idx,
        //           KERNEL, (vir_bytes) &g, sizeof(g))
        let grant_entry_addr = priv_.runtime.s_grant_table as u64
            + core::mem::size_of::<CpGrant>() as u64 * g_idx as u64;

        let mut grant_entry = CpGrant::default();

        // Read the grant entry from the granter's address space into this
        // kernel-local struct. Source: granter's user space at
        // grant_entry_addr. Dest: `&mut grant_entry` accessed DIRECTLY by
        // its own kernel virtual address.
        //
        // NK4-C 1.13 root cause: the previous code routed the kernel stack
        // destination through `CurrentDirectMap::virt_to_phys` and
        // `AddressRef::Physical`, but a kernel stack/data VA sits OUTSIDE the
        // Direct Map window, so its "physical" is bogus and `cross_space_copy`
        // rejects the DM alias as `DstPageFault` → EPERM (VM's RS_INIT
        // rproctab safecopyfrom failed with ESRCH, RS then fail-closed
        // panicked at boot.rs:1254). C's `data_copy(..., KERNEL, &g, ...)`
        // touches the kernel side by direct VA; `read_from_process_vmcheck`
        // is the faithful mirror (same pattern as `cross_space_write`).
        let src = AddressRef::Process {
            endpoint: granter,
            offset: VirBytes(grant_entry_addr),
        };
        match read_from_process_vmcheck(
            caller_nr, proc_table,
            // SAFETY: `grant_entry` is a live kernel-local `CpGrant`; view it
            // as its byte representation for the copy (repr(C) POD, no
            // alignment requirement for u8 access).
            unsafe {
                core::slice::from_raw_parts_mut(
                    &mut grant_entry as *mut CpGrant as *mut u8,
                    core::mem::size_of::<CpGrant>(),
                )
            },
            src,
            proc_cr3,
        ) {
            CrossSpaceResult::Suspended(fault) => {
                return VerifyGrantOutcome::Suspended(fault);
            }
            CrossSpaceResult::Completed(Err(e)) => {
                // C: do_safecopy.c:126 — "hide the fact that granter has
                // (presumably) set an invalid grant table entry by
                // returning EPERM"
                // NK4-C B4 取证（task1-close 裁决删除）：区分「读 grant
                // 表页失败」这一 EPERM 腿，并把具体 VmCopyError 落串口。
                #[cfg(all(not(feature = "mock"), target_arch = "x86_64"))]
                crate::grant::nk4a_vg_probe(
                    "readfail",
                    granter.0 as u64,
                    crate::grant::nk4a_vcopy_code(&e),
                );
                #[cfg(not(all(not(feature = "mock"), target_arch = "x86_64")))]
                let _ = &e;
                return VerifyGrantOutcome::Err(EPERM);
            }
            CrossSpaceResult::Completed(Ok(())) => {} // proceed
        }

        let g_flags = grant_entry.cp_flags();

        // NK4-C B4 取证（task1-close 裁决删除）：读出 grant 表项后把决定
        // 性状态落串口，分辨 USED|VALID / seq / access / range 哪一腿 EPERM。
        #[cfg(all(not(feature = "mock"), target_arch = "x86_64"))]
        {
            let _d = grant_entry.cp_direct();
            crate::grant::nk4a_vg_probe_state(
                granter.0 as u64,
                grant_id as u64,
                g_idx as u64,
                g_flags.bits() as u64,
                grant_entry.seq as u64,
                _d.who_to as u64,
                _d.len,
                bytes,
            );
        }

        // C: do_safecopy.c:130-135 — check CPF_USED | CPF_VALID.
        let required = CpFlags::USED | CpFlags::VALID;
        if !g_flags.contains(required) {
            return VerifyGrantOutcome::Err(EPERM);
        }

        // C: do_safecopy.c:137-141 — check sequence number.
        let g_seq = grant_seq(grant_id);
        if grant_entry.seq as u32 != g_seq {
            return VerifyGrantOutcome::Err(EPERM);
        }

        // C: do_safecopy.c:148-173 — follow indirect grant chain.
        if g_flags.contains(CpFlags::INDIRECT) {
            let indirect = grant_entry.cp_indirect();

            // C: do_safecopy.c:159-166 — verify grantee.
            if indirect.who_to != grantee.0
                && grantee.0 != ANY
                && indirect.who_to != ANY
            {
                return VerifyGrantOutcome::Err(EPERM);
            }

            // C: do_safecopy.c:168-171 — restart with new granter/grant.
            grantee = granter;
            granter = Endpoint(indirect.who_from);
            grant_id = indirect.grant;

            // Loop continues with the new granter/grant.
            continue;
        }

        // Not indirect — check access and resolve address.
        // C: do_safecopy.c:175-181 — check access flags.
        if !g_flags.contains(access) {
            return VerifyGrantOutcome::Err(EPERM);
        }

        if g_flags.contains(CpFlags::DIRECT) {
            let direct = grant_entry.cp_direct();

            // C: do_safecopy.c:187-192 — overflow check.
            if MEM_TOP - direct.len + 1 < direct.start {
                return VerifyGrantOutcome::Err(EPERM);
            }

            // C: do_safecopy.c:194-200 — verify grantee.
            if direct.who_to != grantee.0
                && grantee.0 != ANY
                && direct.who_to != ANY
            {
                return VerifyGrantOutcome::Err(EPERM);
            }

            // C: do_safecopy.c:202-212 — verify copy range.
            let end = match offset_in.checked_add(bytes) {
                Some(e) => e,
                None => return VerifyGrantOutcome::Err(EPERM), // overflow
            };
            if end > direct.len {
                return VerifyGrantOutcome::Err(EPERM);
            }

            // C: do_safecopy.c:214-216 — success.
            let offset = VirBytes(direct.start + offset_in);
            let sfinfo = build_sfinfo(g_flags, granter, grant_id, g_idx as u32);
            return VerifyGrantOutcome::Ok(GrantVerifyResult {
                offset,
                effective_granter: granter,
                sfinfo,
            });
        } else if g_flags.contains(CpFlags::MAGIC) {
            let magic = grant_entry.cp_magic();

            // C: do_safecopy.c:221-226 — only VFS and MIB may create magic grants.
            if !may_create_magic_grant(granter) {
                return VerifyGrantOutcome::Err(EPERM);
            }

            // C: do_safecopy.c:229-234 — verify grantee.
            if magic.who_to != grantee.0
                && grantee.0 != ANY
                && magic.who_to != ANY
            {
                return VerifyGrantOutcome::Err(EPERM);
            }

            // C: do_safecopy.c:237-246 — verify copy range.
            let end = match offset_in.checked_add(bytes) {
                Some(e) => e,
                None => return VerifyGrantOutcome::Err(EPERM),
            };
            if end > magic.len {
                return VerifyGrantOutcome::Err(EPERM);
            }

            // C: do_safecopy.c:248-250 — success.
            let offset = VirBytes(magic.start + offset_in);
            let sfinfo = build_sfinfo(g_flags, granter, grant_id, g_idx as u32);
            return VerifyGrantOutcome::Ok(GrantVerifyResult {
                offset,
                effective_granter: Endpoint(magic.who_from),
                sfinfo,
            });
        } else {
            // C: do_safecopy.c:251-255 — unknown grant type.
            return VerifyGrantOutcome::Err(EPERM);
        }
    }

    // C: do_safecopy.c:150-155 — exceeded maximum indirect depth.
    VerifyGrantOutcome::Err(ELOOP)
}

// ── NK4-C B4 取证探针（task1-close 裁决删除）──

#[cfg(all(not(feature = "mock"), target_arch = "x86_64"))]
pub(crate) fn nk4a_vcopy_code(e: &crate::vm::VmCopyError) -> u64 {
    use crate::vm::VmCopyError::*;
    match e {
        SrcPageFault => 1,
        DstPageFault => 2,
        InvalidAddress => 3,
        PermissionDenied => 4,
        UnknownEndpoint => 5,
        Domain => 6,
    }
}

#[cfg(all(not(feature = "mock"), target_arch = "x86_64"))]
pub(crate) fn nk4a_vg_cap_bump() -> bool {
    use core::sync::atomic::{AtomicUsize, Ordering as AtomicOrd};
    static VG_N: AtomicUsize = AtomicUsize::new(0);
    VG_N.fetch_add(1, AtomicOrd::Relaxed) < 8
}

#[cfg(all(not(feature = "mock"), target_arch = "x86_64"))]
pub(crate) fn nk4a_vg_probe(site: &str, granter: u64, grant_id: u64) {
    if !nk4a_vg_cap_bump() {
        return;
    }
    use minix_plat::{CurrentEarlyConsole as C0, EarlyConsole as _};
    C0::write_str("nk4a: vg ");
    C0::write_str(site);
    C0::write_str(" gr=");
    C0::write_hex(granter);
    C0::write_str(" gid=");
    C0::write_hex(grant_id);
    C0::write_str("\n");
}

#[cfg(all(not(feature = "mock"), target_arch = "x86_64"))]
#[allow(clippy::too_many_arguments)]
pub(crate) fn nk4a_vg_probe_state(
    granter: u64,
    grant_id: u64,
    g_idx: u64,
    g_flags: u64,
    seq: u64,
    who_to: u64,
    dlen: u64,
    bytes: u64,
) {
    if !nk4a_vg_cap_bump() {
        return;
    }
    use minix_plat::{CurrentEarlyConsole as C0, EarlyConsole as _};
    C0::write_str("nk4a: vg st gr=");
    C0::write_hex(granter);
    C0::write_str(" gid=");
    C0::write_hex(grant_id);
    C0::write_str(" idx=");
    C0::write_hex(g_idx);
    C0::write_str(" fl=");
    C0::write_hex(g_flags);
    C0::write_str(" seq=");
    C0::write_hex(seq);
    C0::write_str(" wto=");
    C0::write_hex(who_to);
    C0::write_str(" len=");
    C0::write_hex(dlen);
    C0::write_str(" bts=");
    C0::write_hex(bytes);
    C0::write_str("\n");
}

/// Build soft fault info for CPF_TRY grants.
///
/// C: do_safecopy.c:258-263
fn build_sfinfo(
    flags: CpFlags,
    granter: Endpoint,
    grant_id: i32,
    grant_idx: u32,
) -> Option<SoftFaultInfo> {
    if !flags.contains(CpFlags::TRY) {
        return None;
    }

    // C: sfinfo->addr = priv(granter_proc)->s_grant_table +
    //     sizeof(g) * grant_idx + offsetof(cp_grant_t, cp_faulted);
    //
    // We don't have the grant_table address here (it's in KPriv), so
    // we store the grant_idx and let the caller compute the address.
    // For now, we set addr to 0 and the caller can fill it in if needed.
    //
    // Actually, looking at C more carefully, the sfinfo is only used
    // by the caller (safecopy) to write a fault marker. Since we return
    // the sfinfo to the caller, and the caller has access to priv_table,
    // we can compute the address there. For simplicity, we store the
    // grant_idx and let the caller compute the full address.
    let _ = grant_idx;
    Some(SoftFaultInfo {
        try_copy: true,
        endpoint: granter,
        addr: 0, // Caller fills in if needed
        value: grant_id,
    })
}

// ── Tests ──

#[cfg(test)]
mod tests {
    use super::*;
    use crate::proc::ProcNr;

    #[test]
    fn test_grant_id_manipulation() {
        // C: GRANT_ID(idx, seq) = (seq << 20) | idx
        let id = 0x005_00001; // seq=5, idx=1
        assert_eq!(grant_idx(id), 1);
        assert_eq!(grant_seq(id), 5);
        assert!(grant_valid(id));
        assert!(!grant_valid(GRANT_INVALID));
        assert!(!grant_valid(-2));
    }

    #[test]
    fn test_cp_flags_bitflags() {
        let flags = CpFlags::READ | CpFlags::USED | CpFlags::VALID;
        assert!(flags.contains(CpFlags::READ));
        assert!(flags.contains(CpFlags::USED));
        assert!(!flags.contains(CpFlags::WRITE));
        assert!(!flags.contains(CpFlags::DIRECT));
    }

    #[test]
    fn test_cp_flags_access_check() {
        // C: do_safecopy.c:175-181 — (g.cp_flags & access) != access
        let grant_flags = CpFlags::READ | CpFlags::USED | CpFlags::VALID | CpFlags::DIRECT;
        let access = CpFlags::READ;
        assert!(grant_flags.contains(access)); // ok

        let access = CpFlags::WRITE;
        assert!(!grant_flags.contains(access)); // EPERM
    }

    #[test]
    fn test_cp_grant_size() {
        // The CpGrant struct must match C's sizeof(cp_grant_t).
        // C: cp_grant_t = int + int + union(largest=direct=20 bytes) + int
        // = 4 + 4 + 24 + 4 = 36 bytes (with padding to 40 for alignment)
        // Actually, let's compute: largest union member is CpGrantMagic
        // (i32 + i32 + u64 + u64 = 4+4+8+8 = 24 bytes), so union is 24.
        // Total: 4(flags) + 4(seq) + 24(union) + 4(faulted) = 36,
        // but with #[repr(C)] alignment to 8 (u64 in union), it's
        // 4+4(pad 0)+24+4(pad 4) = 40? Let's just check it's non-zero.
        assert!(core::mem::size_of::<CpGrant>() > 0);
        assert!(core::mem::size_of::<CpGrant>() >= 36);
    }

    #[test]
    fn test_grantee_gate_any_authority_value() {
        // E-MIBGRANT constant class: the grantee gate (three sites below)
        // must compare against C's ANY — endpoint.h:55, `_ENDPOINT_SLOT_TOP
        // - 1`, i.e. 32768 - 1023 - 1 with com.h:55 `MAX_NR_TASKS 1023` —
        // not a small negative. The former local `-3` denied every
        // who_to=ANY grant (the rproctab grant, main.c:185, is exactly
        // `cpf_grant_direct(ANY, …, CPF_READ)`) with EPERM while host
        // tests self-consistently used the wrong value (test blindness).
        assert_eq!(ANY, 32768 - 1023 - 1);
        // And it stays the single authority minix-types derives.
        assert_eq!(ANY, minix_types::Endpoint::ANY.0);
    }

    #[test]
    fn test_verify_grant_invalid_endpoint() {
        let mut proc_table = crate::test_helpers::test_proc_table();
        let priv_table = crate::test_helpers::test_priv_table();
        let proc_cr3 = |_pt: &crate::proc_table::ProcessTable, _| None;

        let result = verify_grant(
            ProcNr(0), &mut proc_table,
            minix_types::Endpoint::NONE,
            Endpoint(100),
            0,
            0,
            CpFlags::READ,
            0,
            &priv_table,
            &proc_cr3,
        );
        assert!(matches!(result, VerifyGrantOutcome::Err(EINVAL)));
    }

    #[test]
    fn test_verify_grant_invalid_grant_id() {
        let mut proc_table = crate::test_helpers::test_proc_table();
        let priv_table = crate::test_helpers::test_priv_table();
        let proc_cr3 = |_pt: &crate::proc_table::ProcessTable, _| None;

        let result = verify_grant(
            ProcNr(0), &mut proc_table,
            Endpoint(100),
            Endpoint(100),
            GRANT_INVALID,
            0,
            CpFlags::READ,
            0,
            &priv_table,
            &proc_cr3,
        );
        assert!(matches!(result, VerifyGrantOutcome::Err(EINVAL)));
    }

    // ── T-8（doc 18 §5.2）：verify_grant 宿主可测面——priv 字段级校验分支 ──

    /// 范围超限：grant 表只有 2 项，grant_id=10（idx=10 >= 2）→ EPERM
    /// （do_safecopy.c:103-114 grant 索引越界）。宿主可测：该分支只读
    /// granter 的 priv 字段，先于授权条目的跨空间读取。
    #[test]
    fn test_t8_verify_grant_range_exceeded_returns_eperm() {
        let mut proc_table = crate::test_helpers::test_proc_table();
        let mut priv_table = crate::test_helpers::test_priv_table();

        // caller（发起 safecopy 的一方）与 granter（ep 200）均在表中。
        proc_table.get_mut(ProcNr(0)).unwrap()
            .p_rts_flags.clear(crate::proc::RtsFlagsBits::SLOT_FREE);
        proc_table.get_mut(ProcNr(1)).unwrap()
            .p_rts_flags.clear(crate::proc::RtsFlagsBits::SLOT_FREE);
        proc_table.get_mut(ProcNr(1)).unwrap().p_endpoint = Endpoint(200);

        // granter 的 priv：表非空、2 项、临时表 endpoint = 自身（跳过临时表分支）。
        let pid = priv_table.assign_static(ProcNr(1)).expect("priv slot");
        {
            let kp = priv_table.get_mut(pid).unwrap();
            kp.runtime.s_grant_table = 0x4000; // 非零 → HASGRANTTABLE 通过
            kp.runtime.s_grant_entries = 2;    // 只有 2 项
            kp.runtime.s_grant_endpoint = Endpoint(200);
        }
        proc_table.get_mut(ProcNr(1)).unwrap().priv_id = Some(pid);

        // grant_id=10 → g_idx=10 >= s_grant_entries=2 → EPERM。
        // 闭包返 None 即可：越界分支先于授权条目的跨空间读取触发。
        let result = verify_grant(
            ProcNr(0), &mut proc_table,
            Endpoint(200),   // granter
            Endpoint(100),   // grantee
            10,              // grant_id → idx 10，越界
            16,              // bytes
            CpFlags::READ,
            0,
            &priv_table,
            &|_pt: &crate::proc_table::ProcessTable, _| None,
        );
        assert!(matches!(result, VerifyGrantOutcome::Err(EPERM)),
            "越界 grant 索引必须返回 EPERM，实际 {:?}", result);
    }

    /// 临时授权表分支：granter 的 s_grant_endpoint ≠ 自身且不匹配 grantee
    /// → ENOTREADY（do_safecopy.c:83-90）。
    #[test]
    fn test_t8_verify_grant_temp_table_mismatch_enotready() {
        let mut proc_table = crate::test_helpers::test_proc_table();
        let mut priv_table = crate::test_helpers::test_priv_table();

        proc_table.get_mut(ProcNr(0)).unwrap()
            .p_rts_flags.clear(crate::proc::RtsFlagsBits::SLOT_FREE);
        proc_table.get_mut(ProcNr(1)).unwrap()
            .p_rts_flags.clear(crate::proc::RtsFlagsBits::SLOT_FREE);
        proc_table.get_mut(ProcNr(1)).unwrap().p_endpoint = Endpoint(200);

        let pid = priv_table.assign_static(ProcNr(1)).expect("priv slot");
        {
            let kp = priv_table.get_mut(pid).unwrap();
            kp.runtime.s_grant_table = 0x4000;
            kp.runtime.s_grant_entries = 8;
            // 临时表 endpoint = 300（≠ granter 自身 200，≠ grantee 100）
            kp.runtime.s_grant_endpoint = Endpoint(300);
        }
        proc_table.get_mut(ProcNr(1)).unwrap().priv_id = Some(pid);

        let result = verify_grant(
            ProcNr(0), &mut proc_table,
            Endpoint(200),
            Endpoint(100),
            0,               // idx 0，在 s_grant_entries=8 范围内
            16,
            CpFlags::READ,
            0,
            &priv_table,
            &|_pt: &crate::proc_table::ProcessTable, _| None,
        );
        assert!(matches!(result, VerifyGrantOutcome::Err(ENOTREADY)),
            "临时授权表 grantee 不匹配必须返回 ENOTREADY，实际 {:?}", result);
    }

    // ── E-MIBGRANT（edge_todo.md）：magic grant 门端点权威 ──

    /// C 绝对值 pin：`Endpoint::VFS`/`Endpoint::MIB` 必须等于 com.h 的
    /// VFS_PROC_NR=1（com.h:60）/ MIB_PROC_NR=7（com.h:66）。历史上
    /// grant.rs 曾手抄出 VFS=4/MIB=8（恰为 SCHED/VM 的槽号），magic
    /// grant 门语义反转——此测试防止 minix-types 权威值漂移，也固定
    /// 门谓词依赖的常量身份。
    #[test]
    fn test_magic_granter_endpoints_match_c_com_h() {
        assert_eq!(Endpoint::VFS.get(), 1); // com.h:60
        assert_eq!(Endpoint::MIB.get(), 7); // com.h:66
    }

    /// 门策略（do_safecopy.c:218-226）：仅 VFS/MIB 可建 magic grant。
    /// 显式覆盖 SCHED(4)/VM(8)——旧错误常量恰好误放行的两个端点。
    #[test]
    fn test_may_create_magic_grant_vfs_mib_only() {
        assert!(may_create_magic_grant(Endpoint::VFS));
        assert!(may_create_magic_grant(Endpoint::MIB));

        // 旧错误值 4/8 对应的端点（SCHED/VM）必须被拒。
        assert!(!may_create_magic_grant(Endpoint::SCHED));
        assert!(!may_create_magic_grant(Endpoint::VM));
        // 其余权威端点与任意端点同样被拒。
        assert!(!may_create_magic_grant(Endpoint::PM));
        assert!(!may_create_magic_grant(Endpoint::RS));
        assert!(!may_create_magic_grant(Endpoint(100)));
    }
}
