//! Single-user rescue state ('s').
//!
//! Covers `minix3/sbin/init/init.c:694-877` (`single_user`).
//! Design contract: `.design/04-design.v1.md §1.1-§1.4`.

use crate::state_machine::StateKind;

/// Whether the password gate must prompt.
pub fn password_gate_required(
    console_secure: bool,
    from_securitylevel: i32,
    root_has_password: bool,
) -> bool {
    // C: typ && (from_securitylevel >= 2 || !(typ->ty_status & TTY_SECURE))
    //     && pp && *pw_passwd != '\0' (init.c:749-750).
    root_has_password && (from_securitylevel >= 2 || !console_secure)
}

/// One password prompt outcome.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PasswordAttempt {
    Success,
    /// Empty input (^D): exit 0 → proceed to multi-user path.
    EmptyExit,
    Retry,
}

/// Classify one prompt attempt (C: init.c:754-762).
pub fn classify_attempt(input_empty: bool, matches: bool) -> PasswordAttempt {
    if input_empty {
        PasswordAttempt::EmptyExit
    } else if matches {
        PasswordAttempt::Success
    } else {
        PasswordAttempt::Retry
    }
}

/// Choose the shell path (C: ALTSHELL block, init.c:781-782).
pub fn choose_shell(altshell_input: &str, default: &str) -> String {
    let trimmed = altshell_input.trim();
    if trimmed.is_empty() {
        default.to_string()
    } else {
        trimmed.to_string()
    }
}

/// Wait-loop outcome (C: init.c:825-873).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WaitOutcome {
    Continue,
    Transition(StateKind),
    RestartSingleUser,
    RebootQuiet,
    ProceedRuncomFastboot,
}

/// Classify one wait observation (pure; I/O stays in the driver).
pub fn classify_wait(
    stopped: bool,
    requested: Option<StateKind>,
    exited_normally: bool,
    signaled: bool,
    termsig_is_kill: bool,
) -> WaitOutcome {
    if stopped {
        return WaitOutcome::Continue;
    }
    if let Some(state) = requested {
        return WaitOutcome::Transition(state);
    }
    if signaled {
        if termsig_is_kill {
            return WaitOutcome::RebootQuiet;
        }
        return WaitOutcome::RestartSingleUser;
    }
    if exited_normally {
        return WaitOutcome::ProceedRuncomFastboot;
    }
    WaitOutcome::RestartSingleUser
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gate_requires_password_matrix() {
        assert!(password_gate_required(false, 0, true));
        assert!(password_gate_required(true, 2, true));
        assert!(!password_gate_required(true, 0, true));
        assert!(!password_gate_required(false, 0, false));
    }

    #[test]
    fn test_empty_input_exits() {
        assert_eq!(classify_attempt(true, false), PasswordAttempt::EmptyExit);
        assert_eq!(classify_attempt(false, true), PasswordAttempt::Success);
        assert_eq!(classify_attempt(false, false), PasswordAttempt::Retry);
    }

    #[test]
    fn test_choose_shell_default_and_alt() {
        assert_eq!(choose_shell("", "/bin/sh"), "/bin/sh");
        assert_eq!(choose_shell("  ", "/bin/sh"), "/bin/sh");
        assert_eq!(choose_shell("/bin/ksh\n", "/bin/sh"), "/bin/ksh");
    }

    #[test]
    fn test_wait_stop_continues() {
        assert_eq!(
            classify_wait(true, None, false, false, false),
            WaitOutcome::Continue
        );
    }

    #[test]
    fn test_wait_requested_transitions() {
        assert_eq!(
            classify_wait(false, Some(StateKind::Death), false, false, false),
            WaitOutcome::Transition(StateKind::Death)
        );
    }

    #[test]
    fn test_wait_sigkill_quiets() {
        assert_eq!(
            classify_wait(false, None, false, true, true),
            WaitOutcome::RebootQuiet
        );
    }

    #[test]
    fn test_wait_normal_proceeds_runcom_fastboot() {
        assert_eq!(
            classify_wait(false, None, true, false, false),
            WaitOutcome::ProceedRuncomFastboot
        );
    }
}
