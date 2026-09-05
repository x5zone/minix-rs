//! `ed` command letter parsing.
//!
//! Ground truth: `minix3/bin/ed/main.c` (`exec_command` at line 465, command
//! cases from line 481: append 481, delete 496, print 650, quit 666,
//! substitute 698, write 803, plus mark handling at line 618). A command is
//! an address range (handled by [`crate::addr`]) followed by one letter,
//! optionally followed by `!` (force), `p`/`l`/`n` (print variants), or a
//! parameter (file name for `e`/`r`/`w`, text for `s`). This module parses
//! the letter and its modifiers; command *execution* stays with the driver.

use crate::EditorError;

/// What an `ed` command letter asks for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    /// `a`: append text after the addressed lines.
    Append,
    /// `c`: change (replace) the addressed lines.
    Change,
    /// `d`: delete the addressed lines.
    Delete,
    /// `i`: insert text before the addressed lines.
    Insert,
    /// `p`: print the addressed lines.
    Print,
    /// `l`: print with visible control characters.
    List,
    /// `n`: print with line numbers.
    Number,
    /// `s`: substitute within the addressed lines.
    Substitute,
    /// `w`: write the addressed lines to the file.
    Write,
    /// `q`: quit (refuses unsaved changes without `!`).
    Quit,
    /// `e`: edit a new file (discards the buffer).
    Edit,
    /// `r`: read a file after the addressed line.
    Read,
    /// `k`: mark the addressed line with a letter.
    Mark,
    /// `m`: move the addressed lines after the target.
    Move,
    /// `t`: copy the addressed lines after the target.
    Transfer,
    /// `j`: join the addressed lines into one.
    Join,
    /// `g` / `v`: global (or inverse global) command over a subcommand.
    Global,
    /// `u`: undo the last change.
    Undo,
    /// `=`: print the addressed line number.
    LineNumber,
}

/// Modifiers trailing a command letter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Modifiers {
    /// `!`: force (quit or write despite warnings).
    pub force: bool,
    /// `p`: print after the command.
    pub print: bool,
    /// `l`: list after the command.
    pub list: bool,
    /// `n`: enumerate after the command.
    pub number: bool,
}

/// Parse the command letter at `text[pos]` plus its `!pln` modifiers.
///
/// Returns the command, its modifiers, and the position past them (ready
/// for the command's parameter, if any). Unknown letters are an error: the
/// C editor prints `?` for all of these through one channel.
pub fn parse_command(text: &str, pos: usize) -> Result<(Command, Modifiers, usize), EditorError> {
    let bytes = text.as_bytes();
    let letter = *bytes.get(pos).ok_or(EditorError::InvalidArgument)?;
    let command = match letter {
        b'a' => Command::Append,
        b'c' => Command::Change,
        b'd' => Command::Delete,
        b'i' => Command::Insert,
        b'p' => Command::Print,
        b'l' => Command::List,
        b'n' => Command::Number,
        b's' => Command::Substitute,
        b'w' => Command::Write,
        b'q' => Command::Quit,
        b'e' => Command::Edit,
        b'r' => Command::Read,
        b'k' => Command::Mark,
        b'm' => Command::Move,
        b't' => Command::Transfer,
        b'j' => Command::Join,
        b'g' | b'v' => Command::Global,
        b'u' => Command::Undo,
        b'=' => Command::LineNumber,
        _ => return Err(EditorError::InvalidArgument),
    };
    let mut modifiers = Modifiers::default();
    let mut cursor = pos + 1;
    while cursor < bytes.len() {
        match bytes[cursor] {
            b'!' => modifiers.force = true,
            b'p' => modifiers.print = true,
            b'l' => modifiers.list = true,
            b'n' => modifiers.number = true,
            _ => break,
        }
        cursor += 1;
    }
    Ok((command, modifiers, cursor))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_letters_parse() {
        let (command, modifiers, used) = parse_command("d", 0).unwrap();
        assert_eq!(command, Command::Delete);
        assert_eq!(modifiers, Modifiers::default());
        assert_eq!(used, 1);
    }

    #[test]
    fn test_no_wq_command() {
        // `ed` has no `wq` command: `w` parses, and the trailing `q`
        // starts the next command (used == 1 proves the split).
        let (command, _, used) = parse_command("wq", 0).unwrap();
        assert_eq!(command, Command::Write);
        assert_eq!(used, 1);
    }

    #[test]
    fn test_modifiers_parse() {
        let (command, modifiers, used) = parse_command("w!", 0).unwrap();
        assert_eq!(command, Command::Write);
        assert!(modifiers.force);
        assert_eq!(used, 2);
        let (_, modifiers, used) = parse_command("dpn", 0).unwrap();
        assert!(modifiers.print && modifiers.number);
        assert_eq!(used, 3);
    }

    #[test]
    fn test_global_both_spellings() {
        assert_eq!(parse_command("g", 0).unwrap().0, Command::Global);
        assert_eq!(parse_command("v", 0).unwrap().0, Command::Global);
    }

    #[test]
    fn test_unknown_letter_rejected() {
        assert_eq!(
            parse_command("x", 0).map(|_| ()),
            Err(EditorError::InvalidArgument)
        );
        assert_eq!(
            parse_command("", 0).map(|_| ()),
            Err(EditorError::InvalidArgument)
        );
    }

    #[test]
    fn test_parse_at_offset() {
        // The caller consumes the address range first (`1,5` is three
        // bytes), then parsing starts at the command letter.
        let (command, _, used) = parse_command("1,5d", 3).unwrap();
        assert_eq!(command, Command::Delete);
        assert_eq!(used, 4);
    }
}
