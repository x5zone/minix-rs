//! Minix-RS compress — the doing half over `minix_compress::lzw`.
//!
//! Ground truth: `minix3/minix/commands/compress/compress.c` (option loop
//! at 354-430, `.Z` framing at 151-157 and 759, `uncompress` name at 334).
//! The deciding half (the LZW core and its width-prefixed stream) lives in
//! the library; this program maps files to `.Z` frames, translates the
//! command line, and writes bytes through `minix_sys`.
//!
//! Declared boundary (11-compress-archive.md §5): the Rust LZW tops out at
//! [`MAX_MAXBITS`] = 12 (4096 codes), so `.Z` frames from a 16-bit C
//! compressor are rejected at [`unframe`] with an honest failure instead
//! of a corrupt answer; `-n` (headerless legacy streams) is not accepted.
//! When no_std program images land, the seams (argv, terminate, whole
//! files through read/write) swap exactly like the other commands'.


#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

extern crate alloc;

use alloc::string::{String, ToString};
use alloc::vec::Vec;
use alloc::format;
use alloc::vec;

#[path = "../bin_support.rs"]
mod support;
use minix_compress::lzw::{compress, decompress, frame_header, unframe, DEFAULT_MAXBITS};
use minix_sys::{read, write, Fd};

const STDIN: Fd = 0;
const STDOUT: Fd = 1;
const STDERR: Fd = 2;


fn fail(message: &str) -> ! {
    let _ = write(STDERR, message.as_bytes());
    let _ = write(STDERR, b"\n");
    support::terminate(1)
}

/// Reads a whole stream into a vector (`read` until EOF).
fn slurp(fd: Fd) -> Vec<u8> {
    let mut data = Vec::new();
    let mut chunk = [0u8; 4096];
    loop {
        match read(fd, &mut chunk) {
            Ok(0) => return data,
            Ok(n) => data.extend_from_slice(&chunk[..n]),
            Err(_) => fail("read failed"),
        }
    }
}

fn emit(fd: Fd, data: &[u8]) {
    if write(fd, data).unwrap_or(0) != data.len() {
        fail("write failed");
    }
}

fn run() -> ! {
    let argv: Vec<String> = support::args();
    let args: Vec<&str> = argv.iter().map(String::as_str).collect();
    // `strcmp(cp, "uncompress") == 0` implies -d (`compress.c:334`).
    let invoked = args.first().copied().unwrap_or("compress");
    let program = invoked.rsplit('/').next().unwrap_or(invoked);
    let mut decompressing = program == "uncompress";

    let mut to_stdout = false;
    let mut force = false;
    let mut verbose = false;
    let mut maxbits = DEFAULT_MAXBITS;
    let mut operands: Vec<&str> = Vec::new();
    let mut at = 1;
    while at < args.len() {
        let arg = args[at];
        if arg == "--" {
            operands.extend(args[at + 1..].iter());
            break;
        }
        if let Some(rest) = arg.strip_prefix('-') {
            let mut chars = rest.chars();
            while let Some(c) = chars.next() {
                match c {
                    'd' => decompressing = true,
                    'c' => to_stdout = true,
                    'f' => force = true,
                    'v' => verbose = true,
                    // `-n` (headerless legacy streams) is declared out of
                    // reach; `-V`/`-r` are C debug/recursive faces the
                    // library has no answer for either.
                    'n' | 'V' | 'r' => fail("option not wired"),
                    'b' => {
                        let digits: String = chars.by_ref().collect();
                        let text = if digits.is_empty() {
                            at += 1;
                            args.get(at).copied().unwrap_or("")
                        } else {
                            &digits
                        };
                        maxbits = match text.parse::<u8>() {
                            Ok(v @ 9..=12) => v,
                            _ => fail("bad maxbits (9..=12 supported)"),
                        };
                        break;
                    }
                    _ => fail("unknown option"),
                }
            }
            at += 1;
        } else {
            operands.push(arg);
            at += 1;
        }
    }

    // Operands: none means standard input to standard output; one names a
    // file (`.Z` appended for compress, stripped for uncompress).
    let (source, target_name, delete_source): (Option<String>, Option<String>, bool) =
        match operands.as_slice() {
            [] => (None, None, false),
            [file] => {
                if decompressing {
                    let text = *file;
                    if !text.ends_with(".Z") {
                        fail("invalid suffix: expected .Z");
                    }
                    let plain = &text[..text.len() - 2];
                    if !force && support::path_exists(plain) {
                        fail("already exists; not overwritten");
                    }
                    (Some(text.to_string()), Some(plain.to_string()), true)
                } else {
                    if !force && support::path_exists(&format!("{file}.Z")) {
                        fail("already exists; not overwritten");
                    }
                    if to_stdout {
                        (Some(file.to_string()), None, false)
                    } else {
                        (Some(file.to_string()), Some(format!("{file}.Z")), true)
                    }
                }
            }
            _ => fail("at most one operand"),
        };

    let input = match &source {
        Some(path) => {
            let fd = minix_sys::open(path, 0, 0).unwrap_or_else(|_| fail("cannot open input"));
            let data = slurp(fd);
            let _ = minix_sys::close(fd);
            data
        }
        None => slurp(STDIN),
    };

    let framed: Vec<u8> = if decompressing {
        let (width, stream) = unframe(&input).unwrap_or_else(|_| fail("corrupt input"));
        let mut replay = Vec::with_capacity(stream.len() + 1);
        replay.push(width);
        replay.extend_from_slice(stream);
        let mut out = vec![0u8; input.len() * 8 + 64];
        match decompress(&replay, &mut out) {
            Ok(used) => out.truncate(used),
            Err(_) => fail("corrupt input"),
        }
        out
    } else {
        let mut inner = vec![0u8; input.len() * 2 + 64];
        let used = match compress(&input, maxbits, &mut inner) {
            Ok(used) => used,
            Err(_) => fail("compression would expand the input"),
        };
        let mut framed = Vec::with_capacity(used + 3);
        framed.extend_from_slice(&frame_header(maxbits));
        // The internal stream repeats the width in its first byte; the .Z
        // header carries it, so the shell splices it out.
        framed.extend_from_slice(&inner[1..used]);
        framed
    };

    match &target_name {
        Some(path) => {
            let flags = 1 | 0x40 | 0x200; // O_WRONLY | O_CREAT | O_TRUNC
            let fd = minix_sys::open(path, flags, 0o644)
                .unwrap_or_else(|_| fail("cannot open output"));
            emit(fd, &framed);
            let _ = minix_sys::close(fd);
            if let (Some(src), true) = (&source, delete_source) {
                support::remove_file(src).unwrap_or_else(|_| fail("cannot remove source"));
            }
            if verbose {
                let saved = if !decompressing && !input.is_empty() {
                    100.0 - (framed.len() as f64 / input.len() as f64) * 100.0
                } else {
                    0.0
                };
                let source_text = source.clone().unwrap_or_default();
                let _ = write(
                    STDERR,
                    format!("{source_text}: {saved:.1}% -- replaced").as_bytes(),
                );
                let _ = write(STDERR, b"\n");
            }
        }
        None => emit(STDOUT, &framed),
    }
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
