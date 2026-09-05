//! Hardware clock option parsing and direction.
//!
//! Ground truth: `minix3/minix/commands/readclock/readclock.c`. Reading uses
//! `RTCDEV_GET_TIME` (near line 73), writing uses `RTCDEV_SET_TIME` (near
//! line 122). The option letters are preview (`-n`), write to hardware
//! (`-w`), register access implies write (`-W`, sets `RTCDEV_CMOSREG` near
//! line 55), year 2000 workaround (`-2`, sets `RTCDEV_Y2KBUG` near line 59),
//! and quiet (`-q`). Usage is `readclock [-nqwW2]` (near line 164). Reads
//! retry at most ten times with five seconds between attempts until a valid
//! time arrives. The execution layer owns the driver call; this module owns
//! the options and the direction.

use crate::SysinfoError;

/// Hardware clock options, one boolean per option letter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ClockOptions {
    /// `-n`: preview (print what would happen, change nothing).
    pub preview: bool,
    /// `-w`: write the system time into the hardware clock.
    pub write_hardware: bool,
    /// `-W`: register access (implies writing to hardware).
    pub register_access: bool,
    /// `-2`: year 2000 workaround.
    pub year_2000_workaround: bool,
    /// `-q`: quiet (print nothing).
    pub quiet: bool,
}

/// Parse one option word (without the leading dash, letters may cluster).
pub fn parse_clock_options(word: &str) -> Result<ClockOptions, SysinfoError> {
    let mut options = ClockOptions::default();
    if word.is_empty() {
        return Err(SysinfoError::InvalidArgument);
    }
    for byte in word.bytes() {
        match byte {
            b'n' => options.preview = true,
            b'w' => options.write_hardware = true,
            b'W' => {
                options.register_access = true;
                options.write_hardware = true;
            }
            b'2' => options.year_2000_workaround = true,
            b'q' => options.quiet = true,
            _ => return Err(SysinfoError::InvalidArgument),
        }
    }
    Ok(options)
}

/// Merge clustered words (each word contributes its letters).
pub fn merge_clock_options(words: &[&str]) -> Result<ClockOptions, SysinfoError> {
    let mut merged = ClockOptions::default();
    if words.is_empty() {
        return Ok(merged);
    }
    for word in words {
        let parsed = parse_clock_options(word)?;
        merged.preview |= parsed.preview;
        merged.write_hardware |= parsed.write_hardware;
        merged.register_access |= parsed.register_access;
        merged.year_2000_workaround |= parsed.year_2000_workaround;
        merged.quiet |= parsed.quiet;
    }
    Ok(merged)
}

/// Transfer direction selected by the options.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClockDirection {
    /// Read the hardware clock into the system clock.
    HardwareToSystem,
    /// Write the system clock into the hardware clock.
    SystemToHardware,
}

/// Direction decision: writing flags select system to hardware.
pub fn clock_direction(options: ClockOptions) -> ClockDirection {
    if options.write_hardware {
        ClockDirection::SystemToHardware
    } else {
        ClockDirection::HardwareToSystem
    }
}

/// Largest number of hardware reads before giving up.
pub const MAX_CLOCK_READS: u32 = 10;

/// True when another read is allowed (`attempts` counts completed reads).
pub fn should_retry_read(attempts: u32, valid: bool) -> bool {
    !valid && attempts < MAX_CLOCK_READS
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_letters_parse() {
        let options = parse_clock_options("nqw").unwrap();
        assert!(options.preview);
        assert!(options.quiet);
        assert!(options.write_hardware);
        assert!(!options.register_access);
    }

    #[test]
    fn test_uppercase_implies_write() {
        let options = parse_clock_options("W").unwrap();
        assert!(options.register_access);
        assert!(options.write_hardware);
    }

    #[test]
    fn test_unknown_letter_rejected() {
        assert_eq!(
            parse_clock_options("x"),
            Err(SysinfoError::InvalidArgument)
        );
        assert_eq!(parse_clock_options(""), Err(SysinfoError::InvalidArgument));
    }

    #[test]
    fn test_words_merge() {
        let options = merge_clock_options(&["n", "2q"]).unwrap();
        assert!(options.preview);
        assert!(options.year_2000_workaround);
        assert!(options.quiet);
    }

    #[test]
    fn test_direction_follows_write_flag() {
        assert_eq!(
            clock_direction(ClockOptions::default()),
            ClockDirection::HardwareToSystem
        );
        let mut options = ClockOptions::default();
        options.write_hardware = true;
        assert_eq!(
            clock_direction(options),
            ClockDirection::SystemToHardware
        );
    }

    #[test]
    fn test_retry_policy() {
        assert!(should_retry_read(0, false));
        assert!(should_retry_read(9, false));
        assert!(!should_retry_read(10, false));
        assert!(!should_retry_read(3, true));
    }
}
