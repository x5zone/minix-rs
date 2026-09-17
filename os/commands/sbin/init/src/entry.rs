//! Entry checks for the init process.
//!
//! Covers `minix3/sbin/init/init.c:229-367` (`main`) and the device-probe
//! entry point `minix3/sbin/init/init.c:1703-1788` (`mfs_dev`).
//! Design contract: `.design/01-design.v1.md §1.1-§1.4`.
//!
//! The module is intentionally free of side effects: argument parsing
//! and entry decisions answer questions, and the machine answers
//! through the [`InitHost`] seam (`path_exists`) — so the probe is
//! testable without a filesystem while the live host still asks the
//! real one.

use crate::host::InitHost;
use minix_sys::Errno;

/// How `/etc/rc` should run (C: `runcom_mode`, init.c:151).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuncomMode {
    Autoboot,
    Fastboot,
}

/// Parsed boot flags (C: `getopt(argc, argv, "sf")`, init.c:287-298).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct BootArgs {
    /// `-s`: start in single-user mode instead of runcom.
    pub single_user: bool,
    /// `-f`: run `/etc/rc` in fastboot mode (skip fs checks).
    pub fastboot: bool,
}

/// Parse boot arguments into [`BootArgs`].
///
/// `argv[0]` (program name) is skipped, matching `getopt` behaviour.
/// Unknown flags and excess positional arguments are collected as
/// warning strings (C: `warning("unrecognized flag ...")` /
/// `warning("ignoring excess arguments")`, init.c:295-301) instead of
/// being logged here, so the function stays pure.
pub fn parse_boot_args(argv: &[String]) -> (BootArgs, Vec<String>) {
    let mut args = BootArgs::default();
    let mut warnings = Vec::new();
    let mut end_of_flags = false;

    for arg in argv.iter().skip(1) {
        if end_of_flags {
            warnings.push("ignoring excess arguments".to_string());
            continue;
        }
        if arg == "--" {
            end_of_flags = true;
            continue;
        }
        if arg.starts_with('-') && arg.len() > 1 {
            for flag in arg.chars().skip(1) {
                match flag {
                    's' => args.single_user = true,
                    'f' => args.fastboot = true,
                    other => {
                        warnings.push(format!("unrecognized flag `{other}'"));
                    }
                }
            }
            continue;
        }
        warnings.push("ignoring excess arguments".to_string());
    }

    (args, warnings)
}

/// Entry identity failure (C: `err(1)` / `errx(1)`, init.c:242-249).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryError {
    /// `getuid() != 0` (C sets `errno = EPERM`).
    NotRoot,
    /// `getpid() != 1` (C: `"already running"`).
    AlreadyRunning,
}

impl EntryError {
    /// Map to the Minix3 errno value (no invented codes).
    pub fn to_errno(self) -> Errno {
        match self {
            EntryError::NotRoot => Errno::EPERM,
            // C uses errx (no errno) for the pid check; EEXIST is the
            // closest stable mapping for "already running" and is only
            // used for the exit-path translation, never as a syscall errno.
            EntryError::AlreadyRunning => Errno::EEXIST,
        }
    }
}

/// Verify the process is entitled to be init.
///
/// Pure wrapper over the C checks `getuid() != 0` (init.c:242-245) and
/// `getpid() != 1` (init.c:248-249). Exiting/logging is the caller's
/// policy, so this function returns `Result` for testability.
pub fn check_identity(uid: u32, pid: i32) -> Result<(), EntryError> {
    if uid != 0 {
        return Err(EntryError::NotRoot);
    }
    if pid != 1 {
        return Err(EntryError::AlreadyRunning);
    }
    Ok(())
}

/// First state to enter (subset of the full machine in 02).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InitialState {
    Runcom,
    SingleUser,
}

/// Resolved entry decision (C: `requested_transition` + `runcom_mode`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EntryDecision {
    pub initial: InitialState,
    pub runcom_mode: RuncomMode,
}

/// Resolve the first state from parsed args and the device-probe result.
///
/// Priority: failed device probe (`console_ok == false`, init.c:269-270)
/// forces single-user, otherwise `-s` selects single-user (init.c:290).
/// Default is runcom (init.c:195) with autoboot mode (init.c:151).
pub fn decide_entry(args: &BootArgs, console_ok: bool) -> EntryDecision {
    let initial = if !console_ok || args.single_user {
        InitialState::SingleUser
    } else {
        InitialState::Runcom
    };
    let runcom_mode = if args.fastboot {
        RuncomMode::Fastboot
    } else {
        RuncomMode::Autoboot
    };
    EntryDecision {
        initial,
        runcom_mode,
    }
}

