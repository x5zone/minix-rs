//! Service life cycle: signal triage, shutdown verdict, restart notes.
//!
//! C: `sef_cb_signal_handler` (main.c:101-122) plus the restart half of
//! `sef_local_startup` (main.c:128-129).
//! Document `10-ipc-lifecycle.md` §3 (decisions D1-D4).
//!
//! Judgement only: which road to take. Deregistering the subtree and
//! exiting the process are boundary effects the service layer performs
//! after the verdict.

/// Termination signal number. C: `SIGTERM 15` — sys/signal.h:67.
pub const SIGTERM: i32 = 15;

/// Incoming signal: termination or anything else.
///
/// C: `if (signo != SIGTERM) return` — main.c:105. Only termination is
/// actionable; the rest is ignored, and the type makes that exhaustive.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Signal {
    /// Termination request. C: `SIGTERM`.
    Terminate,
    /// Any other signal (ignored).
    Other,
}

impl Signal {
    /// Classify a raw signal number.
    pub const fn from_raw(signo: i32) -> Self {
        if signo == SIGTERM {
            Self::Terminate
        } else {
            Self::Other
        }
    }
}

/// Shutdown road: leave cleanly or stay dirty.
///
/// C: the two exits of `sef_cb_signal_handler` (main.c:111-117): both
/// tables empty → deregister and exit zero; otherwise warn and stay.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShutdownVerdict {
    /// Both tables empty: deregister the subtree, exit zero.
    ExitClean,
    /// Something still live: warn and keep serving.
    StayDirty,
}

/// Judge a termination signal against the two emptiness reports.
///
/// `sem_empty` / `shm_empty` come from the table modules (`is_empty`);
/// the service layer executes the verdict (deregister + exit, or warn).
///
/// C: `is_sem_nil() && is_shm_nil()` — main.c:111.
pub const fn shutdown_check(sem_empty: bool, shm_empty: bool) -> ShutdownVerdict {
    if sem_empty && shm_empty {
        ShutdownVerdict::ExitClean
    } else {
        ShutdownVerdict::StayDirty
    }
}

/// Restart loses all dynamic state: the set and segment tables are process
/// memory, so a re-run starts from empty tables. There is no code path
/// that preserves them — the note exists so nobody "optimises" one in.
///
/// C: restart registers the same fresh-start function (main.c:128-129);
/// emptiness after restart is a property of re-execution, not of a call.
pub const RESTART_LOSES_STATE: bool = true;

/// Restart re-runs the registration (same function as fresh start).
///
/// C: `sef_setcb_init_restart(sef_cb_init_fresh)` — main.c:129.
pub const fn restart_registers() -> bool {
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signal_classifies_term_only() {
        // C: main.c:105 — only SIGTERM is actionable.
        assert_eq!(Signal::from_raw(SIGTERM), Signal::Terminate);
        assert_eq!(Signal::from_raw(0), Signal::Other);
        assert_eq!(Signal::from_raw(9), Signal::Other);
        assert_eq!(SIGTERM, 15);
    }

    #[test]
    fn shutdown_exits_when_both_empty() {
        // C: main.c:111-114 — clean road.
        assert_eq!(shutdown_check(true, true), ShutdownVerdict::ExitClean);
    }

    #[test]
    fn shutdown_stays_when_either_live() {
        // C: main.c:111/:117 — all three dirty combinations stay.
        assert_eq!(shutdown_check(false, true), ShutdownVerdict::StayDirty);
        assert_eq!(shutdown_check(true, false), ShutdownVerdict::StayDirty);
        assert_eq!(shutdown_check(false, false), ShutdownVerdict::StayDirty);
    }

    #[test]
    fn restart_notes_hold() {
        // C: main.c:128-129 — restart re-registers; state is lost by
        // re-execution, not by a call anyone could skip. Compile-time
        // pinned: flipping the note breaks the build, not just a test.
        const { assert!(RESTART_LOSES_STATE) }
        assert!(restart_registers());
    }
}
