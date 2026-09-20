//! Minix-RS diff — differential file comparison.
//!
//! Ground truth: `minix3/minix/usr.bin/diff/diff.c` + `diffreg.c`,
//! usage `diff [-qeun] [file1 | -] [file2 | -]`. The deciding half
//! (the 4.4BSD differ: equivalence classes, prune, stone search,
//! output walk; formats normal/`-e`/`-f`/`-n`/`-q`/`-u`) is the
//! library's `diff` module. Exactly one side may be `-` (stdin);
//! named file operands wait on the open-existing seam (edge
//! E-CMDSYSFACE). Directory recursion, `-c` context, `#ifdef`
//! output and the ignore flags are registered corners. The
//! hosted-versus-target seams are the echo template's
//! (`os/commands/bin/fileops/src/bin/echo.rs`): argv via
//! `std::env::args`, termination via the host runtime.


#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

extern crate alloc;
use alloc::format;

use alloc::string::String;
use alloc::vec::Vec;
#[path = "../bin_support.rs"]
mod support;

use minix_textfilter::diff::{diff_lines, split_text, DiffFormat, DiffOptions, DiffStatus};
use minix_sys::read;

fn run() -> ! {
    let argv: Vec<String> = support::args();
    let mut options = DiffOptions::default();
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
                'q' => options.format = DiffFormat::Brief,
                'e' => options.format = DiffFormat::Ed,
                'f' => options.format = DiffFormat::ForwardEd,
                'n' => options.format = DiffFormat::Rcs,
                'u' => options.format = DiffFormat::Unified,
                'd' => options.minimal = true,
                'U' => {
                    // -U n: extended context count.
                    let value = if k + 1 < chars.len() {
                        Some(chars[k + 1..].iter().collect::<String>())
                    } else {
                        i += 1;
                        argv.get(i).cloned()
                    };
                    match value.and_then(|v| v.parse::<usize>().ok()) {
                        Some(n) => {
                            options.context = n;
                            options.format = DiffFormat::Unified;
                        }
                        None => {
                            support::warn(b"diff: bad context count\n");
                            support::terminate(2);
                        }
                    }
                    break;
                }
                _ => {
                    support::warn(format!("diff: illegal option -- {}\n", chars[k]).as_bytes());
                    support::warn(
                        b"usage: diff [-qeuFn] [-U n] file1 file2\n",
                    );
                    support::terminate(2);
                }
            }
            k += 1;
        }
        i += 1;
    }

    if operands.len() != 2 {
        support::warn(b"usage: diff [-qeuFn] [-U n] file1 file2\n");
        support::terminate(2);
    }

    let mut sides: [Vec<u8>; 2] = [Vec::new(), Vec::new()];
    for (side, operand) in operands.iter().enumerate() {
        if operand == "-" {
            let mut chunk = [0u8; 4096];
            loop {
                match read(support::STDIN, &mut chunk) {
                    Ok(0) => break,
                    Ok(n) => sides[side].extend_from_slice(&chunk[..n]),
                    Err(_) => break,
                }
            }
        } else {
            support::warn(
                b"diff: file operands wait on the open-existing seam (edge E-CMDSYSFACE)\n",
            );
            support::terminate(2);
        }
    }
    let text_a = String::from_utf8_lossy(&sides[0]).into_owned();
    let text_b = String::from_utf8_lossy(&sides[1]).into_owned();
    let la = split_text(&text_a);
    let lb = split_text(&text_b);

    let result = diff_lines(&la, &lb, operands[0].as_str(), operands[1].as_str(), &options);
    support::emit(result.text.as_bytes());
    support::terminate(match result.status {
        DiffStatus::Same => 0,
        DiffStatus::Differ => 1,
    });
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
