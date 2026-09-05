//! Line configuration: editing modes, speeds, and control characters.
//!
//! C correspondence: `struct termios` as used through `tty_termios`
//! (`tty.h`), the defaults restored on last close
//! (`termios_defaults`, `tty.c:769-771`), the hangup speed `B0` checked in
//! `select_try` (`tty.c:822-826`), and the control-character indices
//! (`VERASE`, `VKILL`, `VEOF`, `VMIN`, `VTIME`, `VLNEXT`, `VREPRINT`).

/// Output speed meaning "hung up": no operation will ever block.
///
/// C: `B0` (`tty.c:822`): when the output speed is zero, every watched
/// operation reports ready at once.
pub const SPEED_HANGUP: u32 = 0;

/// Input queue capacity in characters.
///
/// C: `TTY_IN_BYTES 256` (`tty.h:12`).
pub const INPUT_QUEUE_SIZE: usize = 256;

/// Output queue capacity referenced by the pseudo-terminal side.
///
/// C: `TTY_OUT_BYTES 2048` (`pty/tty.h:13`).
pub const OUTPUT_QUEUE_SIZE: usize = 2048;

/// Editing and behavior flags of one line (input, output, control, local).
///
/// C: the four flag words `c_iflag`, `c_oflag`, `c_cflag`, `c_lflag` of
/// `struct termios`. Only the flags this driver interprets are modeled;
/// the rest pass through untouched inside the opaque speed and control
/// words.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LineFlags {
    /// Strip input to seven bits (`ISTRIP`).
    pub strip_to_seven_bits: bool,
    /// Map carriage return to newline (`ICRNL`).
    pub map_cr_to_nl: bool,
    /// Ignore carriage return (`IGNCR`).
    pub ignore_cr: bool,
    /// Map newline to carriage return (`INLCR`).
    pub map_nl_to_cr: bool,
    /// Enable extended functions (`IEXTEN`).
    pub extended_functions: bool,
    /// Canonical (line-at-a-time) mode (`ICANON`).
    pub canonical: bool,
    /// Echo input (`ECHO` family, simplified to one bit).
    pub echo: bool,
    /// Echo erase as backspace-space-backspace (`ECHOE`).
    pub echo_erase: bool,
}

impl LineFlags {
    /// Power-on defaults: canonical mode with echo, no mappings.
    ///
    /// C: `termios_defaults` restored on last close (`tty.c:769-771`).
    pub const fn defaults() -> LineFlags {
        LineFlags {
            strip_to_seven_bits: false,
            map_cr_to_nl: true,
            ignore_cr: false,
            map_nl_to_cr: false,
            extended_functions: true,
            canonical: true,
            echo: true,
            echo_erase: true,
        }
    }
}

/// Control characters of one line.
///
/// C: the `c_cc` array (`VERASE`, `VKILL`, `VEOF`, `VEOL`, `VMIN`,
/// `VTIME`, `VLNEXT`, `VREPRINT`, ...). Stored as full code points so the
/// disabled value (traditionally 255) needs no escape hatch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ControlChars {
    /// Erase one character.
    pub erase: u8,
    /// Erase the whole line.
    pub kill: u8,
    /// End of file.
    pub eof: u8,
    /// End of line (alternate).
    pub eol: u8,
    /// Minimum bytes for a non-canonical read.
    pub min: u8,
    /// Timeout in tenths of a second for a non-canonical read.
    pub time: u8,
    /// Quote the next character literally.
    pub literal_next: u8,
    /// Reprint the line.
    pub reprint: u8,
}

impl ControlChars {
    /// Conventional defaults (erase rubout, kill control-U, ...).
    pub const fn defaults() -> ControlChars {
        ControlChars {
            erase: 0x7F,
            kill: 0x15,
            eof: 0x04,
            eol: 0x00,
            min: 1,
            time: 0,
            literal_next: 0x16,
            reprint: 0x12,
        }
    }
}

/// Whole line configuration: flags, controls, and speeds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LineConfig {
    /// Editing and behavior flags.
    pub flags: LineFlags,
    /// Control characters.
    pub controls: ControlChars,
    /// Output speed (zero means hung up).
    pub output_speed: u32,
}

impl LineConfig {
    /// Power-on defaults.
    pub const fn defaults() -> LineConfig {
        LineConfig {
            flags: LineFlags::defaults(),
            controls: ControlChars::defaults(),
            output_speed: 9600,
        }
    }

    /// True when the line is hung up (nothing will ever block).
    pub const fn is_hung_up(self) -> bool {
        self.output_speed == SPEED_HANGUP
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_defaults_are_canonical_with_echo() {
        let config = LineConfig::defaults();
        assert!(config.flags.canonical);
        assert!(config.flags.echo);
        assert!(!config.is_hung_up());
    }

    #[test]
    fn test_hangup_speed_reports_hung_up() {
        let mut config = LineConfig::defaults();
        config.output_speed = SPEED_HANGUP;
        assert!(config.is_hung_up());
    }

    #[test]
    fn test_control_defaults_match_convention() {
        let controls = ControlChars::defaults();
        assert_eq!(controls.erase, 0x7F);
        assert_eq!(controls.eof, 0x04);
        assert_eq!(controls.min, 1);
        assert_eq!(controls.time, 0);
    }

    #[test]
    fn test_queue_sizes_match_c_headers() {
        assert_eq!(INPUT_QUEUE_SIZE, 256);
        assert_eq!(OUTPUT_QUEUE_SIZE, 2048);
    }
}
