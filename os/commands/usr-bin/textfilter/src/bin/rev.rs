//! Minix-RS rev — reverse each line's characters.
//!
//! Ground truth: `minix3/usr.bin/rev/rev.c` (NetBSD), main at lines 59-109:
//! file operands or stdin, every line's characters print in reverse with
//! a newline, and an unopenable file warns and is skipped (exit status stays
//! nonzero, non-fatal). `rev` takes no options (C `getopt("")` at `:70-75`)——
//! an empty optstring does not mean "no option loop": `-Z` is still reported by
//! libc as one out-of-set letter followed by `usage()`, while a bare `-` and
//! tokens after `--` fall back to operands. The
//! character-reversal walk (multibyte kept intact) is the library's tested
//! `reversed_lines`.

#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

extern crate alloc;

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
#[path = "../bin_support.rs"]
mod support;

use minix_textfilter::rev::{parse_rev_args, reversed_lines};

fn run() -> ! {
    let argv: Vec<Vec<u8>> = support::args_bytes();
    // The rendering outlet for the program-name leg is still `&str` (both
    // `GetoptReport::line_bytes` and `warn_line` take `&str`); the library-side
    // byte interface is a registered follow-up. The letter and operand legs
    // start from raw bytes here.
    let prog = String::from_utf8_lossy(support::prog_name_bytes(&argv)).into_owned();
    let refs: Vec<&[u8]> = argv.iter().map(Vec::as_slice).collect();
    // C's option loop has exactly one exit: on `'?'` it calls `usage()`
    // (`rev.c:72-74`), and that `unknown option -- <letter>` line is printed by
    // libc itself before returning (`getopt.c:101-104`). The old version had no
    // option loop, so `-Z` was treated as a file operand and the sentence was
    // missing the `strerror` part. This uses the byte form `line_bytes`: the
    // letter in libc's line is a raw byte (`optopt = *place++`); measuring
    // `rev -é` against the reference prints the single byte `c3`, while
    // rendering as `char` would add one byte.
    let operands = match parse_rev_args(&refs[1..]) {
        Ok(rest) => rest,
        Err(report) => {
            support::warn(&report.line_bytes(&prog));
            usage(&prog);
        }
    };

    // C `do { if (*argv) fopen or warn+continue; while(fgetwln reverse) } while(*argv)`
    // (`rev.c:83-106`): an unopenable file is `warn(3)`, **not** a fatal error;
    // skip it and continue with the next one, setting the exit status to 1.
    let mut status = 0;
    if operands.is_empty() {
        // The read seam goes through `bin_support`'s twin face (host
        // `std::io::stdin`, native `read(STDIN)`): this file used to call raw
        // `minix_sys::read` directly, which **reads no input at all on the
        // host**, so the newly added end-to-end gate exposed it right away
        // (likewise `nl` and `fold`).
        //
        // A read failure is not end of file: after the line loop C tries
        // `ferror` and reports the **stream name**, which for standard input is
        // the initial value `"stdin"` (`rev.c:80-81`, `:101-104`). Whatever has
        // already been read is emitted first (C printed it as it read), then
        // one `warn` line and exit status 1.
        let (input, failure) = support::read_stdin_error();
        emit_reversed(&input);
        if let Some(code) = failure {
            support::warn_line(&prog, "stdin", code);
            status = 1;
        }
        support::terminate(status);
    }
    for operand in operands {
        // Both the file-open and the message-echo legs take `&str`
        // (`os/libs/minix-sys/src/lib.rs:fn open`,
        // `os/libs/minix-types/src/types/errno.rs:fn err_line`), so a byte-form
        // operand can only be folded once here—that gap is a registered
        // follow-up.
        let path = String::from_utf8_lossy(operand).into_owned();
        let input = match support::read_file(&path) {
            Ok(data) => data,
            Err(code) => {
                // C's `warn("%s", *argv)` (`rev.c:86`) is the err(3) three-part
                // form: program name, operand, `strerror` sentence.
                support::warn_line(&prog, &path, code);
                status = 1;
                continue;
            }
        };
        emit_reversed(&input);
    }
    support::terminate(status)
}

/// Emit each reversed line with its newline.
fn emit_reversed(input: &[u8]) {
    for line in reversed_lines(input) {
        let mut out = line.into_bytes();
        out.push(b'\n');
        support::emit(&out);
    }
}

/// C's `usage()` (`rev.c:110-115`): one line, the program name comes from
/// `getprogname()`, and `exit(EXIT_FAILURE)` means exit status 1.
fn usage(prog: &str) -> ! {
    support::warn(format!("usage: {prog} [file ...]\n").as_bytes());
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
