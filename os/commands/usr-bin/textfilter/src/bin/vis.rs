//! Minix-RS vis — visible encoding of stdin and file operands.
//!
//! Ground truth: `minix3/usr.bin/vis/vis.c` (NetBSD). The deciding half
//! (the M-notation grammar, C-style named escapes, and the octal form)
//! is the library's `vis` module; this program encodes stdin or each file
//! operand and prints. `-c` selects C-style escapes, `-o` three-digit
//! octal, and `-n` drops the backslash prefix — the subset of the C's
//! getopt `bcde:F:fhlMmNnoSstw` (vis.c:73) modelled so far; an unmodelled
//! flag is rejected. With operands each is opened, encoded into one output
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

use minix_textfilter::vis::{encode, VisOptions};

fn run() -> ! {
    let argv: Vec<String> = support::args();
    let mut options = VisOptions::default();
    let mut operands: Vec<&str> = Vec::new();
    let mut index = 1;
    while index < argv.len() {
        let arg = argv[index].as_str();
        if arg == "-c" {
            options.cstyle = true;
        } else if arg == "-o" {
            options.octal = true;
        } else if arg == "-n" {
            options.no_slash = true;
        } else if arg.starts_with('-') && arg.len() > 1 {
            // -b -d -e -F -f -h -l -M -m -s -t -w are not in the deciding
            // half yet; reject rather than silently ignore.
            support::warn(format!("vis: flag {arg} is not modelled yet\n").as_bytes());
            support::terminate(1);
        } else {
            operands.push(arg);
        }
        index += 1;
    }

    // `vis.c:151-164`: each operand is opened and encoded into one output
    // stream; an unopenable operand warns and flags the exit status and the
    // run continues. Without operands stdin is the source.
    let mut encoded = String::new();
    let mut rval = 0;
    if operands.is_empty() {
        encoded.push_str(&encode(&support::read_stdin(), &options));
    } else {
        for operand in &operands {
            match support::read_file(operand) {
                Ok(data) => encoded.push_str(&encode(&data, &options)),
                Err(()) => {
                    support::warn(format!("vis: {operand}\n").as_bytes());
                    rval = 1;
                }
            }
        }
    }
    support::emit(encoded.as_bytes());
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
