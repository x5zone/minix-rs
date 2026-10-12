//! Minix-RS expand — tabs to spaces.
//!
//! Ground truth: `minix3/usr.bin/expand/expand.c` (NetBSD). The deciding
//! half (stop lists, the four expansion rules, backspace and newline
//! column tracking) is the library's `expand` module. With no operand the
//! program expands standard input; with operands it expands each file in
//! turn, the tab column restarting per file (C `column = 0` per `freopen`),
//! and a file it cannot open is fatal (C `err(EXIT_FAILURE)`). The `-t`
//! tab-stop list and the obsolete `-N` stop form both parse.

#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

extern crate alloc;

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
#[path = "../bin_support.rs"]
mod support;

use minix_sys::{close, open, read};
use minix_textfilter::expand::{expand, parse_expand_args, ExpandArgs, ExpandError};

fn run() -> ! {
    let argv: Vec<String> = support::args();
    let refs: Vec<&str> = argv.iter().map(String::as_str).collect();
    let ExpandArgs { stops, operands } = match parse_expand_args(&refs[1..]) {
        Ok(parsed) => parsed,
        Err(ExpandError::Usage) => usage(),
        Err(ExpandError::IllegalStops) => {
            support::warn(b"expand: bad tab stop spec\n");
            support::terminate(1);
        }
    };

    // C `do { if (argc>0) freopen(argv[0]); column = 0; getchar… } while(argc>0)`:
    // no operand reads stdin once; otherwise each file is expanded with its own
    // tab-column starting fresh (a fresh `expand` call resets the column), and an
    // unopenable file is a fatal error (C `err(EXIT_FAILURE)`).
    if operands.is_empty() {
        expand_stream(support::STDIN, &stops);
        support::terminate(0);
    }
    for operand in &operands {
        let fd = match open(operand, 0, 0) {
            Ok(fd) => fd,
            Err(_) => {
                support::warn(format!("expand: Cannot open `{operand}'\n").as_bytes());
                support::terminate(1);
            }
        };
        expand_stream(fd, &stops);
        //[SD19-gate] SD-19: close 后 fd 不再使用
        #[allow(clippy::let_underscore_must_use)]
        let _ = close(fd);
    }
    support::terminate(0)
}

/// Read a descriptor to the end and emit its expanded form.
fn expand_stream(fd: i32, stops: &[usize]) {
    let mut input = Vec::new();
    let mut chunk = [0u8; 4096];
    loop {
        match read(fd, &mut chunk) {
            Ok(0) => break,
            Ok(count) => input.extend_from_slice(&chunk[..count]),
            Err(_) => break,
        }
    }
    let text = String::from_utf8_lossy(&input);
    support::emit(expand(&text, stops).as_bytes());
}

fn usage() -> ! {
    support::warn(b"usage: expand [-t tablist]\n");
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
