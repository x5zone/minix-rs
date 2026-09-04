//! Session node model.
//!
//! Covers `minix3/sbin/init/init.c:156-170` (`session_t`), `161-162`
//! (`SE_*`), `1101-1218` (`construct_argv`, `free_session`,
//! `new_session`, `setupargv`).
//! Design contract: `.design/07-design.v1.md §1.1-§1.4`.

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

/// A parsed command (program + argv).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedCommand {
    pub program: String,
    pub argv: Vec<String>,
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
    let getty_program = getty_argv[0].clone();
    let window_parsed = match window {
        Some(w) if !w.trim().is_empty() => {
            let argv = split_command(w).ok_or(BuildReject::OffOrMissing)?;
            Some(ParsedCommand {
                program: argv[0].clone(),
                argv,
            })
        }
        _ => None,
    };
    Ok(Session {
        index,
        process: None,
        flags: SessionFlags::present(),
        device: format!("/dev/{name}"),
        getty: Some(ParsedCommand {
            program: getty_program,
            argv: getty_argv,
        }),
        window: window_parsed,
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
