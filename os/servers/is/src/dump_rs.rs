//! RS dump domain (08-is-dump-rs.md §4.1).
//!
//! C: `minix3/minix/servers/is/dmp_rs.c` (74 lines). One dump over two
//! tables (`SI_PROCPUB_TAB` + `SI_PROC_TAB`): identity (PUB) + state
//! (PRIV), `RS_IN_USE` filter, dual-source 6-char code. Bodies await the
//! A-6 output channel; this module delivers snapshots, encoders, and
//! format constants.
//!
//! `[ARCH: A-4]`: snapshots are wire-contract proposals (`#[repr(C)]`,
//! used-fields subsets); the RS-side GETSYSINFO producer aligns to them
//! (pending RS-crate work, explicit TODO below).

// TODO(P1): [code] [factual] Align RS-crate GETSYSINFO producers with the
// snapshots below — see 08 §4. (current state: IS-side wire proposals)
// (fix: RS `do_getsysinfo` SI_PROCPUB_TAB/SI_PROC_TAB payloads match; A-4).

/// RS public-entry snapshot (used fields only).
///
/// C: `struct rprocpub` — `minix3/minix/include/minix/rs.h:165-184` (subset).
#[derive(Debug, Clone, Copy, Default)]
#[repr(C)]
pub struct RprocpubSnap {
    /// C: `sys_flags` (rs.h:167).
    pub sys_flags: u32,
    /// C: `endpoint` (rs.h:168).
    pub endpoint: i32,
    /// C: `dev_nr` (rs.h:172).
    pub dev_nr: i32,
    /// C: `label[RS_MAX_LABEL_LEN]`, 16 (rs.h:58,177).
    pub label: [u8; 16],
}

/// RS private-entry snapshot (used fields only).
///
/// C: `struct rproc` — `minix3/minix/servers/rs/type.h:63-79` (subset;
/// `r_args[512]` excluded: streams through the output layer, see 08 §3 D1).
#[derive(Debug, Clone, Copy, Default)]
#[repr(C)]
pub struct RprocSnap {
    /// C: `r_pid` (type.h:63).
    pub r_pid: i32,
    /// C: `r_restarts` (type.h:66).
    pub r_restarts: i32,
    /// C: `r_flags` (type.h:68).
    pub r_flags: u32,
    /// C: `r_period` (type.h:71).
    pub r_period: i32,
    /// C: `r_alive_tm` (type.h:73).
    pub r_alive_tm: u32,
}

/// In-use filter. C: `rp->r_flags & RS_IN_USE` (`RS_IN_USE 0x001` —
/// rs/const.h:28; dmp_rs.c:44).
pub const fn rproc_in_use(flags: u32) -> bool {
    flags & 0x001 != 0
}

/// Dual-source status characters.
///
/// C: `s_flags_str(flags, sys_flags)` — dmp_rs.c:61-72 (`RS_ACTIVE 0x800→A`,
/// `RS_UPDATING 0x80→U`, `RS_EXITING 0x002→E`, `RS_NOPINGREPLY 0x008→N` —
/// rs/const.h:28-39; `SF_USE_COPY 0x008→C`, `SF_USE_REPL 0x020→R` —
/// rs.h:194-196). Two sources stay two parameters (never merged).
pub const fn rs_flags_str(flags: u32, sys_flags: u32) -> [u8; 7] {
    [
        if flags & 0x800 != 0 { b'A' } else { b'-' },
        if flags & 0x080 != 0 { b'U' } else { b'-' },
        if flags & 0x002 != 0 { b'E' } else { b'-' },
        if flags & 0x008 != 0 { b'N' } else { b'-' },
        if sys_flags & 0x008 != 0 { b'C' } else { b'-' },
        if sys_flags & 0x020 != 0 { b'R' } else { b'-' },
        0,
    ]
}

/// C: `"Reincarnation Server (RS) system process table dump\n"` — dmp_rs.c:40.
pub const RPROC_TITLE: &str = "Reincarnation Server (RS) system process table dump\n";
/// C: dmp_rs.c:41.
pub const RPROC_COLUMNS: &str =
    "----label---- endpoint- -pid- flags- -dev- -T- alive_tm starts command\n";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_flags_dual_source() {
        // C: dmp_rs.c:61-72; const.h:28-39; rs.h:194-196.
        assert_eq!(&rs_flags_str(0, 0), b"------\x00");
        assert_eq!(&rs_flags_str(0x800, 0), b"A-----\x00");
        assert_eq!(&rs_flags_str(0, 0x028), b"----CR\x00");
        // Same bit value, different sources stay independent.
        assert_eq!(&rs_flags_str(0x008, 0), b"---N--\x00");
        assert_eq!(&rs_flags_str(0, 0x008), b"----C-\x00");
    }

    #[test]
    fn test_in_use_filter() {
        // C: dmp_rs.c:44.
        assert!(rproc_in_use(0x001));
        assert!(!rproc_in_use(0x800));
        assert!(!rproc_in_use(0));
    }

    #[test]
    fn test_snapshots_default_zeroed() {
        assert_eq!(RprocpubSnap::default().label.len(), 16);
        let r = RprocSnap::default();
        assert_eq!((r.r_pid, r.r_flags), (0, 0));
    }

    #[test]
    fn test_format_strings_verbatim() {
        assert!(RPROC_COLUMNS.contains("alive_tm starts command"));
        assert!(RPROC_TITLE.starts_with("Reincarnation Server"));
    }
}
