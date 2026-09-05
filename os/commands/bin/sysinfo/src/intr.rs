//! Interrupt scoped execution options.
//!
//! Ground truth: `minix3/minix/commands/intr/intr.c`. The background flag
//! lives near line 51, the log device `/dev/log` (with `/dev/console` as the
//! fallback on non Minix builds) near line 19, arming the deadline with
//! `alarm` near line 140. Foreground mode brings the command into the
//! foreground with default signal handling; background mode detaches into its
//! own session, redirects input from nowhere and output to the log device,
//! and moves to the root directory. The execution layer owns process setup;
//! this module owns the options.

use crate::SysinfoError;

/// Interrupt scoped execution options.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct IntrOptions {
    /// `-d`: background mode (detach, log to the log device).
    pub background: bool,
    /// `-t seconds`: deadline in seconds, or `None` for no deadline.
    pub deadline_seconds: Option<u32>,
}

/// Parse decimal seconds for the deadline (must fit in `u32`).
pub fn parse_deadline(word: &str) -> Result<u32, SysinfoError> {
    if word.is_empty() {
        return Err(SysinfoError::InvalidArgument);
    }
    let mut value: u64 = 0;
    for byte in word.bytes() {
        if !byte.is_ascii_digit() {
            return Err(SysinfoError::InvalidArgument);
        }
        value = value
            .checked_mul(10)
            .and_then(|scaled| scaled.checked_add((byte - b'0') as u64))
            .ok_or(SysinfoError::InvalidArgument)?;
    }
    if value > u32::MAX as u64 {
        return Err(SysinfoError::InvalidArgument);
    }
    Ok(value as u32)
}

/// Parse the option words before the command (words after `--` end options).
pub fn parse_intr_options(words: &[&str]) -> Result<(IntrOptions, usize), SysinfoError> {
    let mut options = IntrOptions::default();
    let mut index = 0;
    while index < words.len() {
        let word = words[index];
        if word == "--" {
            index += 1;
            break;
        }
        if !word.starts_with('-') || word.len() < 2 {
            break;
        }
        let mut bytes = word.as_bytes()[1..].iter();
        let mut consumed = false;
        while let Some(byte) = bytes.next() {
            match byte {
                b'd' => {
                    options.background = true;
                    consumed = true;
                }
                b't' => {
                    let rest: &[u8] = bytes.as_slice();
                    let text = if rest.is_empty() {
                        index += 1;
                        if index >= words.len() {
                            return Err(SysinfoError::InvalidArgument);
                        }
                        words[index]
                    } else {
                        core::str::from_utf8(rest).map_err(|_| SysinfoError::InvalidArgument)?
                    };
                    options.deadline_seconds = Some(parse_deadline(text)?);
                    consumed = true;
                    break;
                }
                _ => return Err(SysinfoError::InvalidArgument),
            }
        }
        let _ = consumed;
        index += 1;
    }
    Ok((options, index))
}

/// Log device used in background mode.
pub const LOG_DEVICE: &str = "/dev/log";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_background_parses() {
        let (options, next) = parse_intr_options(&["-d", "sleep", "10"]).unwrap();
        assert!(options.background);
        assert_eq!(next, 1);
    }

    #[test]
    fn test_deadline_attached_and_split() {
        let (options, _) = parse_intr_options(&["-t60", "sleep"]).unwrap();
        assert_eq!(options.deadline_seconds, Some(60));
        let (options, _) = parse_intr_options(&["-t", "60", "sleep"]).unwrap();
        assert_eq!(options.deadline_seconds, Some(60));
    }

    #[test]
    fn test_combined_flags() {
        let (options, _) = parse_intr_options(&["-dt30", "sleep"]).unwrap();
        assert!(options.background);
        assert_eq!(options.deadline_seconds, Some(30));
    }

    #[test]
    fn test_double_dash_ends_options() {
        let (options, next) = parse_intr_options(&["--", "-d"]).unwrap();
        assert!(!options.background);
        assert_eq!(next, 1);
    }

    #[test]
    fn test_unknown_flag_rejected() {
        assert_eq!(
            parse_intr_options(&["-x", "sleep"]).map(|pair| pair.0),
            Err(SysinfoError::InvalidArgument)
        );
    }

    #[test]
    fn test_dangling_deadline_rejected() {
        assert_eq!(
            parse_intr_options(&["-t"]).map(|pair| pair.0),
            Err(SysinfoError::InvalidArgument)
        );
    }

    #[test]
    fn test_deadline_bounds() {
        assert_eq!(parse_deadline("60"), Ok(60));
        assert_eq!(
            parse_deadline("99999999999"),
            Err(SysinfoError::InvalidArgument)
        );
        assert_eq!(parse_deadline("6x"), Err(SysinfoError::InvalidArgument));
    }
}
