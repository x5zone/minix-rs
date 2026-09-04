//! KERN_PROC2: the second process listing format.
//!
//! Mirrors the pure halves of `fill_proc2_common` / `fill_proc2_kern` /
//! `fill_proc2_user` / `mib_kern_proc2` (proc.c:600-913). The per-process
//! state comes from 17 (`get_lwp_stat`, `fill_lwp_common`), the slot
//! lookup, the extra headroom, and the time conversion from 16; only the
//! output shape and the row filters are decided here. Reading the tables,
//! reading the clock, and the copy-out loop are transport effects
//! (A-6, A-12); table layouts belong to kernel/PM/VFS.
//!
//! 18-mib-proc2.md.

use minix_types::{
    EINVAL, EPROC_CTTY, EPROC_SLEADER, ESRCH, KERN_PROC_ALL, KERN_PROC_GID, KERN_PROC_PID,
    KERN_PROC_PGRP, KERN_PROC_RGID, KERN_PROC_RUID, KERN_PROC_SESSION, KERN_PROC_TTY,
    KERN_PROC_UID, KI_NGROUPS, LSDEAD, LSRUN, LSSLEEP, LSSTOP, LSZOMB, NZERO, P_CONTROLT,
    P_INMEM, P_SINTR, P_SUGID, P_TRACED, SACTIVE, SDEAD, SSTOP, SZOMB,
};

use super::tables::EXTRA_PROCS;

/// Which rows the caller asked for.
///
/// C: the `switch (req)` in `mib_kern_proc2` — proc.c:815-833. The
/// numbers travel in `call_name[0]`; unknown numbers are rejected.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Proc2Req {
    /// Every row, including the kernel pseudo-process. C: `KERN_PROC_ALL`.
    All,
    /// The one row with this PID. C: `KERN_PROC_PID`.
    Pid,
    /// Rows in this process group. C: `KERN_PROC_PGRP`.
    Pgrp,
    /// Rows in this session. C: `KERN_PROC_SESSION`.
    Session,
    /// Rows on this terminal. C: `KERN_PROC_TTY`.
    Tty,
    /// Rows with this effective user id. C: `KERN_PROC_UID`.
    Uid,
    /// Rows with this real user id. C: `KERN_PROC_RUID`.
    Ruid,
    /// Rows with this effective group id. C: `KERN_PROC_GID`.
    Gid,
    /// Rows with this real group id. C: `KERN_PROC_RGID`.
    Rgid,
}

/// Decode the requested filter; unknown numbers are rejected.
///
/// C: `switch (req)` with `default: return EINVAL` — proc.c:815-833.
pub const fn decode_req(req: i32) -> Option<Proc2Req> {
    match req {
        KERN_PROC_ALL => Some(Proc2Req::All),
        KERN_PROC_PID => Some(Proc2Req::Pid),
        KERN_PROC_PGRP => Some(Proc2Req::Pgrp),
        KERN_PROC_SESSION => Some(Proc2Req::Session),
        KERN_PROC_TTY => Some(Proc2Req::Tty),
        KERN_PROC_UID => Some(Proc2Req::Uid),
        KERN_PROC_RUID => Some(Proc2Req::Ruid),
        KERN_PROC_GID => Some(Proc2Req::Gid),
        KERN_PROC_RGID => Some(Proc2Req::Rgid),
        _ => None,
    }
}

/// Check the query arguments: four name components, a positive row
/// stride, and a non-negative row budget.
///
/// C: `call_namelen != 4`, `elsz <= 0`, `elmax < 0` → `EINVAL` —
/// proc.c:802-808,835-836. (`elmax` repeats the old-buffer length from
/// the message; the comment at :808 says so.)
pub const fn check_proc2_args(namelen: u32, elsz: i32, elmax: i32) -> Result<(), i32> {
    if namelen != 4 {
        return Err(EINVAL);
    }
    if elsz <= 0 || elmax < 0 {
        return Err(EINVAL);
    }
    Ok(())
}

/// Whether the kernel pseudo-process matches this query.
///
/// C: the `kmatch` switch — proc.c:815-833. The kernel has no slot in
/// the PM or VFS tables, so it is matched separately, before the loop.
/// Terminal matching needs the "no terminal" sentinel, which belongs to
/// the device headers — the caller passes it in rather than this module
/// pinning a device number.
pub const fn match_kernel(req: Proc2Req, arg: i64, tty_nodev: i64) -> bool {
    match req {
        Proc2Req::All => true,
        Proc2Req::Pid
        | Proc2Req::Session
        | Proc2Req::Pgrp
        | Proc2Req::Uid
        | Proc2Req::Ruid
        | Proc2Req::Gid
        | Proc2Req::Rgid => arg == 0,
        Proc2Req::Tty => arg == tty_nodev,
    }
}

