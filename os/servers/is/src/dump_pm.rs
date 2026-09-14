//! PM dump domain (06-is-dump-pm.md §4.1).
//!
//! C: `minix3/minix/servers/is/dmp_pm.c` (109 lines). Two dumps over one
//! table (`SI_PROC_TAB`), an 11-char flag code, signal bitmap columns, an
//! alarm countdown (clock face), and a `\r`-paged cursor. Bodies await the
//! A-6 output channel; this module delivers snapshots, encoders, cursor,
//! and format constants.
//!
//! `[ARCH: A-4]`: `MProcSnap` is the wire-contract proposal (`#[repr(C)]`,
//! used-fields subset in C declaration order); the PM-side GETSYSINFO
//! producer aligns to it (pending PM-crate work, explicit TODO below).

// TODO(P1): [code] [factual] Align PM-crate GETSYSINFO producers with
// MProcSnap below — see 06 §4. (current state: IS-side wire proposal)
// (fix: PM `do_getsysinfo` SI_PROC_TAB payload matches this layout; A-4).

/// PM process-table snapshot (used fields only).
///
/// C: `struct mproc` — `minix3/minix/servers/pm/mproc.h` (subset).
#[derive(Debug, Clone, Copy, Default)]
#[repr(C)]
pub struct MProcSnap {
    /// C: `mp_pid` (mproc.h:28).
    pub mp_pid: i32,
    /// C: `mp_parent`, table index (mproc.h:33).
    pub mp_parent: i32,
    /// C: `mp_tracer` (mproc.h:34).
    pub mp_tracer: i32,
    /// C: `mp_name[PROC_NAME_LEN]` (mproc.h:80).
    pub mp_name: [u8; 16],
    /// C: `mp_procgrp` (mproc.h:30).
    pub mp_procgrp: i32,
    /// C: `mp_realuid` (mproc.h:41).
    pub mp_realuid: u32,
    /// C: `mp_effuid` (mproc.h:42).
    pub mp_effuid: u32,
    /// C: `mp_realgid` (mproc.h:44).
    pub mp_realgid: u32,
    /// C: `mp_effgid` (mproc.h:45).
    pub mp_effgid: u32,
    /// C: `mp_nice` (mproc.h:75).
    pub mp_nice: i32,
    /// C: `mp_flags` (mproc.h:66).
    pub mp_flags: u32,
    /// C: `mp_ignore.__bits[0]` (mproc.h:53; only the first word is dumped).
    pub mp_ignore0: u32,
    /// C: `mp_catch.__bits[0]` (mproc.h:54).
    pub mp_catch0: u32,
    /// C: `mp_sigmask.__bits[0]` (mproc.h:55).
    pub mp_sigmask0: u32,
    /// C: `mp_sigpending.__bits[0]` (mproc.h:57).
    pub mp_sigpending0: u32,
    /// C: `mp_timer.tmr_exp_time` (mproc.h:62, timers.h:35).
    pub mp_timer_exp: u32,
}

/// PM slot number (its own pid-0 row is kept). C: `PM_PROC_NR 0` — com.h:59.
pub const PM_PROC_NR_IDX: usize = 0;

/// PM flag characters.
///
/// C: `flags_str` — dmp_pm.c:21-39 (`WAITING 0x2→W`, `ZOMBIE 0x4→Z`,
/// `ALARM_ON 0x10→A`, `EXITING 0x20→E`, `TRACE_STOPPED 0x80→T`,
/// `SIGSUSPENDED 0x100→U`, `VFS_CALL 0x400→F`, `PROC_STOPPED 0x8→s`,
/// `PRIV_PROC 0x2000→p`, `PARTIAL_EXEC 0x4000→x`, `DELAY_CALL 0x20000→d` —
/// mproc.h:87-102).
pub const fn pm_flags_str(flags: u32) -> [u8; 12] {
    [
        if flags & 0x00002 != 0 { b'W' } else { b'-' },
        if flags & 0x00004 != 0 { b'Z' } else { b'-' },
        if flags & 0x00010 != 0 { b'A' } else { b'-' },
        if flags & 0x00020 != 0 { b'E' } else { b'-' },
        if flags & 0x00080 != 0 { b'T' } else { b'-' },
        if flags & 0x00100 != 0 { b'U' } else { b'-' },
        if flags & 0x00400 != 0 { b'F' } else { b'-' },
        if flags & 0x00008 != 0 { b's' } else { b'-' },
        if flags & 0x02000 != 0 { b'p' } else { b'-' },
        if flags & 0x04000 != 0 { b'x' } else { b'-' },
        if flags & 0x20000 != 0 { b'd' } else { b'-' },
        0,
    ]
}

