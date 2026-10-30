//! Minix-RS number — numbers as English words.
//!
//! Ground truth: `minix3/games/number/number.c`, `main` at `:89-123`. Options are
//! `getopt(argc, argv, "l")`, where `-l` is the only letter and anything else answers the
//! usage line on standard error with status 1 (`:95-103`, `:285-290`). Operands are spoken
//! in order (`:116-121`); with none, standard input is read one `fgets` at a time into
//! `char line[256]` (`:92`, `:108-109`) — usable length **255**, because the terminator owns
//! the last byte — and a chunk that holds no newline means the line did not fit, which is
//! fatal (`:110-111`). Every item after the first is preceded by a literal `"...\n"`
//! (`:112-113`, `:118-119`); the number itself is `convert`'s business.
//!
//! The three refusals are plain `errx` calls (`:111`, `:156`, `:164`), so they carry no
//! `strerror` sentence — but `verrx` opens with `getprogname()` plus `": "` and closes with a
//! newline (`minix3/lib/libc/gen/verrx.c:59-62`). The name therefore comes from `argv[0]`, not
//! from the literal word `number`, and the text after `illegal number: ` is whatever C's `line`
//! still holds: only a `.` is overwritten with a terminator (`:149`), so without one the report
//! runs to the end of the buffer — offending bytes and `fgets`' newline included. The usage
//! line is the opposite case: `number.c:288` spells `number` into the literal, so it never
//! follows the name.

#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

extern crate alloc;

use alloc::string::String;
use alloc::vec::Vec;
#[path = "../bin_support.rs"]
mod support;

use alloc::format;
use minix_stdio_games::words::{NumberError, convert};
use support::LineReader;

/// Usable bytes of C's `char line[256]` under `fgets` (`number.c:92`, `:108-109`).
const LINE: usize = 255;

fn run() -> ! {
    let argv: Vec<String> = support::args();
    let prog = support::prog_name(&argv);
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
        // C's number has no `ferror` leg either: `fgets` returning `NULL` leaves the loop, and a read error does the same.
        while reader.next_line().unwrap_or(false) {
            let line = reader.line();
            if !line.contains(&b'\n') {
                errx(prog, b"line too long.", b"");
            }
            if spoken_any {
                support::emit(b"...\n");
            }
            speak(prog, line, linear, &mut out);
            spoken_any = true;
        }
    } else {
        for text in operands {
            if spoken_any {
                support::emit(b"...\n");
            }
            speak(prog, text.as_bytes(), linear, &mut out);
            spoken_any = true;
        }
    }
    support::terminate(0);
}

/// One number through C's `convert`, with its two `errx` exits.
fn speak(prog: &str, line: &[u8], linear: bool, out: &mut [u8]) {
    match convert(line, linear, out) {
        Ok(len) => support::emit(&out[..len]),
        Err(NumberError::Illegal { start, end }) => {
            // C prints the `line` it still holds (`number.c:156`): from its advanced start to
            // the `.` it wrote a terminator at, or to the end of the buffer when no `.` came
            // first — which on the `fgets` path leaves the line's own newline inside the text.
            errx(prog, b"illegal number: ", &line[start..end]);
        }
        Err(NumberError::TooLarge) => errx(prog, b"number too large, max 65 digits.", b""),
        // Unreachable for legal input, whose spelling fits well inside `out`.
        Err(NumberError::NoRoom) => support::terminate(1),
    }
}

/// C's `errx(eval, fmt, ...)`: program name, `": "`, the formatted text, one newline, exit
/// (`verrx.c:59-63`). `phrase` is the format's fixed text and `payload` the `%s` argument it
/// may carry; the two fixed phrases pass an empty payload.
fn errx(prog: &str, phrase: &[u8], payload: &[u8]) -> ! {
    support::warn(format!("{prog}: ").as_bytes());
    support::warn(phrase);
    support::warn(payload);
    support::warn(b"\n");
    support::terminate(1)
}

/// C's `usage()` writes a literal that names `number` itself (`number.c:288`), so this line
/// keeps the word even when `argv[0]` says otherwise.
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
