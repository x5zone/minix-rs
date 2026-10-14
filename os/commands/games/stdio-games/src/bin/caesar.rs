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

/// C's `err(EXIT_FAILURE, "Bad rotation value `%s'", arg)` (`caesar.c:137`).
///
/// `err(3)` 的形状在 `minix3/lib/libc/gen/verrc.c`：`progname: ` + 调用者的串 + `": "` +
/// `strerror(errno)` + 换行。句尾那一句来自本树的 `sys_errlist`（`minix-types` 的
/// [`Errno::message`]），因此它是 Minix3 自己的措词而不是外来的那张表：`EINVAL` 缀
/// `Invalid argument`，`ERANGE` 缀 `Result too large or too small`。
fn bad_rotation(arg: &str, failure: RotationFailure) -> ! {
    support::warn(b"caesar: Bad rotation value `");
    support::warn(arg.as_bytes());
    support::warn(b"': ");
    // 表内的号总有句子；表外才会像 C 那样只留下一个空的逗号位（本例不会发生）。
    if let Some(text) = failure.errno().message() {
        support::warn(text.as_bytes());
    }
    support::warn(b"\n");
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
