//! Minix-RS caesar — rotation cipher filter.
//!
//! Ground truth: `minix3/games/caesar/caesar.c`, `main` at `:190-218`. There is no
//! `getopt` here at all: the argument count alone picks the path (`:192-203`). With no
//! argument C runs `guess_and_rotate` (`:141-187`), which scores the first 2048 bytes,
//! derives a rotation, prints those bytes through that table and then streams the rest
//! through the same one. With one argument the rotation comes from `get_rotation`
//! (`:120-137`; a failure leaves through `err`, whose suffix is this tree's one known
//! omission — see `bad_rotation`). More than one argument is the usage line on standard
//! error with status 1 (`:199-202`). Letters rotate, everything else passes through the
//! identity table unchanged.

#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

extern crate alloc;

use alloc::string::String;
use alloc::vec::Vec;
#[path = "../bin_support.rs"]
mod support;

use minix_stdio_games::caesar::{RotationFailure, guess_rotation, parse_rotation, rotate_byte};

fn run() -> ! {
    let argv: Vec<String> = support::args();
    let input = match argv.len() {
        // C's argument-free mode: the cracker decides the rotation from the text itself
        // (`main` calls `guess_and_rotate`, `caesar.c:192-194`).
        1 => {
            let input = support::read_stdin();
            let rotation = guess_rotation(&input);
            (input, rotation)
        }
        2 => match parse_rotation(&argv[1]) {
            Ok(rotation) => (support::read_stdin(), rotation),
            Err(failure) => bad_rotation(&argv[1], failure),
        },
        _ => usage(),
    };
    let (input, rotation) = input;
    // `print_file` maps every byte through the table and writes it, so bytes outside the
    // cracker's 2048-byte window are still rotated (`:100-117`).
    let mut out = [0u8; 1024];
    let mut start = 0usize;
    while start < input.len() {
        let end = (start + out.len()).min(input.len());
        for (index, byte) in input[start..end].iter().enumerate() {
            out[index] = rotate_byte(*byte, rotation);
        }
        support::emit(&out[..end - start]);
        start = end;
    }
    support::terminate(0);
}

/// C's `usage()`: standard error, status 1 (`caesar.c:199-202`).
fn usage() -> ! {
    support::warn(b"usage: caesar [rotation]\n");
    support::terminate(1);
}

/// C's `err(EXIT_FAILURE, "Bad rotation value `%s'", arg)` (`caesar.c:134-135`).
///
/// C appends the strerror text for the errno it set — `Invalid argument` or `Numerical
/// result out of range` — and this tree has no such table, so the line stops at the
/// quoted value. The distinction is kept as `failure` and reported by the exit status the
/// same way; the missing suffix is registered in the stage ledger.
fn bad_rotation(arg: &str, failure: RotationFailure) -> ! {
    match failure {
        // The suffix C would put here for `EINVAL`: `: Invalid argument`.
        RotationFailure::Invalid => {}
        // …and for `ERANGE`: `: Numerical result out of range`.
        RotationFailure::Range => {}
    }
    support::warn(b"caesar: Bad rotation value `");
    support::warn(arg.as_bytes());
    support::warn(b"'\n");
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
