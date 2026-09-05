//! Signal name/number conversion behind `kill -l` and `kill -s`.
//!
//! Ground truth: `minix3/bin/kill/kill.c` (`signame_to_signum` at line 188
//! compares case insensitively against the system names at line 195, the
//! `-l` list prints them at line 105, the default signal is `SIGTERM` at
//! line 83). The numbers below are exact copies from
//! `minix3/sys/sys/signal.h` lines 52 to 84 (hangup 1 through power 32).

use crate::ProcError;

/// (Name without the `SIG` prefix, number) pairs in signal order.
pub const SIGNALS: [(&str, u8); 32] = [
    ("HUP", 1),
    ("INT", 2),
    ("QUIT", 3),
    ("ILL", 4),
    ("TRAP", 5),
    ("ABRT", 6),
    ("EMT", 7),
    ("FPE", 8),
    ("KILL", 9),
    ("BUS", 10),
    ("SEGV", 11),
    ("SYS", 12),
    ("PIPE", 13),
    ("ALRM", 14),
    ("TERM", 15),
    ("URG", 16),
    ("STOP", 17),
    ("TSTP", 18),
    ("CONT", 19),
    ("CHLD", 20),
    ("TTIN", 21),
    ("TTOU", 22),
    ("IO", 23),
    ("XCPU", 24),
    ("XFSZ", 25),
    ("VTALRM", 26),
    ("PROF", 27),
    ("WINCH", 28),
    ("INFO", 29),
    ("USR1", 30),
    ("USR2", 31),
    ("PWR", 32),
];

/// The default signal when none is named (`SIGTERM`, `kill.c:83`).
pub const DEFAULT_SIGNAL: u8 = 15;

/// Look up a signal by name, case insensitively, with or without the `SIG`
/// prefix (`TERM`, `term`, and `SIGTERM` all name 15). Unknown names are an
/// error, matching the C tool refusing them.
pub fn signame_to_signum(name: &str) -> Result<u8, ProcError> {
    let bare = name.strip_prefix("SIG").unwrap_or(name);
    // `SIG` alone names nothing.
    if bare.is_empty() {
        return Err(ProcError::InvalidArgument);
    }
    for (known, number) in SIGNALS {
        if known.len() == bare.len()
            && known
                .bytes()
                .zip(bare.bytes())
                .all(|(a, b)| a == b.to_ascii_uppercase())
        {
            return Ok(number);
        }
    }
    Err(ProcError::InvalidArgument)
}

/// Look up a signal by number: the canonical name without prefix, or
/// `None` for numbers with no signal.
pub fn signum_to_signame(number: u8) -> Option<&'static str> {
    SIGNALS
        .iter()
        .find(|(_, n)| *n == number)
        .map(|(name, _)| *name)
}

/// Parse a `kill` signal word: a plain number (`9`), or a name with at
/// most one leading dash (`-KILL`, `-TERM`, mirroring how the option parser
/// hands the word over). Out of range numbers are rejected.
pub fn parse_signal(word: &str) -> Result<u8, ProcError> {
    let text = word.strip_prefix('-').unwrap_or(word);
    if !text.is_empty() && text.bytes().all(|b| b.is_ascii_digit()) {
        let mut value: u32 = 0;
        for byte in text.bytes() {
            value = value * 10 + (byte - b'0') as u32;
        }
        if value == 0 || value > 32 {
            return Err(ProcError::InvalidArgument);
        }
        return Ok(value as u8);
    }
    signame_to_signum(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_names_resolve() {
        assert_eq!(signame_to_signum("TERM"), Ok(15));
        assert_eq!(signame_to_signum("term"), Ok(15));
        assert_eq!(signame_to_signum("SIGTERM"), Ok(15));
        assert_eq!(signame_to_signum("KILL"), Ok(9));
        assert_eq!(signame_to_signum("HUP"), Ok(1));
        assert_eq!(signame_to_signum("USR1"), Ok(30));
        assert_eq!(signame_to_signum("PWR"), Ok(32));
    }

    #[test]
    fn test_unknown_names_rejected() {
        assert_eq!(
            signame_to_signum("EXPLODE"),
            Err(ProcError::InvalidArgument)
        );
        assert_eq!(signame_to_signum("SIG"), Err(ProcError::InvalidArgument));
        assert_eq!(signame_to_signum(""), Err(ProcError::InvalidArgument));
    }

    #[test]
    fn test_numbers_round_trip() {
        for (name, number) in SIGNALS {
            assert_eq!(signum_to_signame(number), Some(name));
        }
        assert_eq!(signum_to_signame(0), None);
        assert_eq!(signum_to_signame(33), None);
    }

    #[test]
    fn test_parse_signal_words() {
        assert_eq!(parse_signal("9"), Ok(9));
        assert_eq!(parse_signal("-KILL"), Ok(9));
        assert_eq!(parse_signal("-TERM"), Ok(15));
        assert_eq!(parse_signal("0"), Err(ProcError::InvalidArgument));
        assert_eq!(parse_signal("64"), Err(ProcError::InvalidArgument));
    }

    #[test]
    fn test_default_is_term() {
        assert_eq!(DEFAULT_SIGNAL, 15);
        assert_eq!(signum_to_signame(DEFAULT_SIGNAL), Some("TERM"));
    }
}
