//! Minix-RS col — filter reverse line feeds from input.
//!
//! Ground truth: `minix3/usr.bin/col/col.c` (NetBSD), usage
//! `col [-bfpx] [-l nline]` with no operands. The deciding half (the
//! line-assembly state machine and flush rules) is the library's
//! `col` module — including its option ring (`parse_col_flags`);
//! this program renders the four exits and runs the filter over stdin.

#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

extern crate alloc;

use alloc::string::String;
use alloc::vec::Vec;
#[path = "../bin_support.rs"]
mod support;

use minix_textfilter::GetoptReport;
use minix_textfilter::col::{ColFlagError, filter, parse_col_flags};

fn run() -> ! {
    let argv: Vec<Vec<u8>> = support::args_bytes();
    // The rendering door for the program-name leg still takes `&str`
    // (`GetoptReport::line_bytes` accepts `&str`), and the library's byte door
    // remains unchanged and on the books: a registered limitation. The letter
    // slot and the `-l` value are raw bytes starting here.
    let prog = String::from_utf8_lossy(support::prog_name_bytes(&argv)).into_owned();
    let words: Vec<&[u8]> = argv.iter().map(Vec::as_slice).collect();
    let options = match parse_col_flags(&words[1..]) {
        Ok(options) => options,
        Err(error) => flag_error(&prog, error),
    };

    // The read seam goes through `bin_support`'s twin face (host
    // `std::io::stdin`, target `read(STDIN)`): this file used to call raw
    // `minix_sys::read` and **could not see any input on the host** (the seventh
    // case of the family: the read legs of `nl`/`fold`/`rev`/`colrm`, the write
    // leg of `split`, the read leg of `unexpand`), so `col`'s filtered output
    // had never been exercised at process level.
    let out = filter(&support::read_stdin(), &options);
    if out.warned {
        support::warn(b"col: warning: can't back up -- line already flushed\n");
    }
    support::emit(&out.bytes);
    support::terminate(0)
}

/// The sentence and exit code of each of the four exits, arm by arm per C.
fn flag_error(prog: &str, error: ColFlagError<'_>) -> ! {
    match error {
        ColFlagError::OperandsPresent => usage(),
        // An unknown letter or a `-l` missing its value: the libc line starts
        // with `getprogname()` (on the real machine that is `col`), and only
        // then comes `usage()` from `col.c:154-156` — two lines. Go through the
        // byte form `line_bytes`: the letter slot of the libc line is one raw
        // byte (`getopt.c:79`'s `optopt = *place++` takes one byte at a time,
        // `:103` writes it out verbatim with `%c`). The reference binary
        // `tmp/col-313/col` measures `col -bé` as a single `c3` byte, whereas
        // rendering through `char` would add a second byte (U+00C3 is `c3 83` in
        // UTF-8).
        ColFlagError::UnknownOption(letter) => {
            support::warn(&GetoptReport::UnknownOption(letter).line_bytes(prog));
            usage()
        }
        ColFlagError::MissingValue(letter) => {
            support::warn(&GetoptReport::MissingValue(letter).line_bytes(prog));
            usage()
        }
        // `col.c:145-150` is a bare
        // `fprintf(stderr, "col: bad -l argument %s.\n", optarg)` plus `exit`:
        // the program name is hard-coded, the operand text is included, there is
        // a period, and **no usage follows**. The old form omitted the operand
        // text and the period, and wrongly took this arm when `-l` had no
        // following segment.
        ColFlagError::BadLineCount(value) => {
            support::warn(&bad_line_count(value));
            support::terminate(1);
        }
    }
}

/// The `col.c:145-150` sentence, assembled byte-wise: `optarg` comes from the
/// raw argv bytes, and the echo slot must not be rewritten by any character
/// decode (`col: bad -l argument <raw bytes>.`).
fn bad_line_count(value: &[u8]) -> Vec<u8> {
    let mut out: Vec<u8> = Vec::new();
    out.extend_from_slice(b"col: bad -l argument ");
    out.extend_from_slice(value);
    out.extend_from_slice(b".\n");
    out
}

/// C's `usage()` (`col.c:520-526`): the program name is a **hard-coded** `col`
/// (unlike `rev`, which follows `getprogname()`), one line, exit 1.
fn usage() -> ! {
    support::warn(b"usage: col [-bfpx] [-l nline]\n");
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
