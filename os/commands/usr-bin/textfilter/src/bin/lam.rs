//! Minix-RS lam — laminate files side by side.
//!
//! Ground truth: `minix3/usr.bin/lam/lam.c` (NetBSD), usage
//! `lam [ -[fp] min.max ] [ -s sepstring ] [ -t c ] file ...` with
//! the capitalized `-F -P -S -T` forms persisting the option to the
//! streams that follow. Options bind to the stream operand that
//! follows them (the C mutates `ip` in place). The deciding half
//! (specs and row assembly) is the library's `lam` module; this
//! program parses the command line, reads the streams, and prints
//! the rows. Each operand is read into its stream: a bare `-` reads
//! stdin and anything else is opened (`lam.c:113-116`), with an
//! unopenable operand fatal (`errx(1)`, matching the C), and at least
//! one stream is required. The hosted-versus-target seams are the
//! echo template's (`os/commands/bin/fileops/src/bin/echo.rs`).

#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

extern crate alloc;

use alloc::string::String;
use alloc::vec::Vec;
#[path = "../bin_support.rs"]
mod support;

use minix_sys::read;
use minix_textfilter::lam::{LamSpec, MAX_STREAMS, laminate, parse_width, split_stream};

fn run() -> ! {
    let argv: Vec<Vec<u8>> = support::args_bytes();
    // Sticky capitals: -F -P -S -T carry to every stream after them.
    let mut sticky = [false; 4]; // [P, S, F, T]
    let mut specs: Vec<LamSpec> = Vec::new();
    let mut streams: Vec<Vec<String>> = Vec::new();
    let mut pending = LamSpec::default();
    let mut nofinalnl = false;

    let mut i = 1;
    while i < argv.len() {
        let arg: &[u8] = &argv[i];
        if arg == b"-" || arg.first() != Some(&b'-') || arg.len() == 1 {
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
            // `lam.c:113-116`: a bare `-` reads stdin, anything else is
            // fopen'd, and an unopenable operand is fatal (`errx(1)`).
            let mut bytes = Vec::new();
            if arg == b"-" {
                let mut chunk = [0u8; 4096];
                loop {
                    match read(support::STDIN, &mut chunk) {
                        Ok(0) => break,
                        Ok(n) => bytes.extend_from_slice(&chunk[..n]),
                        Err(_) => break,
                    }
                }
            } else {
                // The open call takes `&str` (registered `ARGV-BYTE-PATH-IN-LIBS`), so
                // the fold happens only at this point; what the message reads out is the
                // original bytes.
                let shown = String::from_utf8_lossy(arg).into_owned();
                match support::read_file(&shown) {
                    Ok(data) => bytes = data,
                    Err(_) => {
                        let mut line: Vec<u8> = b"lam: open ".to_vec();
                        line.extend_from_slice(arg);
                        line.push(b'\n');
                        support::warn(&line);
                        support::terminate(1);
                    }
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
        let flags: &[u8] = &arg[1..];
        let mut k = 0usize;
        while k < flags.len() {
            let raw = flags[k];
            let c = raw.to_ascii_lowercase();
            match c {
                b's' => {
                    match flag_value(&argv, &mut i, flags, &mut k) {
                        // The `-s` value goes into LamSpec.sep (the engine side takes a
                        // `String`, so invalid UTF-8 content reaches it folded): the fold
                        // happens only at this point, and the separator in the output is
                        // therefore the folded one.
                        Some(v) => {
                            pending.sep = String::from_utf8_lossy(v).into_owned();
                        }
                        None => need_after(b's'),
                    }
                    sticky[1] = raw.is_ascii_uppercase();
                }
                b't' => {
                    match flag_value(&argv, &mut i, flags, &mut k) {
                        Some(v) => {
                            pending.eol = *v.first().unwrap_or(&b'\n');
                        }
                        None => need_after(b't'),
                    }
                    sticky[3] = raw.is_ascii_uppercase();
                    nofinalnl = true;
                }
                b'f' | b'p' => {
                    let value = match flag_value(&argv, &mut i, flags, &mut k) {
                        Some(v) => v,
                        None => need_after(c),
                    };
                    match parse_width(value) {
                        Ok(w) => {
                            pending.width = Some(w);
                            if c == b'p' {
                                pending.pad = true;
                                sticky[0] = raw.is_ascii_uppercase();
                            } else {
                                sticky[2] = raw.is_ascii_uppercase();
                            }
                        }
                        Err(_) => {
                            let mut line: Vec<u8> = b"lam: invalid format string `".to_vec();
                            line.extend_from_slice(value);
                            line.extend_from_slice(b"`\n");
                            support::warn(&line);
                            support::terminate(1);
                        }
                    }
                }
                _ => {
                    let mut line: Vec<u8> = b"lam: What do you mean by -".to_vec();
                    line.push(raw);
                    line.extend_from_slice(b"?\n");
                    support::warn(&line);
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

/// The value of `-s`/`-t`/`-f`/`-p`: the rest of this token, or the next word — handed
/// back as the raw argv bytes.
fn flag_value<'a>(
    argv: &'a [Vec<u8>],
    i: &mut usize,
    flags: &'a [u8],
    k: &mut usize,
) -> Option<&'a [u8]> {
    *k += 1;
    if *k < flags.len() {
        return Some(&flags[*k..]);
    }
    *i += 1;
    argv.get(*i).map(Vec::as_slice)
}

fn need_after(c: u8) -> ! {
    let mut line: Vec<u8> = b"lam: Need string after -".to_vec();
    line.push(c);
    line.extend_from_slice(b"\n");
    support::warn(&line);
    support::terminate(1);
}

fn usage() -> ! {
    support::warn(b"\nUsage:  lam [ -[fp] min.max ] [ -s sepstring ] [ -t c ] file ...\n");
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
