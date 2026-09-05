//! Magnetic tape control vocabulary.
//!
//! Ground truth: `minix3/minix/commands/mt/mt.c`. The `tape_operation_t`
//! table pairs each command word with an input and output control operation
//! and a count rule: `SELF` commands are interpreted inside the tool (only
//! `status`), `IGN` commands ignore the count field, `NNG` commands accept a
//! zero or positive count, `POS` commands require a strictly positive count.
//! Device status decodes to `DS_OK`, `DS_ERR`, or `DS_EOF`, and the sense key
//! table decodes the drive sense byte. The command line shape is
//! `mt [-f device] command [count]`, and the `TAPE` environment variable
//! supplies the default device.

use crate::MaintError;

/// How a tape command treats its count field.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CountRule {
    /// The count field is ignored (rewind, offline, end of media, status,
    /// retension, erase).
    Ignored,
    /// The count must be zero or positive (backward space file, density,
    /// block size selection).
    ZeroOrPositive,
    /// The count must be strictly positive (marks, forward spacing).
    Positive,
}

/// One magnetic tape command.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TapeCommand {
    /// Command word as typed (`eof`, `fsf`, `rewind`, and so on).
    pub name: &'static str,
    /// Count rule for this command.
    pub rule: CountRule,
    /// True for the interpreted `status` command (no drive operation).
    pub interpreted: bool,
}

/// Full command table in `mt.c` order.
pub const TAPE_COMMANDS: &[TapeCommand] = &[
    TapeCommand { name: "eof", rule: CountRule::Positive, interpreted: false },
    TapeCommand { name: "weof", rule: CountRule::Positive, interpreted: false },
    TapeCommand { name: "fsf", rule: CountRule::Positive, interpreted: false },
    TapeCommand { name: "fsr", rule: CountRule::Positive, interpreted: false },
    TapeCommand { name: "bsf", rule: CountRule::ZeroOrPositive, interpreted: false },
    TapeCommand { name: "bsr", rule: CountRule::Positive, interpreted: false },
    TapeCommand { name: "eom", rule: CountRule::Ignored, interpreted: false },
    TapeCommand { name: "rewind", rule: CountRule::Ignored, interpreted: false },
    TapeCommand { name: "offline", rule: CountRule::Ignored, interpreted: false },
    TapeCommand { name: "rewoffl", rule: CountRule::Ignored, interpreted: false },
    TapeCommand { name: "status", rule: CountRule::Ignored, interpreted: true },
    TapeCommand { name: "retension", rule: CountRule::Ignored, interpreted: false },
    TapeCommand { name: "erase", rule: CountRule::Ignored, interpreted: false },
    TapeCommand { name: "density", rule: CountRule::ZeroOrPositive, interpreted: false },
    TapeCommand { name: "blksize", rule: CountRule::ZeroOrPositive, interpreted: false },
    TapeCommand { name: "blocksize", rule: CountRule::ZeroOrPositive, interpreted: false },
];

/// Look up a command word by exact match.
pub fn parse_tape_command(word: &str) -> Result<TapeCommand, MaintError> {
    TAPE_COMMANDS
        .iter()
        .find(|command| command.name == word)
        .copied()
        .ok_or(MaintError::InvalidArgument)
}

/// Check a count against the command rule (`None` means no count was given).
pub fn check_tape_count(command: TapeCommand, count: Option<i64>) -> Result<i64, MaintError> {
    match command.rule {
        CountRule::Ignored => Ok(count.unwrap_or(1)),
        CountRule::ZeroOrPositive => match count {
            None => Ok(1),
            Some(value) if value >= 0 => Ok(value),
            Some(_) => Err(MaintError::InvalidArgument),
        },
        CountRule::Positive => match count {
            Some(value) if value > 0 => Ok(value),
            _ => Err(MaintError::InvalidArgument),
        },
    }
}

/// Device status reported by the drive.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeviceStatus {
    /// Drive ready.
    Ready,
    /// Drive error.
    Error,
    /// End of file mark reached.
    EndOfFile,
}

/// Decode the numeric device state (`DS_OK`, `DS_ERR`, `DS_EOF` in `mt.c`).
pub fn decode_device_status(value: u32) -> Result<DeviceStatus, MaintError> {
    match value {
        0 => Ok(DeviceStatus::Ready),
        1 => Ok(DeviceStatus::Error),
        2 => Ok(DeviceStatus::EndOfFile),
        _ => Err(MaintError::DeviceError),
    }
}

/// Decode the drive sense key (low four bits of the sense byte).
pub fn decode_sense_key(sense: u8) -> &'static str {
    match sense & 0x0F {
        0x00 => "no sense information",
        0x01 => "recovered error",
        0x02 => "not ready",
        0x03 => "medium error",
        0x04 => "hardware error",
        0x05 => "illegal request",
        0x06 => "unit attention",
        0x07 => "data protect",
        0x08 => "blank check",
        0x0B => "aborted command",
        0x0D => "volume overflow",
        0x0E => "miscompare",
        _ => "sense reserved",
    }
}

