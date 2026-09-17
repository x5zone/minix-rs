//! Child exit status decoding (C: the `<sys/wait.h>` macro family).
//!
//! init's state machine branches on how a child died: exited normally,
//! killed by a signal, or stopped (visible only under WUNTRACED). C
//! spells those tests as macros over a raw status integer
//! (`minix3/sys/sys/wait.h:56-69`); the same bits become a sum type
//! here so the mutual exclusions are compiler-checked instead of
//! re-derived at every call site. Upstreamed from the init crate
//! (edge E-INITSYS ③).

/// Option bits for `waitpid` (C: `minix3/sys/sys/wait.h:76-79`).
pub const WNOHANG: i32 = 0x0000_0001;
/// C: `WUNTRACED` (`wait.h:77`) — report stopped children too.
pub const WUNTRACED: i32 = 0x0000_0002;

/// C: `EINTR 4` (`sys/errno.h:18`) — a waitpid answer can carry it and
/// the [`Errno`] authority exposes no associated constant yet.
pub const EINTR: i32 = minix_types::EINTR;

/// How one waited-for child ended.
///
/// The variants mirror the C test macros: `Exited` is WIFEXITED,
/// `Signaled` is WIFSIGNALED (with WCOREDUMP as the flag), and
/// `Stopped` is WIFSTOPPED. Minix3's `wait.h` defines no
/// `WIFCONTINUED`, so there is no continued variant; a status of
/// 0xffff decodes as stopped, which is the header-faithful reading.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WaitStatus {
    /// WIFEXITED: normal exit, `code` is WEXITSTATUS.
    Exited {
        /// WEXITSTATUS bits.
        code: i32,
    },
    /// WIFSIGNALED: killed by `termsig` (WTERMSIG), with WCOREDUMP
    /// as `core_dumped`.
    Signaled {
        /// WTERMSIG.
        termsig: i32,
        /// WCOREDUMP.
        core_dumped: bool,
    },
    /// WIFSTOPPED: stopped by `stopsig` (WSTOPSIG).
    Stopped {
        /// WSTOPSIG.
        stopsig: i32,
    },
}

/// Decode a raw `waitpid` status integer.
///
/// Bit layout from `minix3/sys/sys/wait.h`: the low 7 bits carry the
/// terminating signal (`_WSTATUS`, :54); 0177 there means stopped
/// (:55) and zero means exited (:60); bit 0200 is the core flag
/// (:68); bits 8-15 carry the exit code or the stop signal (:61,
/// :57). Every 16-bit combination lands in exactly one variant, so
/// decoding cannot fail — unlike nix's `WaitStatus::from_raw`, no
/// `EINVAL` path is needed because the Minix3 bit layout has no
/// impossible combination.
pub fn from_raw(status: i32) -> WaitStatus {
    let low = status & 0x7f;
    if low == 0x7f {
        WaitStatus::Stopped {
            stopsig: (status >> 8) & 0xff,
        }
    } else if low == 0 {
        WaitStatus::Exited {
            code: (status >> 8) & 0xff,
        }
    } else {
        WaitStatus::Signaled {
            termsig: low,
            core_dumped: status & 0x80 != 0,
        }
    }
}

impl WaitStatus {
    /// WIFSIGNALED and WTERMSIG equal `signum`: did this signal kill
    /// the child?
    pub fn signaled_by(self, signum: i32) -> bool {
        matches!(self, WaitStatus::Signaled { termsig, .. } if termsig == signum)
    }

    /// WIFSIGNALED: killed by any signal.
    pub fn signaled(self) -> bool {
        matches!(self, WaitStatus::Signaled { .. })
    }

    /// WIFSTOPPED: stopped, still a child.
    pub fn stopped(self) -> bool {
        matches!(self, WaitStatus::Stopped { .. })
    }

    /// WIFEXITED: ran to completion.
    pub fn exited(self) -> bool {
        matches!(self, WaitStatus::Exited { .. })
    }

    /// WEXITSTATUS for an exited child, else `None`.
    pub fn exit_code(self) -> Option<i32> {
        match self {
            WaitStatus::Exited { code } => Some(code),
            _ => None,
        }
    }
}

/// Recompose a status the way C builds test vectors
/// (`W_EXITCODE(ret, sig)` = `ret << 8 | sig`, `wait.h:69`).
#[cfg(test)]
pub(crate) fn exit_code_raw(code: i32) -> i32 {
    code << 8
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_zero_status_is_clean_exit() {
        assert_eq!(from_raw(0), WaitStatus::Exited { code: 0 });
        assert!(from_raw(0).exited());
    }

    #[test]
    fn test_exit_code_from_high_byte() {
        // W_EXITCODE(3, 0) = 3 << 8 (wait.h:69).
        let status = from_raw(exit_code_raw(3));
        assert_eq!(status, WaitStatus::Exited { code: 3 });
        assert_eq!(status.exit_code(), Some(3));
    }

    #[test]
    fn test_signal_death_decodes_termsig() {
        // W_EXITCODE(0, SIGTERM) = 15.
        let status = from_raw(15);
        assert_eq!(
            status,
            WaitStatus::Signaled { termsig: 15, core_dumped: false }
        );
        assert!(status.signaled());
        assert!(!status.exited());
    }

    #[test]
    fn test_killed_by_matches_termsig_only() {
        let kill = from_raw(9);
        assert!(kill.signaled_by(9));
        assert!(!kill.signaled_by(1));
        let hup = from_raw(1);
        assert!(hup.signaled_by(1));
    }

    #[test]
    fn test_core_flag_is_bit_0200() {
        // WCOREDUMP: status & 0200 (wait.h:68) — SIGABRT with core.
        let status = from_raw(6 | 0x80);
        assert_eq!(
            status,
            WaitStatus::Signaled { termsig: 6, core_dumped: true }
        );
    }

    #[test]
    fn test_stopped_is_low_bits_0177() {
        // W_STOPCODE(sig) = sig << 8 | 0177 (wait.h:70).
        let status = from_raw((18 << 8) | 0x7f);
        assert_eq!(status, WaitStatus::Stopped { stopsig: 18 });
        assert!(status.stopped());
    }

    #[test]
    fn test_continued_status_has_no_variant() {
        // 0xffff: Minix3 wait.h has no WIFCONTINUED, so the
        // header-faithful decode is "stopped" (low bits 0177).
        assert_eq!(from_raw(0xffff), WaitStatus::Stopped { stopsig: 0xff });
    }
}
