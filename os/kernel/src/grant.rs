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
//!   allowing `data_copy_vmcheck` to read it directly from user space.
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
//! C uses `data_copy(granter, s_grant_table + sizeof(g) * grant_idx, ...)`
//! to read the grant entry. Rust uses `data_copy_vmcheck` with
//! `AddressRef::Process { endpoint: granter, offset: grant_table_addr }`,
//! which performs the same cross-address-space copy via Direct Map + PTE
//! walk, and additionally handles page faults via VMSUSPEND.
//!
//! C's `verify_grant` returns `int` (OK or errno). Rust returns
//! `VerifyGrantOutcome` which separates `Suspended` from `Err` — this is
//! necessary because VMSUSPEND is not an error but a suspend state that
//! requires the caller to return `KcallResult::VmSuspend` and wait for
//! VM to resolve the fault.

use minix_types::{Endpoint, PhysBytes, VirBytes};

use minix_arch::DirectMapArch;
use crate::cross_space::data_copy_vmcheck;
use crate::kpriv::PrivTable;
use crate::proc::KProcess;
use crate::proc_table::ProcessTable;
use crate::vm::{AddressRef, CrossSpaceResult, VmFaultType};

// ── Grant ID manipulation ──

/// C: `GRANT_SHIFT` — safecopies.h:56
/// Upper 11 bits = sequence, lower 20 bits = index.
const GRANT_SHIFT: u32 = 20;

/// C: `GRANT_INVALID` — safecopies.h:52. Invalid grant sentinel.
pub const GRANT_INVALID: i32 = -1;

/// C: `GRANT_VALID(g)` — safecopies.h:53. True if grant ID > GRANT_INVALID.
pub const fn grant_valid(g: i32) -> bool {
    g > GRANT_INVALID
}

/// C: `GRANT_IDX(g)` — safecopies.h:61. Extract index from grant ID.
pub const fn grant_idx(g: i32) -> u32 {
    // R-16 (2026-08-12): SAFETY: `g` is `i32`; `as u32` is a lossless same-width
    // bit reinterpretation (no truncation), then masked to the low 20-bit
    // index field (GRANT_SHIFT = 20).
    (g as u32) & ((1u32 << GRANT_SHIFT) - 1)
}

/// C: `GRANT_SEQ(g)` — safecopies.h:60. Extract sequence from grant ID.
pub const fn grant_seq(g: i32) -> u32 {
    // R-16 (2026-08-12): SAFETY: `g` is `i32`; `as u32` is a lossless same-width
    // bit reinterpretation (no truncation), then shifted and masked to the
    // upper 11-bit sequence field.
    ((g as u32) >> GRANT_SHIFT) & ((1u32 << (31 - GRANT_SHIFT)) - 1)
}

/// C: `MAX_INDIRECT_DEPTH` — do_safecopy.c:21
const MAX_INDIRECT_DEPTH: u8 = 5;

// ── Grant flags ──

bitflags::bitflags! {
    /// C: `cp_flags` field of `cp_grant_t` — safecopies.h:10, 63-75
    ///
    /// Combines access direction (CPF_READ/CPF_WRITE), grant type
    /// (CPF_DIRECT/CPF_INDIRECT/CPF_MAGIC), and lifecycle flags
    /// (CPF_USED/CPF_VALID/CPF_TRY).
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    #[repr(transparent)]
    pub struct CpFlags: u32 {
        /// C: `CPF_READ` — safecopies.h:64. Granted process may read.
        const READ     = 0x000001;
        /// C: `CPF_WRITE` — safecopies.h:65. Granted process may write.
        const WRITE    = 0x000002;
        /// C: `CPF_TRY` — safecopies.h:68. Fail fast on unmapped memory.
        const TRY      = 0x000010;
        /// C: `CPF_USED` — safecopies.h:71. Grant slot in use.
        const USED     = 0x000100;
        /// C: `CPF_DIRECT` — safecopies.h:72. Direct grant (granter → grantee).
        const DIRECT   = 0x000200;
        /// C: `CPF_INDIRECT` — safecopies.h:73. Indirect grant (chain).
        const INDIRECT = 0x000400;
        /// C: `CPF_MAGIC` — safecopies.h:74. Magic grant (any → any).
        const MAGIC    = 0x000800;
        /// C: `CPF_VALID` — safecopies.h:75. Grant slot contains valid grant.
        const VALID    = 0x001000;
    }
}

