//! Runcom state ('r'): execute /etc/rc.
//!
//! Covers `minix3/sbin/init/init.c:879-1014` (`runetcrc`, `runcom`).
//! Design contract: `.design/05-design.v1.md §1.1-§1.3`.

use crate::entry::RuncomMode;

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
pub fn classify_rc_exit(
    stopped: bool,
    catatonia_requested: bool,
    terminated_by_sigterm: bool,
    exited: bool,
    exit_code: i32,
) -> RcOutcome {
    if stopped {
        return RcOutcome::Continue;
    }
    if catatonia_requested && terminated_by_sigterm {
        return RcOutcome::RebootQuiet;
    }
    if !exited {
        return RcOutcome::SingleUser;
    }
    if exit_code != 0 {
        return RcOutcome::SingleUser;
    }
    RcOutcome::ReadTtys
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
            classify_rc_exit(false, false, false, true, 0),
            RcOutcome::ReadTtys
        );
    }

    #[test]
    fn test_nonzero_goes_single_user() {
        assert_eq!(
            classify_rc_exit(false, false, false, true, 1),
            RcOutcome::SingleUser
        );
    }

    #[test]
    fn test_abnormal_goes_single_user() {
        assert_eq!(
            classify_rc_exit(false, false, false, false, 0),
            RcOutcome::SingleUser
        );
    }

    #[test]
    fn test_catatonia_sigterm_quiets() {
        assert_eq!(
            classify_rc_exit(false, true, true, false, 0),
            RcOutcome::RebootQuiet
        );
    }
}
