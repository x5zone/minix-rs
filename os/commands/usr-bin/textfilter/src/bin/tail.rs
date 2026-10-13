//! Minix-RS tail — last lines of a stream.
//!
//! Ground truth: `minix3/usr.bin/tail/tail.c` (NetBSD), `tail [-n count |
//! -count] [file ...]` with a default of ten lines. The ring capacity is fixed
//! at 32 (the library's `TailWindow`), so counts above 32 are rejected as out
//! of range — a declared cap. With no operand tail reads standard input; with
//! operands it prints the last `count` lines of each file, prefixing a
//! `==> name <==` banner when there is more than one file (this NetBSD `tail`
//! has no `-q`/`-v` to change that) with a blank line between files, and an
//! unopenable/unstatable file warns and is skipped (exit status stays
//! nonzero). Operands open with `fopen`, so `-` is a literal file name.
//!
//! Not modelled (rejected rather than mistaken for an operand): the follow
//! modes `-f`/`-F`. Byte mode `-c N`, block mode `-b N`, reverse mode `-r` and
//! the forward offsets `-n +N` / `-c +N` / `-b +N` are all modelled — C
//! `tail.c`/`forward.c`/`reverse.c`: `-r` prints lines bottom-up (`REVERSE`
//! reverses every line, `RLINES` = an explicit `-n` reverses the last `count`),
//! `-r` combined with `-c`/`-b` is the `RBYTES` byte-window line reverse
//! (`reverse_bytes`), and a `+N` value takes the forward arm of the `ARG` macro
//! so `tail -n +20` starts at line 20 rather than taking the last twenty lines.

#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

extern crate alloc;
use alloc::format;
use alloc::vec;

use alloc::string::String;
use alloc::vec::Vec;
#[path = "../bin_support.rs"]
mod support;

use minix_textfilter::window::{LineWindow, TailError, TailStyle, TailWindow, parse_tail_args};

fn run() -> ! {
    let argv: Vec<String> = support::args();
    let args: Vec<&str> = argv.iter().map(String::as_str).collect();
    // argv folding lives in the deciding half (`parse_tail_args`): BSD
    // `obsolete()` + `getopt("Fb:c:fn:r")`, with attached-or-next counts and
    // option clustering. The old inline whole-token matcher leaked attached
    // values (`tail -n10 f`, `tail -c100 f`) and unknown flags (`tail -x f`)
    // into the operand list, silently treating them as file names.
    let parsed = match parse_tail_args(&args[1..]) {
        Ok(parsed) => parsed,
        Err(e) => arg_error(e),
    };
    let style = parsed.style;
    let operands: Vec<String> = parsed.operands;

    // The fixed-capacity ring rejects 0 and counts above 32 (declared cap), and
    // only the non-zero `LastLines` walk is backed by it (`over_ring_cap`).
    if style.over_ring_cap() {
        support::terminate(1);
    }

    // C `for each file: fopen+fstat (ierr on failure), banner when argc>1,
    // forward()`. No operands read stdin. The banner comes after the successful
    // open because C `continue`s past a file it cannot open without printing one.
    let show_banner = operands.len() > 1;
    let mut first = true;
    let mut status = 0;

    if operands.is_empty() {
        tail_stream(&support::read_stdin(), style);
        support::terminate(0);
    }
    for operand in &operands {
        let data = match support::read_file(operand) {
            Ok(data) => data,
            Err(()) => {
                support::warn(format!("tail: {operand}\n").as_bytes());
                status = 1;
                continue;
            }
        };
        if show_banner {
            let sep = if first { "" } else { "\n" };
            support::emit(format!("{sep}==> {operand} <==\n").as_bytes());
        }
        first = false;
        tail_stream(&data, style);
    }
    support::terminate(status)
}

