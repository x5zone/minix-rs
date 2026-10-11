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
use minix_textfilter::fold::{fold_input, FoldOptions};

fn run() -> ! {
    let argv: Vec<String> = support::args();
    let mut options = FoldOptions::new(80);
    let mut operands: Vec<String> = Vec::new();
    let mut index = 1;
    while index < argv.len() {
        let arg = argv[index].as_str();
        if arg == "-b" {
            options.count_bytes = true;
        } else if arg == "-s" {
            options.split_words = true;
        } else if arg == "-w" {
            index += 1;
            if index >= argv.len() {
                usage();
            }
            set_width(&mut options, &argv[index]);
        } else if arg.starts_with('-')
            && arg.len() > 1
            && arg[1..].bytes().all(|b| b.is_ascii_digit())
        {
            // Obsolete `-N` width form.
            set_width(&mut options, &arg[1..]);
        } else if arg.starts_with('-') && arg.len() > 1 {
            usage();
        } else {
            operands.push(argv[index].clone());
        }
        index += 1;
    }
    if options.width == 0 {
        support::warn(b"fold: illegal width value\n");
        support::terminate(1);
    }

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

fn set_width(options: &mut FoldOptions, word: &str) {
    match word.parse::<usize>() {
        // A zero width would fold every character; the C's `atoi <= 0`
        // check rejects it.
        Ok(0) | Err(_) => {
            support::warn(b"fold: illegal width value\n");
        }
        Ok(width) => options.width = width,
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
