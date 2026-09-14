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
#[derive(Debug, Clone, Copy)]
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
    /// C: `r_args[MAX_COMMAND_LEN]` (type.h:79, rs/const.h:18 = 512) —
    /// the NUL-separated raw command; rproc_dmp prints it verbatim as the
    /// trailing `%s` column (dmp_rs.c:64). (V1-P1-3: field added with the
    /// execution face — the args column is the dump's raison d'être.)
    pub r_args: [u8; RS_MAX_COMMAND],
}

/// C: `MAX_COMMAND_LEN 512` — rs/const.h:18.
pub const RS_MAX_COMMAND: usize = 512;
/// C: `rprocpub[NR_SYS_PROCS]` / `rproc[NR_SYS_PROCS]` — dmp_rs.c:21-22
/// (`NR_SYS_PROCS 64`, sys_config.h:9).
pub const RS_TABLE_LEN: usize = 64;

impl Default for RprocSnap {
    // Manual: `[u8; 512]` has no `Default` (ds.rs neighbouring-payload
    // convention) — all-zero bytes are the C BSS initialiser.
    fn default() -> Self {
        Self {
            r_pid: 0,
            r_restarts: 0,
            r_flags: 0,
            r_period: 0,
            r_alive_tm: 0,
            r_args: [0; RS_MAX_COMMAND],
        }
    }
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

// ── render (V1-P1-3 execution face) ───────────────────────────────

use crate::PCStr;
use core::fmt;

/// rproc_dmp page cursor: PmCursor/VfsCursor's skip shape over the 64-slot
/// RS tables — the skip predicate is `RS_IN_USE` (C: dmp_rs.c:44), the
/// 22-bound/`\r`/wrap semantics are identical (C static prev_i, :50).
#[derive(Debug, Clone, Copy, Default)]
pub struct RsCursor {
    next: usize,
    n: u32,
}

impl RsCursor {
    pub const fn new() -> Self {
        Self { next: 0, n: 0 }
    }

    pub const fn next(&self) -> usize {
        self.next
    }

    /// C: `if (!(rp->r_flags & RS_IN_USE)) continue; if (++n > 22) break;`
    pub const fn push(&mut self, in_use: bool, idx: usize) -> bool {
        if !in_use {
            return true; // skipped, uncounted
        }
        self.n += 1;
        if self.n > 22 {
            self.next = idx;
            false // break: row NOT printed
        } else {
            true
        }
    }

    /// C: `if (i >= NR_SYS_PROCS) i = 0; else printf("--more--\r");`
    pub const fn finish(&mut self, exhausted: bool) {
        if exhausted {
            self.next = 0;
        }
    }
}

/// `rproc_dmp` (dmp_rs.c:26-57): one row per in-use RS service — identity
/// from the PUB table, state from the PRIV table, args verbatim.
pub fn render_rproc(
    out: &mut dyn fmt::Write,
    pubt: &[RprocpubSnap],
    privt: &[RprocSnap],
    cur: &mut RsCursor,
) -> fmt::Result {
    out.write_str(RPROC_TITLE)?;
    out.write_str(RPROC_COLUMNS)?;
    let mut exhausted = true;
    let len = privt.len().min(pubt.len());
    for i in 0..len {
        let (rpub, rp) = (&pubt[i], &privt[i]);
        if !rproc_in_use(rp.r_flags) {
            continue; // C: `if (!(rp->r_flags & RS_IN_USE)) continue;`
        }
        if !cur.push(true, i) {
            out.write_str(MORE_CR_RS)?;
            exhausted = false;
            break;
        }
        write!(
            out,
            "{:>13} {:9} {:5} {:>6} {:4} {:4} {:8} {:5}{}",
            PCStr(&rpub.label),
            rpub.endpoint,
            rp.r_pid,
            PCStr(&rs_flags_str(rp.r_flags, rpub.sys_flags)),
            rpub.dev_nr,
            rp.r_period,
            rp.r_alive_tm,
            rp.r_restarts,
            PCStr(&rp.r_args)
        )?;
        out.write_str("\n")?;
    }
    cur.finish(exhausted);
    Ok(())
}

/// C: `printf("--more--\r")` — dmp_rs.c:52.
pub const MORE_CR_RS: &str = "--more--\r";

#[cfg(test)]
mod tests {

    #[test]
    fn test_render_rproc_row_and_filter() {
        // C: dmp_rs.c:41-52 — IN_USE filter, %13s label, %6s flags, args
        // verbatim; pagination via RsCursor.
        let mut pubt = [RprocpubSnap::default(); 2];
        let mut privt = [RprocSnap::default(); 2];
        pubt[0].label = name16(b"driver.tty");
        pubt[0].sys_flags = 0x008; // SF_USE_COPY → C
        privt[0].r_flags = 0x801; // IN_USE + ACTIVE → "A- - -C-"
        privt[0].r_pid = 15;
        privt[0].r_args = args(b"tty /dev/c0");
        pubt[1].label = name16(b"dead.svc");
        privt[1].r_flags = 0; // not in use → skipped
        let mut out = String::new();
        let mut cur = RsCursor::new();
        render_rproc(&mut out, &pubt, &privt, &mut cur).unwrap();
        assert!(!out.contains("dead.svc"), "RS_IN_USE filter drops dead entries");
        assert_eq!(cur.next(), 0, "exhaustion wraps");
    }

    fn name16(name: &[u8]) -> [u8; 16] {
        let mut n = [0u8; 16];
        n[..name.len()].copy_from_slice(name);
        n
    }

    fn args(a: &[u8]) -> [u8; RS_MAX_COMMAND] {
        let mut n = [0u8; RS_MAX_COMMAND];
        n[..a.len()].copy_from_slice(a);
        n
    }

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
