//! Minix-RS paste — merge or join lines.
//!
//! Ground truth: `minix3/usr.bin/paste/paste.c` (NetBSD): `-s` joins
//! each input's lines with the cycling delimiters, parallel mode merges
//! across inputs; the delimiter default is a tab and the list cycles
//! per gap. File operands wait for the gated open-existing call, so
//! this build serves one stdin stream (serial join, or pass-through in
//! parallel mode — a single stream has nothing to merge with).


#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

extern crate alloc;

use alloc::string::String;
use alloc::vec::Vec;
#[path = "../bin_support.rs"]
mod support;

use minix_textfilter::paste::{join_serial, parse};
use minix_sys::read;

fn run() -> ! {
    let argv: Vec<String> = support::args();
    let words: Vec<&str> = argv[1..].iter().map(String::as_str).collect();
    let options = match parse(&words) {
        Ok(options) => options,
        Err(_) => {
            support::warn(b"paste: [-s] [-d delimiters] file ...\n");
            support::terminate(2);
        }
    };

    let mut input = Vec::new();
    let mut chunk = [0u8; 4096];
    loop {
        match read(support::STDIN, &mut chunk) {
            Ok(0) => break,
            Ok(count) => input.extend_from_slice(&chunk[..count]),
            Err(_) => break,
        }
    }
    let text = core::str::from_utf8(&input).unwrap_or("");
    let mut lines: Vec<&str> = text.split('\n').collect();
    if options.serial {
        if lines.last() == Some(&"") {
            lines.pop();
        }
        support::emit(join_serial(&lines, &options).as_bytes());
    } else {
        // Parallel mode with one stream: no second stream to merge.
        for line in lines {
            if line.is_empty() {
                continue;
            }
            let mut out = line.as_bytes().to_vec();
            out.push(b'\n');
            support::emit(&out);
        }
    }
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
