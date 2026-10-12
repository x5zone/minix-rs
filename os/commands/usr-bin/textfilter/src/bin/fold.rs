//! Minix-RS fold — wrap lines at a width.
//!
//! Ground truth: `minix3/usr.bin/fold/fold.c` (NetBSD). The deciding
//! half (the column rules for `\b`/`\r`/`\t`, the width break, the
//! last-space preference without `-s`) is the library's `fold` module.
//! With no operand the program folds standard input; with operands it folds
//! each file in turn (the column/word state restarting per file), and a file
//! it cannot open is fatal (C `err(1)`). The obsolete `-N` width form parses
//! too (`-w` is the modern spelling).

#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

extern crate alloc;

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
#[path = "../bin_support.rs"]
mod support;

use minix_sys::{close, open, read};
use minix_textfilter::fold::{fold_input, parse_fold_args, FoldArgs, FoldError, FoldOptions};

fn run() -> ! {
    let argv: Vec<String> = support::args();
    let refs: Vec<&str> = argv.iter().map(String::as_str).collect();
    let FoldArgs { options, operands } = match parse_fold_args(&refs[1..]) {
        Ok(parsed) => parsed,
        Err(FoldError::Usage) => usage(),
        Err(FoldError::IllegalWidth) => {
            support::warn(b"fold: illegal width value\n");
            support::terminate(1);
        }
    };

    // C `if (!*argv) fold(stdin); else for each argv freopen() … err(1) on
    // failure`: no operand folds stdin once; otherwise each file is folded in
    // turn (a fresh `fold_input` call restarts the column/word state), and an
    // unopenable file is fatal.
    if operands.is_empty() {
        fold_stream(support::STDIN, &options);
        support::terminate(0);
    }
    for operand in &operands {
        let fd = match open(operand, 0, 0) {
            Ok(fd) => fd,
            Err(_) => {
                support::warn(format!("fold: {operand}\n").as_bytes());
                support::terminate(1);
            }
        };
        fold_stream(fd, &options);
        //[SD19-gate] SD-19: close 后 fd 不再使用
        #[allow(clippy::let_underscore_must_use)]
        let _ = close(fd);
    }
    support::terminate(0)
}

/// Read a descriptor to the end and emit each folded line.
fn fold_stream(fd: i32, options: &FoldOptions) {
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
    for line in fold_input(&text, options) {
        let mut out = line.into_bytes();
        out.push(b'\n');
        support::emit(&out);
    }
}

fn usage() -> ! {
    support::warn(b"usage: fold [-b] [-s] [-w width | -width]\n");
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
