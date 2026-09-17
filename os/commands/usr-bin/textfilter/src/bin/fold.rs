//! Minix-RS fold — wrap lines at a width.
//!
//! Ground truth: `minix3/usr.bin/fold/fold.c` (NetBSD). The deciding
//! half (the column rules for `\b`/`\r`/`\t`, the width break, the
//! last-space preference without `-s`) is the library's `fold` module;
//! this program reads stdin, folds, and prints. File operands wait for
//! the gated open-existing call; the obsolete `-N` width form parses
//! too (`-w` is the modern spelling).

#[path = "../bin_support.rs"]
mod support;

use minix_textfilter::fold::{fold_input, FoldOptions};
use minix_sys::read;

fn main() {
    let argv: Vec<String> = std::env::args().collect();
    let mut options = FoldOptions::new(80);
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
        } else if arg.starts_with('-') && arg.len() > 1 && arg[1..].bytes().all(|b| b.is_ascii_digit())
        {
            // Obsolete `-N` width form.
            set_width(&mut options, &arg[1..]);
        } else {
            support::warn(b"fold: file operands wait for the open call (edge E-CMDSYSFACE)\n");
            support::terminate(1);
        }
        index += 1;
    }
    if options.width == 0 {
        support::warn(b"fold: illegal width value\n");
        support::terminate(1);
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
    let text = std::str::from_utf8(&input).unwrap_or("");
    for line in fold_input(text, &options) {
        let mut out = line.into_bytes();
        out.push(b'\n');
        support::emit(&out);
    }
    support::terminate(0);
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
