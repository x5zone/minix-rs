//! Minix-RS head — first lines of a stream.
//!
//! Ground truth: `minix3/usr.bin/head/head.c` (NetBSD), `head [-n count |
//! -count] [-qv] [-c count] [file ...]` with a default of ten lines. The
//! window keeps borrowed lines for however many the count asks for and no more
//! than the input offered (`window.rs` `HeadWindow`), so any count is honoured.
//! With no operand head reads standard input;
//! with operands it prints the first `count` lines of each file, prefixing a
//! `==> name <==` banner when there is more than one file (always under `-v`,
//! never under `-q`), with a blank line between files. Operands are opened
//! with `fopen`, so `-` is a literal file name (not stdin, unlike `cut`), and
//! an unopenable file warns and is skipped (exit status stays nonzero). Byte
//! mode `-c N` emits the first `N` raw bytes of each file, ignoring the line
//! count (`head.c:149-167`); `N` must be positive.

#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

extern crate alloc;
use alloc::format;
use alloc::vec;

use alloc::string::String;
use alloc::vec::Vec;
#[path = "../bin_support.rs"]
mod support;

use minix_textfilter::window::{
    HeadArgError, HeadArgs, HeadWindow, Line, LineWindow, parse_head_args,
};

fn run() -> ! {
    let argv: Vec<Vec<u8>> = support::args_bytes();
    // Both C's diagnostic lines and its usage line start with `getprogname()`
    // (`head.c:200-202` passes the program name as `%s`), so the command name
    // cannot be hard-coded. The program-name leg goes to `warn`/`warn_line` and
    // to `line_bytes`'s `&str` name field (registered `ARGV-BYTE-PATH-IN-LIBS`),
    // so it is folded once here.
    let prog = String::from_utf8_lossy(support::prog_name_bytes(&argv));
    let args: Vec<&[u8]> = argv.iter().map(Vec::as_slice).collect();
    let HeadArgs {
        count,
        bytecnt,
        qflag,
        vflag,
        operands,
    } = match parse_head_args(&args[1..]) {
        Ok(parsed) => parsed,
        // libc's line comes first, usage after (`getopt.c:101-104`, `:132-136`).
        // The letter byte goes through `line_bytes`: `getopt.c:79`'s
        // `optopt = *place++` takes one byte at a time and `:103`'s `%c` writes
        // it out verbatim, so `-é` (`c3 a9`) reports `c3`; the extraction in
        // `src/window.rs`'s loop is already `bytes[pos] as char`, and that only
        // holds if both sides have the same shape.
        Err(HeadArgError::Getopt(report)) => {
            support::warn(&report.line_bytes(&prog));
            usage(&prog)
        }
        Err(HeadArgError::IllegalCount { byte, value }) => {
            // C `errx(1, "illegal byte count -- %s", optarg)` (`head.c:92-102`):
            // goes through `vwarnx`, so no `errno` sentence is appended; and the
            // `%s` hands back the **raw bytes of optarg**, so that segment must not
            // pass through `&str` — the sentence is assembled from bytes and `value`
            // is appended directly.
            let kind: &[u8] = if byte { b"byte" } else { b"line" };
            let mut line = prog.as_bytes().to_vec();
            line.extend_from_slice(b": illegal ");
            line.extend_from_slice(kind);
            line.extend_from_slice(b" count -- ");
            line.extend_from_slice(&value);
            line.push(b'\n');
            support::warn(&line);
            support::terminate(1);
        }
    };

    // C `if (*argv) for each file: fopen, banner when (vflag || (qflag==0 &&
    // argc>1)) with a leading blank between files, then head(); else head(stdin).
    // The banner comes after the successful open because C `continue`s past a file
    // it cannot open without printing one.
    let show_banner = vflag || (!qflag && operands.len() > 1);
    let mut first = true;
    let mut status = 0;

    if operands.is_empty() {
        head_stream(&support::read_stdin(), count, bytecnt);
        support::terminate(0);
    }
    for operand in &operands {
        // Both opening the file and `warn("%s", *argv)` need a `&str` (registered
        // `ARGV-BYTE-PATH-IN-LIBS`), so it is folded once here; **the banner leg is
        // not folded** — this shell prints it itself (`head.c:130-131`), and what it
        // hands back must be the raw argv bytes.
        let path = String::from_utf8_lossy(operand);
        let data = match support::read_file(&path) {
            Ok(data) => data,
            // C `head.c:124-127` is `warn("%s", *argv)` + `eval = 1` + `continue`:
            // all three components — program name, operand, `strerror` sentence —
            // are required (an older spelling used `warn(3)` as if it were `warnx(3)`).
            Err(code) => {
                support::warn_line(&prog, &path, code);
                status = 1;
                continue;
            }
        };
        if show_banner {
            let mut banner: Vec<u8> = if first { Vec::new() } else { vec![b'\n'] };
            banner.extend_from_slice(b"==> ");
            banner.extend_from_slice(operand);
            banner.extend_from_slice(b" <==\n");
            support::emit(&banner);
        }
        first = false;
        head_stream(&data, count, bytecnt);
    }
    support::terminate(status)
}

/// Emit the head of one stream: the first `bytecnt` raw bytes in byte mode
/// (`head.c`'s `fread` loop), else the first `count` lines through the window.
fn head_stream(input: &[u8], count: usize, bytecnt: usize) {
    if bytecnt > 0 {
        support::emit(first_bytes_of(input, bytecnt));
        return;
    }
    // `parse_head_args` has already refused a non-positive count with C's
    // `illegal line count`/`illegal byte count` message (`head.c:90-103`), so
    // the window always opens here.
    let Ok(mut window) = HeadWindow::new(count) else {
        support::terminate(1);
    };
    for line in support::lines_of(input) {
        window.push(line);
    }
    let want = (window.seen() as usize).min(count);
    let mut survivors: Vec<Line> = vec![Line::new(b""); want];
    let drained = window.drain(&mut survivors);
    for line in &survivors[..drained] {
        let mut out = line.bytes().to_vec();
        // C's line shape is a character-by-character copy (`head.c:169-172`); no
        // terminator is invented for a last line that has none.
        out.extend_from_slice(line.eol());
        support::emit(&out);
    }
}

/// The first `bytecnt` bytes of `input`, or all of it when shorter — C's byte
/// mode `fread`s up to the remaining count per file (`head.c:149-167`).
fn first_bytes_of(input: &[u8], bytecnt: usize) -> &[u8] {
    &input[..bytecnt.min(input.len())]
}

/// `head`'s usage line, verbatim against `head.c:199-203`
/// (`"usage: %s [-n lines] [file ...]\n"` plus `getprogname()`, exit 1). Note that
/// this C line carries no `-c`/`-q`/`-v` letters at all — upstream simply omits
/// them, and this shell follows C instead of correcting the list on its behalf.
fn usage(prog: &str) -> ! {
    support::warn(format!("usage: {prog} [-n lines] [file ...]\n").as_bytes());
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
    use super::first_bytes_of;

    #[test]
    fn first_bytes_truncates_to_count() {
        assert_eq!(first_bytes_of(b"abcdef", 3), b"abc");
    }

    #[test]
    fn first_bytes_short_input_returns_all() {
        assert_eq!(first_bytes_of(b"ab", 10), b"ab");
    }

    #[test]
    fn first_bytes_of_empty_is_empty() {
        assert_eq!(first_bytes_of(b"", 5), b"");
    }
}
