//! Terminal lines: minor decoding, console redirect, and line mapping.
//!
//! C correspondence: `line2tty` in `minix3/minix/drivers/tty/tty/tty.c:264
//! -292`, the minor defines in `minix3/minix/drivers/tty/tty/tty.h:7-9`
//! and `minix3/minix/include/minix/dmap.h:95`, and the line counts in
//! `minix3/minix/include/minix/config.h:44-45`.

/// First minor of the console range.
///
/// C: `CONS_MINOR 0` (`dmap.h:95`).
pub const CONSOLE_MINOR: u32 = 0;

/// Minor of the log device, redirected to the console line.
///
/// C: `LOG_MINOR 15` (`tty.h:8`).
pub const LOG_MINOR: u32 = 15;

/// First minor of the serial range.
///
/// C: `RS232_MINOR 16` (`tty.h:9`).
pub const SERIAL_MINOR: u32 = 16;

/// Minor of the video device, served by a separate branch.
///
/// C: `VIDEO_MINOR 125` (`tty.h:10`).
pub const VIDEO_MINOR: u32 = 125;

/// Number of system consoles.
///
/// C: `NR_CONS 4` (`config.h:44`).
pub const CONSOLE_COUNT: u32 = 4;

/// Number of serial lines.
///
/// C: `NR_RS_LINES 4` (`config.h:45`).
pub const SERIAL_COUNT: u32 = 4;

/// One addressable terminal line.
///
/// C: the branches of `line2tty`: console range, serial range, video, or
/// nothing. The log minor never survives decoding: it is rewritten to the
/// configured console line first (`tty.c:270-271`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LineId {
    /// System console by index (zero-based).
    Console(u32),
    /// Serial line by index (zero-based).
    Serial(u32),
    /// Video device (served outside the line table).
    Video,
}

impl LineId {
    /// Decode a raw minor into a line.
    ///
    /// `console_line` is the configured console (redirection target of the
    /// console alias and the log device). Inactive lines are refused by
    /// the caller with the active set; decoding itself is total over the
    /// number space.
    pub const fn decode(minor: u32, console_line: u32) -> Option<LineId> {
        let minor = if minor == CONSOLE_MINOR || minor == LOG_MINOR {
            console_line
        } else {
            minor
        };
        if minor == VIDEO_MINOR {
            return Some(LineId::Video);
        }
        // Console range starts at zero, so the low bound needs no check;
        // the subtraction stays exact for every minor below the top.
        if minor.wrapping_sub(CONSOLE_MINOR) < CONSOLE_COUNT {
            return Some(LineId::Console(minor - CONSOLE_MINOR));
        }
        if minor >= SERIAL_MINOR && minor < SERIAL_MINOR + SERIAL_COUNT {
            return Some(LineId::Serial(minor - SERIAL_MINOR));
        }
        None
    }

    /// Table slot of this line; video has no slot.
    ///
    /// C: `tty_addr(line - CONS_MINOR)` and `tty_addr(line - RS232_MINOR +
    /// NR_CONS)` (`tty.c:279-282`): consoles occupy the low slots,
    /// serials follow.
    pub const fn slot(self) -> Option<usize> {
        match self {
            LineId::Console(index) => Some(index as usize),
            LineId::Serial(index) => Some((CONSOLE_COUNT + index) as usize),
            LineId::Video => None,
        }
    }

    /// True for the log alias (write-only diagnostics convention).
    ///
    /// C: `do_open` refuses read access on the log device when it lands on
    /// the console (`tty.c:734-737`).
    pub const fn is_log_alias(minor: u32) -> bool {
        minor == LOG_MINOR
    }
}

/// Number of line-table slots (consoles plus serials).
pub const LINE_SLOTS: usize = (CONSOLE_COUNT + SERIAL_COUNT) as usize;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_console_range_decodes_to_slots() {
        assert_eq!(LineId::decode(0, 0), Some(LineId::Console(0)));
        assert_eq!(LineId::decode(3, 0), Some(LineId::Console(3)));
        assert_eq!(LineId::decode(0, 0).unwrap().slot(), Some(0));
        assert_eq!(LineId::decode(3, 0).unwrap().slot(), Some(3));
    }

    #[test]
    fn test_log_alias_redirects_to_console_line() {
        assert_eq!(LineId::decode(15, 2), Some(LineId::Console(2)));
        assert!(LineId::is_log_alias(15));
        assert!(!LineId::is_log_alias(0));
    }

    #[test]
    fn test_serial_range_follows_consoles() {
        assert_eq!(LineId::decode(16, 0), Some(LineId::Serial(0)));
        assert_eq!(LineId::decode(19, 0), Some(LineId::Serial(3)));
        assert_eq!(LineId::decode(16, 0).unwrap().slot(), Some(4));
        assert_eq!(LineId::decode(20, 0), None);
    }

    #[test]
    fn test_video_has_no_slot_and_unknown_is_refused() {
        assert_eq!(LineId::decode(125, 0), Some(LineId::Video));
        assert_eq!(LineId::decode(125, 0).unwrap().slot(), None);
        assert_eq!(LineId::decode(100, 0), None);
    }

    #[test]
    fn test_constants_match_c_headers() {
        assert_eq!(CONSOLE_COUNT, 4);
        assert_eq!(SERIAL_COUNT, 4);
        assert_eq!(LINE_SLOTS, 8);
        assert_eq!(LOG_MINOR, 15);
        assert_eq!(VIDEO_MINOR, 125);
    }
}
