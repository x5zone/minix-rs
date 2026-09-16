//! The `echo` deciding half: flag detection and output layout.
//!
//! Ground truth is `minix3/bin/echo/echo.c` (NetBSD echo.c 1.19): the flag
//! check at lines 61-67 carries the standing rule "this utility may NOT do
//! getopt(3) option parsing" — only a leading `-n` is the flag, every later
//! `-n` is data; the argument loop at lines 69-72 separates arguments with
//! single spaces and never leaves a trailing one; the newline at lines 74-75
//! disappears with `-n`; the exit codes at lines 77-79 are 1 on a write
//! failure and 0 otherwise.
//!
//! The doing half — writing each piece to standard output and exiting —
//! lives in `src/bin/echo.rs`. This split keeps the layout decisions
//! testable without any I/O: the caller hands in an emit sink, the module
//! drives it piece by piece, and a failed write stops the emission the way
//! C's single end-of-stream `ferror` check turns into exit 1.

/// Drives the output for one `echo` invocation.
///
/// `argv` includes the program name in slot 0 (the same convention as
/// init's `parse_boot_args`), so the flag check looks at slot 1 only.
/// The sink receives each argument, each inter-argument space, and the
/// trailing newline as separate byte pieces; it returns `false` when the
/// write failed. On the first `false`, emission stops and [`echo_emit`]
/// returns `false` — the caller answers with exit code 1 (echo.c:77-78).
/// A clean run returns `true` after emitting everything.
///
/// No allocation: the pieces borrow from `argv`, so the sink can write
/// straight from the borrowed bytes.
pub fn echo_emit(argv: &[&str], mut emit: impl FnMut(&[u8]) -> bool) -> bool {
    let (trailing_newline, body) = parse(argv);
    for (index, arg) in body.iter().enumerate() {
        if !emit(arg.as_bytes()) {
            return false;
        }
        if index + 1 < body.len() && !emit(b" ") {
            return false;
        }
    }
    if trailing_newline && !emit(b"\n") {
        return false;
    }
    true
}

/// Splits argv into (trailing newline wanted, arguments to print).
///
/// C: echo.c:61-67. The program name in slot 0 is skipped; exactly one
/// leading `-n` is consumed as the flag and suppresses the newline. There
/// is no `--` end-of-flags marker and no later `-n` recognition — both are
/// ordinary data (`echo -n -n` prints `-n` with no newline).
fn parse<'a, 'b>(argv: &'a [&'b str]) -> (bool, &'a [&'b str]) {
    match argv.split_first() {
        Some((_, rest)) if !rest.is_empty() && rest[0] == "-n" => (false, &rest[1..]),
        Some((_, rest)) => (true, rest),
        // Real execve always delivers a program name; the empty case falls
        // back to the plain newline behaviour instead of panicking.
        None => (true, &[]),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Runs [`echo_emit`] against a collecting sink: returns the pieces in
    /// order plus the delivery verdict.
    fn collect(argv: &[&str]) -> (Vec<Vec<u8>>, bool) {
        let mut pieces = Vec::new();
        let delivered = echo_emit(argv, |piece| {
            pieces.push(piece.to_vec());
            true
        });
        (pieces, delivered)
    }

    #[test]
    fn test_no_args_prints_bare_newline() {
        let (pieces, delivered) = collect(&["echo"]);
        assert_eq!(pieces, vec![b"\n".to_vec()]);
        assert!(delivered);
    }

    #[test]
    fn test_single_arg_then_newline() {
        let (pieces, _) = collect(&["echo", "hello"]);
        assert_eq!(pieces, vec![b"hello".to_vec(), b"\n".to_vec()]);
    }

    #[test]
    fn test_arguments_separated_by_single_spaces() {
        let (pieces, _) = collect(&["echo", "a", "b", "c"]);
        assert_eq!(
            pieces,
            vec![
                b"a".to_vec(),
                b" ".to_vec(),
                b"b".to_vec(),
                b" ".to_vec(),
                b"c".to_vec(),
                b"\n".to_vec(),
            ]
        );
    }

    #[test]
    fn test_leading_nflag_prints_nothing_extra() {
        // C: `echo -n` with no further arguments emits no bytes at all.
        let (pieces, _) = collect(&["echo", "-n"]);
        assert!(pieces.is_empty());
    }

    #[test]
    fn test_leading_nflag_suppresses_only_the_newline() {
        let (pieces, _) = collect(&["echo", "-n", "hello"]);
        assert_eq!(pieces, vec![b"hello".to_vec()]);
    }

    #[test]
    fn test_second_nflag_is_data_not_flag() {
        // echo.c:61 forbids getopt: only the first argument can be the flag,
        // so `echo -n -n` prints `-n` with no trailing newline.
        let (pieces, _) = collect(&["echo", "-n", "-n"]);
        assert_eq!(pieces, vec![b"-n".to_vec()]);
    }

    #[test]
    fn test_double_dash_is_ordinary_data() {
        let (pieces, _) = collect(&["echo", "--", "x"]);
        assert_eq!(
            pieces,
            vec![b"--".to_vec(), b" ".to_vec(), b"x".to_vec(), b"\n".to_vec()]
        );
    }

    #[test]
    fn test_empty_argument_keeps_both_spaces() {
        let (pieces, _) = collect(&["echo", "a", "", "b"]);
        assert_eq!(
            pieces,
            vec![
                b"a".to_vec(),
                b" ".to_vec(),
                b"".to_vec(),
                b" ".to_vec(),
                b"b".to_vec(),
                b"\n".to_vec(),
            ]
        );
    }

    #[test]
    fn test_write_failure_stops_emission_and_reports() {
        // The sink fails on the second piece (the first space): emission
        // must not continue past the failure, and the verdict is false —
        // the caller's cue for exit 1 (echo.c:77-78).
        let mut calls = 0;
        let delivered = echo_emit(&["echo", "a", "b"], |piece| {
            calls += 1;
            let _ = piece;
            calls < 2
        });
        assert_eq!(calls, 2);
        assert!(!delivered);
    }

    #[test]
    fn test_write_failure_on_trailing_newline_reports() {
        let mut calls = 0;
        let delivered = echo_emit(&["echo", "a"], |piece| {
            calls += 1;
            let _ = piece;
            calls < 2
        });
        assert_eq!(calls, 2);
        assert!(!delivered);
    }
}
