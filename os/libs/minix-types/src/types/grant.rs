//! Grant wire layout — the shared authority for C's `cp_grant_t` family
//! (`minix3/minix/include/minix/safecopies.h`).
//!
//! A grant table lives in the **granter's own address space** (C libsys
//! `safecopies.c`: libc manages slots, free list and sequence numbers in
//! user memory; `sys_setgrant` only registers the table address/size with
//! the kernel — `grant_direct`/`revoke` never trap). The kernel's
//! `verify_grant` reads these slots cross-space, so the layout is a
//! two-side wire contract: consumers are the kernel (`os/kernel/src/
//! grant.rs`) and every user-space client library that hands out grants
//! (minix-sys grant table, consumed by DS/devman/RS/VM clients).
//!
//! E-DSWIRE moved the family here from `os/kernel/src/grant.rs` (same
//! single-authority discipline as the SYS_* numbers): user-space cannot
//! depend on the kernel crate, and duplicating `cp_grant_t` would be the
//! exact two-truths pattern E-REQWIRE retired.

/// C: `GRANT_SHIFT` — safecopies.h:56. Upper 11 bits = sequence, lower
/// 20 bits = index.
pub const GRANT_SHIFT: u32 = 20;

/// C: `GRANT_MAX_SEQ` — safecopies.h:58.
pub const GRANT_MAX_SEQ: u32 = 1 << (31 - GRANT_SHIFT);

/// C: `GRANT_MAX_IDX` — safecopies.h:59.
pub const GRANT_MAX_IDX: u32 = 1 << GRANT_SHIFT;

/// C: `GRANT_INVALID` — safecopies.h:52. Invalid grant sentinel.
pub const GRANT_INVALID: i32 = -1;

/// C: `GRANT_VALID(g)` — safecopies.h:53.
pub const fn grant_valid(g: i32) -> bool {
    g > GRANT_INVALID
}

/// C: `GRANT_ID(idx, seq)` — safecopies.h:62.
pub const fn grant_id(idx: u32, seq: u32) -> i32 {
    ((seq << GRANT_SHIFT) | idx) as i32
}

/// C: `GRANT_IDX(g)` — safecopies.h:61.
pub const fn grant_idx(g: i32) -> u32 {
    // `g as u32` is a lossless same-width bit reinterpretation (R-16).
    (g as u32) & (GRANT_MAX_IDX - 1)
}

/// C: `GRANT_SEQ(g)` — safecopies.h:60.
pub const fn grant_seq(g: i32) -> u32 {
    (g as u32 >> GRANT_SHIFT) & (GRANT_MAX_SEQ - 1)
}

bitflags::bitflags! {
    /// C: `cp_flags` field of `cp_grant_t` — safecopies.h:10, 63-75.
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

/// C: `cp_u.cp_direct` — safecopies.h:14-18.
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

/// C: `cp_u.cp_indirect` — safecopies.h:19-23.
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

/// C: `cp_u.cp_magic` — safecopies.h:24-30.
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

/// C: `cp_u.cp_free` — safecopies.h:31-34.
#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct CpGrantFree {
    /// C: `cp_next` — next free slot index or -1.
    pub next: i32,
}

/// C: `cp_u` union — safecopies.h:12-36.
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

/// C: `cp_grant_t` — safecopies.h:7-37. One grant-table slot.
///
/// Layout contract: the kernel reads these slots cross-space in
/// `verify_grant`, so field order/offsets/padding are fixed by C.
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

impl Default for CpGrant {
    /// C zeroed/free-slot shape: `cp_u.cp_free.cp_next` meaningful only
    /// for library-managed free lists; zeroed is the neutral value.
    fn default() -> Self {
        Self {
            flags: 0,
            seq: 0,
            u: CpGrantUnion { free: CpGrantFree::default() },
            faulted: 0,
        }
    }
}

impl CpGrant {
    /// Safe accessor for the flags as [`CpFlags`] (truncates the
    /// internal flags — C's `cp_flags & CPF_*` read pattern).
    pub fn cp_flags(&self) -> CpFlags {
        CpFlags::from_bits_truncate(self.flags as u32)
    }

    /// Safe accessor for direct grant fields.
    ///
    /// # Safety contract (checked by the caller, not the type)
    ///
    /// The caller must have verified `self.cp_flags().contains(CpFlags::
    /// DIRECT)` before calling — the wrapper cannot check it internally
    /// (single read), so it keeps the kernel's original safe-surface shape:
    /// the variant read is the one unsafe block inside.
    pub fn cp_direct(&self) -> CpGrantDirect {
        // SAFETY: the caller's flags check (see contract above) guarantees
        // the DIRECT variant is active; reading a union field is safe for
        // Copy types when the correct variant is active.
        unsafe { self.u.direct }
    }

    /// Safe accessor for indirect grant fields (contract: INDIRECT active —
    /// same shape as [`Self::cp_direct`]).
    pub fn cp_indirect(&self) -> CpGrantIndirect {
        // SAFETY: caller verified the INDIRECT variant.
        unsafe { self.u.indirect }
    }

    /// Safe accessor for magic grant fields (contract: MAGIC active — same
    /// shape as [`Self::cp_direct`]).
    pub fn cp_magic(&self) -> CpGrantMagic {
        // SAFETY: caller verified the MAGIC variant.
        unsafe { self.u.magic }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::mem::{offset_of, size_of};

    /// C: safecopies.h:7-37 布局见证(i386 原生与 LP64 同布局:union 最大
    /// 成员 direct 为 24 字节(who_to@0、start@8、len@16,align 8),u 落
    /// @8..32,faulted @32,尾对齐到 40;kernel 侧 `test_cp_grant_size`
    /// 同钉)。
    #[test]
    fn test_cp_grant_layout() {
        assert_eq!(size_of::<CpGrant>(), 40);
        assert_eq!(offset_of!(CpGrant, flags), 0);
        assert_eq!(offset_of!(CpGrant, seq), 4);
        assert_eq!(offset_of!(CpGrant, u), 8);
        assert_eq!(offset_of!(CpGrant, faulted), 32);
    }

    /// Grant-ID 打包/拆解(safecopies.h:56-62):seq 高 11 位、idx 低 20 位。
    #[test]
    fn test_grant_id_roundtrip() {
        assert_eq!(GRANT_SHIFT, 20);
        assert_eq!(grant_id(5, 3), (3 << 20) | 5);
        let g = grant_id(0x1234, 0x2A);
        assert_eq!(grant_idx(g), 0x1234);
        assert_eq!(grant_seq(g), 0x2A);
        assert!(grant_valid(g));
        assert!(!grant_valid(GRANT_INVALID));
        // 上界:idx 20 位、seq 11 位。
        assert_eq!(GRANT_MAX_IDX, 1 << 20);
        assert_eq!(GRANT_MAX_SEQ, 1 << 11);
    }
}