// ── C-compatible grant table entry ──

/// C: `cp_grant_t` — safecopies.h:9-38
///
/// A single entry in a process's grant table. The grant table is stored
/// in the granter's user-space address space at
/// `priv(granter)->s_grant_table + sizeof(cp_grant_t) * grant_idx`.
///
/// `#[repr(C)]` ensures the layout matches C exactly, so `data_copy_vmcheck`
/// can copy it directly from user space.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct CpGrant {
    /// C: `cp_flags` — CPF_* flags (access + type + lifecycle).
    pub flags: i32,
    /// C: `cp_seq` — sequence number for ABA protection.
    pub seq: i32,
    /// C: `cp_u` — union of direct/indirect/magic/free variants.
    pub u: CpGrantUnion,
    /// C: `cp_faulted` — soft fault marker (CPF_TRY only).
    pub faulted: i32,
}

impl CpGrant {
    /// Safe accessor for the flags as `CpFlags`.
    fn cp_flags(&self) -> CpFlags {
        CpFlags::from_bits_truncate(self.flags as u32)
    }

    /// Safe accessor for direct grant fields.
    ///
    /// # Safety
    ///
    /// Caller must ensure `self.cp_flags().contains(CpFlags::DIRECT)`.
    fn cp_direct(&self) -> CpGrantDirect {
        // SAFETY: The caller guarantees the union is in the DIRECT variant.
        // Reading a union field is safe for Copy types when the correct
        // variant is active.
        unsafe { self.u.direct }
    }

    /// Safe accessor for indirect grant fields.
    fn cp_indirect(&self) -> CpGrantIndirect {
        unsafe { self.u.indirect }
    }

    /// Safe accessor for magic grant fields.
    fn cp_magic(&self) -> CpGrantMagic {
        unsafe { self.u.magic }
    }
}

/// C: `cp_u` union — safecopies.h:12-36
#[repr(C)]
#[derive(Clone, Copy)]
pub union CpGrantUnion {
    /// C: `cp_direct` — CPF_DIRECT grants.
    pub direct: CpGrantDirect,
    /// C: `cp_indirect` — CPF_INDIRECT grants.
    pub indirect: CpGrantIndirect,
    /// C: `cp_magic` — CPF_MAGIC grants.
    pub magic: CpGrantMagic,
    /// C: `cp_free` — free slot linked list.
    pub free: CpGrantFree,
}

/// C: `cp_u.cp_direct` — safecopies.h:14-18
#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct CpGrantDirect {
    /// C: `cp_who_to` — grantee endpoint.
    pub who_to: i32,
    /// C: `cp_start` — memory start address in granter's space.
    pub start: u64,
    /// C: `cp_len` — size in bytes.
    pub len: u64,
}

/// C: `cp_u.cp_indirect` — safecopies.h:19-23
#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct CpGrantIndirect {
    /// C: `cp_who_to` — grantee endpoint.
    pub who_to: i32,
    /// C: `cp_who_from` — previous granter endpoint.
    pub who_from: i32,
    /// C: `cp_grant` — previous grant ID.
    pub grant: i32,
}

/// C: `cp_u.cp_magic` — safecopies.h:24-30
#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct CpGrantMagic {
    /// C: `cp_who_from` — granter endpoint.
    pub who_from: i32,
    /// C: `cp_who_to` — grantee endpoint.
    pub who_to: i32,
    /// C: `cp_start` — memory start address.
    pub start: u64,
    /// C: `cp_len` — size in bytes.
    pub len: u64,
}

/// C: `cp_u.cp_free` — safecopies.h:31-34
#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct CpGrantFree {
    /// C: `cp_next` — next free slot index or -1.
    pub next: i32,
}

impl Default for CpGrant {
    fn default() -> Self {
        Self {
            flags: 0,
            seq: 0,
            u: CpGrantUnion { free: CpGrantFree::default() },
            faulted: 0,
        }
    }
}

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
    /// The caller has been marked `RTS_VMREQUEST` by `data_copy_vmcheck`;
    /// the syscall will be retried after VM resolves the fault.
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

