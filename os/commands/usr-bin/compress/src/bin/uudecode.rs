//! Minix-RS `uudecode` — the doing half over `minix_compress::uu`.
//!
//! Ground truth: `minix3/usr.bin/uudecode/uudecode.c`. The byte decoding lives
//! in the library (`parse_begin`, `decode_uu_stream`, `decode_base64_stream`);
//! this program finds the `begin`/`begin-base64` header, decodes the body, and
//! writes the result — to standard output under `-p` (or when the header names
//! `/dev/stdout`/`-`), otherwise to a freshly created file whose mode is the
//! header's mode masked with 0666 (`uudecode.c:215-226`).
//!
//! Wired: `uudecode [-m] [-o outfile] [encoded-file ...]`. `-m` forces base64
//! (`uudecode.c:80-81`); `-o` overrides the embedded file name (`:83-84`); `-p`
//! routes to standard output (`:85-86`); with no file operand the archive is
//! read from standard input (`:120`). Exit status is 1 if any archive failed,
//! else 0 (`uudecode.c:119`).
//!
//! Declared residuals (honest gaps against C): the `~user/file` form needs
//! `getpwnam`, which is unwrapped, so a leading `~` is taken as a literal
//! component; the strict `"end"` / `"===="` footer check and the per-`readword`
//! `short file` diagnostics are not reproduced (a malformed body simply stops
//! the decoder); and real-machine file creation is gated by the writable-root
//! work (`E5`) exactly like the other write-side commands — the code and
//! its host-tested decoding are what this round delivers.

#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

extern crate alloc;

#[path = "../bin_support.rs"]
mod support;

use alloc::string::{String, ToString};
use alloc::vec::Vec;
use minix_compress::uu::{decode_base64_stream, decode_uu_stream, parse_begin};
use minix_sys::{chmod, close, open, read, write, Fd};

const O_WRONLY: i32 = 0x0000_0001;
const O_CREAT: i32 = 0x0000_0200;
const O_TRUNC: i32 = 0x0000_0400;

fn warn(message: &str) {
    let mut line = String::from("uudecode: ");
    line.push_str(message);
    line.push('\n');
    support::warn(line.as_bytes());
}

fn usage() -> ! {
    support::warn(b"usage: uudecode [-m] [-p | -o outfile] [encoded-file ...]\n");
    support::terminate(1);
}

/// Read a whole descriptor into a vector (`read` until EOF).
fn slurp(fd: Fd) -> Vec<u8> {
    let mut data = Vec::new();
    let mut chunk = [0u8; 4096];
    loop {
        match read(fd, &mut chunk) {
            Ok(0) => return data,
            Ok(n) => data.extend_from_slice(&chunk[..n]),
            Err(_) => return data,
        }
    }
}

/// Decode one uu/base64 archive from `input`, writing to standard output or to
/// the named file. Returns `true` on any failure (C's `decode()` return).
fn decode_archive(input: &[u8], force_base64: bool, output_override: Option<&str>) -> bool {
    // Find the begin line; everything after it is the body.
    let lines: Vec<&[u8]> = input.split(|&b| b == b'\n').collect();
    let start = match lines.iter().position(|&l| parse_begin(l).is_some()) {
        Some(i) => i,
        None => {
            warn("no \"begin\" line");
            return true;
        }
    };
    let header = parse_begin(lines[start]).unwrap();
    let base64 = force_base64 || header.base64;
    let body = &lines[start + 1..];
    let decoded = if base64 {
        decode_base64_stream(body)
    } else {
        decode_uu_stream(body)
    };

    // Resolve the destination: the -o override wins, else the header name.
    let dest_str: String = match output_override {
        Some(o) => o.to_string(),
        None => match core::str::from_utf8(&header.name) {
            Ok(s) => s.to_string(),
            Err(_) => {
                warn("invalid file name in header");
                return true;
            }
        },
    };
    if dest_str.is_empty() {
        warn("no file name");
        return true;
    }

    // /dev/stdout and - mean standard output (uudecode.c:203-214).
    if dest_str == "/dev/stdout" || dest_str == "-" {
        support::emit(&decoded);
        return false;
    }

    // Otherwise create the output file and apply the header's mode.
    let fd = match open(&dest_str, O_WRONLY | O_CREAT | O_TRUNC, 0o666) {
        Ok(fd) => fd,
        Err(_) => {
            warn(&alloc::format!("cannot create {}", dest_str));
            return true;
        }
    };
    if write(fd, &decoded).is_err() {
        let _ = close(fd);
        warn(&alloc::format!("write error on {}", dest_str));
        return true;
    }
    let _ = close(fd);
    let _ = chmod(&dest_str, header.mode & 0o666);
    false
}

/// The whole program body; both `main` forms call it and it never returns.
fn run() -> ! {
    let argv: Vec<String> = support::args();
    let args: Vec<&str> = argv.iter().map(String::as_str).collect();

    let mut force_base64 = false;
    let mut output_override: Option<String> = None;
    let mut files: Vec<String> = Vec::new();
    let mut i = 1;
    while i < args.len() {
        let a = args[i];
        if a == "--" {
            files.extend(args[i + 1..].iter().map(|s| s.to_string()));
            break;
        }
        if a.starts_with('-') && a.len() > 1 {
            match &a[1..] {
                "m" => force_base64 = true,
                "p" => output_override = Some("/dev/stdout".to_string()),
                "o" => {
                    i += 1;
                    output_override = Some(args.get(i).copied().unwrap_or("").to_string());
                }
                _ => usage(),
            }
            i += 1;
            continue;
        }
        files.push(a.to_string());
        i += 1;
    }

    let mut failed = false;
    if files.is_empty() {
        let input = slurp(support::STDIN);
        failed |= decode_archive(&input, force_base64, output_override.as_deref());
    } else {
        for f in &files {
            let fd = match open(f, 0, 0) {
                Ok(fd) => fd,
                Err(_) => {
                    warn(&alloc::format!("cannot open {}", f));
                    failed = true;
                    continue;
                }
            };
            let input = slurp(fd);
            let _ = close(fd);
            failed |= decode_archive(&input, force_base64, output_override.as_deref());
        }
    }

    support::terminate(if failed { 1 } else { 0 });
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