/// One pagination step outcome. C: dmp_pm.c:55-71.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PmAction {
    /// pid-0 non-PM row: skipped, uncounted.
    Skip,
    /// Row emitted.
    Emit,
    /// 23rd candidate: stop, print `--more--\r`, resume here next round.
    More,
}

/// dmp_pm.c page cursor: one instance per dump (two `static prev_i` —
/// dmp_pm.c:50/81). Differs from 05's PROCLOOP: C breaks on `++n > 22`
/// (22 rows emitted), not `>= 22`.
#[derive(Debug, Clone, Copy, Default)]
pub struct PmCursor {
    next: usize,
    n: u32,
}

impl PmCursor {
    pub const fn new() -> Self {
        Self { next: 0, n: 0 }
    }

    pub const fn resume_from(i: usize) -> Self {
        Self { next: i, n: 0 }
    }

    pub const fn next(&self) -> usize {
        self.next
    }

    /// Advances past table row `idx` with the given pid.
    /// C: `if (mp->mp_pid == 0 && i != PM_PROC_NR) continue; if (++n > 22) break;`
    pub const fn push(&mut self, pid: i32, idx: usize) -> PmAction {
        if pid == 0 && idx != PM_PROC_NR_IDX {
            return PmAction::Skip;
        }
        self.n += 1;
        if self.n > 22 {
            self.next = idx;
            return PmAction::More;
        }
        PmAction::Emit
    }

    /// Ends the round: exhaustion wraps to 0 without a marker, break keeps
    /// the resume index. C: `if (i >= NR_PROCS) i = 0; else printf("--more--\r"); prev_i = i;`
    pub const fn finish(&mut self, exhausted: bool) {
        if exhausted {
            self.next = 0;
        }
    }
}

/// Alarm countdown display value.
///
/// C: `ALARM_ON ? exp - uptime : "-"` (dmp_pm.c:100-102, `%8lu`). `clock_t`
/// wraps (see getticks' own 32-bit note — libsys/getticks.c:11); the
/// subtraction is wrapping by C unsigned-long promotion, mirrored here.
/// Returns `None` for the dash case.
pub const fn alarm_left(alarm_on: bool, exp: u32, uptime: u32) -> Option<u32> {
    if alarm_on {
        Some(exp.wrapping_sub(uptime))
    } else {
        None
    }
}

/// C: `ALARM_ON 0x10` — mproc.h:89 (sigaction alarm gate, dmp_pm.c:100).
pub const ALARM_ON: u32 = 0x10;

// ── render (V1-P1-3 execution face) ───────────────────────────────

use crate::PCStr;
use core::fmt;

/// `mproc_dmp` row loop (dmp_pm.c:55-72): skip `pid == 0` except PM's own
/// slot, `++n > 22` breaks (23rd candidate, not printed), `--more--\r`
/// on break, wrap to 0 on exhaustion.
pub fn render_mproc(
    out: &mut dyn fmt::Write,
    tab: &[MProcSnap],
    cur: &mut PmCursor,
) -> fmt::Result {
    out.write_str(MPROC_TITLE)?;
    out.write_str(MPROC_COLUMNS)?;
    let mut exhausted = true;
    for (i, mp) in tab.iter().enumerate() {
        match cur.push(mp.mp_pid, i) {
            PmAction::Skip => continue,
            PmAction::More => {
                out.write_str(MORE_CR)?;
                exhausted = false;
                break;
            }
            PmAction::Emit => {}
        }
        let parent_pid = tab
            .get(mp.mp_parent as usize)
            .map(|p| p.mp_pid)
            .unwrap_or(0); // C reads mproc[parent] unguarded; a bad parent
                           // index is a producer bug — render 0 instead of
                           // panicking (deliberate hardening, documented).
        write!(
            out,
            "{:<8.8} {:4}{:4}{:4}  {:5} {:5} {:5}  ",
            PCStr(&mp.mp_name),
            i as i32,
            mp.mp_parent,
            mp.mp_tracer,
            mp.mp_pid,
            parent_pid,
            mp.mp_procgrp
        )?;
        write!(
            out,
            "{:2}({:2})  {:2}({:2})   ",
            mp.mp_realuid as i32,
            mp.mp_effuid as i32,
            mp.mp_realgid as i32,
            mp.mp_effgid as i32
        )?;
        writeln!(out, " {:3}  {}  ", mp.mp_nice, PCStr(&pm_flags_str(mp.mp_flags)))?;
    }
    cur.finish(exhausted);
    Ok(())
}

