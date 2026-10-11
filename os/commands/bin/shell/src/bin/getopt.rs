//! Minix-RS `getopt` — the doing half over `minix_shell::getopt`.
//!
//! Ground truth: `minix3/usr.bin/getopt/getopt.c` (the Henry Spencer demo)
//! driving `minix3/lib/libc/stdlib/getopt.c`. `getopt <optstring> <params…>`
//! parses `<params…>` as if they were a program's argv (optstring is argv[1],
//! scanning starts at argv[2]), then prints a shell-ready command line:
//! `" -X"` per flag, `" -X <arg>"` per option-with-value, `" --"`, then the
//! operands, then a newline. An unknown option makes `getopt(3)` write a
//! message to standard error and the command exit 1.
//!
//! [`render`] reproduces the whole command's byte output from pure inputs, so
//! the full behavior is host-testable without a kernel write channel; the
//! thin `run` just wires the three return values to the write streams. The
//! parser is the crate's faithful `getopt(3)` port.

#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

extern crate alloc;

#[path = "../bin_support.rs"]
mod support;

use alloc::vec::Vec;
use minix_shell::getopt::{Getopt, Outcome};

/// The program name C's `getprogname()` reports for the error messages —
/// the last path component of `argv[0]` (`getopt` when run plainly).
fn progname(argv0: &[u8]) -> &[u8] {
    match argv0.iter().rposition(|&c| c == b'/') {
        Some(i) => &argv0[i + 1..],
        None => argv0,
    }
}

/// The `getopt(1)` command's full rendering: `(stdout bytes, stderr bytes,
/// exit status)`. Pure — no I/O — mirroring the C loop
/// (`getopt.c:24-40`) plus the two parser diagnostics.
fn render(argv: &[&[u8]], name: &[u8], optstring: &[u8]) -> (Vec<u8>, Vec<u8>, i32) {
    let mut out: Vec<u8> = Vec::new();
    let mut err: Vec<u8> = Vec::new();
    let mut status = 0;
    let mut parser = Getopt::new(2);
    loop {
        match parser.next(argv, optstring) {
            Outcome::Found { letter, argument } => {
                out.extend_from_slice(b" -");
                out.push(letter);
                if let Some(value) = argument {
                    out.push(b' ');
                    out.extend_from_slice(value);
                }
            }
            Outcome::Unknown { letter } => {
                // getopt(3) prints only when opterr is on (it is) and the
                // option string does not begin with ':'.
                if optstring.first() != Some(&b':') {
                    err.extend_from_slice(name);
                    err.extend_from_slice(b": unknown option -- ");
                    err.push(letter);
                    err.push(b'\n');
                }
                status = 1;
            }
            Outcome::MissingArgument { letter, colon_mode } => {
                if colon_mode {
                    // C returns ':' (BADARG) with no message; getopt(1)'s
                    // switch falls to default and prints " -:" (a degenerate
                    // path — getopt(1) with a ':'-leading option string).
                    out.extend_from_slice(b" -:");
                    let _ = letter;
                } else {
                    err.extend_from_slice(name);
                    err.extend_from_slice(b": option requires an argument -- ");
                    err.push(letter);
                    err.push(b'\n');
                    status = 1;
                }
            }
            Outcome::End { next_index } => {
                out.extend_from_slice(b" --");
                for operand in &argv[next_index.min(argv.len())..] {
                    out.push(b' ');
                    out.extend_from_slice(operand);
                }
                out.push(b'\n');
                break;
            }
        }
    }
    (out, err, status)
}

fn run() -> ! {
    let argv: Vec<alloc::string::String> = support::args();
    let argv_refs: Vec<&[u8]> = argv.iter().map(|s| s.as_bytes()).collect();
    let name = progname(argv_refs.first().copied().unwrap_or(b"getopt"));
    let optstring: &[u8] = argv_refs.get(1).copied().unwrap_or(b"");

    let (out, err, status) = render(&argv_refs, name, optstring);
    if !err.is_empty() {
        support::warn(&err);
    }
    support::emit(&out);
    support::terminate(status);
}

#[cfg(all(not(test), target_os = "none"))]
#[unsafe(no_mangle)]
extern "Rust" fn main() -> i32 {
    run()
}

#[cfg(any(test, not(target_os = "none")))]
fn main() {
    run()
}

#[cfg(test)]
mod tests {
    use super::*;

    // argv as byte slices: [progname, optstring, params…].
    fn argv_of<'x>(parts: &[&'x str]) -> Vec<&'x [u8]> {
        parts.iter().map(|s| s.as_bytes()).collect()
    }

    fn render_of(parts: &[&str]) -> (Vec<u8>, Vec<u8>, i32) {
        let argv = argv_of(parts);
        // optstring is argv[1]; progname is the basename of argv[0].
        let name = progname(argv[0]);
        let optstring = argv.get(1).copied().unwrap_or(b"");
        render(&argv, name, optstring)
    }

    #[test]
    fn two_flags_then_operand() {
        let (out, err, status) = render_of(&["getopt", "ab", "-a", "-b", "foo"]);
        assert_eq!(out, b" -a -b -- foo\n");
        assert!(err.is_empty());
        assert_eq!(status, 0);
    }

    #[test]
    fn cluster_flags() {
        // `-ab` in one token yields both flags.
        let (out, _, status) = render_of(&["getopt", "ab", "-ab", "tail"]);
        assert_eq!(out, b" -a -b -- tail\n");
        assert_eq!(status, 0);
    }

    #[test]
    fn option_with_separate_value() {
        let (out, _, status) = render_of(&["getopt", "ab:", "-a", "-b", "val", "rest"]);
        assert_eq!(out, b" -a -b val -- rest\n");
        assert_eq!(status, 0);
    }

    #[test]
    fn option_with_attached_value() {
        let (out, _, status) = render_of(&["getopt", "abc:", "-abcVALUE", "p", "q"]);
        assert_eq!(out, b" -a -b -c VALUE -- p q\n");
        assert_eq!(status, 0);
    }

    #[test]
    fn double_dash_delimits_operands() {
        let (out, _, status) = render_of(&["getopt", "a", "-a", "--", "-b", "x"]);
        assert_eq!(out, b" -a -- -b x\n");
        assert_eq!(status, 0);
    }

    #[test]
    fn unknown_option_errors_and_exits_one() {
        let (out, err, status) = render_of(&["getopt", "ab", "-a", "-z"]);
        assert_eq!(out, b" -a --\n");
        assert_eq!(err, b"getopt: unknown option -- z\n");
        assert_eq!(status, 1);
    }

    #[test]
    fn missing_argument_errors_and_exits_one() {
        let (out, err, status) = render_of(&["getopt", "a:", "-a"]);
        assert_eq!(out, b" --\n");
        assert_eq!(err, b"getopt: option requires an argument -- a\n");
        assert_eq!(status, 1);
    }

    #[test]
    fn progname_takes_last_path_component() {
        let (out, err, _) = render_of(&["/usr/bin/getopt", "ab", "-z"]);
        assert_eq!(out, b" --\n");
        // the diagnostic uses the basename, not the full path
        assert_eq!(err, b"getopt: unknown option -- z\n");
    }
}
