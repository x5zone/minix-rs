//! Minix-RS ppt — paper tape punch and reader.
//!
//! Ground truth: `minix3/games/ppt/ppt.c` (NetBSD 1.19), `main` at `:66-127`.
//! Encode (default): the edge rule `___________` first, then one punched line per
//! byte of the operands (a punched blank **between** operands, none after the last,
//! `:107-115`) or per byte of standard input (`:116-118`), and the edge rule again
//! (`:119`). Decode (`-d`): operands are refused outright (`:95-96`), tape lines come
//! from `fgets` into `buf[132]` (`:98-105`), and a line with no feed hole ends the run
//! — but only once the tape has synced; before the first good line such a line is
//! preamble and is skipped (`:100-105`). The run owes a newline unless its last byte
//! already was one (`:104`, `:112-113`).
//!
//! Options are `getopt(argc, argv, "dh")` (`:74-85`): `-d` selects the direction, and
//! `-h` is **not** a help flag — it shares `case '?'` and prints the usage line on
//! standard error with status 1 (`:58-63`).

#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

extern crate alloc;

use alloc::string::String;
use alloc::vec::Vec;
#[path = "../bin_support.rs"]
mod support;

use minix_stdio_games::ppt::{EDGE, decode_line, punch_byte};
use support::LineReader;

fn run() -> ! {
    let argv: Vec<String> = support::args();
    let mut decode = false;
    let mut first = 1;
    // `getopt(argc, argv, "dh")` walks the letters inside one argument, stops at the
    // first non-option, and consumes `--` as the end-of-options marker (`ppt.c:74-89`).
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
                b'd' => decode = true,
                // `-h` shares the error arm with the unknown letters, so there is no
                // help output on this path at all (`:80-83`).
                b'h' => usage(),
                _ => usage(),
            }
        }
        first += 1;
    }
    let rest = &argv[first..];
    if decode {
        decode_tape(rest);
    } else {
        punch(rest);
    }
    support::terminate(0)
}

/// C's `usage()`: the line goes to standard error with the program name, and the
/// status is 1 (`ppt.c:58-63`). The name is this command's own, not a host path.
fn usage() -> ! {
    support::warn(b"usage: ppt [-d] [string ...]\n");
    support::terminate(1);
}

fn punch(strings: &[String]) {
    support::emit(EDGE);
    support::emit(b"\n");
    let mut piece = [0u8; 16];
    if strings.is_empty() {
        let mut chunk = [0u8; 256];
        loop {
            let read = support::stdin_chunk(&mut chunk);
            let count = match read {
                Ok(0) => break,
                Ok(count) => count,
                Err(_) => break,
            };
            for &byte in &chunk[..count] {
                punch_one(byte, &mut piece);
            }
        }
    } else {
        for (index, string) in strings.iter().enumerate() {
            for &byte in string.as_bytes() {
                punch_one(byte, &mut piece);
            }
            if index + 1 < strings.len() {
                punch_one(b' ', &mut piece);
            }
        }
    }
    support::emit(EDGE);
    support::emit(b"\n");
}

fn punch_one(byte: u8, piece: &mut [u8]) {
    match punch_byte(byte, piece) {
        Ok(len) => support::emit(&piece[..len]),
        Err(_) => support::terminate(1),
    }
}

fn decode_tape(strings: &[String]) {
    if !strings.is_empty() {
        usage();
    }
    let mut input = [0u8; 132];
    let mut reader = LineReader::new(support::stdin_chunk, &mut input);
    let mut synced = false;
    let mut needs_newline = false;
    loop {
        // When `!feof(stdin)` C takes `err(1, "fgets")` (`ppt.c:109-111`): a
        // read **error** and end of input are two different things — the
        // former exits with 1 and does not emit the owed newline, only the
        // latter falls through to the wrap-up below.
        let has_line = match reader.next_line() {
            Ok(has_line) => has_line,
            Err(code) => {
                let mut line = [0u8; 96];
                let len = support::err_line("ppt", "fgets", code, &mut line);
                support::warn(&line[..len]);
                support::warn(b"\n");
                support::terminate(1);
            }
        };
        if !has_line {
            break;
        }
        match decode_line(reader.line()) {
            Some(byte) => {
                synced = true;
                support::emit(&[byte]);
                needs_newline = byte != b'\n';
            }
            // A sync-less line before the tape starts is preamble.
            None if synced => {
                if needs_newline {
                    support::emit(b"\n");
                }
                support::terminate(0);
            }
            None => {}
        }
    }
    // Only two ways reach here: end of input, or a tape that hit a line
    // without a sync hole and already left early. The read-error arm left
    // above, matching C's `err(1, "fgets")` shape.
    if needs_newline {
        support::emit(b"\n");
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
