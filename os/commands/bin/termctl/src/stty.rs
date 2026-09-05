//! `stty` argument parsing into operations.
//!
//! Ground truth: `minix3/bin/stty/stty.c` (operand handling around line
//! 137), flag tables in `modes.c` (`cmodes`, `imodes`, `lmodes`, `omodes`
//! with their `specialmodes` companions at lines 65 to 175; `echo`/`ECHO`
//! at line 121), and the search order in `modeset` (line 208: control,
//! input, local, output). An operand is one of:
//!
//! - A speed (`9600`, `115200`, ...): sets both directions.
//! - A control character assignment (`intr ^C`, `erase undef`, ...): the
//!   name must be known, the value parsed by caret rules.
//! - A flag word (`echo`, `-echo`, `icanon`, ...): set or clear one flag.
//!
//! Anything else is an error. Applying operations to a real terminal stays
//! with the execution layer; the flag identities below (which group each
//! name belongs to) mirror the four C tables.

use crate::TermError;
use crate::baud::parse_speed;
use crate::cchar::{lookup, parse_value};

/// Which flag group a name belongs to (mirroring the four C tables).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FlagGroup {
    /// Control modes (`cmodes`).
    Control,
    /// Input modes (`imodes`).
    Input,
    /// Local modes (`lmodes`).
    Local,
    /// Output modes (`omodes`).
    Output,
}

/// One parsed `stty` operand.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SttyOp<'a> {
    /// Set both directions to `baud`.
    Speed(u32),
    /// Assign control character `slot` the value `value`.
    ControlChar(u8, u8),
    /// Set (`true`) or clear (`false`) the named flag in its group.
    Flag(FlagGroup, &'a str, bool),
}

/// (Name, group) pairs for the flag words this module recognises: the
/// common subset across the four C tables (flow control, canonical mode,
/// echo and its variants, signal and extension switches, output
/// postprocessing). The full tables hold hundreds of entries (baud
/// variants, character sizes, exotic switches); the execution layer owns
/// the complete generated table, this core covers the words scripts
/// actually use.
pub const FLAGS: [(&str, FlagGroup); 16] = [
    ("parenb", FlagGroup::Control),
    ("parodd", FlagGroup::Control),
    ("cs8", FlagGroup::Control),
    ("hupcl", FlagGroup::Control),
    ("istrip", FlagGroup::Input),
    ("ixon", FlagGroup::Input),
    ("ixoff", FlagGroup::Input),
    ("icrnl", FlagGroup::Input),
    ("icanon", FlagGroup::Local),
    ("echo", FlagGroup::Local),
    ("echoe", FlagGroup::Local),
    ("echok", FlagGroup::Local),
    ("isig", FlagGroup::Local),
    ("iexten", FlagGroup::Local),
    ("opost", FlagGroup::Output),
    ("onlcr", FlagGroup::Output),
];

/// Parse `stty` operands into operations.
///
/// `args` are the words after the program name. A word is tried as a speed
/// first (all digits), then as `name value` for control characters (the
/// name must be known), then as a flag word with optional leading dash.
/// Unknown words are an error, matching the C tool refusing the whole
/// command line.
pub fn parse_args<'a>(args: &[&'a str]) -> Result<([SttyOp<'a>; 16], usize), TermError> {
    let mut ops: [SttyOp<'a>; 16] = [SttyOp::Speed(9600); 16];
    let mut count = 0;
    let mut index = 0;
    let push = |ops: &mut [SttyOp<'a>; 16],
                    count: &mut usize,
                    op: SttyOp<'a>|
     -> Result<(), TermError> {
        if *count >= ops.len() {
            return Err(TermError::InvalidArgument);
        }
        ops[*count] = op;
        *count += 1;
        Ok(())
    };
    while index < args.len() {
        let word = args[index];
        if !word.is_empty() && word.bytes().all(|b| b.is_ascii_digit()) {
            push(&mut ops, &mut count, SttyOp::Speed(parse_speed(word)?))?;
            index += 1;
            continue;
        }
        if lookup(word).is_ok() {
            index += 1;
            let value = args.get(index).ok_or(TermError::InvalidArgument)?;
            let (slot, _) = lookup(word).map_err(|_| TermError::InvalidArgument)?;
            push(
                &mut ops,
                &mut count,
                SttyOp::ControlChar(slot, parse_value(value)?),
            )?;
            index += 1;
            continue;
        }
        let (negated, name) = match word.strip_prefix('-') {
            Some(rest) => (true, rest),
            None => (false, word),
        };
        match FLAGS.iter().find(|(known, _)| *known == name) {
            Some((_, group)) => {
                push(&mut ops, &mut count, SttyOp::Flag(*group, name, !negated))?;
                index += 1;
            }
            None => return Err(TermError::InvalidArgument),
        }
    }
    Ok((ops, count))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cchar::DISABLED;

    #[test]
    fn test_speed_operand() {
        let (ops, count) = parse_args(&["9600"]).unwrap();
        assert_eq!(count, 1);
        assert_eq!(ops[0], SttyOp::Speed(9600));
    }

    #[test]
    fn test_control_char_assignment() {
        let (ops, count) = parse_args(&["intr", "^C"]).unwrap();
        assert_eq!(count, 1);
        assert_eq!(ops[0], SttyOp::ControlChar(8, 0x03));
        let (ops, _) = parse_args(&["erase", "undef"]).unwrap();
        assert_eq!(ops[0], SttyOp::ControlChar(3, DISABLED));
    }

    #[test]
    fn test_flag_set_and_clear() {
        let (ops, count) = parse_args(&["echo", "-icanon"]).unwrap();
        assert_eq!(count, 2);
        assert_eq!(ops[0], SttyOp::Flag(FlagGroup::Local, "echo", true));
        assert_eq!(ops[1], SttyOp::Flag(FlagGroup::Local, "icanon", false));
    }

    #[test]
    fn test_mixed_command_line() {
        let (ops, count) = parse_args(&["9600", "intr", "^C", "-echo"]).unwrap();
        assert_eq!(count, 3);
        assert_eq!(ops[0], SttyOp::Speed(9600));
    }

    #[test]
    fn test_unknown_words_rejected() {
        assert_eq!(parse_args(&["bogus"]), Err(TermError::InvalidArgument));
        assert_eq!(parse_args(&["intr"]), Err(TermError::InvalidArgument));
        assert_eq!(parse_args(&["-bogus"]), Err(TermError::InvalidArgument));
    }
}