/// `sigaction_dmp` (dmp_pm.c:74-110): same table + alarm countdown against
/// `uptime` (C `getticks()`).
pub fn render_sigaction(
    out: &mut dyn fmt::Write,
    tab: &[MProcSnap],
    cur: &mut PmCursor,
    uptime: u32,
) -> fmt::Result {
    out.write_str(SIGACTION_TITLE)?;
    out.write_str(SIGACTION_COLUMNS)?;
    let mut exhausted = true;
    for (i, mp) in tab.iter().enumerate() {
        match cur.push(mp.mp_pid, i) {
            PmAction::Skip => continue,
            PmAction::More => {
                out.write_str(MORE_CR)?;
                exhausted = false;
                break;
            }
            PmAction::Emit => {}
        }
        write!(out, "{:<8.8}  {:3}  ", PCStr(&mp.mp_name), i as i32)?;
        write!(
            out,
            " {:08x} {:08x} {:08x} ",
            mp.mp_ignore0, mp.mp_catch0, mp.mp_sigmask0
        )?;
        write!(out, "{:08x}  ", mp.mp_sigpending0)?;
        match alarm_left(mp.mp_flags & ALARM_ON != 0, mp.mp_timer_exp, uptime) {
            Some(left) => write!(out, "{:8}", left)?,
            None => out.write_str("       -")?,
        }
        out.write_str("\n")?;
    }
    cur.finish(exhausted);
    Ok(())
}

/// C: `"Process manager (PM) process table dump\n"` — dmp_pm.c:52.
pub const MPROC_TITLE: &str = "Process manager (PM) process table dump\n";
/// C: dmp_pm.c:53.
pub const MPROC_COLUMNS: &str =
    "-process- -nr-pnr-tnr- --pid--ppid--pgrp- -uid--  -gid--  -nice- -flags-----\n";
/// C: `"Process manager (PM) signal action dump\n"` — dmp_pm.c:86.
pub const SIGACTION_TITLE: &str = "Process manager (PM) signal action dump\n";
/// C: dmp_pm.c:87.
pub const SIGACTION_COLUMNS: &str =
    "-process- -nr- --ignore- --catch- --block- -pending- -alarm---\n";
/// C: `printf("--more--\r")` — dmp_pm.c:70/108. Carriage return, NOT `\n`
/// (differs from 05's `--more--\n`).
pub const MORE_CR: &str = "--more--\r";

#[cfg(test)]
mod tests {
    use super::*;

    fn mp(pid: i32, name: &[u8]) -> MProcSnap {
        let mut n = [0u8; 16];
        n[..name.len()].copy_from_slice(name);
        MProcSnap { mp_pid: pid, mp_name: n, mp_procgrp: 1, mp_nice: 0, ..Default::default() }
    }

    #[test]
    fn test_flags_full_and_zero() {
        // C: dmp_pm.c:21-39; mproc.h:87-102. All 11 bits → "WZAETUFspxd".
        assert_eq!(&pm_flags_str(0), b"-----------\x00");
        assert_eq!(&pm_flags_str(0x265BE), b"WZAETUFspxd\x00");
    }

    #[test]
    fn test_flags_single_bits() {
        assert_eq!(pm_flags_str(0x00002)[0], b'W');
        assert_eq!(pm_flags_str(0x00004)[1], b'Z');
        assert_eq!(pm_flags_str(0x00010)[2], b'A');
        assert_eq!(pm_flags_str(0x00020)[3], b'E');
        assert_eq!(pm_flags_str(0x00080)[4], b'T');
        assert_eq!(pm_flags_str(0x00100)[5], b'U');
        assert_eq!(pm_flags_str(0x00400)[6], b'F');
        assert_eq!(pm_flags_str(0x00008)[7], b's');
        assert_eq!(pm_flags_str(0x02000)[8], b'p');
        assert_eq!(pm_flags_str(0x04000)[9], b'x');
        assert_eq!(pm_flags_str(0x20000)[10], b'd');
    }

