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
    /// `G` / `V`: interactive global (asks per line); execution needs the
    /// input loop, so the executor reports it as unwired.
    GlobalInteractive,
    /// `u`: undo the last change.
    Undo,
    /// `=`: print the addressed line number.
    LineNumber,
    /// `h`: print the last error message (`main.c:575`).
    Help,
    /// `H`: toggle error explanations on every error (`main.c:583`).
    HelpMode,
    /// `f`: print or set the default file name (`main.c:541`).
    Filename,
    /// `P`: toggle the command prompt (`main.c:668`, `prompt ? NULL : dps`).
    PromptToggle,
    /// `E`: edit unconditionally, discarding changes (`main.c:510`).
    EditForce,
    /// `W`: append the addressed lines to the file (`main.c:803`).
    WriteAppend,
    /// `x`: DES encryption key (`main.c:837`); without DES the C editor
    /// errors with "crypt unavailable", and so does the executor.
    Crypt,
    /// `z`: scroll through the buffer (`main.c:848`).
    Scroll,
    /// `!`: run a shell line (`main.c:868`).
    Shell,
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
    /// `q` / `Q` glued onto `w` (`wq`): quit after writing. The C editor
    /// reads the letter right after `w` inside the write case
    /// (`main.c:804-807`), so this is one command, not two.
    pub quit_after: bool,
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
        b'G' | b'V' => Command::GlobalInteractive,
        b'u' => Command::Undo,
        b'=' => Command::LineNumber,
        b'h' => Command::Help,
        b'H' => Command::HelpMode,
        b'f' => Command::Filename,
        b'P' => Command::PromptToggle,
        b'E' => Command::EditForce,
        b'W' => Command::WriteAppend,
        b'x' => Command::Crypt,
        b'z' => Command::Scroll,
        b'!' => Command::Shell,
        _ => return Err(EditorError::InvalidArgument),
    };
    let mut modifiers = Modifiers::default();
    let mut cursor = pos + 1;
    // `wq`/`wQ` is one command in C: the write case reads the glued letter
    // and turns it into quit-after-write (`main.c:804-807`).
    if command == Command::Write && matches!(bytes.get(cursor), Some(b'q') | Some(b'Q')) {
        modifiers.quit_after = true;
        cursor += 1;
    }
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
    fn test_wq_glues_quit_onto_write() {
        // `wq` is ONE command in C: the write case reads the glued letter
        // and turns it into quit-after-write (`main.c:804-807`), so the
        // parse reports Write plus the quit flag, not two commands.
        let (command, modifiers, used) = parse_command("wq", 0).unwrap();
        assert_eq!(command, Command::Write);
        assert!(modifiers.quit_after);
        assert_eq!(used, 2);
        // A glued letter that is not `q`/`Q` stays unconsumed (`wx` is
        // `w` followed by the file name `x`).
        let (command, modifiers, used) = parse_command("wx", 0).unwrap();
        assert_eq!(command, Command::Write);
        assert!(!modifiers.quit_after);
        assert_eq!(used, 1);
    }

    #[test]
    fn test_letters_parse() {
        let (command, modifiers, used) = parse_command("d", 0).unwrap();
        assert_eq!(command, Command::Delete);
        assert_eq!(modifiers, Modifiers::default());
        assert_eq!(used, 1);
    }

    #[test]
    fn test_remaining_c_letters_parse() {
        // The full C letter set (`main.c:481-895`): each dispatch case has
        // a variant, including the ones execution reports as unwired.
        let cases = [
            (b'h', Command::Help),
            (b'H', Command::HelpMode),
            (b'f', Command::Filename),
            (b'P', Command::PromptToggle),
            (b'E', Command::EditForce),
            (b'W', Command::WriteAppend),
            (b'x', Command::Crypt),
            (b'z', Command::Scroll),
            (b'!', Command::Shell),
            (b'G', Command::GlobalInteractive),
            (b'V', Command::GlobalInteractive),
        ];
        for (letter, expected) in cases {
            let text = [letter as char, ' '].iter().collect::<String>();
            let (command, _, used) = parse_command(&text, 0).unwrap();
            assert_eq!(command, expected, "letter {}", letter as char);
            assert_eq!(used, 1);
        }
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
        // `x` parses (crypt, `main.c:837`); genuinely unknown letters do
        // not (`y` has no dispatch case in C either).
        assert_eq!(
            parse_command("y", 0).map(|_| ()),
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
