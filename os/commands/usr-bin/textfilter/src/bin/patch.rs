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

use alloc::string::String;
use alloc::vec::Vec;
use alloc::format;
#[path = "../bin_support.rs"]
mod support;

use minix_textfilter::patch::{apply_patch, parse_patch, PatchOptions};
use minix_sys::read;

fn run() -> ! {
    let argv: Vec<String> = support::args();
    let mut options = PatchOptions::default();
    let mut patchfile: Option<String> = None;
    let mut operands: Vec<String> = Vec::new();

    let mut i = 1;
    while i < argv.len() {
        let arg = argv[i].clone();
        if !arg.starts_with('-') || arg.len() == 1 {
            operands.push(arg);
            i += 1;
            continue;
        }
        let chars: Vec<char> = arg.chars().skip(1).collect();
        let mut k = 0usize;
        while k < chars.len() {
            match chars[k] {
                'F' => {
                    let value = if k + 1 < chars.len() {
                        Some(chars[k + 1..].iter().collect::<String>())
                    } else {
                        i += 1;
                        argv.get(i).cloned()
                    };
                    match value.and_then(|v| v.parse::<usize>().ok()) {
                        Some(n) => options.max_fuzz = n,
                        None => {
                            support::warn(b"patch: bad fuzz value\n");
                            support::terminate(2);
                        }
                    }
                    break;
                }
                'i' => {
                    k += 1;
                    if k < chars.len() {
                        patchfile = Some(chars[k..].iter().collect());
                    } else {
                        i += 1;
                        patchfile = argv.get(i).cloned();
                        if patchfile.is_none() {
                            usage();
                        }
                    }
                }
                'u' | 'n' => {} // format hints: the parser detects shapes itself
                'c' | 'e' | 'R' => {
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
        support::warn(
            format!(
                "patch: target file `{}` waits on the open-existing seam (edge E-CMDSYSFACE)\n",
                target
            )
            .as_bytes(),
        );
        support::terminate(2);
    }

    let text = match patchfile {
        Some(path) => match support::read_file(&path) {
            Ok(v) => String::from_utf8_lossy(&v).into_owned(),
            Err(_) => {
                support::warn(format!("patch: can't read {}\n", path).as_bytes());
                support::terminate(2);
            }
        },
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
                support::warn(format!("patch: {} hunks read; no target file given\n", hunks.len()).as_bytes());
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
