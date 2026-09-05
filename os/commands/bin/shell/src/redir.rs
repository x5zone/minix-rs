//! Redirection operator recognition.
//!
//! Ground truth: `minix3/bin/sh/redir.c` (400 lines; node types such as
//! `NTOFD`/`NFROMFD` at line 130, push/pop/undo discipline in the opening
//! comment at lines 99 to 119). The C layer duplicates file descriptors and
//! restores them around commands. This module answers the prior question:
//! which leading word shapes are redirections at all, and what do they ask
//! for? Descriptor juggling stays with the executor.
//!
//! Recognised shapes (optional leading digits are the target descriptor):
//!
//! - `<file`: read standard input from the file.
//! - `>file`, `>|file`: write standard output (the second always overwrites).
//! - `>>file`: append standard output.
//! - `<>file`: open the file for reading and writing.
//! - `>&target`, `<&target`: duplicate a descriptor (`target` is a number
//!   or `-`, meaning close).
//! - `n>file`, `n>>file`, `n<file`, `n>&target`, `n<&target`: same with an
//!   explicit descriptor (`2>` routes error output).

use crate::ShellError;

/// Where a redirection sends or takes bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RedirOp {
    /// `<`: standard input reads the file.
    Read,
    /// `>`: output truncates the file.
    Write,
    /// `>|`: output truncates even with overwrite protection on.
    Clobber,
    /// `>>`: output appends to the file.
    Append,
    /// `<>`: the file opens for reading and writing.
    ReadWrite,
    /// `>&`: duplicate onto the descriptor (or close for `-`).
    DuplicateWrite,
    /// `<&`: duplicate from the descriptor (or close for `-`).
    DuplicateRead,
}

/// One parsed redirection: what, on which descriptor, to where.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Redirection<'a> {
    /// The operation.
    pub op: RedirOp,
    /// Target descriptor (`0` input, `1` output, `2` error output by
    /// default; explicit digits override).
    pub fd: u8,
    /// File path or descriptor number (or `-`) following the operator.
    pub target: &'a str,
}

/// Decide whether `word` is a redirection, parsing it when it is.
///
/// Returns `Ok(None)` for ordinary words (including bare `>` with no
/// target, which the executor rejects later with the filename attached).
/// A redirection without its target word is a syntax error here: the lexer
/// already split words, so `echo >` arrives with nothing after the
/// operator.
pub fn parse_redir(word: &str) -> Result<Option<Redirection<'_>>, ShellError> {
    let bytes = word.as_bytes();
    let mut pos = 0;
    while pos < bytes.len() && bytes[pos].is_ascii_digit() {
        pos += 1;
    }
    let digits = &word[..pos];
    let rest = &word[pos..];
    // Two byte operators first: `>>` must win over `>`, `>&` over `>`,
    // `<>` over `<`, `>|` is its own operator.
    for operator in [">>", ">|", "<>", ">&", "<&", ">", "<"] {
        if let Some(after) = rest.strip_prefix(operator) {
            let op = match operator {
                ">>" => RedirOp::Append,
                ">|" => RedirOp::Clobber,
                "<>" => RedirOp::ReadWrite,
                ">&" => RedirOp::DuplicateWrite,
                "<&" => RedirOp::DuplicateRead,
                ">" => RedirOp::Write,
                "<" => RedirOp::Read,
                _ => continue,
            };
            return finish(digits, op, after, default_fd(op));
        }
    }
    Ok(None)
}

/// Default descriptor per operation: input reads 0, everything else writes
/// through 1 (duplication inherits its side's default).
fn default_fd(op: RedirOp) -> u8 {
    match op {
        RedirOp::Read | RedirOp::DuplicateRead => 0,
        _ => 1,
    }
}

fn finish<'a>(
    digits: &str,
    op: RedirOp,
    after: &'a str,
    default: u8,
) -> Result<Option<Redirection<'a>>, ShellError> {
    if after.is_empty() {
        return Err(ShellError::InvalidSyntax);
    }
    let fd = if digits.is_empty() {
        default
    } else {
        parse_fd(digits)?
    };
    Ok(Some(Redirection {
        op,
        fd,
        target: after,
    }))
}

fn parse_fd(digits: &str) -> Result<u8, ShellError> {
    if digits.len() > 3 {
        return Err(ShellError::InvalidSyntax);
    }
    let mut value: u32 = 0;
    for byte in digits.bytes() {
        value = value * 10 + (byte - b'0') as u32;
    }
    if value > 255 {
        return Err(ShellError::InvalidSyntax);
    }
    Ok(value as u8)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_output_truncate() {
        let redir = parse_redir(">out.txt").unwrap().unwrap();
        assert_eq!(redir.op, RedirOp::Write);
        assert_eq!(redir.fd, 1);
        assert_eq!(redir.target, "out.txt");
    }

    #[test]
    fn test_error_redirect_with_fd() {
        let redir = parse_redir("2>err.txt").unwrap().unwrap();
        assert_eq!(redir.op, RedirOp::Write);
        assert_eq!(redir.fd, 2);
    }

    #[test]
    fn test_append_and_input() {
        assert_eq!(
            parse_redir(">>log").unwrap().unwrap().op,
            RedirOp::Append
        );
        let read = parse_redir("<in.txt").unwrap().unwrap();
        assert_eq!((read.op, read.fd), (RedirOp::Read, 0));
    }

    #[test]
    fn test_duplicate_and_close() {
        let dup = parse_redir("2>&1").unwrap().unwrap();
        assert_eq!((dup.op, dup.fd, dup.target), (RedirOp::DuplicateWrite, 2, "1"));
        let close = parse_redir(">&-").unwrap().unwrap();
        assert_eq!(close.target, "-");
    }

    #[test]
    fn test_read_write_and_clobber() {
        assert_eq!(
            parse_redir("<>db").unwrap().unwrap().op,
            RedirOp::ReadWrite
        );
        assert_eq!(
            parse_redir(">|out").unwrap().unwrap().op,
            RedirOp::Clobber
        );
    }

    #[test]
    fn test_plain_word_is_none() {
        assert_eq!(parse_redir("echo"), Ok(None));
        assert_eq!(parse_redir("file>name"), Ok(None));
    }

    #[test]
    fn test_missing_target_rejected() {
        assert_eq!(parse_redir(">"), Err(ShellError::InvalidSyntax));
        assert_eq!(parse_redir("2>>"), Err(ShellError::InvalidSyntax));
    }
}