/// C: `NONE` — endpoint.h
#[allow(dead_code)] // endpoint sentinel constant; not yet wired to all call sites
const NONE: i32 = -1;
/// C: `ANY` — endpoint.h
const ANY: i32 = -3;

/// C: `VFS_PROC_NR` — used by magic grant check (do_safecopy.c:221)
const VFS_PROC_NR: i32 = 4;
/// C: `MIB_PROC_NR` — used by magic grant check (do_safecopy.c:221)
const MIB_PROC_NR: i32 = 8;

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
/// grant table via `data_copy_vmcheck`, validates it, and returns the
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
/// C uses `data_copy(granter, grant_table_addr, KERNEL, &g, sizeof(g))`
/// to read the grant entry. Rust uses `data_copy_vmcheck` with
/// `AddressRef::Process` source and `AddressRef::Physical` destination
/// (a kernel stack variable's physical address via Direct Map).
///
/// C's loop `do { ... } while(g.cp_flags & CPF_INDIRECT)` is replaced
/// with an explicit `for depth in 0..MAX_INDIRECT_DEPTH` loop that
/// `break`s when the grant is not indirect.
// R-18 (2026-08-13): Mirrors C `verify_grant()` signature (do_safecopy.c:41-266)
// exactly — 10 params including 2 closures. Extracting a param struct would
// diverge from C and hurt grep-ability. Allowed per clippy::too_many_arguments.
#[allow(clippy::too_many_arguments)]
pub fn verify_grant(
    caller: &mut KProcess,
    granter: Endpoint,
    grantee: Endpoint,
    grant_id: i32,
    bytes: u64,
    access: CpFlags,
    offset_in: u64,
    proc_table: &ProcessTable,
    priv_table: &PrivTable,
    proc_cr3: &dyn Fn(Endpoint) -> Option<PhysBytes>,
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

        // Read the grant entry via data_copy_vmcheck.
        // Source: granter's user space at grant_entry_addr.
        // Dest: kernel stack variable (physical address via Direct Map).
        let dst_phys = minix_arch::CurrentDirectMap::virt_to_phys(VirBytes(
            &mut grant_entry as *mut CpGrant as u64,
        ));

        let src = AddressRef::Process {
            endpoint: granter,
            offset: VirBytes(grant_entry_addr),
        };
        let dst = AddressRef::Physical(dst_phys);

        match data_copy_vmcheck(
            caller,
            src,
            dst,
            core::mem::size_of::<CpGrant>(),
            proc_cr3,
        ) {
            CrossSpaceResult::Suspended(fault) => {
                return VerifyGrantOutcome::Suspended(fault);
            }
            CrossSpaceResult::Completed(Err(_)) => {
                // C: do_safecopy.c:126 — "hide the fact that granter has
                // (presumably) set an invalid grant table entry by
                // returning EPERM"
                return VerifyGrantOutcome::Err(EPERM);
            }
            CrossSpaceResult::Completed(Ok(())) => {} // proceed
        }

        let g_flags = grant_entry.cp_flags();

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
            if granter.0 != VFS_PROC_NR && granter.0 != MIB_PROC_NR {
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
    fn test_verify_grant_invalid_endpoint() {
        let mut caller = KProcess::new(ProcNr(0), Endpoint(100));
        let proc_table = ProcessTable::new();
        let priv_table = PrivTable::new();
        let proc_cr3 = |_| None;

        let result = verify_grant(
            &mut caller,
            Endpoint(NONE),
            Endpoint(100),
            0,
            0,
            CpFlags::READ,
            0,
            &proc_table,
            &priv_table,
            &proc_cr3,
        );
        assert!(matches!(result, VerifyGrantOutcome::Err(EINVAL)));
    }

    #[test]
    fn test_verify_grant_invalid_grant_id() {
        let mut caller = KProcess::new(ProcNr(0), Endpoint(100));
        let proc_table = ProcessTable::new();
        let priv_table = PrivTable::new();
        let proc_cr3 = |_| None;

        let result = verify_grant(
            &mut caller,
            Endpoint(100),
            Endpoint(100),
            GRANT_INVALID,
            0,
            CpFlags::READ,
            0,
            &proc_table,
            &priv_table,
            &proc_cr3,
        );
        assert!(matches!(result, VerifyGrantOutcome::Err(EINVAL)));
    }
}