/// Emit the tail of one open descriptor for the resolved [`TailStyle`]. The
/// forward shapes print raw bytes from an offset (C `forward()`'s `getc` loop
/// copies what is left of the file, newlines and all, so no line is invented
/// and a trailing unterminated line stays unterminated); the reverse shapes
/// print whole lines bottom-up (C `reverse()`).
fn tail_stream(input: &[u8], style: TailStyle) {
    // C's zero arms come back with nothing written and a successful exit:
    // `forward.c` seeks to the end for `RLINES`/`RBYTES` with `off == 0`, and
    // `reverse.c` returns before touching the input for any other style with
    // `off == 0`. Only `REVERSE` (bare `-r`) prints the whole file.
    if style.selects_nothing() {
        return;
    }
    match style {
        TailStyle::SkipBytes { off } => support::emit(bytes_from(input, off)),
        TailStyle::SkipLines { off } => support::emit(bytes_after_lines(input, off)),
        TailStyle::LastBytes { off } => support::emit(last_bytes_of(input, off)),
        TailStyle::LastLines { off } => {
            let mut window = match TailWindow::new(off) {
                Ok(window) => window,
                Err(_) => support::terminate(1),
            };
            for line in support::lines_of(input) {
                window.push(line);
            }
            let mut survivors: Vec<&str> = vec![""; off.min(32)];
            let drained = window.drain(&mut survivors);
            for line in &survivors[..drained] {
                emit_line(line);
            }
        }
        TailStyle::ReverseAll => {
            for line in reversed_lines(&support::lines_of(input), None) {
                emit_line(line);
            }
        }
        // `off == 0` prints nothing, which is C's early return at
        // `reverse.c:79-80` (`style != REVERSE && off == 0`).
        TailStyle::ReverseLines { off } => {
            for line in reversed_lines(&support::lines_of(input), Some(off)) {
                emit_line(line);
            }
        }
        TailStyle::ReverseBytes { off } => {
            for line in reverse_bytes(input, off) {
                emit_line(line);
            }
        }
    }
}

fn emit_line(line: &str) {
    let mut out = line.as_bytes().to_vec();
    out.push(b'\n');
    support::emit(&out);
}

/// The line order C `reverse()` prints: `RLINES` (`Some(off)`) takes the last
/// `off` lines then reverses them (`reverse.c:138-141` stops the backward walk
/// after `off` lines); `REVERSE` (`None`) reverses every line. `Some(0)` selects
/// no line at all, matching C's early return.
fn reversed_lines<'a>(lines: &[&'a str], take: Option<usize>) -> Vec<&'a str> {
    let selected: &[&'a str] = match take {
        Some(off) => &lines[lines.len().saturating_sub(off)..],
        None => lines,
    };
    selected.iter().rev().copied().collect()
}

/// C `tail -r` with `-c`/`-b` (`RBYTES`, `reverse.c` `r_reg`): the lines of the
/// trailing `off`-byte window, printed bottom-up. The window can begin mid-line;
/// r_reg still emits that leading fragment (its leftover `WR`), which is
/// precisely what a byte-window line split yields, so the reverse is exact.
fn reverse_bytes(input: &[u8], off: usize) -> Vec<&str> {
    let window = last_bytes_of(input, off);
    let lines = support::lines_of(window);
    reversed_lines(&lines, None)
}

/// The last `off` bytes of `input`, or all of it when shorter — C `tail -c N`
/// prints the final N bytes of each file.
fn last_bytes_of(input: &[u8], off: usize) -> &[u8] {
    &input[input.len().saturating_sub(off)..]
}

/// C `forward()` + `FBYTES` (`forward.c:100-113`): print from byte `off + 1` on.
/// C clamps the seek to the file size for regular files and stops at EOF for
/// pipes, either way an offset past the end prints nothing.
fn bytes_from(input: &[u8], off: usize) -> &[u8] {
    &input[off.min(input.len())..]
}

/// C `forward()` + `FLINES` (`forward.c:115-127`): consume input through the
/// `off`-th newline, then print everything after it. `off == 0` prints the whole
/// input (that is `tail -n +1`), and an input with fewer than `off` newlines
/// leaves C at EOF, which prints nothing.
fn bytes_after_lines(input: &[u8], off: usize) -> &[u8] {
    if off == 0 {
        return input;
    }
    let mut seen = 0usize;
    for (index, byte) in input.iter().enumerate() {
        if *byte == b'\n' {
            seen += 1;
            if seen == off {
                return &input[index + 1..];
            }
        }
    }
    &[]
}

/// Map a [`TailError`] from `parse_tail_args` to its diagnostic and exit status,
/// then terminate.
fn arg_error(e: TailError) -> ! {
    match e {
        TailError::Usage => usage(),
        TailError::NotModelledFollow => {
            support::warn(b"tail: this option is not modelled (line-based ring, forward only)\n");
            support::terminate(1);
        }
        TailError::IllegalCount(v) => {
            // C `xerrx(1, "illegal offset -- %s", optarg)` (`tail.c:90`).
            support::warn(format!("tail: illegal offset -- {v}\n").as_bytes());
            support::terminate(1);
        }
        TailError::IllegalOption(v) => {
            // C `obsolete()`'s fatal arm: `xerrx(1, "illegal option -- %s", *argv)`.
            support::warn(format!("tail: illegal option -- {v}\n").as_bytes());
            support::terminate(1);
        }
    }
}

