//! The two service command lines.
//!
//! Ground truth keeps two different commands apart, and so does this
//! module:
//!
//! - `minix-service` (`minix3/minix/commands/minix-service/minix-service.8`)
//!   talks to the reincarnation server: `minix-service (up|run|edit|update)
//!   <binary> [...]`, `minix-service (down|refresh|restart|clone) <label>`,
//!   `minix-service shutdown`.
//! - `service` (`minix3/usr.sbin/service/service`, a shell script) wraps the
//!   boot script directories: `service [-elv]`, `service [-ev] name
//!   [name...]`, `service [-v] name action`.
//!
//! The C side performs effects directly (issuing reincarnation server
//! requests, running a boot script). This module models only the *shape* of
//! a valid invocation so the program layer can decide before acting.

use crate::SchedError;

/// What the caller asks the reincarnation server to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReincarnationAction {
    /// Start a new system service from a binary.
    Up,
    /// Run a binary as a service without registering it permanently.
    Run,
    /// Edit the configuration of a service.
    Edit,
    /// Update a running service in place.
    Update,
    /// Terminate the service called label.
    Down,
    /// Re-read the configuration of the service called label.
    Refresh,
    /// Stop then start the service called label.
    Restart,
    /// Duplicate the service called label.
    Clone,
    /// Shut the whole service layer down (takes no target).
    Shutdown,
}

/// A parsed `minix-service` invocation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReincarnationCommand<'a> {
    /// Requested action.
    pub action: ReincarnationAction,
    /// Binary path (for `up`/`run`/`edit`/`update`) or service label (for
    /// `down`/`refresh`/`restart`/`clone`); absent only for `shutdown`.
    pub target: Option<&'a str>,
    /// Trailing words (`-args`, `-label`, `-period`, ...) passed through
    /// verbatim to the reincarnation server request.
    pub extra: &'a [&'a str],
}

/// Parse the words after the `minix-service` program name.
pub fn parse_minix_service_args<'a>(
    argv: &'a [&'a str],
) -> Result<ReincarnationCommand<'a>, SchedError> {
    let [action_word, rest @ ..] = argv else {
        return Err(SchedError::InvalidArgument);
    };
    let action = parse_reincarnation_action(action_word)?;
    if action == ReincarnationAction::Shutdown {
        if rest.is_empty() {
            return Ok(ReincarnationCommand {
                action,
                target: None,
                extra: &[],
            });
        }
        return Err(SchedError::InvalidArgument);
    }
    let [target, extra @ ..] = rest else {
        return Err(SchedError::InvalidArgument);
    };
    Ok(ReincarnationCommand {
        action,
        target: Some(check_name(target)?),
        extra,
    })
}

fn parse_reincarnation_action(word: &str) -> Result<ReincarnationAction, SchedError> {
    match word {
        "up" => Ok(ReincarnationAction::Up),
        "run" => Ok(ReincarnationAction::Run),
        "edit" => Ok(ReincarnationAction::Edit),
        "update" => Ok(ReincarnationAction::Update),
        "down" => Ok(ReincarnationAction::Down),
        "refresh" => Ok(ReincarnationAction::Refresh),
        "restart" => Ok(ReincarnationAction::Restart),
        "clone" => Ok(ReincarnationAction::Clone),
        "shutdown" => Ok(ReincarnationAction::Shutdown),
        _ => Err(SchedError::InvalidArgument),
    }
}

/// A parsed `service` (boot script wrapper) invocation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RcServiceCommand<'a> {
    /// `-e`: list enabled scripts (or test the named ones).
    pub list_enabled: bool,
    /// `-l`: list every script in boot order.
    pub list_all: bool,
    /// `-v`: say in which directory each script was found.
    pub verbose: bool,
    /// Script names the command applies to (empty with bare `-l`/`-e`).
    pub names: &'a [&'a str],
    /// Action handed to the script (`start`, `stop`, `restart`, `status`,
    /// or any script specific word); absent when only listing or testing.
    pub action: Option<&'a str>,
}

