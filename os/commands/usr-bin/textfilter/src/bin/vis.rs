//! Minix-RS vis — visible encoding of stdin and file operands.
//!
//! Ground truth: `minix3/usr.bin/vis/vis.c` (NetBSD). The deciding half
//! (the M-notation grammar, C-style named escapes, and the octal form)
//! is the library's `vis` module; this program encodes stdin or each file
//! operand and prints. `-c` selects C-style escapes, `-o` three-digit
//! octal, `-b` (`VIS_NOSLASH`) drops the backslash prefix, `-l` ends each line
//! with `\$`, and `-n` copies the stream with each backslash doubled — five of
//! the C's getopt letters `bcde:F:fhlMmNnoSstw` (vis.c:73); an unmodelled flag is
//! rejected. With operands each is opened, encoded into one output
//! stream, and an unopenable operand warns and flags the exit status
//! (`vis.c:151-164`); a bare `-` is a literal file name (`fopen`), not a
//! stdin alias. Without operands stdin is the source.

#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

extern crate alloc;

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
#[path = "../bin_support.rs"]
mod support;

use minix_textfilter::vis::{VisOptions, copy_doubling_backslashes, encode};

fn run() -> ! {
    let argv: Vec<String> = support::args();
    let mut options = VisOptions::default();
    let mut none = false;
    let mut operands: Vec<&str> = Vec::new();
    let mut index = 1;
    while index < argv.len() {
        let arg = argv[index].as_str();
        if arg == "-c" {
            options.cstyle = true;
        } else if arg == "-o" {
            options.octal = true;
        } else if arg == "-b" {
            options.no_slash = true;
        } else if arg == "-l" {
            options.mark_eol = true;
        } else if arg == "-n" {
            none = true;
        } else if arg.starts_with('-') && arg.len() > 1 {
            // -d -e -F -f -h -M -m -s -t -w are not in the deciding half yet;
            // reject rather than silently ignore. (`-d` is C's debug counter,
            // compiled out of this build, so C itself sends it to `default:`.)
            support::warn(format!("vis: flag {arg} is not modelled yet\n").as_bytes());
            support::terminate(1);
        } else {
            operands.push(arg);
        }
        index += 1;
    }

    // `vis.c:151-164`: each operand is opened and encoded into one output
    // stream; an unopenable operand warns and flags the exit status and the
    // run continues. Without operands stdin is the source. `-n` is tested first
    // in C's `process()` loop, so it overrides `-l` and the encoding.
    let mut encoded: Vec<u8> = Vec::new();
    let mut rval = 0;
    let mut consume = |data: &[u8]| {
        if none {
            encoded.extend_from_slice(&copy_doubling_backslashes(data));
        } else {
            encoded.extend_from_slice(encode(data, &options).as_bytes());
        }
    };
    if operands.is_empty() {
        consume(&support::read_stdin());
    } else {
        for operand in &operands {
            match support::read_file(operand) {
                Ok(data) => consume(&data),
                Err(()) => {
                    support::warn(format!("vis: {operand}\n").as_bytes());
                    rval = 1;
                }
            }
        }
    }
    support::emit(&encoded);
    support::terminate(rval);
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
