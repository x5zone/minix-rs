//! MINIX_PROC: the ProcFS door (list + data).
//!
//! Mirrors the pure halves of `mib_minix_proc_list` /
//! `mib_minix_proc_data` (proc.c:1177-1288). The table pull comes from
//! 16 (`update_tables`); only the row filter, the PID resolution, and
//! the flag mapping are decided here. Reading the tables and the
//! copy-out are transport effects (A-6, A-12). The row layouts live in
//! `minix_types` (`MinixProcList`/`MinixProcData`) because ProcFS reads
//! them field by field (A-5).
//!
//! 20-mib-minix-proc.md.

use minix_types::{
    EINVAL, ESRCH, MPDF_RUNNABLE, MPDF_STOPPED, MPDF_SYSTEM, MPDF_ZOMBIE, MPLF_IN_USE, MPLF_ZOMBIE,
};

/// Check the name: exactly one component (the PID).
///
/// C: `call_namelen != 1 → EINVAL` — proc.c:1230-1231. Only one PID per
/// query is possible today; the comment says so (:1226-1229).
pub const fn check_data_namelen(namelen: u32) -> Result<(), i32> {
    if namelen != 1 {
        return Err(EINVAL);
    }
    Ok(())
}

/// Whether this PID names a kernel task.
///
/// C: ProcFS semantics — a negative PID is a kernel task, anything else
/// a user process — proc.c:1238-1242. Unlike the CTL_KERN nodes, which
/// use -1 for "everything", negative here counts down the task slots.
pub const fn is_task_pid(pid: i32) -> bool {
    pid < 0
}

/// Resolve a task PID to its kernel slot.
///
/// C: `pid < -NR_TASKS → ESRCH`, else `kslot = pid + NR_TASKS` —
/// proc.c:1243-1248. The task count travels as a parameter: it is a
/// build fact, not a wire value.
pub const fn resolve_task_slot(pid: i32, nr_tasks: i32) -> Result<i32, i32> {
    if pid < -nr_tasks {
        return Err(ESRCH);
    }
    Ok(pid + nr_tasks)
}

/// Whether a list row is included.
///
/// C: `!(mp_flags & IN_USE) || mp_pid <= 0 → continue` —
/// proc.c:1199-1200. Empty slots and the kernel pseudo-row stay out.
pub const fn list_row_included(in_use: bool, pid: i32) -> bool {
    in_use && pid > 0
}

/// List-row flags: in use always, plus zombie.
///
/// C: `mpl_flags = MPLF_IN_USE`, plus `MPLF_ZOMBIE` for zombies —
/// proc.c:1202-1204.
pub const fn list_flags(zombie: bool) -> u32 {
    if zombie {
        MPLF_IN_USE | MPLF_ZOMBIE
    } else {
        MPLF_IN_USE
    }
}

/// One process's liveness, in priority order.
///
/// C: the `if/else` chain — proc.c:1267-1272. Zombie beats stopped
/// beats runnable; anything else reports nothing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcState {
    /// Dead but not collected.
    Zombie,
    /// Stopped for debugging.
    Stopped,
    /// On a run queue.
    Runnable,
    /// None of the above.
    Other,
}

/// Data-row flags: system service plus liveness.
///
/// C: `PRIV_PROC → MPDF_SYSTEM`, then the liveness chain —
/// proc.c:1265-1272. The system-service bit is independent of the
/// chain; only one liveness bit is ever set.
pub const fn data_flags(priv_proc: bool, state: ProcState) -> u32 {
    let mut flags = 0;
    if priv_proc {
        flags |= MPDF_SYSTEM;
    }
    match state {
        ProcState::Zombie => flags |= MPDF_ZOMBIE,
        ProcState::Stopped => flags |= MPDF_STOPPED,
        ProcState::Runnable => flags |= MPDF_RUNNABLE,
        ProcState::Other => {}
    }
    flags
}

/// Where the reported name comes from.
///
/// C: user rows copy `mproc`'s name, task rows the kernel row's name —
/// proc.c:1280-1285 (`kslot >= NR_TASKS` decides).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NameSource {
    /// Copy the user-process name.
    User,
    /// Copy the kernel task name.
    Task,
}

/// Pick the name source by slot.
///
/// C: proc.c:1280-1285. The task count travels as a parameter.
pub const fn name_source(kslot: i32, nr_tasks: i32) -> NameSource {
    if kslot >= nr_tasks {
        NameSource::User
    } else {
        NameSource::Task
    }
}

/// Which management flags apply: user rows use their row, tasks none.
///
/// C: `mflags = (pid > 0) ? mproc_tab[mslot].mp_flags : 0` —
/// proc.c:1261.
pub const fn mflags_for(pid: i32, mslot_flags: u32) -> u32 {
    if pid > 0 {
        mslot_flags
    } else {
        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_data_args() {
        // One name component (proc.c:1230-1231).
        assert_eq!(check_data_namelen(1), Ok(()));
        assert_eq!(check_data_namelen(0), Err(EINVAL));
        assert_eq!(check_data_namelen(2), Err(EINVAL));
        // Negative PID is a task, the rest are processes (:1238-1242).
        assert!(is_task_pid(-1));
        assert!(is_task_pid(-7));
        assert!(!is_task_pid(0));
        assert!(!is_task_pid(12));
        // Task slots count down from the top (:1243-1248).
        assert_eq!(resolve_task_slot(-1, 8), Ok(7));
        assert_eq!(resolve_task_slot(-8, 8), Ok(0));
        assert_eq!(resolve_task_slot(-9, 8), Err(ESRCH));
    }

    #[test]
    fn test_list_rows() {
        // Only live, positive-PID rows (:1199-1200).
        assert!(list_row_included(true, 12));
        assert!(!list_row_included(false, 12));
        assert!(!list_row_included(true, 0));
        assert!(!list_row_included(true, -3));
        // Flags: in use always, plus zombie (:1202-1204).
        assert_eq!(list_flags(false), MPLF_IN_USE);
        assert_eq!(list_flags(true), MPLF_IN_USE | MPLF_ZOMBIE);
    }

    #[test]
    fn test_data_flags() {
        // System-service bit rides along (:1265-1266).
        assert_eq!(data_flags(false, ProcState::Other), 0);
        assert_eq!(data_flags(true, ProcState::Other), MPDF_SYSTEM);
        // Liveness chain: zombie beats stopped beats runnable.
        assert_eq!(data_flags(false, ProcState::Zombie), MPDF_ZOMBIE);
        assert_eq!(data_flags(false, ProcState::Stopped), MPDF_STOPPED);
        assert_eq!(data_flags(false, ProcState::Runnable), MPDF_RUNNABLE);
        assert_eq!(
            data_flags(true, ProcState::Runnable),
            MPDF_SYSTEM | MPDF_RUNNABLE
        );
        // Name source and management flags (:1261, :1280-1285).
        assert_eq!(name_source(8, 8), NameSource::User);
        assert_eq!(name_source(7, 8), NameSource::Task);
        assert_eq!(mflags_for(12, 0xFF), 0xFF);
        assert_eq!(mflags_for(-1, 0xFF), 0);
        assert_eq!(mflags_for(0, 0xFF), 0);
    }
}