/// Parse the words after the `service` program name.
///
/// Flags may bundle (`-ev`). The last word is the action when two or more
/// non flag words are present... precisely: with the NetBSD shape `service
/// name action`, everything but the final word names scripts and the final
/// word is the action; with a single word the command tests whether that
/// script is enabled. A bare flag-only invocation only lists.
pub fn parse_rc_service_args<'a>(
    argv: &'a [&'a str],
) -> Result<RcServiceCommand<'a>, SchedError> {
    let mut list_enabled = false;
    let mut list_all = false;
    let mut verbose = false;
    let mut index = 0;
    while index < argv.len() {
        let word = argv[index];
        if word.len() > 1 && word.starts_with('-') {
            for flag in word.bytes().skip(1) {
                match flag {
                    b'e' => list_enabled = true,
                    b'l' => list_all = true,
                    b'v' => verbose = true,
                    _ => return Err(SchedError::InvalidArgument),
                }
            }
            index += 1;
        } else {
            break;
        }
    }
    if list_enabled && list_all {
        return Err(SchedError::InvalidArgument);
    }
    let operands = &argv[index..];
    let (names, action) = match operands {
        [] => (&[][..], None),
        [single] => {
            check_name(single)?;
            (&operands[..1], None)
        }
        [names @ .., action] => {
            for name in names.iter() {
                check_name(name)?;
            }
            (names, Some(check_action_word(action)?))
        }
    };
    Ok(RcServiceCommand {
        list_enabled,
        list_all,
        verbose,
        names,
        action,
    })
}

fn check_action_word(word: &str) -> Result<&str, SchedError> {
    if word.is_empty() {
        return Err(SchedError::InvalidArgument);
    }
    check_name(word)
}

fn check_name(name: &str) -> Result<&str, SchedError> {
    if name.is_empty() {
        return Err(SchedError::InvalidArgument);
    }
    // Absolute paths are legitimate (service binaries live under
    // /service), but a `..` component would escape the script or service
    // directory, so it is rejected wherever it appears.
    if name.split('/').any(|component| component == "..") {
        return Err(SchedError::InvalidArgument);
    }
    let ok = name
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-' || b == b'.' || b == b'/');
    if ok {
        Ok(name)
    } else {
        Err(SchedError::InvalidArgument)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_up_with_binary_and_options() {
        let argv: [&str; 5] = ["up", "/service/vm", "-args", "a", "b"];
        let command = parse_minix_service_args(&argv).unwrap();
        assert_eq!(command.action, ReincarnationAction::Up);
        assert_eq!(command.target, Some("/service/vm"));
        assert_eq!(command.extra, &["-args", "a", "b"]);
    }

    #[test]
    fn test_down_with_label() {
        let command = parse_minix_service_args(&["down", "vm"]).unwrap();
        assert_eq!(command.action, ReincarnationAction::Down);
        assert_eq!(command.target, Some("vm"));
    }

    #[test]
    fn test_shutdown_takes_no_target() {
        let command = parse_minix_service_args(&["shutdown"]).unwrap();
        assert_eq!(command.action, ReincarnationAction::Shutdown);
        assert_eq!(command.target, None);
    }

    #[test]
    fn test_shutdown_with_target_rejected() {
        assert_eq!(
            parse_minix_service_args(&["shutdown", "vm"]),
            Err(SchedError::InvalidArgument)
        );
    }

    #[test]
    fn test_missing_target_rejected() {
        assert_eq!(
            parse_minix_service_args(&["down"]),
            Err(SchedError::InvalidArgument)
        );
        assert_eq!(parse_minix_service_args(&[]), Err(SchedError::InvalidArgument));
    }

    #[test]
    fn test_unknown_reincarnation_action_rejected() {
        assert_eq!(
            parse_minix_service_args(&["explode", "vm"]),
            Err(SchedError::InvalidArgument)
        );
    }

    #[test]
    fn test_rc_name_action_form() {
        let command = parse_rc_service_args(&["cron", "restart"]).unwrap();
        assert_eq!(command.names, &["cron"]);
        assert_eq!(command.action, Some("restart"));
    }

    #[test]
    fn test_rc_single_name_tests_enabled() {
        let command = parse_rc_service_args(&["cron"]).unwrap();
        assert_eq!(command.names, &["cron"]);
        assert_eq!(command.action, None);
    }

    #[test]
    fn test_rc_bundled_flags() {
        let command = parse_rc_service_args(&["-ev", "cron"]).unwrap();
        assert!(command.list_enabled);
        assert!(command.verbose);
        assert!(!command.list_all);
    }

    #[test]
    fn test_rc_conflicting_list_flags_rejected() {
        assert_eq!(
            parse_rc_service_args(&["-el"]),
            Err(SchedError::InvalidArgument)
        );
    }

    #[test]
    fn test_rc_unknown_flag_rejected() {
        assert_eq!(
            parse_rc_service_args(&["-x"]),
            Err(SchedError::InvalidArgument)
        );
    }

    #[test]
    fn test_rc_path_separator_in_name_rejected() {
        // `..` smuggling stays out of the script directory lookup; `/` is
        // allowed because script paths may be absolute.
        assert_eq!(
            parse_rc_service_args(&["../etc/passwd", "start"]),
            Err(SchedError::InvalidArgument)
        );
        assert_eq!(
            parse_rc_service_args(&["cron;reboot", "start"]),
            Err(SchedError::InvalidArgument)
        );
    }
}
