//! Minix-RS wc — count lines, words, and bytes.
//!
//! Ground truth: `minix3/usr.bin/wc/wc.c` (NetBSD). With no operand the
//! standard input is counted and printed nameless; with operands each file is
//! counted and printed with its name, and a `total` line is appended when more
//! than one operand is given. A file that cannot be opened warns and is left
//! out (the exit status stays nonzero). `wc.c`'s `print_counts` renders every
//! count as ` %7llu` (a blank then a seven-column right-justified field) in the
//! order newlines, words, the byte-or-character column, longest; the byte/char
//! column is a single slot (the two flags are mutually exclusive), and the
//! selection plus the fixed column width live in the tested
//! [`minix_textfilter::count::format_counts`].
//!
//! Flags `-l -w -c -m -L` select the columns; with no flag the default is
//! lines, words, and bytes (C `doline = doword = dobyte = true`). `-c` counts
//! bytes, `-m` counts UTF-8 characters; later of the two wins, as in C. The
//! streaming `Counter` engine is fed chunk by chunk, so a whole file is never
//! held in memory.

#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

extern crate alloc;

#[path = "../bin_support.rs"]
mod support;

use alloc::format;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use minix_sys::{close, open, read};
use minix_textfilter::count::{format_counts, CharField, Counter};

/// The count chunk size (the C `wc` reads in `MAXBSIZE` steps).
const CHUNK: usize = 4096;

fn usage() -> ! {
    support::emit(b"usage: wc [-L | -m | -l | -w | -c] [file ...]\n");
    support::terminate(1);
}

/// Count one open descriptor into a fresh counter and fold it into `total`
/// (C accumulates `tlinect += linect`, …, `tlongest = max`).
fn count_into(fd: i32, total: &mut Counter) -> Result<Counter, ()> {
    let mut counter = Counter::new();
    let mut chunk = [0u8; CHUNK];
    loop {
        match read(fd, &mut chunk) {
            Ok(0) => break,
            Ok(n) => {
                if counter.add(&chunk[..n]).is_err() {
                    return Err(());
                }
            }
            Err(_) => return Err(()),
        }
    }
    let done = counter.finish();
    total.lines += done.lines;
    total.words += done.words;
    total.bytes += done.bytes;
    total.chars += done.chars;
    if done.longest > total.longest {
        total.longest = done.longest;
    }
    Ok(done)
}

/// Format one count result (via the tested `format_counts`) and write it out.
fn emit_counts(show: (bool, bool, CharField, bool), counts: &Counter, name: Option<&[u8]>) {
    let mut out: Vec<u8> = vec![0u8; 4096];
    match format_counts(&mut out, show.0, show.1, show.2, show.3, counts, name) {
        Ok(n) => support::emit(&out[..n]),
        Err(_) => support::terminate(1),
    }
}

/// The whole program body; both `main` forms call it and it never returns.
fn run() -> ! {
    let argv: Vec<String> = support::args();
    let args: Vec<&str> = argv.iter().map(String::as_str).collect();

    let mut doline = false;
    let mut doword = false;
    let mut dobyte = false;
    let mut dochar = false;
    let mut dolongest = false;
    let mut any_flag = false;
    let mut i = 1;
    while i < args.len() {
        let a = args[i];
        if a == "--" {
            i += 1;
            break;
        }
        if a.len() > 1 && a.starts_with('-') {
            any_flag = true;
            for flag in a[1..].bytes() {
                match flag {
                    b'l' => doline = true,
                    b'w' => doword = true,
                    b'c' => {
                        dobyte = true;
                        dochar = false;
                    }
                    b'm' => {
                        dochar = true;
                        dobyte = false;
                    }
                    b'L' => dolongest = true,
                    _ => usage(),
                }
            }
            i += 1;
        } else {
            break;
        }
    }
    if !any_flag {
        doline = true;
        doword = true;
        dobyte = true;
    }
    // C's single byte-or-character column: -m (dochar) wins the value; else -c.
    let char_field = if dochar {
        CharField::Chars
    } else if dobyte {
        CharField::Bytes
    } else {
        CharField::Off
    };
    let show = (doline, doword, char_field, dolongest);
    let operands: Vec<&str> = args[i..].to_vec();

    let mut status = 0;
    let mut total = Counter::new();

    if operands.is_empty() {
        // Nameless standard input (C: cnt(NULL), printed with no filename).
        match count_into(support::STDIN, &mut total) {
            Ok(counts) => emit_counts(show, &counts, None),
            Err(_) => {
                support::warn(b"wc: read error\n");
                support::terminate(1);
            }
        }
        support::terminate(0);
    }

    for operand in &operands {
        let fd = match open(operand, 0, 0) {
            Ok(fd) => fd,
            Err(_) => {
                // C warns and skips the file; no output line for it.
                support::warn(format!("wc: {operand}\n").as_bytes());
                status = 1;
                continue;
            }
        };
        let result = count_into(fd, &mut total);
        #[allow(clippy::let_underscore_must_use)]
        let _ = close(fd);
        match result {
            Ok(counts) => emit_counts(show, &counts, Some(operand.as_bytes())),
            Err(_) => {
                support::warn(format!("wc: {operand}\n").as_bytes());
                status = 1;
            }
        }
    }

    // C: `dototal = (argc > 1)` — the total line appears only past one operand.
    if operands.len() > 1 {
        emit_counts(show, &total, Some(b"total"));
    }
    support::terminate(status)
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
