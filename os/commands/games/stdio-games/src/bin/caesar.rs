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
    // C 的 `caesar` 也不叫 `setprogname`；`getprogname()` 由 `crt0-common.c:156-165`
    // 从 `argv[0]` 起那一段，`err`/`errx` 都以它开头。
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
    // cracker's 2048-byte window are still rotated (`:100-117`). 写失手是 C 的
    // `putchar` 返 `EOF` 那一支：立刻 `err(EXIT_FAILURE, "<stdout>")`，缀号并退。
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
    // 之后才轮到输入那一支（`caesar.c:205-208`）：C 看的是 `ferror(stdin)`，而 stdio 的
    // 错标志不带号，所以它是 `errx`——只印程序名与 `<stdin>`，不缀句子。
    if read_error.is_some() {
        errx(prog, "<stdin>");
    }
    support::terminate(0);
}

/// C 的 `errx(EXIT_FAILURE, …)`：程序名、短语、**不缀句子**，写完退 1。
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
/// `err(3)` 的形状在 `minix3/lib/libc/gen/verrc.c`：`progname: ` + 调用者的串 + `": "` +
/// `strerror(errno)` + 换行。句尾那一句来自本树的 `sys_errlist`（`minix-types` 的
/// [`Errno::message`]），因此它是 Minix3 自己的措词而不是外来的那张表：`EINVAL` 缀
/// `Invalid argument`，`ERANGE` 缀 `Result too large or too small`。
fn bad_rotation(prog: &str, arg: &str, failure: RotationFailure) -> ! {
    // 交回 `support::warn_line`（形状与句子都在那一份权威里），程名不再硬写；
    // `Bad rotation value `<原串>'` 是 C 格式串里的部分，逐字保留。
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
