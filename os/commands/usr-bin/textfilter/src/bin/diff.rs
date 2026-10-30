//! Minix-RS diff — differential file comparison.
//!
//! Ground truth: `minix3/minix/usr.bin/diff/diff.c` + `diffreg.c`,
//! usage `diff [-qeun] [file1 | -] [file2 | -]`. The deciding half
//! (the 4.4BSD differ: equivalence classes, prune, stone search,
//! output walk; formats normal/`-e`/`-f`/`-n`/`-q`/`-u`) is the
//! library's `diff` module. Exactly one side may be `-` (stdin); named
//! file operands are opened and read (`diffreg.c` `fopen`), and an
//! unopenable side is fatal (`status |= 2` → exit 2, since there is
//! nothing to compare). Directory recursion, `-c` context, `#ifdef`
//! output and the ignore flags are registered corners. The
//! hosted-versus-target seams are the echo template's
//! (`os/commands/bin/fileops/src/bin/echo.rs`): argv via
//! `std::env::args`, termination via the host runtime.

#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

extern crate alloc;

use alloc::string::String;
use alloc::vec::Vec;
#[path = "../bin_support.rs"]
mod support;

use minix_sys::read;
use minix_textfilter::diff::{DiffFormat, DiffOptions, DiffStatus, diff_lines, split_text};

/// `-U` 的取值判据照 C 的 `atoi` 只认 ASCII 数字；折前折后同判，只改报文里的字节。
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
    let mut options = DiffOptions::default();
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
                b'q' => options.format = DiffFormat::Brief,
                b'e' => options.format = DiffFormat::Ed,
                b'f' => options.format = DiffFormat::ForwardEd,
                b'n' => options.format = DiffFormat::Rcs,
                b'u' => options.format = DiffFormat::Unified,
                b'd' => options.minimal = true,
                b'U' => {
                    // -U n: extended context count.
                    let value: Option<&[u8]> = if k + 1 < flags.len() {
                        Some(&flags[k + 1..])
                    } else {
                        i += 1;
                        argv.get(i).map(Vec::as_slice)
                    };
                    match value.and_then(parse_usize) {
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
                    // 集外字母的字母位按原字节（C 的 `%c`）。
                    let mut line: Vec<u8> = b"diff: illegal option -- ".to_vec();
                    line.push(flags[k]);
                    line.push(b'\n');
                    support::warn(&line);
                    support::warn(b"usage: diff [-qeuFn] [-U n] file1 file2\n");
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
        if *operand == b"-" {
            let mut chunk = [0u8; 4096];
            loop {
                match read(support::STDIN, &mut chunk) {
                    Ok(0) => break,
                    Ok(n) => sides[side].extend_from_slice(&chunk[..n]),
                    Err(_) => break,
                }
            }
        } else {
            // 开档口要 `&str`（在册 `ARGV-BYTE-PATH-IN-LIBS`），折只发生在这一刻；
            // 报文里念出去的是原字节。
            let shown = String::from_utf8_lossy(operand).into_owned();
            match support::read_file(&shown) {
                Ok(data) => sides[side] = data,
                Err(_) => {
                    // C `diffreg` sets `status |= 2` and exits 2; a side
                    // that will not open leaves nothing to compare, so it
                    // is fatal here too.
                    let mut line: Vec<u8> = b"diff: ".to_vec();
                    line.extend_from_slice(operand);
                    line.push(b'\n');
                    support::warn(&line);
                    support::terminate(2);
                }
            }
        }
    }
    let text_a = String::from_utf8_lossy(&sides[0]).into_owned();
    let text_b = String::from_utf8_lossy(&sides[1]).into_owned();
    let la = split_text(&text_a);
    let lb = split_text(&text_b);

    // 输出报头里的两个档名是内容半的一条腿（在册 `DIFF-LABEL-CONTENT-TEXTSEAM`）：
    // `diff_lines` 的标签口收 `&str`，折只发生在这一刻，带裸字节的档名在报头里
    // 仍是折过的。
    let label_a = String::from_utf8_lossy(operands[0]).into_owned();
    let label_b = String::from_utf8_lossy(operands[1]).into_owned();
    let result = diff_lines(&la, &lb, &label_a, &label_b, &options);
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
