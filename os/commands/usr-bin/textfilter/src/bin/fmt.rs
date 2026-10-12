//! Minix-RS `fmt` — the doing half over `minix_textfilter::fmt`.
//!
//! Ground truth: `minix3/usr.bin/fmt/fmt.c`. The reflow algorithm lives in the
//! library (`fmt_bytes`); this program parses the flags, reads the file operands
//! (standard input when none) concatenating them in order as C does with its
//! shared globals across files, and writes the reflowed text to stdout.
//!
//! Wired: `-g goal`, `-m`/`-w max` (both set the max length, as in C), `-r`
//! (raw), `-C` (center, handed to the library's `fmt -C` branch), the old BSD
//! positional `[goal [max]]` compatibility form, and the `max > goal` sanity
//! check (`fmt.c:152-155`). Declared residual: the mail-header heuristic (see
//! the library doc).

#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

extern crate alloc;

#[path = "../bin_support.rs"]
mod support;

use alloc::string::String;
use alloc::vec::Vec;
use minix_textfilter::fmt::{FmtOptions, fmt_bytes};

fn fail(message: &str) -> ! {
    let mut line = String::from("fmt: ");
    line.push_str(message);
    line.push('\n');
    support::warn(line.as_bytes());
    support::terminate(1)
}

/// Parse a decimal byte length the way C's `getnum` does for whole-number
/// operands; anything non-numeric is `None`.
fn parse_len(s: &str) -> Option<usize> {
    if s.is_empty() || !s.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    s.parse::<usize>().ok()
}

/// The whole program body; both `main` forms call it and it never returns.
fn run() -> ! {
    let argv: Vec<String> = support::args();
    let args: Vec<&str> = argv.iter().map(String::as_str).collect();

    let mut opts = FmtOptions::default();
    let mut compat = true;
    let mut i = 1;
    macro_rules! value {
        () => {{
            i += 1;
            args.get(i).copied().unwrap_or("")
        }};
    }

    // Leading option letters (getopt stops at the first non-option operand).
    while i < args.len() {
        let a = args[i];
        if a == "--" {
            i += 1;
            break;
        }
        if !a.starts_with('-') || a.len() == 1 {
            break; // a bare "-" names stdin; anything else is an operand
        }
        let flag = &a[1..];
        match flag {
            "g" => {
                opts.goal = parse_len(value!()).unwrap_or_else(|| fail("bad goal number"));
                compat = false;
            }
            "m" | "w" => {
                opts.max = parse_len(value!()).unwrap_or_else(|| fail("bad max number"));
                compat = false;
            }
            "r" => opts.raw = true,
            "C" => opts.center = true,
            _ => fail("unknown option"),
        }
        i += 1;
    }

    // Old BSD positional form: `fmt [goal [max]] [files]`, only when no flag
    // cleared compatibility and the leading operands are plain numbers.
    if compat {
        let positional_goal = args.get(i).and_then(|&g| parse_len(g));
        if let Some(goal) = positional_goal {
            opts.goal = goal;
            i += 1;
            if let Some(max) = args.get(i).and_then(|&m| parse_len(m)) {
                opts.max = max;
                i += 1;
            }
        }
    }

    if opts.max <= opts.goal {
        fail("max length must be greater than goal length");
    }

    // Read every operand (a `-` names standard input); concatenate in order.
    let mut input: Vec<u8> = Vec::new();
    if i >= args.len() {
        input = read_stdin();
    } else {
        for f in &args[i..] {
            if *f == "-" {
                input.extend(read_stdin());
            } else {
                input.extend(support::read_file(f).unwrap_or_else(|_| fail("cannot open input")));
            }
        }
    }

    support::emit(&fmt_bytes(&input, &opts));
    support::terminate(0);
}

fn read_stdin() -> Vec<u8> {
    let mut out = Vec::new();
    let mut chunk = [0u8; 4096];
    loop {
        match minix_sys::read(support::STDIN, &mut chunk) {
            Ok(0) => break,
            Ok(n) => out.extend_from_slice(&chunk[..n]),
            Err(_) => break,
        }
    }
    out
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
