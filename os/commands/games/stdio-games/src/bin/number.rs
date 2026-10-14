//! Minix-RS number — numbers as English words.
//!
//! Ground truth: `minix3/games/number/number.c`, `main` at `:91-126`. Options are
//! `getopt(argc, argv, "l")`, where `-l` is the only letter and anything else answers the
//! usage line on standard error with status 1 (`:85-90`, `:299-303`). Operands are spoken
//! in order (`:117-122`); with none, standard input is read one `fgets` at a time into
//! `char line[256]` — usable length **255**, because the terminator owns the last byte —
//! and a chunk that holds no newline means the line did not fit, which is fatal
//! (`:109-111`). Every item after the first is preceded by a literal `"...\n"`
//! (`:110-111`, `:119-120`); the number itself is `convert`'s business.
//!
//! The failure messages are plain `errx` calls with no `strerror` suffix, so this shell
//! reproduces them exactly: `illegal number: ` plus the text C's `line` still held,
//! `number too large, max 65 digits.`, and `line too long.`

#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

extern crate alloc;

use alloc::string::String;
use alloc::vec::Vec;
#[path = "../bin_support.rs"]
mod support;

use minix_stdio_games::words::{NumberError, convert};
use support::LineReader;

/// Usable bytes of C's `char line[256]` under `fgets` (`number.c:94`, `:108`).
const LINE: usize = 255;

fn run() -> ! {
    let argv: Vec<String> = support::args();
    let mut linear = false;
    let mut first = 1usize;
    while first < argv.len() {
        let arg = argv[first].as_str();
        if !arg.starts_with('-') || arg == "-" {
            break;
        }
        if arg == "--" {
            first += 1;
            break;
        }
        for letter in arg.as_bytes()[1..].iter() {
            match letter {
                b'l' => linear = true,
                _ => usage(),
            }
        }
        first += 1;
    }
    let operands = &argv[first..];
    let mut out = [0u8; 4096];
    let mut spoken_any = false;
    if operands.is_empty() {
        let mut input = [0u8; LINE];
        let mut reader = LineReader::new(support::stdin_chunk, &mut input);
        // C 的 number 也没有 `ferror` 那条腿：`fgets` 返回 `NULL` 就跳出循环，读出错同样如此。
        while reader.next_line().unwrap_or(false) {
            let line = reader.line();
            if !line.contains(&b'\n') {
                support::warn(b"number: line too long.\n");
                support::terminate(1);
            }
            if spoken_any {
                support::emit(b"...\n");
            }
            speak(line, linear, &mut out);
            spoken_any = true;
        }
    } else {
        for text in operands {
            if spoken_any {
                support::emit(b"...\n");
            }
            speak(text.as_bytes(), linear, &mut out);
            spoken_any = true;
        }
    }
    support::terminate(0);
}

/// One number through C's `convert`, with its two `errx` exits.
fn speak(line: &[u8], linear: bool, out: &mut [u8]) {
    match convert(line, linear, out) {
        Ok(len) => support::emit(&out[..len]),
        Err(NumberError::Illegal { start, end }) => {
            // C prints the `line` it still has: from its advanced start up to the `.` it
            // already turned into a terminator, when there was one.
            support::warn(b"number: illegal number: ");
            support::warn(&line[start..end]);
            support::warn(b"\n");
            support::terminate(1);
        }
        Err(NumberError::TooLarge) => {
            support::warn(b"number: number too large, max 65 digits.\n");
            support::terminate(1);
        }
        // Unreachable for legal input, whose spelling fits well inside `out`.
        Err(NumberError::NoRoom) => support::terminate(1),
    }
}

fn usage() -> ! {
    support::warn(b"usage: number [# ...]\n");
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
