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
}
