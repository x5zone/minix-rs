//! Minix-RS wc — count lines, words, and bytes.
//!
//! Ground truth: `minix3/usr.bin/wc/wc.c` (NetBSD), `wc [-l | -w | -c]`:
//! with no flags all three counts print, in lines-words-bytes order,
//! separated by single blanks; flags select a subset in that same order.
//! The counting engine is the library's streaming `Counter` (fed chunk by
//! chunk, no collection). The C output aligns counts into columns for
//! file operands; the stdin single-stream form here prints plain
//! blank-separated numbers. File operands wait for the gated
//! open-existing call.


#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

extern crate alloc;

use alloc::string::String;
use alloc::vec::Vec;
#[path = "../bin_support.rs"]
mod support;

use minix_textfilter::count::Counter;
use minix_sys::read;

fn run() -> ! {
    let argv: Vec<String> = support::args();
    let mut lines = false;
    let mut words = false;
    let mut bytes = false;
    let mut any_flag = false;
    for arg in &argv[1..] {
        if arg.starts_with('-') && arg.len() > 1 {
            any_flag = true;
            for flag in arg[1..].bytes() {
                match flag {
                    b'l' => lines = true,
                    b'w' => words = true,
                    b'c' => bytes = true,
                    _ => {
                        support::emit(b"usage: wc [-l | -w | -c]\n");
                        support::terminate(1);
                    }
                }
            }
        } else {
            support::emit(b"wc: file operands wait for the open call (edge E-CMDSYSFACE)\n");
            support::terminate(1);
        }
    }
    if !any_flag {
        lines = true;
        words = true;
        bytes = true;
    }

    let mut counter = Counter::new();
    let mut chunk = [0u8; 4096];
    loop {
        match read(support::STDIN, &mut chunk) {
            Ok(0) => break,
            Ok(count) => {
                if counter.add(&chunk[..count]).is_err() {
                    support::terminate(1);
                }
            }
            // Without a kernel the transport answers explicitly; end of
            // input is the only honest reading of it here.
            Err(_) => break,
        }
    }
    let totals = counter.finish();

    let mut out = [0u8; 64];
    let mut length = 0;
    let mut first = true;
    let mut put = |value: u64| {
        if !first {
            out[length] = b' ';
            length += 1;
        }
        first = false;
        length += support::utoa_u64(value, &mut out[length..]);
    };
    if lines {
        put(totals.lines);
    }
    if words {
        put(totals.words);
    }
    if bytes {
        put(totals.bytes);
    }
    out[length] = b'\n';
    length += 1;
    support::emit(&out[..length]);
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
