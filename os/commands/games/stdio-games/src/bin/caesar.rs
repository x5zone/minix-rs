//! Minix-RS caesar — rotation cipher filter.
//!
//! Ground truth: `minix3/games/caesar/caesar.c`, `main` at `:190-218`. There is no
//! `getopt` here at all: the argument count alone picks the path (`:192-203`). With no
//! argument C runs `guess_and_rotate` (`:141-187`), which scores the first 2048 bytes,
//! derives a rotation, prints those bytes through that table and then streams the rest
//! through the same one. With one argument the rotation comes from `get_rotation`
//! (`:120-137`; a failure leaves through `err`, now rendered by the shared
//! `err_line` authority — see `bad_rotation`). More than one argument is the usage
//! line on standard error with status 1 (`:199-202`).
//!
//! The three remaining legs are C's stream failures, and they are two families: a
//! `putchar` that returns `EOF` leaves at once through `err(EXIT_FAILURE,
//! "<stdout>")` (`:103-107`, `:116-120`) — program name, `<stdout>`, **and the
//! number's sentence** — while the two checks at the end of `main` look at stdio's
//! error flags, which carry no number, so they are `errx` (`<stdin>` at `:205-208`,
//! `<stdout>` after the flush at `:211-214`). The flush leg has no counterpart here:
//! this shell writes unbuffered per chunk, so a failed write is already reported by
//! the `err` leg above rather than at the end — declared, not faked. Letters rotate, everything else passes through the
//! identity table unchanged.

#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

extern crate alloc;

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
#[path = "../bin_support.rs"]
mod support;

use minix_stdio_games::caesar::{RotationFailure, guess_rotation, parse_rotation, rotate_byte};

fn run() -> ! {
    let argv: Vec<String> = support::args();
    // C's `caesar` does not call `setprogname` either; `getprogname()` takes
    // the segment starting at `argv[0]` (`crt0-common.c:156-165`), and both
    // `err`/`errx` prefix their output with it.
    let prog = support::prog_name(&argv);
    let (input, rotation, read_error) = match argv.len() {
        // C's argument-free mode: the cracker decides the rotation from the text itself
        // (`main` calls `guess_and_rotate`, `caesar.c:192-194`).
        1 => {
            let (input, failure) = support::read_stdin_all();
            let rotation = guess_rotation(&input);
            (input, rotation, failure)
        }
        2 => match parse_rotation(&argv[1]) {
            Ok(rotation) => {
                let (input, failure) = support::read_stdin_all();
                (input, rotation, failure)
            }
            Err(failure) => bad_rotation(prog, &argv[1], failure),
        },
        _ => usage(),
    };
    // `print_file` maps every byte through the table and writes it, so bytes outside the
    // cracker's 2048-byte window are still rotated (`:100-117`). A failed
    // write is the branch where C's `putchar` returns `EOF`: immediately
    // `err(EXIT_FAILURE, "<stdout>")`, with the errno number appended, then
    // exit.
    let mut out = [0u8; 1024];
    let mut start = 0usize;
    while start < input.len() {
        let end = (start + out.len()).min(input.len());
        for (index, byte) in input[start..end].iter().enumerate() {
            out[index] = rotate_byte(*byte, rotation);
        }
        if let Err(code) = support::write_stdout(&out[..end - start]) {
            support::warn_line(prog, "<stdout>", code);
            support::terminate(1);
        }
        start = end;
    }
    // Only then does the input branch get its turn (`caesar.c:205-208`): C
    // looks at `ferror(stdin)`, and stdio error flags carry no number, so it is
    // `errx` — it prints only the program name and `<stdin>`, with no sentence
    // suffix.
    if read_error.is_some() {
        errx(prog, "<stdin>");
    }
    support::terminate(0);
}

/// C's `errx(EXIT_FAILURE, …)`: program name, phrase, **no sentence suffix**;
/// exit 1 after writing.
fn errx(prog: &str, phrase: &str) -> ! {
    support::warn(format!("{prog}: {phrase}\n").as_bytes());
    support::terminate(1)
}

/// C's `usage()`: standard error, status 1 (`caesar.c:199-202`).
fn usage() -> ! {
    support::warn(b"usage: caesar [rotation]\n");
    support::terminate(1);
}

/// C's `err(EXIT_FAILURE, "Bad rotation value `%s'", arg)` (`caesar.c:137`).
///
/// The shape of `err(3)` is in `minix3/lib/libc/gen/verrc.c`: `progname: ` + the
/// caller's string + `": "` + `strerror(errno)` + newline. The trailing sentence
/// comes from this tree's `sys_errlist` (`minix-types`' [`Errno::message`]), so
/// it is Minix3's own wording rather than the foreign table: `EINVAL` appends
/// `Invalid argument`, `ERANGE` appends `Result too large or too small`.
fn bad_rotation(prog: &str, arg: &str, failure: RotationFailure) -> ! {
    // Handed back to `support::warn_line` (both shape and sentence live in
    // that single authority), and the program name is no longer hard-coded;
    // `Bad rotation value `<raw argument>'` is the part from C's format string,
    // kept verbatim.
    support::warn_line(
        prog,
        &format!("Bad rotation value `{arg}'"),
        failure.errno(),
    );
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
