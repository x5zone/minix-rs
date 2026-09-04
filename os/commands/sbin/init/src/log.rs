//! Logging and fatal-signal path for init.
//!
//! Covers `minix3/sbin/init/init.c:440-511` (`stall`, `warning`,
//! `emergency`, `disaster`). `print_console` (`#if 0`) and `badsys`
//! (non-Minix branch) are deliberately not modelled.
//! Design contract: `.design/03-design.v1.md §1.1-§1.4`.
//! Syslog gap: ARCH A-3 (no syslog service yet; console fallback).

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

/// Log sink boundary (C: `vsyslog` + `closelog`).
pub trait LogSink {
    fn log(&mut self, severity: Severity, message: &str);
}

/// In-memory fake sink for tests.
#[derive(Debug, Default)]
pub struct FakeLogSink {
    pub records: Vec<(Severity, String)>,
}

impl LogSink for FakeLogSink {
    fn log(&mut self, severity: Severity, message: &str) {
        self.records.push((severity, message.to_string()));
    }
}

/// Console fallback sink (ARCH A-3: syslog deferred).
#[derive(Debug, Default)]
pub struct ConsoleLogSink;

impl LogSink for ConsoleLogSink {
    fn log(&mut self, _severity: Severity, _message: &str) {
        // Deferred: write to console once minix_sys::write lands.
        // Deliberately silent today rather than panicking.
    }
}

/// Clock boundary so `stall` sleeps are testable (ARCH A-10).
pub trait Clock {
    fn sleep_secs(&mut self, secs: u64);
}

/// Fake clock records sleeps instead of blocking.
#[derive(Debug, Default)]
pub struct FakeClock {
    pub slept: Vec<u64>,
}

impl Clock for FakeClock {
    fn sleep_secs(&mut self, secs: u64) {
        self.slept.push(secs);
    }
}

/// Log and sleep (C: `stall`, init.c:440-450).
pub fn stall(sink: &mut dyn LogSink, clock: &mut dyn Clock, message: &str) {
    sink.log(Severity::Alert, message);
    clock.sleep_secs(STALL_TIMEOUT_SECS);
}

/// Log without sleeping (C: `warning`, init.c:457-466).
pub fn warning(sink: &mut dyn LogSink, message: &str) {
    sink.log(Severity::Alert, message);
}

/// Log an emergency (C: `emergency`, init.c:472-481).
pub fn emergency(sink: &mut dyn LogSink, message: &str) {
    sink.log(Severity::Emerg, message);
}

/// What `disaster` asks the caller to do (C: `_exit(sig)`, init.c:510).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DisasterAction {
    ExitWith(i32),
}

/// Handle a fatal signal (C: `disaster`, init.c:504-511).
///
/// Records the fatal message, sleeps so it can be read, and returns the
/// requested exit code instead of exiting inline (testable).
pub fn disaster(
    sink: &mut dyn LogSink,
    clock: &mut dyn Clock,
    sig: i32,
    sig_name: &str,
) -> DisasterAction {
    sink.log(Severity::Emerg, &format!("fatal signal: {sig_name}"));
    clock.sleep_secs(STALL_TIMEOUT_SECS);
    DisasterAction::ExitWith(sig)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stall_logs_alert_and_sleeps_30() {
        let mut sink = FakeLogSink::default();
        let mut clock = FakeClock::default();
        stall(&mut sink, &mut clock, "can't exec sh");
        assert_eq!(sink.records.len(), 1);
        assert_eq!(sink.records[0].0, Severity::Alert);
        assert_eq!(clock.slept, vec![STALL_TIMEOUT_SECS]);
    }

    #[test]
    fn test_warning_logs_alert_without_sleep() {
        let mut sink = FakeLogSink::default();
        let mut clock = FakeClock::default();
        warning(&mut sink, "ignoring excess arguments");
        assert_eq!(sink.records[0].0, Severity::Alert);
        assert!(clock.slept.is_empty());
    }

    #[test]
    fn test_emergency_logs_emerg() {
        let mut sink = FakeLogSink::default();
        emergency(&mut sink, "cannot get kernel security level");
        assert_eq!(sink.records[0].0, Severity::Emerg);
    }

    #[test]
    fn test_disaster_records_and_requests_exit() {
        let mut sink = FakeLogSink::default();
        let mut clock = FakeClock::default();
        let action = disaster(&mut sink, &mut clock, 11, "SIGSEGV");
        assert!(sink.records[0].1.contains("SIGSEGV"));
        assert_eq!(clock.slept, vec![STALL_TIMEOUT_SECS]);
        assert_eq!(action, DisasterAction::ExitWith(11));
    }

    #[test]
    fn test_console_sink_never_panics_on_empty() {
        let mut sink = ConsoleLogSink;
        sink.log(Severity::Alert, "");
        sink.log(Severity::Emerg, "");
    }
}
