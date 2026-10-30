//! Minix-RS patch — apply a diff file to an original.
//!
//! Ground truth: `minix3/usr.bin/patch/patch.c` + `pch.c`, usage
//! `patch [-bceEflnNrstuv] [-F fuzz] [origfile [patchfile]]`. The
//! deciding half (unified/normal hunk parsing, offset search with
//! fuzz, apply walk) is the library's `patch` module. The patch text
//! is read from stdin or `-i`; the target file operand waits on the
//! open-existing seam (edge E-CMDSYSFACE), so hosted runs exercise
//! the engine through `-o` writing nothing — in fact this shell only
//! reports hunk outcomes to stderr and exits with the C's status
//! convention (0 = all applied, 1 = some hunk failed, 2 = trouble).
//! Context-format hunks, ed scripts, reverse (`-R`) and reject files
//! are registered corners. The hosted-versus-target seams are the
//! echo template's (`os/commands/bin/fileops/src/bin/echo.rs`).

#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

extern crate alloc;

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
#[path = "../bin_support.rs"]
mod support;

use minix_sys::read;
use minix_textfilter::patch::{PatchOptions, apply_patch, parse_patch};

/// `-F` 的取值判据照 C 的 `atoi` 只认 ASCII 数字；折前折后同判，只改报文里的字节。
fn parse_usize(bytes: &[u8]) -> Option<usize> {
    if bytes.is_empty() {
        return None;
    }
    let mut value: usize = 0;
    for &digit in bytes {
        if !digit.is_ascii_digit() {
            return None;
        }
        value = value
            .checked_mul(10)?
            .checked_add(usize::from(digit - b'0'))?;
    }
    Some(value)
}

fn run() -> ! {
    let argv: Vec<Vec<u8>> = support::args_bytes();
    let mut options = PatchOptions::default();
    let mut patchfile: Option<Vec<u8>> = None;
    let mut operands: Vec<&[u8]> = Vec::new();

    let mut i = 1;
    while i < argv.len() {
        let arg: &[u8] = &argv[i];
        if arg.first() != Some(&b'-') || arg.len() == 1 {
            operands.push(arg);
            i += 1;
            continue;
        }
        let flags: &[u8] = &arg[1..];
        let mut k = 0usize;
        while k < flags.len() {
            match flags[k] {
                b'F' => {
                    let value: Option<&[u8]> = if k + 1 < flags.len() {
                        Some(&flags[k + 1..])
                    } else {
                        i += 1;
                        argv.get(i).map(Vec::as_slice)
                    };
                    match value.and_then(parse_usize) {
                        Some(n) => options.max_fuzz = n,
                        None => {
                            support::warn(b"patch: bad fuzz value\n");
                            support::terminate(2);
                        }
                    }
                    break;
                }
                b'i' => {
                    k += 1;
                    if k < flags.len() {
                        patchfile = Some(flags[k..].to_vec());
                    } else {
                        i += 1;
                        patchfile = argv.get(i).cloned();
                        if patchfile.is_none() {
                            usage();
                        }
                    }
                }
                b'u' | b'n' => {} // format hints: the parser detects shapes itself
                b'c' | b'e' | b'R' => {
                    support::warn(
                        b"patch: context-format/ed/reverse patches are registered corners\n",
                    );
                    support::terminate(2);
                }
                _ => usage(),
            }
            k += 1;
        }
        i += 1;
    }

    if operands.len() > 1 {
        usage();
    }
    if let Some(target) = operands.first() {
        // 档名位按原字节（那条 seam 在册 E-CMDSYSFACE）。
        let mut line: Vec<u8> = b"patch: target file `".to_vec();
        line.extend_from_slice(target);
        line.extend_from_slice(b"' waits on the open-existing seam (edge E-CMDSYSFACE)\n");
        support::warn(&line);
        support::terminate(2);
    }

    let text = match &patchfile {
        Some(path) => {
            // 开档口要 `&str`（在册 `ARGV-BYTE-PATH-IN-LIBS`），折只发生在这一刻；
            // 报文里念出去的是原字节。
            let shown = String::from_utf8_lossy(path).into_owned();
            match support::read_file(&shown) {
                Ok(v) => String::from_utf8_lossy(&v).into_owned(),
                Err(_) => {
                    let mut line: Vec<u8> = b"patch: can't read ".to_vec();
                    line.extend_from_slice(path);
                    line.push(b'\n');
                    support::warn(&line);
                    support::terminate(2);
                }
            }
        }
        None => {
            let mut v = Vec::new();
            let mut chunk = [0u8; 4096];
            loop {
                match read(support::STDIN, &mut chunk) {
                    Ok(0) => break,
                    Ok(n) => v.extend_from_slice(&chunk[..n]),
                    Err(_) => break,
                }
            }
            String::from_utf8_lossy(&v).into_owned()
        }
    };

    // No target operand: parse and report, applying to an empty file
    // is meaningless — the C would prompt; here the engine reports.
    match parse_patch(&text) {
        Ok(hunks) => {
            if operands.is_empty() {
                support::warn(
                    format!("patch: {} hunks read; no target file given\n", hunks.len()).as_bytes(),
                );
                support::terminate(if hunks.is_empty() { 1 } else { 0 });
            }
            let input: Vec<&str> = Vec::new();
            let outcome = apply_patch(&input, &hunks, &options);
            support::warn(
                format!(
                    "patch: applied {} hunks, {} failed\n",
                    outcome.applied,
                    outcome.failed.len()
                )
                .as_bytes(),
            );
            support::terminate(if outcome.failed.is_empty() { 0 } else { 1 });
        }
        Err(err) => {
            support::warn(format!("patch: {}\n", err).as_bytes());
            support::terminate(2);
        }
    }
}

fn usage() -> ! {
    support::warn(b"usage: patch [-Fu] [-i patchfile] [origfile]\n");
    support::terminate(2);
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