    #[test]
    fn test_skip_matrix() {
        // C: `mp_pid == 0 && i != PM_PROC_NR` — dmp_pm.c:56.
        let mut c = PmCursor::new();
        assert_eq!(c.push(0, PM_PROC_NR_IDX), PmAction::Emit); // PM itself kept
        assert_eq!(c.push(0, 5), PmAction::Skip);
        assert_eq!(c.push(7, 9), PmAction::Emit);
    }

    #[test]
    fn test_22_emit_then_more() {
        // C: `++n > 22` break — dmp_pm.c:57 (differs from 05's >=).
        let mut c = PmCursor::new();
        for _ in 0..22 {
            assert_eq!(c.push(1, 0), PmAction::Emit);
        }
        assert_eq!(c.push(1, 22), PmAction::More);
        assert_eq!(c.next(), 22);
    }

    #[test]
    fn test_finish_wrap_and_resume() {
        // C: `if (i >= NR_PROCS) i = 0; else printf("--more--\r")` — :69-71.
        let mut c = PmCursor::resume_from(22);
        c.finish(false);
        assert_eq!(c.next(), 22);
        c.finish(true);
        assert_eq!(c.next(), 0);
    }

    #[test]
    fn test_alarm_left_and_dash() {
        // C: dmp_pm.c:100-102.
        assert_eq!(alarm_left(true, 1100, 1000), Some(100));
        assert_eq!(alarm_left(false, 1100, 1000), None);
        assert_eq!(alarm_left(true, 50, u32::MAX), Some(51)); // wrapping
    }

    #[test]
    fn test_format_strings_verbatim() {
        assert!(MPROC_COLUMNS.starts_with("-process- -nr-pnr-tnr-"));
        assert!(SIGACTION_COLUMNS.contains("--ignore- --catch- --block-"));
        assert_eq!(MORE_CR, "--more--\r");
    }

    #[test]
    fn test_render_mproc_row_and_pagination() {
        // C: dmp_pm.c:55-72 — %8.8s name, %4d slot/parent/tracer triplet,
        // the parent's PID resolved through the table, %2d(%2d) uid/gid,
        // %3d nice + 11-char flag code; pid-0 rows skipped except slot 0.
        let mut tab = [MProcSnap::default(); 4];
        tab[0] = mp(0, b"pm");
        tab[1] = mp(1, b"init");
        tab[2] = mp(0, b"");
        tab[3] = mp(2, b"sh");
        tab[3].mp_parent = 1;
        let mut out = String::new();
        let mut cur = PmCursor::new();
        render_mproc(&mut out, &tab, &mut cur).unwrap();
        // Header + 3 rows (slot 2 skipped) + trailing blank from the last \n.
        assert_eq!(out.matches('\n').count(), 5, "title + columns + 3 rows");
        // Name column (%8.8s) + slot triplet (%4d ×3).
                // Segments (from the C printfs): %8.8s name + %4d triplet, the
        // parent's PID resolved through the table, %2d(%2d) uid/gid,
        // %3d nice + the 11-char flag code.
        assert!(out.contains("pm          0   0   0      0     0     1"), "PM row (pid 0, own slot kept)");
        assert!(out.contains("init        1   0   0      1     0     1"), "init row");
        assert!(out.contains("sh          3   1   0      2     1     1"), "sh row: parent 1 → parent pid 1");
        assert!(out.contains("   0( 0)   0( 0)      0  -----------  "), "uid/gid + nice + flags");
        assert!(!out.contains("--more--"), "exhausted table wraps without the marker");
        assert_eq!(cur.next(), 0, "exhaustion wraps to 0");
    }

    #[test]
    fn test_render_sigaction_alarm_columns() {
        // C: dmp_pm.c:93-104 — four %08x bitmaps + the alarm countdown
        // (exp - uptime) or the dash placeholder.
        let mut tab = [MProcSnap::default(); 1];
        tab[0] = mp(1, b"clock");
        tab[0].mp_flags = ALARM_ON;
        tab[0].mp_timer_exp = 1100;
        let mut out = String::new();
        render_sigaction(&mut out, &tab, &mut PmCursor::new(), 1000).unwrap();
        assert!(out.contains("clock       0   00000000 00000000 00000000 00000000       100\n"),
            "ALARM_ON shows exp - uptime (1100-1000)");
        let mut out2 = String::new();
        tab[0].mp_flags = 0;
        render_sigaction(&mut out2, &tab, &mut PmCursor::new(), 1000).unwrap();
        assert!(out2.contains("       -\n"), "no alarm prints the dash placeholder");
    }
}