/// Tape drive behind the command words.
///
/// Motion control stays with the execution layer (input and output control
/// calls); this trait exposes only position and status so the pure table can
/// be tested without a drive.
pub trait TapeBackend {
    /// Current logical file number.
    fn file_number(&self) -> u64;
    /// Current device status.
    fn status(&self) -> DeviceStatus;
    /// Move forward over `files` end-of-file marks.
    fn forward_files(&mut self, files: u64) -> Result<(), MaintError>;
    /// Rewind to the start of the medium.
    fn rewind(&mut self) -> Result<(), MaintError>;
}

/// In-memory tape (position only, always ready).
pub struct MemoryTape {
    files: u64,
}

impl MemoryTape {
    /// A tape positioned at the start.
    pub fn new() -> Self {
        MemoryTape { files: 0 }
    }
}

impl Default for MemoryTape {
    fn default() -> Self {
        Self::new()
    }
}

impl TapeBackend for MemoryTape {
    fn file_number(&self) -> u64 {
        self.files
    }

    fn status(&self) -> DeviceStatus {
        DeviceStatus::Ready
    }

    fn forward_files(&mut self, files: u64) -> Result<(), MaintError> {
        self.files = self
            .files
            .checked_add(files)
            .ok_or(MaintError::InvalidArgument)?;
        Ok(())
    }

    fn rewind(&mut self) -> Result<(), MaintError> {
        self.files = 0;
        Ok(())
    }
}

/// Null tape (no medium loaded, every motion fails).
pub struct NullTape;

impl TapeBackend for NullTape {
    fn file_number(&self) -> u64 {
        0
    }

    fn status(&self) -> DeviceStatus {
        DeviceStatus::Error
    }

    fn forward_files(&mut self, _files: u64) -> Result<(), MaintError> {
        Err(MaintError::DeviceError)
    }

    fn rewind(&mut self) -> Result<(), MaintError> {
        Err(MaintError::DeviceError)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_table_holds_sixteen_words() {
        assert_eq!(TAPE_COMMANDS.len(), 16);
    }

    #[test]
    fn test_aliases_share_rules() {
        assert_eq!(
            parse_tape_command("eof").unwrap().rule,
            parse_tape_command("weof").unwrap().rule
        );
        assert_eq!(
            parse_tape_command("blksize").unwrap().rule,
            parse_tape_command("blocksize").unwrap().rule
        );
        assert_eq!(
            parse_tape_command("offline").unwrap().rule,
            parse_tape_command("rewoffl").unwrap().rule
        );
    }

    #[test]
    fn test_unknown_command_rejected() {
        assert_eq!(
            parse_tape_command("fastforward"),
            Err(MaintError::InvalidArgument)
        );
    }

    #[test]
    fn test_positive_requires_count() {
        let command = parse_tape_command("fsf").unwrap();
        assert_eq!(check_tape_count(command, Some(2)), Ok(2));
        assert_eq!(
            check_tape_count(command, Some(0)),
            Err(MaintError::InvalidArgument)
        );
        assert_eq!(
            check_tape_count(command, None),
            Err(MaintError::InvalidArgument)
        );
    }

    #[test]
    fn test_zero_or_positive_accepts_zero() {
        let command = parse_tape_command("bsf").unwrap();
        assert_eq!(check_tape_count(command, Some(0)), Ok(0));
        assert_eq!(check_tape_count(command, None), Ok(1));
        assert_eq!(
            check_tape_count(command, Some(-1)),
            Err(MaintError::InvalidArgument)
        );
    }

    #[test]
    fn test_ignored_keeps_default() {
        let command = parse_tape_command("rewind").unwrap();
        assert_eq!(check_tape_count(command, None), Ok(1));
        assert_eq!(check_tape_count(command, Some(99)), Ok(99));
    }

    #[test]
    fn test_status_is_interpreted() {
        assert!(parse_tape_command("status").unwrap().interpreted);
        assert!(!parse_tape_command("rewind").unwrap().interpreted);
    }

    #[test]
    fn test_status_decodes() {
        assert_eq!(decode_device_status(0), Ok(DeviceStatus::Ready));
        assert_eq!(decode_device_status(1), Ok(DeviceStatus::Error));
        assert_eq!(decode_device_status(2), Ok(DeviceStatus::EndOfFile));
        assert_eq!(
            decode_device_status(9),
            Err(MaintError::DeviceError)
        );
    }

    #[test]
    fn test_sense_keys_decode() {
        assert_eq!(decode_sense_key(0x03), "medium error");
        assert_eq!(decode_sense_key(0x05), "illegal request");
    }

    #[test]
    fn test_memory_tape_moves() {
        let mut tape = MemoryTape::new();
        tape.forward_files(3).unwrap();
        assert_eq!(tape.file_number(), 3);
        tape.rewind().unwrap();
        assert_eq!(tape.file_number(), 0);
        assert_eq!(tape.status(), DeviceStatus::Ready);
    }

    #[test]
    fn test_null_tape_fails() {
        let mut tape = NullTape;
        assert_eq!(tape.status(), DeviceStatus::Error);
        assert_eq!(
            tape.forward_files(1),
            Err(MaintError::DeviceError)
        );
    }
}
