//! Session node model.
//!
//! Covers `minix3/sbin/init/init.c:156-170` (`session_t`), `161-162`
//! (`SE_*`), `1101-1218` (`construct_argv`, `free_session`,
//! `new_session`, `setupargv`).
//! Design contract: `.design/07-design.v1.md §1.1-§1.4`.

use alloc::{string::String, string::ToString, vec::Vec};

/// Shutdown flag (C: `SE_SHUTDOWN`, init.c:161).
pub const SE_SHUTDOWN: u8 = 0x1;
/// Present-in-ttys flag (C: `SE_PRESENT`, init.c:162).
pub const SE_PRESENT: u8 = 0x2;

/// Session status flags.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SessionFlags(u8);

impl SessionFlags {
    pub fn empty() -> Self {
        SessionFlags(0)
    }

    pub fn present() -> Self {
        SessionFlags(SE_PRESENT)
    }

    pub fn contains(self, flag: u8) -> bool {
        self.0 & flag != 0
    }

    pub fn set(&mut self, flag: u8) {
        self.0 |= flag;
    }

    pub fn clear(&mut self, flag: u8) {
        self.0 &= !flag;
    }
}

/// A spawn request: the exec path plus the full argv.
///
/// Minix3 execs children in two shapes, and one field cannot express
/// both. Getty and window commands exec their first ttys word as the
/// path (`execv(sp->se_getty_argv[0], ...)`, init.c:1365), so there
/// `exec_path` equals `argv[0]`. The rc shell and the shutdown hooks
/// exec a fixed absolute path with a bare argv[0] (`execv(INIT_BSHELL,
/// ...)` where argv[0] is "sh", init.c:913; `execl("/sbin/shutdown",
/// "shutdown", ...)`, init.c:521-522) — there the two differ.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedCommand {
    /// Path handed to the exec call.
    pub exec_path: String,
    /// Full argv, including argv[0] as the child should see it.
    pub argv: Vec<String>,
}

impl ParsedCommand {
    /// Build from a word-split command line where the first word is
    /// both path and argv[0] (the getty/window shape).
    pub fn path_is_first_word(argv: Vec<String>) -> ParsedCommand {
        let exec_path = argv.first().cloned().unwrap_or_default();
        ParsedCommand { exec_path, argv }
    }
}

/// Split a command line on whitespace (C: `construct_argv`, init.c:1101-1118).
///
/// Empty/blank commands yield `None` (C: NULL).
pub fn split_command(command: &str) -> Option<Vec<String>> {
    let parts: Vec<String> = command.split_whitespace().map(str::to_string).collect();
    if parts.is_empty() {
        None
    } else {
        Some(parts)
    }
}

/// One login session (C: `session_t` minus intrusive list links).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Session {
    pub index: usize,
    pub process: Option<i32>,
    pub flags: SessionFlags,
    pub device: String,
    pub getty: Option<ParsedCommand>,
    pub window: Option<ParsedCommand>,
    /// Wall-clock seconds of the last spawn (C: `se_started`), the
    /// getty debounce input; 0 = never started.
    pub started_secs: i64,
}

/// Why a session could not be built.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuildReject {
    OffOrMissing,
}

/// Build a session from one ttys entry (C: `new_session`+`setupargv`).
///
/// `status_on=false` (or empty name/getty) rejects with
/// [`BuildReject::OffOrMissing`] (C: init.c:1147-1149). Device is
/// `/dev/` + name (C: init.c:1159). Getty command is
/// `"<getty> <name>"` split into argv (C: init.c:1193-1196).
pub fn build_session(
    index: usize,
    name: &str,
    getty: &str,
    window: Option<&str>,
    status_on: bool,
) -> Result<Session, BuildReject> {
    if !status_on || name.is_empty() || getty.is_empty() {
        return Err(BuildReject::OffOrMissing);
    }
    let getty_cmd = format!("{getty} {name}");
    let getty_argv = split_command(&getty_cmd).ok_or(BuildReject::OffOrMissing)?;
    let window_parsed = match window {
        Some(w) if !w.trim().is_empty() => {
            let argv = split_command(w).ok_or(BuildReject::OffOrMissing)?;
            Some(ParsedCommand::path_is_first_word(argv))
        }
        _ => None,
    };
    Ok(Session {
        index,
        process: None,
        flags: SessionFlags::present(),
        device: format!("/dev/{name}"),
        getty: Some(ParsedCommand::path_is_first_word(getty_argv)),
        window: window_parsed,
        started_secs: 0,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_split_simple() {
        assert_eq!(
            split_command("getty tty1"),
            Some(vec!["getty".to_string(), "tty1".to_string()])
        );
    }

    #[test]
    fn test_split_empty_none() {
        assert_eq!(split_command("   "), None);
    }

    #[test]
    fn test_build_rejects_off() {
        assert_eq!(
            build_session(1, "tty1", "getty", None, false),
            Err(BuildReject::OffOrMissing)
        );
    }

    #[test]
    fn test_build_device_prefix() {
        let s = build_session(2, "tty1", "getty", None, true).unwrap();
        assert_eq!(s.device, "/dev/tty1");
        assert_eq!(s.flags, SessionFlags::present());
    }

    #[test]
    fn test_flags_bits() {
        let mut f = SessionFlags::empty();
        f.set(SE_SHUTDOWN);
        assert!(f.contains(SE_SHUTDOWN));
        f.clear(SE_SHUTDOWN);
        assert!(!f.contains(SE_SHUTDOWN));
    }

    #[test]
    fn test_window_optional() {
        let s = build_session(1, "tty1", "getty", Some("Xwindow"), true).unwrap();
        assert!(s.window.is_some());
        let s2 = build_session(1, "tty1", "getty", None, true).unwrap();
        assert_eq!(s2.window, None);
    }
}