fn usage() -> ! {
    support::warn(b"usage: tail [-r] [-b N | -c N | -n N] [file ...]\n");
    support::terminate(1);
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
    use super::{bytes_after_lines, bytes_from, last_bytes_of, reverse_bytes, reversed_lines};

    #[test]
    fn last_bytes_truncates_to_count() {
        assert_eq!(last_bytes_of(b"abcdef", 3), b"def");
    }

    #[test]
    fn last_bytes_short_input_returns_all() {
        assert_eq!(last_bytes_of(b"ab", 10), b"ab");
    }

    #[test]
    fn last_bytes_of_empty_is_empty() {
        assert_eq!(last_bytes_of(b"", 5), b"");
    }

    #[test]
    fn bytes_from_skips_the_offset() {
        // `-c +3` is `off = 2` (`ARG` gives back one unit), i.e. byte three on.
        assert_eq!(bytes_from(b"abcdef", 2), b"cdef");
    }

    #[test]
    fn bytes_from_zero_is_whole_input() {
        // `-c +1` folds to `off = 0`: C breaks out of the `FBYTES` case without
        // seeking, so the whole file prints.
        assert_eq!(bytes_from(b"abcdef", 0), b"abcdef");
    }

    #[test]
    fn bytes_from_past_end_prints_nothing() {
        // C clamps the seek to `st_size` (`forward.c`), and a pipe stops at EOF —
        // either way nothing is left to print.
        assert_eq!(bytes_from(b"abc", 9), b"");
    }

    #[test]
    fn bytes_after_lines_lands_on_the_nth_line() {
        let input = b"one\ntwo\nthree\nfour\n";
        // `-n +1` → off 0 → whole file; `-n +2` → off 1 → past the first newline.
        assert_eq!(bytes_after_lines(input, 0), input);
        assert_eq!(bytes_after_lines(input, 1), b"two\nthree\nfour\n");
        assert_eq!(bytes_after_lines(input, 3), b"four\n");
    }

    #[test]
    fn bytes_after_lines_keeps_a_final_unterminated_line() {
        // C copies the remaining bytes character by character, so a last line
        // without a newline keeps its shape instead of gaining one.
        let input = b"a\nb world";
        assert_eq!(bytes_after_lines(input, 1), b"b world");
    }

    #[test]
    fn bytes_after_lines_short_input_prints_nothing() {
        // Fewer newlines than `off`: C's `getc` loop reaches EOF and prints
        // nothing.
        assert_eq!(bytes_after_lines(b"only\n", 5), b"");
    }

    #[test]
    fn reversed_lines_all_reverses_every_line() {
        let lines = ["1", "2", "3"];
        assert_eq!(reversed_lines(&lines, None), ["3", "2", "1"]);
    }

    #[test]
    fn reversed_lines_last_n_reversed() {
        // -r -n 2 on [1,2,3,4] -> last 2 (3,4) reversed -> 4,3.
        let lines = ["1", "2", "3", "4"];
        assert_eq!(reversed_lines(&lines, Some(2)), ["4", "3"]);
    }

    #[test]
    fn reversed_lines_count_exceeding_input_is_all() {
        let lines = ["a", "b"];
        assert_eq!(reversed_lines(&lines, Some(5)), ["b", "a"]);
    }

    #[test]
    fn reversed_lines_zero_takes_nothing() {
        // C `reverse.c:79-80`: a non-`REVERSE` style with `off == 0` returns
        // before printing, so `tail -r -n +1` prints nothing rather than the
        // whole file.
        let lines = ["a", "b"];
        assert!(reversed_lines(&lines, Some(0)).is_empty());
    }

    #[test]
    fn reverse_bytes_reverses_lines_of_trailing_window() {
        // -r -c on "aaa\nbbb\nccc\n": last 8 bytes = "bbb\nccc\n" -> ccc,bbb.
        let input = b"aaa\nbbb\nccc\n";
        assert_eq!(reverse_bytes(input, 8), ["ccc", "bbb"]);
        // window starts mid-line (last 7 = "bb\nccc\n"); r_reg emits the leading
        // fragment too, so "bb" survives as its own line.
        assert_eq!(reverse_bytes(input, 7), ["ccc", "bb"]);
        // byte offset past the input reverses the whole file's lines.
        assert_eq!(reverse_bytes(input, 99), ["ccc", "bbb", "aaa"]);
        // -r -c +1 folds to RBYTES with off 0, whose empty window holds no line.
        assert_eq!(reverse_bytes(input, 0), Vec::<&str>::new());
    }
}
