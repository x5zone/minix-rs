//! Minix-RS split — split input into numbered chunk files.
//!
//! Ground truth: `minix3/usr.bin/split/split.c` (NetBSD): chunks of
//! 1000 lines (`-l` moves it) named by a two-letter suffix `aa`, `ab`,
//! ... The deciding half (chunk computation and suffix enumeration) is
//! the library's `split` module; this build reads an input file (or
//! stdin) and writes each chunk to `prefix<suffix>` created with
//! `O_WRONLY|O_CREAT|O_TRUNC` at DEFFILEMODE (`split.c:332`). The input
//! operand special-cases `-` as stdin (`split.c:136`, unlike grep/sed);
//! the second operand is the output prefix (default `x`). `-a` (suffix
//! width) is wired; `-b` (bytes) and `-n` (chunk count) are not in the
//! deciding half and are rejected. Real-machine file creation is gated at C-57/E5.

#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

extern crate alloc;
use alloc::format;

use alloc::string::{String, ToString};
use alloc::vec::Vec;
#[path = "../bin_support.rs"]
mod support;

use minix_sys::{close, open, write};
use minix_textfilter::split::{SplitError, chunk_bytes, chunks, parse_split_args, suffix};

// Output-file create flags (`fcntl.h`), mirroring the `uudecode` precedent:
// O_WRONLY 0x1, O_CREAT 0x200, O_TRUNC 0x400; DEFFILEMODE 0666.
const O_WRONLY: i32 = 0x1;
const O_CREAT: i32 = 0x200;
const O_TRUNC: i32 = 0x400;

fn run() -> ! {
    let argv: Vec<String> = support::args();
    let args: Vec<&str> = argv.iter().map(String::as_str).collect();
    // argv folding lives in the deciding half (`parse_split_args`): BSD getopt
    // `"0123456789b:l:a:n:"` with attached-or-next `-l` values and the obsolete
    // all-digit `-N`. `-a`/`-b`/`-n` (suffix width, byte split, chunk count) are
    // reported unmodelled — the deciding half splits on line counts only.
    let parsed = match parse_split_args(&args[1..]) {
        Ok(p) => p,
        Err(e) => arg_error(e),
    };
    let lines_per_chunk = parsed.lines_per_chunk;
    let sfxlen = parsed.sfxlen;
    let byte_size = parsed.byte_size;
    let operands: Vec<&str> = parsed.operands;
    // `split.c:143`: the second operand is the output prefix, default "x".
    let prefix = operands.get(1).copied().unwrap_or("x");

    // `split.c:135-139`: `-` reads stdin (split, unlike grep/sed, special-cases
    // it), a named input is opened, no operand reads stdin, and an unopenable
    // input is fatal (`err(1, "%s", *argv)`).
    let input = match operands.first() {
        None => support::read_stdin(),
        Some(name) if *name == "-" => support::read_stdin(),
        Some(name) => match support::read_file(name) {
            Ok(data) => data,
            Err(()) => {
                support::warn(format!("split: {name}\n").as_bytes());
                support::terminate(1);
            }
        },
    };

    let text = core::str::from_utf8(&input).unwrap_or("");
    let mut lines: Vec<String> = text.split('\n').map(ToString::to_string).collect();
    if lines.last().is_some_and(|line| line.is_empty()) {
        lines.pop();
    }
    // `-b`: byte-addressed split (`split1`) over the raw input, no line
    // reconstruction — each fixed-size chunk is written verbatim.
    if let Some(n) = byte_size {
        for (index, chunk) in chunk_bytes(&input, n).iter().enumerate() {
            let path = match out_path(prefix, index, sfxlen) {
                Some(path) => path,
                None => {
                    support::warn(b"split: suffixes exhausted\n");
                    support::terminate(1);
                }
            };
            if create_write(&path, chunk).is_err() {
                support::warn(format!("split: {path}\n").as_bytes());
                support::terminate(1);
            }
        }
        support::terminate(0);
    }
    for (index, chunk_lines) in chunks(&lines, lines_per_chunk).iter().enumerate() {
        // `split.c:332`: each chunk lands in `prefix + suffix(index)`, created
        // with O_WRONLY|O_CREAT|O_TRUNC at DEFFILEMODE.
        let path = match out_path(prefix, index, sfxlen) {
            Some(path) => path,
            None => {
                support::warn(b"split: suffixes exhausted\n");
                support::terminate(1);
            }
        };
        if create_write(&path, &render_chunk(chunk_lines)).is_err() {
            support::warn(format!("split: {path}\n").as_bytes());
            support::terminate(1);
        }
    }
    support::terminate(0);
}

