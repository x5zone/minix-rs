//! System logger selectors, actions, and sinks.
//!
//! Ground truth: `minix3/usr.sbin/syslogd/syslogd.c`. Action kinds run from
//! `F_FILE` (regular file) through `F_FIFO` (first-in-first-out queue) near
//! lines 130 to 137. Priorities split into facility (`LOG_FACMASK`) and
//! severity (`LOG_PRIMASK`) near line 1488; messages without a facility fall
//! back to the default user priority `DEFUPRI`, kernel messages without one
//! fall back to `DEFSPRI` (near lines 60 to 61). The execution layer owns file
//! writes and network forwarding; this module owns decoding and routing.

use crate::ServiceError;

/// Action kinds (where one log line goes).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogAction {
    /// Regular file.
    File,
    /// Terminal device.
    Terminal,
    /// Console terminal.
    Console,
    /// Remote machine.
    Forward,
    /// Named user list.
    Users,
    /// Everyone logged on.
    Wall,
    /// Pipe to a program.
    Pipe,
    /// First-in-first-out queue file.
    Fifo,
}

/// Numeric action code in `syslogd.c` order (unused is zero).
pub fn action_number(action: LogAction) -> u8 {
    match action {
        LogAction::File => 1,
        LogAction::Terminal => 2,
        LogAction::Console => 3,
        LogAction::Forward => 4,
        LogAction::Users => 5,
        LogAction::Wall => 6,
        LogAction::Pipe => 7,
        LogAction::Fifo => 8,
    }
}

/// Decode a numeric action code.
pub fn decode_action(value: u8) -> Result<LogAction, ServiceError> {
    match value {
        1 => Ok(LogAction::File),
        2 => Ok(LogAction::Terminal),
        3 => Ok(LogAction::Console),
        4 => Ok(LogAction::Forward),
        5 => Ok(LogAction::Users),
        6 => Ok(LogAction::Wall),
        7 => Ok(LogAction::Pipe),
        8 => Ok(LogAction::Fifo),
        _ => Err(ServiceError::InvalidArgument),
    }
}

/// Facility numbers (a subset of the classic facility table).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Facility {
    /// Kernel messages.
    Kern,
    /// User level messages.
    User,
    /// Mail system.
    Mail,
    /// System daemons.
    Daemon,
    /// Authorization system.
    Auth,
    /// Local use zero through seven share one variant with an index.
    Local(u8),
}

/// Severity numbers in increasing urgency.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    /// Debug information.
    Debug,
    /// Informational message.
    Info,
    /// Noticeable but normal condition.
    Notice,
    /// Warning condition.
    Warning,
    /// Error condition.
    Error,
    /// Critical condition.
    Critical,
    /// Action must be taken immediately.
    Alert,
    /// System is unusable.
    Emergency,
}

/// Parse a severity word.
pub fn parse_severity(word: &str) -> Result<Severity, ServiceError> {
    match word {
        "debug" => Ok(Severity::Debug),
        "info" => Ok(Severity::Info),
        "notice" => Ok(Severity::Notice),
        "warning" | "warn" => Ok(Severity::Warning),
        "err" | "error" => Ok(Severity::Error),
        "crit" => Ok(Severity::Critical),
        "alert" => Ok(Severity::Alert),
        "emerg" | "panic" => Ok(Severity::Emergency),
        _ => Err(ServiceError::InvalidArgument),
    }
}

/// One decoded priority (facility plus severity).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Priority {
    /// Facility part.
    pub facility: Facility,
    /// Severity part.
    pub severity: Severity,
}

/// Decode a numeric priority: low three bits are severity, the rest facility.
pub fn decode_priority(value: u32) -> Priority {
    let severity = match (value & 0x07) as u8 {
        0 => Severity::Emergency,
        1 => Severity::Alert,
        2 => Severity::Critical,
        3 => Severity::Error,
        4 => Severity::Warning,
        5 => Severity::Notice,
        6 => Severity::Info,
        _ => Severity::Debug,
    };
    let facility = match (value & !0x07) >> 3 {
        0 => Facility::Kern,
        1 => Facility::User,
        2 => Facility::Mail,
        3 => Facility::Daemon,
        4 => Facility::Auth,
        index if (16..=23).contains(&index) => Facility::Local((index - 16) as u8),
        _ => Facility::User,
    };
    Priority { facility, severity }
}

/// One selector (`facility.severity`, either side may be `*` or `none`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Selector {
    /// Facility name, or `None` for any facility.
    pub facility: Option<&'static str>,
    /// Minimum severity, or `None` for `none` (this facility is excluded).
    pub minimum: Option<Severity>,
}