/// Probe whether `/dev/console` is reachable (C: `mfs_dev`'s stat,
/// init.c:1729).
///
/// The MAKEDEV fork/exec fallback (init.c:1759-1787) lands with the
/// process entities; until then a missing console falls straight back
/// to single-user, which is also where C ends up when MAKEDEV cannot
/// save the day. The `#if 0` debug block (init.c:1716-1756) is
/// deliberately not modelled (dead code).
pub fn console_present(host: &dyn InitHost, console_path: &str) -> bool {
    matches!(host.path_exists(console_path), Ok(true))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::host::ScriptHost;

    fn argv(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn test_parse_no_args_defaults() {
        let (args, warns) = parse_boot_args(&argv(&["init"]));
        assert_eq!(args, BootArgs::default());
        assert!(warns.is_empty());
    }

    #[test]
    fn test_parse_single_user_flag() {
        let (args, warns) = parse_boot_args(&argv(&["init", "-s"]));
        assert!(args.single_user);
        assert!(!args.fastboot);
        assert!(warns.is_empty());
    }

    #[test]
    fn test_parse_fastboot_flag() {
        let (args, warns) = parse_boot_args(&argv(&["init", "-f"]));
        assert!(args.fastboot);
        assert!(!args.single_user);
        assert!(warns.is_empty());
    }

    #[test]
    fn test_parse_combined_flags() {
        let (args, warns) = parse_boot_args(&argv(&["init", "-sf"]));
        assert!(args.single_user && args.fastboot);
        assert!(warns.is_empty());
    }

    #[test]
    fn test_parse_unknown_flag_warns() {
        let (args, warns) = parse_boot_args(&argv(&["init", "-z"]));
        assert_eq!(args, BootArgs::default());
        assert_eq!(warns.len(), 1);
        assert!(warns[0].contains('z'));
    }

    #[test]
    fn test_parse_excess_args_warn() {
        let (_args, warns) = parse_boot_args(&argv(&["init", "extra"]));
        assert_eq!(warns, vec!["ignoring excess arguments".to_string()]);
    }

    #[test]
    fn test_identity_root_and_pid1_ok() {
        assert_eq!(check_identity(0, 1), Ok(()));
    }

    #[test]
    fn test_identity_non_root_fails() {
        assert_eq!(check_identity(1000, 1), Err(EntryError::NotRoot));
    }

    #[test]
    fn test_identity_wrong_pid_fails() {
        assert_eq!(check_identity(0, 42), Err(EntryError::AlreadyRunning));
    }

    #[test]
    fn test_decide_defaults_to_runcom() {
        let d = decide_entry(&BootArgs::default(), true);
        assert_eq!(
            d,
            EntryDecision {
                initial: InitialState::Runcom,
                runcom_mode: RuncomMode::Autoboot,
            }
        );
    }

    #[test]
    fn test_decide_single_user_flag() {
        let args = BootArgs {
            single_user: true,
            fastboot: false,
        };
        assert_eq!(
            decide_entry(&args, true).initial,
            InitialState::SingleUser
        );
    }

    #[test]
    fn test_decide_console_failure_forces_single_user() {
        let d = decide_entry(&BootArgs::default(), false);
        assert_eq!(d.initial, InitialState::SingleUser);
    }

    #[test]
    fn test_entry_error_maps_to_errno() {
        assert_eq!(EntryError::NotRoot.to_errno(), Errno::EPERM);
        assert_eq!(
            EntryError::AlreadyRunning.to_errno(),
            Errno::EEXIST
        );
    }

    #[test]
    fn test_probe_asks_the_host_seam() {
        // Present console: the scripted host answers from its table.
        let mut host = ScriptHost::default();
        host.paths.push(("/dev/console".into(), true));
        assert!(console_present(&host, "/dev/console"));

        // Missing console: probe false, entry falls to single-user.
        let mut missing = ScriptHost::default();
        missing.paths.push(("/dev/console".into(), false));
        assert!(!console_present(&missing, "/dev/console"));
        assert_eq!(
            decide_entry(&BootArgs::default(), console_present(&missing, "/dev/console")).initial,
            InitialState::SingleUser
        );

        // An erroring probe (ENOSYS on the real machine before
        // E-INITSYS) counts as "no console" — honest failure, and the
        // entry decision treats it exactly like C treats a missing
        // console device.
        assert!(!console_present(&crate::host::MinixSysHost, "/dev/console"));
    }
}
