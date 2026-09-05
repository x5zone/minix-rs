//! Log rotation and difference-list repair vocabulary.
//!
//! Ground truth: `minix3/minix/commands/rotate/rotate.sh` (two arguments: the
//! log path and the generation count to keep; generations are `log.1.bz2`
//! through `log.{keep}.bz2`; the oldest generation is dropped, middle
//! generations shift up by one, the current log is compressed into generation
//! one, the current log is truncated) and `minix3/minix/commands/fix/fix.c`
//! (input lines hold at most `LINELEN 1024` characters, chunks are append,
//! delete, or change hunks with `a`, `d`, `c` command letters, verified
//! against the original file before applying).

use crate::MaintError;

/// One planned file move inside a rotation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RotationMove {
    /// Generation being moved (1 means the current log, higher means older).
    pub from_generation: u64,
    /// Destination generation (always one higher than the source).
    pub to_generation: u64,
}

/// Plan a log rotation without touching the file system.
///
/// `keep` is the generation count to keep (must be at least one). The plan
/// holds `keep minus one` moves: generation `keep minus one` moves to `keep`,
/// down to generation one moving to two. Dropping the oldest generation,
/// compressing the current log into generation one, and truncating the current
/// log are implied by the plan and stay with the execution layer. The plan is
/// written into `out` (each slot one move, oldest first) and the move count
/// is returned.
pub fn plan_rotation(keep: u64, out: &mut [RotationMove]) -> Result<usize, MaintError> {
    if keep == 0 {
        return Err(MaintError::InvalidArgument);
    }
    let moves = (keep as usize).saturating_sub(1);
    if out.len() < moves {
        return Err(MaintError::InvalidArgument);
    }
    for (index, slot) in out.iter_mut().enumerate().take(moves) {
        let from = keep - 1 - index as u64;
        *slot = RotationMove {
            from_generation: from,
            to_generation: from + 1,
        };
    }
    Ok(moves)
}

/// Difference-list chunk command.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FixCommand {
    /// Append lines after the original line.
    Append,
    /// Delete original lines.
    Delete,
    /// Replace original lines with new lines.
    Change,
}

/// Parse one difference-list command letter (`a`, `d`, `c` from `fix.c`).
pub fn parse_fix_command(letter: u8) -> Result<FixCommand, MaintError> {
    match letter {
        b'a' => Ok(FixCommand::Append),
        b'd' => Ok(FixCommand::Delete),
        b'c' => Ok(FixCommand::Change),
        _ => Err(MaintError::InvalidArgument),
    }
}

/// Maximum input line length accepted by the repair tool.
pub const FIX_LINE_LIMIT: usize = 1024;

/// Check that a repaired line fits the tool limit.
pub fn check_fix_line_length(length: usize) -> Result<(), MaintError> {
    if length > FIX_LINE_LIMIT {
        return Err(MaintError::InvalidArgument);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_single_generation_has_no_moves() {
        let mut out = [RotationMove { from_generation: 0, to_generation: 0 }; 4];
        assert_eq!(plan_rotation(1, &mut out), Ok(0));
    }

    #[test]
    fn test_three_generations_shift_twice() {
        let mut out = [RotationMove { from_generation: 0, to_generation: 0 }; 4];
        let count = plan_rotation(3, &mut out).unwrap();
        assert_eq!(count, 2);
        assert_eq!(
            out[0],
            RotationMove { from_generation: 2, to_generation: 3 }
        );
        assert_eq!(
            out[1],
            RotationMove { from_generation: 1, to_generation: 2 }
        );
    }

    #[test]
    fn test_zero_keep_rejected() {
        let mut out = [RotationMove { from_generation: 0, to_generation: 0 }; 4];
        assert_eq!(plan_rotation(0, &mut out), Err(MaintError::InvalidArgument));
    }

    #[test]
    fn test_small_buffer_rejected() {
        let mut out = [RotationMove { from_generation: 0, to_generation: 0 }; 1];
        assert_eq!(plan_rotation(4, &mut out), Err(MaintError::InvalidArgument));
    }

    #[test]
    fn test_fix_letters_parse() {
        assert_eq!(parse_fix_command(b'a'), Ok(FixCommand::Append));
        assert_eq!(parse_fix_command(b'd'), Ok(FixCommand::Delete));
        assert_eq!(parse_fix_command(b'c'), Ok(FixCommand::Change));
        assert_eq!(parse_fix_command(b'x'), Err(MaintError::InvalidArgument));
    }

    #[test]
    fn test_line_limit_enforced() {
        assert_eq!(check_fix_line_length(1024), Ok(()));
        assert_eq!(
            check_fix_line_length(1025),
            Err(MaintError::InvalidArgument)
        );
    }
}