/// Output path for chunk `index`: `prefix` + the `width`-letter suffix, or
/// `None` once the suffix space (`26^width`) is exhausted.
fn out_path(prefix: &str, index: usize, width: usize) -> Option<String> {
    suffix(index, width).map(|suffix| format!("{prefix}{suffix}"))
}

/// Renders one chunk back into bytes: each line followed by a newline (the
/// chunk lines carry no terminator; `split` writes whole lines).
fn render_chunk(chunk_lines: &[String]) -> Vec<u8> {
    let mut data = Vec::new();
    for line in chunk_lines {
        data.extend_from_slice(line.as_bytes());
        data.push(b'\n');
    }
    data
}

/// Creates (or truncates) `path`, writes `bytes`, then closes it. Mirrors
/// `split.c`'s `open(O_WRONLY|O_CREAT|O_TRUNC, DEFFILEMODE)` plus a partial
/// write loop; a short or failed write returns `Err`.
fn create_write(path: &str, bytes: &[u8]) -> Result<(), ()> {
    let fd = match open(path, O_WRONLY | O_CREAT | O_TRUNC, 0o666) {
        Ok(fd) => fd,
        Err(_) => return Err(()),
    };
    let mut written = 0usize;
    let mut result = Ok(());
    while written < bytes.len() {
        match write(fd, &bytes[written..]) {
            Ok(0) => {
                result = Err(());
                break;
            }
            Ok(n) => written += n,
            Err(_) => {
                result = Err(());
                break;
            }
        }
    }
    // [SD19-gate] SD-19: close 后 fd 不再使用
    #[allow(clippy::let_underscore_must_use)]
    let _ = close(fd);
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn out_path_prefixes_the_two_letter_suffix() {
        assert_eq!(out_path("x", 0, 2).as_deref(), Some("xaa"));
        assert_eq!(out_path("x", 1, 2).as_deref(), Some("xab"));
        assert_eq!(out_path("part.", 26, 2).as_deref(), Some("part.ba"));
        // -a 1：单字母后缀。
        assert_eq!(out_path("x", 0, 1).as_deref(), Some("xa"));
        assert_eq!(out_path("x", 26, 1), None);
    }

    #[test]
    fn out_path_is_none_once_suffixes_exhaust() {
        // aa..zz is 676 names; index 675 is the last, 676 has none.
        assert!(out_path("x", 675, 2).is_some());
        assert_eq!(out_path("x", 676, 2), None);
    }

    #[test]
    fn render_chunk_terminates_each_line() {
        let lines = ["a".to_string(), "b".to_string()];
        assert_eq!(render_chunk(&lines), b"a\nb\n".to_vec());
    }

    #[test]
    fn render_chunk_of_no_lines_is_empty() {
        assert!(render_chunk(&[]).is_empty());
    }
}

fn usage() -> ! {
    support::warn(b"usage: split [-l lines]\n");
    support::terminate(2);
}

/// Map a [`SplitError`] from `parse_split_args` to the exact C error path (the
/// message and exit code) the old in-bin parser used, then terminate.
fn arg_error(e: SplitError<'_>) -> ! {
    match e {
        SplitError::Usage => usage(),
        SplitError::IllegalLineCount(v) => {
            support::warn(format!("split: {v}: illegal line count.\n").as_bytes());
            support::terminate(1);
        }
        SplitError::IllegalByteCount(v) => {
            support::warn(format!("split: {v}: illegal byte count.\n").as_bytes());
            support::terminate(1);
        }
        SplitError::IllegalSuffixLength(v) => {
            support::warn(format!("split: {v}: illegal suffix length.\n").as_bytes());
            support::terminate(1);
        }
        SplitError::Unmodelled => {
            support::warn(b"split: -n option not modelled yet\n");
            support::terminate(2);
        }
    }
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
