//! Minix-RS `nl` — the doing half over `minix_textfilter::nl`.
//!
//! Ground truth: `minix3/usr.bin/nl/nl.c`. Numbering, formats and the page
//! delimiter are the library (`nl_lines`); this program parses the flags,
//! reads the file operands (standard input when none), strips newlines into
//! lines, and emits the numbered result.
//!
//! Wired: the three logical-page sections with their numbering types via
//! `-b`/`-h`/`-f` (a/t/n), `-v`/`-i`/`-l` (start, step, width), `-n ln|rn|rz`
//! (format), `-p` (do not restart numbering at a `\:` page delimiter). Declared
//! residual: `number_regex` (`-b p…`), `-d` delimiter, `-s` separator, `-w`
//! blank-run counts — requesting them reports "option not wired" and exits 1.

#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

extern crate alloc;

#[path = "../bin_support.rs"]
mod support;

use alloc::string::{String, ToString};
use alloc::vec::Vec;
use minix_textfilter::nl::{NlOptions, NumberType, NumFormat, nl_lines};

fn fail(message: &str) -> ! {
    let mut line = String::from("nl: ");
    line.push_str(message);
    line.push('\n');
    support::warn(line.as_bytes());
    support::terminate(1)
}

/// The whole program body; both `main` forms call it and it never returns.
fn run() -> ! {
    let argv: Vec<String> = support::args();
    let args: Vec<&str> = argv.iter().map(String::as_str).collect();

    let mut opts = NlOptions::default();
    let mut files: Vec<String> = Vec::new();
    let mut i = 1;
    macro_rules! value {
        () => {{
            i += 1;
            args.get(i).copied().unwrap_or("")
        }};
    }
    while i < args.len() {
        let a = args[i];
        if a == "--" {
            files.extend(args[i + 1..].iter().map(|s| s.to_string()));
            break;
        }
        if a == "-" || !a.starts_with('-') || a.len() == 1 {
            files.push(a.to_string());
            i += 1;
            continue;
        }
        let flag = &a[1..];
        match flag {
            "p" => opts.restart = false,
            "v" => {
                opts.startnum = value!().parse().unwrap_or(0);
            }
            "i" => {
                opts.incr = value!().parse().unwrap_or(0);
                if opts.incr == 0 {
                    fail("-i increment must be non-zero");
                }
            }
            "l" => {
                opts.width = value!().parse().unwrap_or(0);
            }
            "n" => {
                opts.format = match value!() {
                    "ln" => NumFormat::Left,
                    "rn" => NumFormat::Right,
                    "rz" => NumFormat::RightZero,
                    _ => fail("-n must be ln, rn or rz"),
                };
            }
            "b" | "h" | "f" => {
                let vt = value!();
                let kind = match vt {
                    "a" => NumberType::All,
                    "n" => NumberType::None,
                    "t" => NumberType::Nonempty,
                    _ if vt.starts_with('p') => fail("number_regex (-b/-h/-f p…) not wired"),
                    _ => fail("bad numbering type (want a, n or t)"),
                };
                match flag {
                    "b" => opts.body = kind,
                    "h" => opts.header = kind,
                    _ => opts.footer = kind,
                }
            }
            "d" | "s" | "w" => {
                let _ = value!();
                fail("option not wired (residual: -d/-s/-w)");
            }
            _ => fail("unknown option"),
        }
        i += 1;
    }

    // Read every operand (a `-` names standard input); concatenate in order.
    let mut input: Vec<u8> = Vec::new();
    if files.is_empty() {
        input = read_stdin();
    } else {
        for f in &files {
            if f == "-" {
                input.extend(read_stdin());
            } else {
                input.extend(support::read_file(f).unwrap_or_else(|_| fail("cannot open input")));
            }
        }
    }

    let mut lines: Vec<&[u8]> = input.split(|&b| b == b'\n').collect();
    if lines.last() == Some(&&b""[..]) {
        lines.pop();
    }

    let out = nl_lines(&lines, &opts);
    let mut rendered = Vec::new();
    for line in &out {
        rendered.extend_from_slice(line);
        rendered.push(b'\n');
    }
    support::emit(&rendered);
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