/// Whether one table row matches this query.
///
/// C: the per-row `switch (req)` — proc.c:860-898. Session and process
/// group share the process-group field (job control is still a TODO in
/// C, :865, so both compare against it). Terminal matching: a revoked
/// terminal matches nothing (revoke(2) is still a TODO in C, :871-872);
/// the caller passes the zombies' terminal as "no device" already
/// (C never reads a zombie's file slot, :873-875), so this function only
/// applies the no-device rule.
pub const fn match_row(
    req: Proc2Req,
    arg: i64,
    pid: i64,
    procgrp: i64,
    effuid: u32,
    realuid: u32,
    effgid: u32,
    realgid: u32,
    tty: i64,
    tty_nodev: i64,
    tty_revoke: i64,
    no_dev: i64,
) -> bool {
    match req {
        Proc2Req::All => true,
        Proc2Req::Pid => arg == pid,
        Proc2Req::Session | Proc2Req::Pgrp => arg == procgrp,
        Proc2Req::Tty => {
            if arg == tty_revoke {
                return false;
            }
            if arg == tty_nodev {
                return tty == no_dev;
            }
            if arg == no_dev || arg != tty {
                return false;
            }
            true
        }
        Proc2Req::Uid => arg == effuid as i64,
        Proc2Req::Ruid => arg == realuid as i64,
        Proc2Req::Gid => arg == effgid as i64,
        Proc2Req::Rgid => arg == realgid as i64,
    }
}

/// Check one PID lookup: a missed slot or a zombie is "no such process".
///
/// C: `get_mslot` miss or zombie → `ESRCH` — proc.c:564-566.
pub const fn check_pid_slot(missed: bool, zombie: bool) -> Result<(), i32> {
    if missed || zombie {
        return Err(ESRCH);
    }
    Ok(())
}

/// Map the thread state to the process state pair.
///
/// C: the `switch (p->p_stat)` — proc.c:760-782. Almost every state maps
/// onto itself with its NetBSD real-state twin; the one exception is
/// "almost a zombie", which ps(1) cannot display, so it is reported as
/// "awaiting collection" while keeping the dead real state (:777).
pub const fn map_stat(lwp_stat: i32) -> (i32, i32) {
    match lwp_stat {
        LSRUN => (LSRUN, SACTIVE),
        LSSLEEP => (LSSLEEP, SACTIVE),
        LSSTOP => (LSSTOP, SSTOP),
        LSZOMB => (LSZOMB, SZOMB),
        LSDEAD => (LSZOMB, SDEAD),
        _ => (LSZOMB, SDEAD),
    }
}

/// Whether the interruptible-sleep flag carries over to the process flags.
///
/// C: `if (p->p_flag & L_SINTR) p->p_realflag |= P_SINTR` — proc.c:767-768.
/// Only sleeping rows can carry it (only they reach this line).
pub const fn carry_sinter(lwp_stat: i32, sinter: bool) -> bool {
    lwp_stat == LSSLEEP && sinter
}

/// Extended flags: terminal active, session leader.
///
/// C: `p->p_eflag` — proc.c:709-713. A zombie has no terminal, so the
/// caller resolves that first (see [`zombie_tty`]); session leadership
/// is "PID equals process group" today (job control is still a TODO in
/// C, :712).
pub const fn eflag(has_tty: bool, is_leader: bool) -> u32 {
    let mut flags = 0;
    if has_tty {
        flags |= EPROC_CTTY;
    }
    if is_leader {
        flags |= EPROC_SLEADER;
    }
    flags
}

/// Process flags: in memory always, plus history, tracing, terminal.
///
/// C: `p->p_flag` — proc.c:717-723. `TAINTED` means the process once ran
/// set-uid/set-gid; `mp_tracer != NO_TRACER` means a debugger watches it.
pub const fn pflag(tainted: bool, traced: bool, has_tty: bool) -> u32 {
    let mut flags = P_INMEM;
    if tainted {
        flags |= P_SUGID;
    }
    if traced {
        flags |= P_TRACED;
    }
    if has_tty {
        flags |= P_CONTROLT;
    }
    flags
}

