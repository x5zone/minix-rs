//! Logging and fatal-signal path for init.
//!
//! Covers `minix3/sbin/init/init.c:440-511` (`stall`, `warning`,
//! `emergency`, `disaster`). `print_console` (`#if 0`) and `badsys`
//! (non-Minix branch) are deliberately not modelled.
//! Design contract: `.design/03-design.v1.md §1.1-§1.4`.
//! Syslog gap: ARCH A-3 (no syslog service yet; console fallback).
//!
//! All output and sleeping go through the [`InitHost`] seam: the
//! console half replaces the earlier `LogSink`/`Clock` pair, and the
//! scripted host records what a test would have captured.

use crate::host::InitHost;

/// Syslog severity subset used by init.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    /// C: `LOG_ALERT` (`stall`/`warning`).
    Alert,
    /// C: `LOG_EMERG` (`emergency`/`disaster`).
    Emerg,
}

/// Stall timeout in seconds (C: `STALL_TIMEOUT`, init.c:95).
pub const STALL_TIMEOUT_SECS: u64 = 30;

/// Log and sleep (C: `stall`, init.c:440-450).
pub fn stall(host: &mut dyn InitHost, message: &str) {
    host.console_write(Severity::Alert, message);
    let _ = host.sleep_secs(STALL_TIMEOUT_SECS);
}

/// Log without sleeping (C: `warning`, init.c:457-466).
pub fn warning(host: &mut dyn InitHost, message: &str) {
    host.console_write(Severity::Alert, message);
}

/// Log an emergency (C: `emergency`, init.c:472-481).
pub fn emergency(host: &mut dyn InitHost, message: &str) {
    host.console_write(Severity::Emerg, message);
}

/// What `disaster` asks the caller to do (C: `_exit(sig)`, init.c:510).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DisasterAction {
    ExitWith(i32),
}

/// Handle a fatal signal (C: `disaster`, init.c:504-511).
///
/// Records the fatal message, sleeps so it can be read, and returns
/// the requested exit code instead of exiting inline — exiting is the
/// caller's branch, which keeps the function testable.
pub fn disaster(host: &mut dyn InitHost, sig: i32, sig_name: &str) -> DisasterAction {
    host.console_write(Severity::Emerg, &format!("fatal signal: {sig_name}"));
    let _ = host.sleep_secs(STALL_TIMEOUT_SECS);
    DisasterAction::ExitWith(sig)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::host::ScriptHost;

    #[test]
    fn test_stall_logs_alert_and_sleeps_30() {
        let mut host = ScriptHost::default();
        stall(&mut host, "can't exec sh");
        assert_eq!(host.console.len(), 1);
        assert_eq!(host.console[0].0, Severity::Alert);
        assert_eq!(host.slept, vec![STALL_TIMEOUT_SECS]);
    }

    #[test]
    fn test_warning_logs_alert_without_sleep() {
        let mut host = ScriptHost::default();
        warning(&mut host, "ignoring excess arguments");
        assert_eq!(host.console[0].0, Severity::Alert);
        assert!(host.slept.is_empty());
    }

    #[test]
    fn test_emergency_logs_emerg() {
        let mut host = ScriptHost::default();
        emergency(&mut host, "cannot get kernel security level");
        assert_eq!(host.console[0].0, Severity::Emerg);
    }

    #[test]
    fn test_disaster_records_and_requests_exit() {
        let mut host = ScriptHost::default();
        let action = disaster(&mut host, 11, "SIGSEGV");
        assert!(host.console[0].1.contains("SIGSEGV"));
        assert_eq!(host.slept, vec![STALL_TIMEOUT_SECS]);
        assert_eq!(action, DisasterAction::ExitWith(11));
    }
}
