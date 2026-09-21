//! Minix-RS tsort — topological sort.
//!
//! Ground truth: `minix3/usr.bin/tsort/tsort.c` (NetBSD). The deciding
//! half (pair parsing, the removal passes, cycle detection and the
//! one-node cycle break) is the library's `tsort` module; this program
//! reads whitespace-separated token pairs from stdin, prints the order,
//! and reports cycles on standard error ("cycle in data" plus the
//! members — suppressed by `-q`). An odd token count leaves with status
//! 1 ("odd data count", tsort.c:183).


#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

extern crate alloc;

use alloc::string::String;
use alloc::vec::Vec;
#[path = "../bin_support.rs"]
mod support;

use minix_textfilter::tsort::{tsort, TsortError};
use minix_sys::read;

fn run() -> ! {
    let argv: Vec<String> = support::args();
    let mut quiet = false;
    for arg in &argv[1..] {
        match arg.as_str() {
            "-q" => quiet = true,
            _ => {
                support::warn(b"usage: tsort [-lq] [file]\n");
                support::terminate(2);
            }
        }
    }

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
    let tokens: Vec<&str> = text.split_whitespace().collect();
    let result = match tsort(&tokens) {
        Ok(result) => result,
        Err(_) => {
            support::warn(b"tsort: odd data count\n");
            support::terminate(1);
        }
    };
    for cycle in &result.cycles {
        if quiet {
            continue;
        }
        let mut message = b"cycle in data\n".to_vec();
        for member in &cycle.members {
            message.extend_from_slice(member.as_bytes());
            message.push(b'\n');
        }
        support::warn(&message);
    }
    for name in &result.order {
        let mut out = name.as_bytes().to_vec();
        out.push(b'\n');
        support::emit(&out);
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
