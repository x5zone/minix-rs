//! Minix-RS lam — laminate files side by side.
//!
//! Ground truth: `minix3/usr.bin/lam/lam.c` (NetBSD), usage
//! `lam [ -[fp] min.max ] [ -s sepstring ] [ -t c ] file ...` with
//! the capitalized `-F -P -S -T` forms persisting the option to the
//! streams that follow. Options bind to the stream operand that
//! follows them (the C mutates `ip` in place). The deciding half
//! (specs and row assembly) is the library's `lam` module; this
//! program parses the command line, reads the streams, and prints
//! the rows. Named file operands are rejected until the open-existing
//! seam lands (edge E-CMDSYSFACE); `-` (stdin) works, and at least
//! one stream is required. The hosted-versus-target seams are the
//! echo template's (`os/commands/bin/fileops/src/bin/echo.rs`).


#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

extern crate alloc;
use alloc::format;

use alloc::string::String;
use alloc::vec::Vec;
#[path = "../bin_support.rs"]
mod support;

use minix_textfilter::lam::{laminate, parse_width, split_stream, LamSpec, MAX_STREAMS};
use minix_sys::read;

fn run() -> ! {
    let argv: Vec<String> = support::args();
    // Sticky capitals: -F -P -S -T carry to every stream after them.
    let mut sticky = [false; 4]; // [P, S, F, T]
    let mut specs: Vec<LamSpec> = Vec::new();
    let mut streams: Vec<Vec<String>> = Vec::new();
    let mut pending = LamSpec::default();
    let mut nofinalnl = false;

    let mut i = 1;
    while i < argv.len() {
        let arg = argv[i].clone();
        if arg == "-" || !arg.starts_with('-') || arg.len() == 1 {
            if arg != "-" {
                support::warn(
                    b"lam: file operands wait on the open-existing seam (edge E-CMDSYSFACE)\n",
                );
                support::terminate(1);
            }
            if specs.len() >= MAX_STREAMS {
                support::warn(b"lam: too many input files\n");
                support::terminate(1);
            }
            // Inheritance defaults (lam.c:130-135): the previous
            // stream's values when the sticky capital is set.
            if let Some(prev) = specs.last().cloned() {
                if sticky[1] {
                    pending.sep = prev.sep;
                }
                if sticky[0] || sticky[2] {
                    pending.width = prev.width;
                }
                if sticky[3] {
                    pending.eol = prev.eol;
                }
            }
            let mut bytes = Vec::new();
            let mut chunk = [0u8; 4096];
            loop {
                match read(support::STDIN, &mut chunk) {
                    Ok(0) => break,
                    Ok(n) => bytes.extend_from_slice(&chunk[..n]),
                    Err(_) => break,
                }
            }
            let eol = pending.eol;
            specs.push(pending.clone());
            streams.push(split_stream(&bytes, eol));
            pending = LamSpec::default();
            i += 1;
            continue;
        }
        // An option: applies to the next stream operand.
        let chars: Vec<char> = arg.chars().skip(1).collect();
        let mut k = 0usize;
        while k < chars.len() {
            let raw = chars[k];
            let c = raw.to_ascii_lowercase();
            match c {
                's' => {
                    match flag_value(&argv, &mut i, &chars, &mut k) {
                        Some(v) => pending.sep = v,
                        None => need_after('s'),
                    }
                    sticky[1] = raw.is_ascii_uppercase();
                }
                't' => {
                    match flag_value(&argv, &mut i, &chars, &mut k) {
                        Some(v) => {
                            pending.eol = *v.as_bytes().first().unwrap_or(&b'\n');
                        }
                        None => need_after('t'),
                    }
                    sticky[3] = raw.is_ascii_uppercase();
                    nofinalnl = true;
                }
                'f' | 'p' => {
                    let value = match flag_value(&argv, &mut i, &chars, &mut k) {
                        Some(v) => v,
                        None => need_after(c),
                    };
                    match parse_width(&value) {
                        Ok(w) => {
                            pending.width = Some(w);
                            if c == 'p' {
                                pending.pad = true;
                                sticky[0] = raw.is_ascii_uppercase();
                            } else {
                                sticky[2] = raw.is_ascii_uppercase();
                            }
                        }
                        Err(_) => {
                            support::warn(
                                format!("lam: invalid format string `{}`\n", value).as_bytes(),
                            );
                            support::terminate(1);
                        }
                    }
                }
                _ => {
                    support::warn(format!("lam: What do you mean by -{}?\n", raw).as_bytes());
                    usage();
                }
            }
            k += 1;
        }
        i += 1;
    }

    if specs.is_empty() {
        support::warn(b"lam - laminate files\n");
        usage();
    }

    let rows = laminate(&streams, &specs);
    let mut out: Vec<u8> = Vec::new();
    for row in &rows {
        out.extend_from_slice(row.as_bytes());
        if !nofinalnl {
            out.push(b'\n');
        }
    }
    support::emit(&out);
    support::terminate(0);
}

fn flag_value(argv: &[String], i: &mut usize, chars: &[char], k: &mut usize) -> Option<String> {
    *k += 1;
    if *k < chars.len() {
        return Some(chars[*k..].iter().collect());
    }
    *i += 1;
    argv.get(*i).cloned()
}

fn need_after(c: char) -> ! {
    support::warn(format!("lam: Need string after -{}\n", c).as_bytes());
    support::terminate(1);
}

fn usage() -> ! {
    support::warn(
        b"\nUsage:  lam [ -[fp] min.max ] [ -s sepstring ] [ -t c ] file ...\n",
    );
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
