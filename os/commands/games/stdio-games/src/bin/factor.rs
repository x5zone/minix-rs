//! Minix-RS factor — integer factorisation.
//!
//! Ground truth: `minix3/games/factor/factor.c`, usage `factor [value ...]`
//! (`:268`). Options are `getopt(argc, argv, "")` (`:147-152`): any argument starting with
//! `-` other than `-` or `--` answers the usage line, and C's `usage()` leaves with status
//! **0** (`:266-270`). Each value prints one line, `value:` then a space and a factor per
//! division (`pr_fact`, `:228-252`). Values arrive from `argv` with operands, one per line
//! of standard input without them (`:156-179`), and a value the firewall cannot accept stops
//! the whole run with C's sentence on standard error (`:168`, `:174`, `:219`).

#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

extern crate alloc;

use alloc::string::String;
use alloc::vec::Vec;
#[path = "../bin_support.rs"]
mod support;

use minix_stdio_games::factor::{
    NEGATIVE_MSG, NUMBERS_MSG, OVERFLOW_ERRNO, Verdict, render_factors, trial_divide, verdict,
};
use support::LineReader;

fn run() -> ! {
    let argv: Vec<String> = support::args();
    let mut index = option_scan(&argv);
    if index < argv.len() {
        // Operands: C hands each one to `decode`-style parsing in turn and stops at the
        // first rejection, because every diagnostic is an `errx` (`factor.c:172-179`).
        while index < argv.len() {
            factor_word(&argv[index], false);
            index += 1;
        }
    } else {
        let mut input = [0u8; 80];
        let mut reader = LineReader::new(support::stdin_chunk, &mut input);
        loop {
            // C has two ways to end: `fgets` returning `NULL` **and**
            // `ferror(stdin)` is the error case, which takes `err(1, "stdin")`
            // (`factor.c:157-161`); merely reaching the end is `exit(0)`.
            let has_line = match reader.next_line() {
                Ok(has_line) => has_line,
                Err(code) => {
                    let mut line = [0u8; 96];
                    let len = support::err_line("factor", "stdin", code, &mut line);
                    support::warn(&line[..len]);
                    support::warn(b"\n");
                    support::terminate(1);
                }
            };
            if !has_line {
                break;
            }
            // The line still carries its newline here, which `verdict` treats as C's `buf`
            // does; no copy is needed because the line is used before the next read.
            let line = core::str::from_utf8(reader.line()).unwrap_or("");
            factor_word(line, true);
        }
    }
    support::terminate(0);
}

/// C's `getopt(argc, argv, "")` over the argument vector (`factor.c:147-157`).
///
/// Returns the index of the first operand. With an empty optstring `getopt` either consumes
/// `--` as the end-of-options marker or stops at the first non-option, so exactly one
/// argument decides where the operands begin. A lone `-` is such a non-option and therefore
/// an operand (`:173` answers it with the `numbers <= 1` sentence), while any other `-`-led
/// argument reaches `case '?'` and the usage line.
fn option_scan(argv: &[String]) -> usize {
    match argv.get(1) {
        Some(arg) if arg == "--" => 2,
        Some(arg) if arg.starts_with('-') && arg != "-" => usage(),
        _ => 1,
    }
}

/// One value through C's verdict ladder: print its factors, skip it, or leave.
fn factor_word(word: &str, from_stdin: bool) {
    match verdict(word, from_stdin) {
        Verdict::Skip => {}
        Verdict::Value(value) => {
            let mut factors = [0u64; 64];
            let count = match trial_divide(value, &mut factors) {
                Ok(count) => count,
                Err(_) => support::terminate(1),
            };
            let mut out = [0u8; 256];
            match render_factors(value, &factors[..count], &mut out) {
                Ok(len) => {
                    support::emit(&out[..len]);
                    support::emit(b"\n");
                }
                Err(_) => support::terminate(1),
            }
        }
        Verdict::Overflow => {
            // In C this leg is `err(1, "%s", str)` (`factor.c:123-126`): program
            // name, original string, colon-space, then the sentence for the
            // number `strtoul` set. It differs from `Illegal` in sentence and in
            // timing—overflow checks `errno` first and the end-of-line check
            // comes after (`:127`), so an oversized number with a trailing tail
            // gets this sentence and says nothing about the format.
            support::warn(b"factor: ");
            support::warn(word.trim_end_matches(['\n', '\r']).as_bytes());
            support::warn(b": ");
            if let Some(text) = OVERFLOW_ERRNO.message() {
                support::warn(text.as_bytes());
            }
            support::warn(b"\n");
            support::terminate(1);
        }
        Verdict::NotPermitted => errx(NUMBERS_MSG),
        Verdict::Negative => errx(NEGATIVE_MSG),
        Verdict::Illegal => {
            // C's sentence is `errx(1, "%s: illegal numeric format.", argv[0])` (`:177`).
            // On the operand path `argv[0]` is the value itself; on the stdin path C has
            // already advanced `argv` past the operands and prints a spent pointer, so this
            // shell names the offending line instead — a declared divergence, not a fix.
            support::warn(b"factor: ");
            support::warn(word.trim_end_matches(['\n', '\r']).as_bytes());
            support::warn(b": illegal numeric format.\n");
            support::terminate(1);
        }
    }
}

/// C's `errx`: the program name, the sentence, then status 1 (`factor.c:168`, `:174`, `:219`).
fn errx(message: &str) -> ! {
    support::warn(b"factor: ");
    support::warn(message.as_bytes());
    support::warn(b"\n");
    support::terminate(1)
}

/// C's `usage()` prints to standard error and leaves with status 0 (`factor.c:266-270`).
fn usage() -> ! {
    support::warn(b"usage: factor [value ...]\n");
    support::terminate(0);
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
