//! Minix-RS colrm — remove columns from each line.
//!
//! Ground truth: `minix3/usr.bin/colrm/colrm.c` (NetBSD), main at lines
//! 61-121: `colrm [start [stop]]`; every input byte advances a display
//! column (tab rounds up to the next multiple of eight, backspace pulls
//! one back, newline resets) and bytes whose new column falls inside
//! the removed range are dropped. The deciding half (the column walk and
//! the keep/drop rule, plus the operand face) is the library's `colrm`
//! module; this program runs the option ring, prints the three diagnostic
//! lines, and filters stdin. `colrm` takes no file operands at all —
//! its operands are column numbers.

#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

extern crate alloc;

use alloc::string::String;
use alloc::vec::Vec;
#[path = "../bin_support.rs"]
mod support;

use minix_textfilter::colrm::{ColrmOperand, filter_column, parse_columns};
use minix_textfilter::empty_optstring_operands_bytes;

fn run() -> ! {
    let argv: Vec<Vec<u8>> = support::args_bytes();
    let prog = String::from_utf8_lossy(support::prog_name_bytes(&argv)).into_owned();
    let words: Vec<&[u8]> = argv[1..].iter().map(Vec::as_slice).collect();
    // `colrm.c:67` has an empty optstring, the same shape as `rev.c:70`: libc
    // first prints one out-of-set letter (`getopt.c:101-104`), then the
    // `default: usage()` at `:68-72` prints usage, while a bare `-` and any
    // token after `--` fall back to operands (the operands are column numbers,
    // not file names). The out-of-set-letter line goes through the byte form
    // `line_bytes`: libc's letter slot is **one bare byte** behind the dash
    // (`getopt.c:79` `optopt = *place++` consumes one byte at a time, `:103`
    // prints it verbatim with `%c`), so a reference build measured `colrm -é`
    // printing the single byte `c3`.
    let operands = match empty_optstring_operands_bytes(&words) {
        Ok(rest) => rest,
        Err(report) => {
            support::warn(&report.line_bytes(&prog));
            usage();
        }
    };
    let (start, stop) = match parse_columns(&operands) {
        Ok(range) => range,
        Err(ColrmOperand::TooManyOperands) => usage(),
        Err(ColrmOperand::IllegalColumn(word)) => {
            // C's `errx(1, "illegal column -- %s", argv[..])`
            // (`colrm.c:80-86`): `errx` carries only the program name and the
            // sentence, with no `strerror` suffix; the `%s` prints the **raw
            // bytes of the operand**, so those bytes never round-trip through
            // text.
            support::warn(&illegal_column(&prog, word));
            support::terminate(1);
        }
        Err(ColrmOperand::StartAfterStop) => {
            // C's `err(1, "illegal start and stop columns")` (`colrm.c:94-95`)
            // is `err` rather than `errx`, so the three-part form appends a
            // `strerror(errno)` sentence as well; by the time we reach here
            // `errno` is still 0 and the sentence text is decided by each
            // platform's strerror table (the reference build prints `Success`
            // here, while Minix's table has its own sentence for 0), so this
            // shell only guarantees the shape.
            support::warn_line(
                &prog,
                "illegal start and stop columns",
                minix_sys::Errno::from_i32(0),
            );
            support::terminate(1);
        }
    };

    // The read seam goes through `bin_support`'s twin face (host
    // `std::io::stdin`, real hardware `read(STDIN)`). C reads and writes as it
    // goes (`getchar`/`putchar`); collecting everything first and filtering
    // after produces the same output.
    //
    // Both legs can fail and C hands both to the same `check()`
    // (`colrm.c:123-130`): it tests `feof` first, then `ferror`, and reports
    // the **stream name** on error — `err(1, "%s", stream == stdin ? "stdin" :
    // "stdout")`. A write error happens inside the loop (`putchar` returning
    // EOF checks the stream and exits on the spot), so it precedes a read
    // error that is only discovered at end of input; this shell keeps that
    // order and prints one line each, exiting 1.
    let (input, read_failure) = support::read_stdin_error();
    let filtered = filter_column(&input, start, stop);
    match support::emit_error(&filtered) {
        Some(code) => {
            support::warn_line(&prog, "stdout", code);
            support::terminate(1);
        }
        None => match read_failure {
            Some(code) => {
                support::warn_line(&prog, "stdin", code);
                support::terminate(1);
            }
            // Clean read and clean write are C's `check(stdin)` taking the
            // `feof` branch: `exit(0)`.
            None => support::terminate(0),
        },
    }
}

/// The `errx(1, "illegal column -- %s", argv[..])` line: program name, sentence, and the **raw bytes** of the operand.
fn illegal_column(prog: &str, word: &[u8]) -> Vec<u8> {
    let mut out: Vec<u8> = Vec::new();
    out.extend_from_slice(prog.as_bytes());
    out.extend_from_slice(b": illegal column -- ");
    out.extend_from_slice(word);
    out.push(b'\n');
    out
}

/// C's `usage()` (`colrm.c:132-137`): one line, exit 1. The command name in
/// that line is hard-coded `"colrm"` (unlike `rev.c:110-115`, which follows
/// `getprogname()`), so the program name is not spliced in here.
fn usage() -> ! {
    support::warn(b"usage: colrm [start [stop]]\n");
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