/// Reported niceness: the stored value plus the zero point.
///
/// C: `p->p_nice = mp->mp_nice + NZERO` — proc.c:742.
pub const fn nice_output(mp_nice: i32) -> i32 {
    mp_nice + NZERO
}

/// Reported group count: capped at the output slots.
///
/// C: `p->p_ngroups = MIN(mp->mp_ngroups, KI_NGROUPS)` — proc.c:734.
pub const fn groups_capped(ngroups: usize) -> usize {
    if ngroups < KI_NGROUPS {
        ngroups
    } else {
        KI_NGROUPS
    }
}

/// Reported thread count: zombies have none.
///
/// C: `p->p_nlwps = (zombie) ? 0 : 1` — proc.c:753.
pub const fn nlwps(zombie: bool) -> u32 {
    if zombie {
        0
    } else {
        1
    }
}

/// A zombie's terminal: zombies own no terminal.
///
/// C: `tty = (!zombie) ? fp->fpl_tty : NO_DEV` — proc.c:707. The caller
/// passes the "no device" value; this module never pins device numbers.
pub const fn zombie_tty(zombie: bool, tty: i64, no_dev: i64) -> i64 {
    if zombie {
        no_dev
    } else {
        tty
    }
}

/// Bytes copied per row: the smaller of the caller's stride and the
/// structure.
///
/// C: `copysz = MIN(elsz, sizeof(proc2))` — proc.c:842. The caller may
/// report a larger stride than the structure (forward compatibility);
/// only the structure is copied. The structure size travels as a
/// parameter because its layout is a transport concern (A-4).
pub const fn copy_size(elsz: u64, struct_size: u64) -> u64 {
    if elsz < struct_size {
        elsz
    } else {
        struct_size
    }
}

