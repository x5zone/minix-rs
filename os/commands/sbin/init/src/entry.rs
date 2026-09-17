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
/// The MAKEDEV fork/exec fallback (init.c:1757-1787) is the
/// [`ensure_console`] half of this module.
pub fn console_present(host: &dyn InitHost, console_path: &str) -> bool {
    matches!(host.path_exists(console_path), Ok(true))
}

/// Ensure the console exists, running the MAKEDEV helper when it does
/// not (C: `mfs_dev`'s MAKEDEV branch, init.c:1757-1787).
///
/// C runs this as a throwaway helper process — `_exit(10/11/12)` are
/// that helper's codes — while the probe here runs in-process, so the
/// helper exits collapse into `false` plus the same warnings. The
/// helper's `chdir("/dev")` collapses into an absolute script path;
/// C's `access("./MAKEDEV")` preference picks `/dev/MAKEDEV` after
/// that chdir, so the absolute form is the same file.
pub fn ensure_console(host: &mut dyn InitHost, console_path: &str) -> bool {
    if console_present(host, console_path) {
        return true;
    }
    let script = "/dev/MAKEDEV";
    let cmd = crate::session::ParsedCommand {
        exec_path: "/bin/sh".to_string(),
        argv: vec![
            "sh".to_string(),
            script.to_string(),
            "-MM".to_string(),
            "init".to_string(),
        ],
    };
    match host.fork() {
        Ok(0) => {
            let err = host.exec(&cmd);
            crate::log::warning(host, &format!("can't exec `{script}': {err}"));
            host.exit_process(10);
        }
        Err(_) => {
            crate::log::warning(host, "Unable to run MAKEDEV");
            false
        }
        Ok(pid) => {
            match host.waitpid(pid, 0) {
                Ok((_, status)) => {
                    if status.exit_code() != Some(0) {
                        crate::log::warning(
                            host,
                            &format!("MAKEDEV exit status {:?}", status.exit_code()),
                        );
                    }
                }
                Err(_) => crate::log::warning(host, "Unable to run MAKEDEV"),
            }
            console_present(host, console_path)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::host::ScriptHost;
    use minix_sys::Errno;

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

    #[test]
    fn test_ensure_console_runs_makedev_and_rechecks() {
        use std::panic::{catch_unwind, AssertUnwindSafe};
        // Console missing → MAKEDEV child runs; the recheck still
        // fails, so the C helper's _exit(11) collapses into `false`.
        let mut host = ScriptHost::default();
        host.fork_outcomes.push(Ok(0));
        host.exec_outcomes.push(Errno::EPERM);
        let _ = catch_unwind(AssertUnwindSafe(|| {
            assert!(!ensure_console(&mut host, "/dev/console"));
        }));
        let cmd = &host.exec_requests[0];
        assert_eq!(cmd.argv, vec!["sh", "/dev/MAKEDEV", "-MM", "init"]);
        assert_eq!(host.exits, vec![10]);

        // A successful MAKEDEV that created the console: helper exits
        // cleanly, the recheck succeeds.
        let mut host = ScriptHost::default();
        host.fork_outcomes.push(Ok(8));
        host.wait_outcomes
            .push(Ok((8, crate::wait::WaitStatus::Exited { code: 0 })));
        host.paths.push(("/dev/console".into(), true));
        assert!(ensure_console(&mut host, "/dev/console"));
    }

    #[test]
    fn test_ensure_console_fork_failure_is_false_not_panic() {
        let mut host = ScriptHost::default();
        host.fork_outcomes.push(Err(Errno::EAGAIN));
        assert!(!ensure_console(&mut host, "/dev/console"));
        assert!(host.console.iter().any(|(_, m)| m.contains("MAKEDEV")));
    }
}
