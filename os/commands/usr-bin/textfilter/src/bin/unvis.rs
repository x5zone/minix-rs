//! Minix-RS unvis — decode visible-encoded stdin and file operands.
//!
//! Ground truth: `minix3/usr.bin/unvis/unvis.c` (NetBSD): the decoder
//! state machine is the library's `vis::decode`; this program decodes
//! stdin or each file operand and prints. With operands each is opened
//! and decoded into one output stream, and an unopenable operand warns
//! and is skipped while the exit status stays success (`unvis.c:94-104`);
//! without, stdin is the source. The HTTP/MIME flag families (`-e`, `-H`,
//! `-h`, `-m`) are later batches and are rejected.


#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

extern crate alloc;

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
#[path = "../bin_support.rs"]
mod support;

use minix_textfilter::vis::decode;

fn run() -> ! {
    let argv: Vec<String> = support::args();
    let mut operands: Vec<&str> = Vec::new();
    for arg in &argv[1..] {
        if arg.starts_with('-') && arg.len() > 1 {
            support::warn(b"unvis: flag families beyond the default decoder are later batches\n");
            support::terminate(1);
        } else {
            operands.push(arg.as_str());
        }
    }

    // `unvis.c:94-104`: each operand is opened and decoded into one output
    // stream; an unopenable operand warns and is skipped, and the exit status
    // still returns success (unlike vis, which flags it). No operands means
    // stdin is the source.
    let mut out: Vec<u8> = Vec::new();
    if operands.is_empty() {
        let input = support::read_stdin();
        out.extend(decode(core::str::from_utf8(&input).unwrap_or("")));
    } else {
        for operand in &operands {
            match support::read_file(operand) {
                Ok(data) => out.extend(decode(core::str::from_utf8(&data).unwrap_or(""))),
                Err(()) => support::warn(format!("unvis: {operand}\n").as_bytes()),
            }
        }
    }
    support::emit(&out);
    support::terminate(0);
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
