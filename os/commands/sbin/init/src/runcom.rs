//! Runcom state ('r'): execute /etc/rc.
//!
//! Covers `minix3/sbin/init/init.c:879-1014` (`runetcrc`, `runcom`).
//! Design contract: `.design/05-design.v1.md §1.1-§1.3`.

use crate::entry::RuncomMode;
use crate::state_machine::sig;
use crate::wait::WaitStatus;

/// Assemble `sh /etc/rc [autoboot]` argv (C: init.c:897-900).
pub fn rc_argv(mode: RuncomMode) -> Vec<String> {
    let mut argv = vec!["sh".to_string(), "/etc/rc".to_string()];
    if mode == RuncomMode::Autoboot {
        argv.push("autoboot".to_string());
    }
    argv
}

/// Where a finished `/etc/rc` run goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RcOutcome {
    SingleUser,
    Continue,
    RebootQuiet,
    ReadTtys,
}

/// Classify one wait observation for the rc child (C: init.c:931-968).
///
/// A stopped rc keeps the loop going; SIGTERM arriving together with a
/// catatonia request means "shut down, quietly" (init.c:949-957); any
/// other death — signal or nonzero exit — falls back to single-user
/// (init.c:959-966); only a clean zero exit reaches read_ttys
/// (init.c:968).
pub fn classify_rc_exit(status: WaitStatus, catatonia_requested: bool) -> RcOutcome {
    if status.stopped() {
        return RcOutcome::Continue;
    }
    if catatonia_requested && status.signaled_by(sig::SIGNAL_TERMINATE) {
        return RcOutcome::RebootQuiet;
    }
    match status {
        WaitStatus::Exited { code } if code == 0 => RcOutcome::ReadTtys,
        _ => RcOutcome::SingleUser,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rc_argv_autoboot_has_third() {
        assert_eq!(rc_argv(RuncomMode::Autoboot).len(), 3);
        assert_eq!(rc_argv(RuncomMode::Autoboot)[2], "autoboot");
    }

    #[test]
    fn test_rc_argv_fastboot_truncated() {
        assert_eq!(rc_argv(RuncomMode::Fastboot).len(), 2);
    }

    #[test]
    fn test_zero_exit_goes_read_ttys() {
        assert_eq!(
            classify_rc_exit(WaitStatus::Exited { code: 0 }, false),
            RcOutcome::ReadTtys
        );
    }

    #[test]
    fn test_nonzero_goes_single_user() {
        assert_eq!(
            classify_rc_exit(WaitStatus::Exited { code: 1 }, false),
            RcOutcome::SingleUser
        );
    }

    #[test]
    fn test_abnormal_goes_single_user() {
        let killed = WaitStatus::Signaled { termsig: 11, core_dumped: true };
        assert_eq!(
            classify_rc_exit(killed, false),
            RcOutcome::SingleUser
        );
    }

    #[test]
    fn test_catatonia_sigterm_quiets() {
        let term = WaitStatus::Signaled { termsig: 15, core_dumped: false };
        assert_eq!(classify_rc_exit(term, true), RcOutcome::RebootQuiet);
    }

    #[test]
    fn test_catatonia_without_sigterm_still_single_user() {
        // The quiet path needs BOTH the catatonia request and SIGTERM.
        let kill = WaitStatus::Signaled { termsig: 9, core_dumped: false };
        assert_eq!(
            classify_rc_exit(kill, true),
            RcOutcome::SingleUser
        );
    }

    #[test]
    fn test_stopped_rc_continues() {
        let stopped = WaitStatus::Stopped { stopsig: 18 };
        assert_eq!(
            classify_rc_exit(stopped, false),
            RcOutcome::Continue
        );
    }
}
