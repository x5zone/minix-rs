//! Minix-RS pig — pig latin filter.
//!
//! Ground truth: `minix3/games/pig/pig.c`. Usage is bare `pig` (`:131-135`), the
//! rotation itself lives in the deciding half (`pigout`, `:94-130`), and this shell owns
//! the byte-at-a-time stream of `main` (`:62-92`), whose shapes are easy to miss:
//!
//! * a **word is a run of `isalpha` bytes only** (`:78`), so digits, underscores and
//!   punctuation are passed straight through and cut words apart;
//! * every non-alpha byte triggers the pending word and is then emitted itself
//!   (`:83-89`), which is what separates the rotated words;
//! * **no pending word is flushed at end of input** — the loop exits on `EOF` and goes
//!   straight to `exit(0)` (`:76-92`), so a last word that never met a separator is
//!   dropped;
//! * a run of 1025 letters is C's `ate too much!` (`:78-79`), reported and fatal;
//! * operands are counted and then never read (`:66-75`): only a `-x` answers the usage
//!   line, and it leaves with status 1.

#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

extern crate alloc;

use alloc::string::String;
use alloc::vec::Vec;
#[path = "../bin_support.rs"]
mod support;

use minix_stdio_games::words::pig_word;

/// C's `buf[1024]` for the word being accumulated (`pig.c:64`).
const WORD_LIMIT: usize = 1024;

fn run() -> ! {
    let argv: Vec<String> = support::args();
    option_scan(&argv);
    let mut word = [0u8; WORD_LIMIT];
    let mut len = 0usize;
    let mut out = [0u8; WORD_LIMIT + 4];
    for byte in support::read_stdin() {
        if byte.is_ascii_alphabetic() {
            if len >= word.len() {
                // C tests before storing, so the 1025th letter of one run is the one that
                // trips the check (`pig.c:78-79`).
                support::warn(b"pig: ate too much!\n");
                support::terminate(1);
            }
            word[len] = byte;
            len += 1;
            continue;
        }
        if len != 0 {
            emit_word(&word[..len], &mut out);
            len = 0;
        }
        // The separator itself goes out unchanged, exactly as `putchar(ch)` does (`:89`).
        support::emit(&[byte]);
    }
    // A word still pending when the input ends is never printed (`:76` leaves the loop on
    // `EOF` and `:91` exits at once); the accumulated length is simply dropped.
    let _ = len;
    support::terminate(0);
}

/// C's `getopt(argc, argv, "")` (`pig.c:66-71`) and what follows it.
///
/// With an empty optstring `getopt` consumes `--` as the end-of-options marker and stops at
/// the first non-option, so a lone `-` is an operand. Any other `-x` answers the usage line
/// with status 1. Whatever operands remain are **counted and never read** — `main` goes on
/// to consume standard input regardless (`:76`), so they are ignored here rather than
/// reported.
fn option_scan(argv: &[String]) {
    match argv.get(1) {
        Some(arg) if arg == "--" => {}
        Some(arg) if arg.starts_with('-') && arg != "-" => usage(),
        _ => {}
    }
}

/// One accumulated word through C's `pigout()`; a word that cannot be rotated is emitted
/// as it stands, which is all C's own failure path leaves on standard output.
fn emit_word(word: &[u8], out: &mut [u8]) {
    let text = core::str::from_utf8(word).unwrap_or("");
    match pig_word(text, out) {
        Ok(len) => support::emit(&out[..len]),
        Err(_) => support::emit(word),
    }
}

fn usage() -> ! {
    support::warn(b"usage: pig\n");
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
