//! Daemon session setup shared by transfer and terminal daemons.
//!
//! Ground truth: `minix3/libexec/ftpd/ftpd.c` (command dispatch, login
//! accounting in `logutmp.c` and `logwtmp.c`), `minix3/libexec/telnetd/`
//! (terminal negotiation), and the remote shell daemon. All three greet the
//! peer with a banner, authenticate the user, optionally confine the session
//! to a change-root directory, and record the login. The execution layer owns
//! sockets and privilege changes; this module owns the setup record and the
//! admission decision.

use crate::ServiceError;

/// Authentication outcome for one session attempt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthOutcome {
    /// Credentials accepted.
    Accepted,
    /// Credentials rejected (wrong password or unknown user).
    Rejected,
    /// Login denied by policy (listed in the deny file).
    Denied,
}

/// One daemon session setup.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SessionSetup<'a> {
    /// User name supplied by the peer.
    pub user: &'a str,
    /// Banner line sent before authentication.
    pub banner: &'a str,
    /// Change-root directory, or `None` when the session sees the full tree.
    pub chroot: Option<&'a str>,
    /// Authentication outcome.
    pub auth: AuthOutcome,
}

/// Decide whether a session may proceed.
///
/// Empty user names are malformed. Rejected credentials report not found (the
/// daemon must not reveal whether the name exists). Denied logins report
/// denied. Only accepted sessions proceed.
pub fn admit_session(setup: SessionSetup<'_>) -> Result<(), ServiceError> {
    if setup.user.is_empty() {
        return Err(ServiceError::InvalidArgument);
    }
    match setup.auth {
        AuthOutcome::Accepted => Ok(()),
        AuthOutcome::Rejected => Err(ServiceError::NotFound),
        AuthOutcome::Denied => Err(ServiceError::Denied),
    }
}

/// True when the session is confined to a change-root directory.
pub fn is_confined(setup: SessionSetup<'_>) -> bool {
    setup.chroot.is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn accepted() -> SessionSetup<'static> {
        SessionSetup {
            user: "alice",
            banner: "220 ready",
            chroot: None,
            auth: AuthOutcome::Accepted,
        }
    }

    #[test]
    fn test_accepted_proceeds() {
        assert_eq!(admit_session(accepted()), Ok(()));
        assert!(!is_confined(accepted()));
    }

    #[test]
    fn test_rejected_reports_not_found() {
        let setup = SessionSetup {
            auth: AuthOutcome::Rejected,
            ..accepted()
        };
        assert_eq!(admit_session(setup), Err(ServiceError::NotFound));
    }

    #[test]
    fn test_denied_reports_denied() {
        let setup = SessionSetup {
            auth: AuthOutcome::Denied,
            ..accepted()
        };
        assert_eq!(admit_session(setup), Err(ServiceError::Denied));
    }

    #[test]
    fn test_empty_user_rejected() {
        let setup = SessionSetup {
            user: "",
            ..accepted()
        };
        assert_eq!(
            admit_session(setup),
            Err(ServiceError::InvalidArgument)
        );
    }

    #[test]
    fn test_chroot_confines() {
        let setup = SessionSetup {
            chroot: Some("/var/ftp"),
            ..accepted()
        };
        assert!(is_confined(setup));
    }

    #[test]
    fn test_error_numbers_match_unix() {
        assert_eq!(ServiceError::InvalidArgument.as_errno(), 22);
        assert_eq!(ServiceError::NotFound.as_errno(), 2);
        assert_eq!(ServiceError::Denied.as_errno(), 13);
    }
}