/// Extra bytes reserved on length-estimate calls over whole-table queries.
///
/// C: `if (oldp == NULL && req != KERN_PROC_PID) off += EXTRA_PROCS * elsz`
/// — proc.c:909-910. A single-PID estimate cannot grow (at most one
/// row), so only whole-table estimates reserve headroom for forks
/// between the estimate call and the fill call (16's headroom rule).
pub const fn headroom(oldp_null: bool, is_pid_req: bool, elsz: u64) -> u64 {
    if oldp_null && !is_pid_req {
        EXTRA_PROCS as u64 * elsz
    } else {
        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_req_decode() {
        // Filter numbers (sysctl.h:383-391); unknown numbers rejected.
        assert_eq!(decode_req(0), Some(Proc2Req::All));
        assert_eq!(decode_req(1), Some(Proc2Req::Pid));
        assert_eq!(decode_req(4), Some(Proc2Req::Tty));
        assert_eq!(decode_req(8), Some(Proc2Req::Rgid));
        assert_eq!(decode_req(9), None);
        assert_eq!(decode_req(-1), None);
    }

    #[test]
    fn test_proc2_args() {
        // Four name components, positive stride, non-negative budget.
        assert_eq!(check_proc2_args(4, 100, 10), Ok(()));
        assert_eq!(check_proc2_args(3, 100, 10), Err(EINVAL));
        assert_eq!(check_proc2_args(4, 0, 10), Err(EINVAL));
        assert_eq!(check_proc2_args(4, 100, -1), Err(EINVAL));
        // PID lookup: miss or zombie is "no such process".
        assert_eq!(check_pid_slot(false, false), Ok(()));
        assert_eq!(check_pid_slot(true, false), Err(ESRCH));
        assert_eq!(check_pid_slot(false, true), Err(ESRCH));
    }

    #[test]
    fn test_kernel_match() {
        // The kernel matches "everything", and any zero-arg query.
        assert!(match_kernel(Proc2Req::All, 5, 100));
        assert!(match_kernel(Proc2Req::Pid, 0, 100));
        assert!(!match_kernel(Proc2Req::Pid, 5, 100));
        assert!(match_kernel(Proc2Req::Uid, 0, 100));
        assert!(!match_kernel(Proc2Req::Gid, 3, 100));
        // Terminal queries match the kernel only on "no terminal".
        assert!(match_kernel(Proc2Req::Tty, 100, 100));
        assert!(!match_kernel(Proc2Req::Tty, 7, 100));
    }

    #[test]
    fn test_row_filters() {
        // Unfiltered and identity filters.
        assert!(match_row(Proc2Req::All, 0, 9, 9, 0, 0, 0, 0, 0, -1, -2, 0));
        assert!(match_row(Proc2Req::Pid, 9, 9, 9, 0, 0, 0, 0, 0, -1, -2, 0));
        assert!(!match_row(Proc2Req::Pid, 8, 9, 9, 0, 0, 0, 0, 0, -1, -2, 0));
        // Session and process group share the group field.
        assert!(match_row(Proc2Req::Session, 9, 1, 9, 0, 0, 0, 0, 0, -1, -2, 0));
        assert!(match_row(Proc2Req::Pgrp, 9, 1, 9, 0, 0, 0, 0, 0, -1, -2, 0));
        assert!(!match_row(Proc2Req::Pgrp, 4, 1, 9, 0, 0, 0, 0, 0, -1, -2, 0));
        // User and group filters read their own fields.
        assert!(match_row(Proc2Req::Uid, 10, 1, 1, 10, 0, 0, 0, 0, -1, -2, 0));
        assert!(!match_row(Proc2Req::Ruid, 10, 1, 1, 10, 0, 0, 0, 0, -1, -2, 0));
        assert!(match_row(Proc2Req::Gid, 20, 1, 1, 0, 0, 20, 0, 0, -1, -2, 0));
        // Terminal: revoked matches nothing; no-terminal matches the
        // terminal-less; otherwise exact match only.
        assert!(!match_row(Proc2Req::Tty, -2, 1, 1, 0, 0, 0, 0, 5, -1, -2, 0));
        assert!(match_row(Proc2Req::Tty, -1, 1, 1, 0, 0, 0, 0, 0, -1, -2, 0));
        assert!(!match_row(Proc2Req::Tty, -1, 1, 1, 0, 0, 0, 0, 5, -1, -2, 0));
        assert!(match_row(Proc2Req::Tty, 5, 1, 1, 0, 0, 0, 0, 5, -1, -2, 0));
        assert!(!match_row(Proc2Req::Tty, 6, 1, 1, 0, 0, 0, 0, 5, -1, -2, 0));
    }

    #[test]
    fn test_stat_map() {
        // Four states map onto themselves; "almost a zombie" is shown as
        // "awaiting collection" with the dead real state (proc.c:777).
        assert_eq!(map_stat(LSRUN), (LSRUN, SACTIVE));
        assert_eq!(map_stat(LSSLEEP), (LSSLEEP, SACTIVE));
        assert_eq!(map_stat(LSSTOP), (LSSTOP, SSTOP));
        assert_eq!(map_stat(LSZOMB), (LSZOMB, SZOMB));
        assert_eq!(map_stat(LSDEAD), (LSZOMB, SDEAD));
        // The interruptible mark only travels on sleeping rows.
        assert!(carry_sinter(LSSLEEP, true));
        assert!(!carry_sinter(LSSLEEP, false));
        assert!(!carry_sinter(LSRUN, true));
    }

    #[test]
    fn test_flags_and_fields() {
        // Extended flags: terminal active, session leader.
        assert_eq!(eflag(false, false), 0);
        assert_eq!(eflag(true, false), EPROC_CTTY);
        assert_eq!(eflag(false, true), EPROC_SLEADER);
        assert_eq!(eflag(true, true), EPROC_CTTY | EPROC_SLEADER);
        // Process flags: in memory always, plus history/trace/terminal.
        assert_eq!(pflag(false, false, false), P_INMEM);
        assert_eq!(pflag(true, false, false), P_INMEM | P_SUGID);
        assert_eq!(pflag(false, true, true), P_INMEM | P_TRACED | P_CONTROLT);
        // Niceness, group cap, thread count, zombie terminal.
        assert_eq!(nice_output(0), NZERO);
        assert_eq!(nice_output(-5), NZERO - 5);
        assert_eq!(groups_capped(4), 4);
        assert_eq!(groups_capped(99), KI_NGROUPS);
        assert_eq!(nlwps(false), 1);
        assert_eq!(nlwps(true), 0);
        assert_eq!(zombie_tty(false, 7, 0), 7);
        assert_eq!(zombie_tty(true, 7, 0), 0);
    }

    #[test]
    fn test_copy_math() {
        // Copy the smaller of stride and structure.
        assert_eq!(copy_size(100, 200), 100);
        assert_eq!(copy_size(300, 200), 200);
        assert_eq!(copy_size(200, 200), 200);
        // Headroom only on whole-table length estimates.
        assert_eq!(headroom(true, false, 100), EXTRA_PROCS as u64 * 100);
        assert_eq!(headroom(true, true, 100), 0);
        assert_eq!(headroom(false, false, 100), 0);
    }
}