/// True when a message at `severity` passes `selector`.
pub fn selector_passes(selector: Selector, severity: Severity) -> bool {
    match selector.minimum {
        None => false,
        Some(minimum) => severity >= minimum,
    }
}

/// Log sink behind the routing decision.
pub trait LogSink {
    /// Store one line for `action`; lines longer than the sink stay rejected.
    fn store(&mut self, action: LogAction, line: &str) -> Result<(), ServiceError>;
    /// Stored line count.
    fn len(&self) -> usize;
    /// True when nothing is stored.
    fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// In-memory sink (keeps counts per action, drops the text).
pub struct MemorySink {
    counts: [u32; 8],
}

impl MemorySink {
    /// An empty sink.
    pub fn new() -> Self {
        MemorySink { counts: [0; 8] }
    }

    /// Lines stored for `action`.
    pub fn count_of(&self, action: LogAction) -> u32 {
        self.counts[action_number(action) as usize - 1]
    }
}

impl Default for MemorySink {
    fn default() -> Self {
        Self::new()
    }
}

impl LogSink for MemorySink {
    fn store(&mut self, action: LogAction, line: &str) -> Result<(), ServiceError> {
        if line.is_empty() || line.len() > 1024 {
            return Err(ServiceError::InvalidArgument);
        }
        let slot = &mut self.counts[action_number(action) as usize - 1];
        *slot = slot.saturating_add(1);
        Ok(())
    }

    fn len(&self) -> usize {
        self.counts.iter().map(|count| *count as usize).sum()
    }
}

/// Null sink (every store is silently accepted and forgotten).
pub struct NullSink;

impl LogSink for NullSink {
    fn store(&mut self, _action: LogAction, line: &str) -> Result<(), ServiceError> {
        if line.is_empty() {
            return Err(ServiceError::InvalidArgument);
        }
        Ok(())
    }

    fn len(&self) -> usize {
        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_action_codes_round_trip() {
        for value in 1u8..=8 {
            let action = decode_action(value).unwrap();
            assert_eq!(action_number(action), value);
        }
        assert_eq!(decode_action(0), Err(ServiceError::InvalidArgument));
        assert_eq!(decode_action(9), Err(ServiceError::InvalidArgument));
    }

    #[test]
    fn test_severities_parse() {
        assert_eq!(parse_severity("info"), Ok(Severity::Info));
        assert_eq!(parse_severity("warn"), Ok(Severity::Warning));
        assert_eq!(parse_severity("error"), Ok(Severity::Error));
        assert_eq!(parse_severity("panic"), Ok(Severity::Emergency));
        assert_eq!(
            parse_severity("loud"),
            Err(ServiceError::InvalidArgument)
        );
    }

    #[test]
    fn test_severity_ordering() {
        assert!(Severity::Debug < Severity::Emergency);
        assert!(Severity::Error >= Severity::Warning);
    }

    #[test]
    fn test_priority_decodes() {
        // Facility user (1), severity info (6): (1 << 3) | 6.
        let priority = decode_priority((1 << 3) | 6);
        assert_eq!(priority.facility, Facility::User);
        assert_eq!(priority.severity, Severity::Info);
    }

    #[test]
    fn test_selector_passes() {
        let selector = Selector {
            facility: Some("mail"),
            minimum: Some(Severity::Warning),
        };
        assert!(selector_passes(selector, Severity::Error));
        assert!(!selector_passes(selector, Severity::Info));
        let excluded = Selector {
            facility: Some("mail"),
            minimum: None,
        };
        assert!(!selector_passes(excluded, Severity::Emergency));
    }

    #[test]
    fn test_memory_sink_counts() {
        let mut sink = MemorySink::new();
        sink.store(LogAction::File, "hello").unwrap();
        sink.store(LogAction::File, "world").unwrap();
        sink.store(LogAction::Wall, "broadcast").unwrap();
        assert_eq!(sink.count_of(LogAction::File), 2);
        assert_eq!(sink.len(), 3);
    }

    #[test]
    fn test_memory_sink_rejects_empty_and_huge() {
        let mut sink = MemorySink::new();
        assert_eq!(
            sink.store(LogAction::File, ""),
            Err(ServiceError::InvalidArgument)
        );
        let huge = "x";
        // Build a 1025 byte line without heap in production style.
        let mut long = [b'x'; 1025];
        long[0] = b'y';
        let text = core::str::from_utf8(&long).unwrap();
        assert_eq!(
            sink.store(LogAction::File, text),
            Err(ServiceError::InvalidArgument)
        );
        let _ = huge;
    }

    #[test]
    fn test_null_sink_forgets() {
        let mut sink = NullSink;
        sink.store(LogAction::Wall, "hello").unwrap();
        assert_eq!(sink.len(), 0);
    }
}
