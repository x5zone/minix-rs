//! Minix-RS random — the doing half over `minix_text_games::lottery`.
//!
//! Ground truth: `minix3/games/random/random.c`. `random` is a filter: it rolls once
//! per line of standard input and writes a selected line out whole (the `selected` test
//! is at `:126`, the per-character `putchar` loop at `:127-137`). The denominator comes
//! from the operands: none means 2, one means `strtod(*argv, &ep)`, two or more mean
//! usage (`:89-104`). Seeding mixes in the clock and the process id (`mix_seed`'s
//! deciding-half semantics, `:106-107`).
//!
//! The operand step, the filter and the option loop all follow C, and the denominator is
//! C's `double`: the decimal legs of `strtod` (leading whitespace, sign, decimal point,
//! exponent, `inf`/`nan`) and the two `errno == ERANGE` legs (overflow, underflow to 0)
//! are wired. The standard-output leg follows C too: after each line it checks for a
//! failure and reports `err(2, "stdout")` with exit 2 (`random.c:131-133`). Still open
//! are the hexadecimal float form `0x1p3` and the `nan(0x...)` payload, recorded in
//! `rewrite-notes/18-stage-commands/todo.md` under `RANDOM-DENOM-LEGS`.

#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

extern crate alloc;

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

#[path = "../bin_support.rs"]
mod support;
use minix_text_games::TextGameError;
use minix_text_games::lottery::{
    NO_OPERAND_DENOMINATOR, NOT_VALID_SENTENCE, OperandLeg, Ring, USAGE, exit_status_value,
    judge_operands, line_selected, mix_seed, scan_ring, unknown_option_line,
};

fn run() -> ! {
    let argv: Vec<Vec<u8>> = support::args_bytes();
    // The program name folds once (the two `&str` mouths of `line_bytes` and the
    // three-part form, registered `ARGV-BYTE-PATH-IN-LIBS`).
    let prog = String::from_utf8_lossy(support::prog_name_bytes(&argv)).into_owned();
    // C's `argc -= optind; switch (argc)` (`random.c:86-104`). The option loop runs
    // first and the operands are what remains (C's `argc -= optind`); only after the
    // count is judged does reading the input begin.
    let tokens: Vec<&[u8]> = argv[1..].iter().map(Vec::as_slice).collect();
    let (exit_status, operands) = match scan_ring(&tokens) {
        Ring::Done {
            exit_status,
            operands_at,
            ..
        } => (exit_status, &tokens[operands_at..]),
        Ring::Unknown(letter) => {
            // The libc line comes first and the `usage()` line after; the two differ in
            // whether they carry a name: the first uses `getprogname()`, while the
            // `random` in the second is hard-coded in the string. The letter goes through
            // `unknown_option_line`'s byte exit: `optopt = *place++` at `getopt.c:79`
            // takes one byte at a time, and holding `c3` as a character in a `String`
            // would drag a second byte `83` along.
            support::warn(&unknown_option_line(&prog, letter));
            support::warn(USAGE.as_bytes());
            support::terminate(1);
        }
    };
    let operands: Vec<&[u8]> = operands.to_vec();
    let denominator = match judge_operands(&operands) {
        OperandLeg::NoOperandDefault => NO_OPERAND_DENOMINATOR,
        OperandLeg::Accepted(value) => value,
        OperandLeg::TooMany => {
            // The `usage()` line carries no program-name prefix; `random` is hard-coded
            // in the string (`random.c:143-147`)
            support::warn(USAGE.as_bytes());
            support::terminate(1);
        }
        OperandLeg::NotValid => {
            // `errx(1, "denominator is not valid.")` (`:98-99`): the prefix follows
            // `argv[0]`, but the operand is not in the sentence — `errx` has no `%s` slot
            support::warn(format!("{}: {NOT_VALID_SENTENCE}\n", prog).as_bytes());
            support::terminate(1);
        }
        OperandLeg::OutOfRange => {
            // `err(1, "%s", *argv)` (`:96-97`): the three-part form, with the operand
            // (raw bytes) in the slot and the sentence for the number at the end.
            // `err_line`'s label mouth takes `&str`, so the line is assembled here.
            let mut line: Vec<u8> = prog.as_bytes().to_vec();
            line.extend_from_slice(b": ");
            line.extend_from_slice(operands[0]);
            line.extend_from_slice(b": ");
            line.extend_from_slice(
                minix_sys::Strerror(minix_sys::Errno::from_i32(
                    TextGameError::OutOfRange.as_errno(),
                ))
                .to_string()
                .as_bytes(),
            );
            line.push(b'\n');
            support::warn(&line);
            support::terminate(1);
        }
    };

    let micros = support::epoch_micros().unwrap_or(0);
    let pid = minix_sys::getpid().unwrap_or(0) as u32;
    let mut state = mix_seed(micros / 1_000_000, (micros % 1_000_000) as u32, pid);

    if exit_status {
        // C's `if (random_exit) return ((denom * random()) / RANDOM_MAX);`
        // (`:109-111`): this branch returns before reading standard input, so it prints
        // nothing and the exit code is the number drawn
        state = state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        support::terminate(exit_status_value(denominator, (state >> 33) as u64));
    }

    let (bytes, read_failure) = support::read_stdin_error();
    if let Some(code) = read_failure {
        // The `ferror(stdin)` path is `err(2, "stdin")` (`random.c:135-136`): the
        // three-part form, with the word `stdin` in the slot rather than a file name (C's
        // `ifd` starts at 0, and standard input and named files share the same `read()`
        // leg), and the exit code is 2. An earlier version printed an invented
        // `random: read error` and exited 1.
        support::warn_line(&prog, "stdin", code);
        support::terminate(2);
    }
    // C makes one pass over the stream (`random.c:126-137`): a selected line is written
    // out whole, and the next line gets a new roll. Selecting no line is not an error —
    // the loop finishes into `exit(0)`, and C outputs nothing and exits 0 for that shape.
    let text = String::from_utf8_lossy(&bytes).into_owned();
    for line in text.lines() {
        // LCG with the Numerical Recipes constants, taking the high bits before the
        // modulo. C rolls libc's `random()`, whose bit sequence is not reproduced (a
        // declared carrier difference for this branch); the `1/denom` test follows C.
        state = state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        let draw = (state >> 33) & 0x7fff_ffff;
        if line_selected(denominator, draw) {
            // C checks `ferror(stdout)` after writing each line and reports
            // `err(2, "stdout")` on failure (`random.c:131-133`): the three-part form
            // carries the word `stdout` in the slot rather than a file name, and the exit
            // code is 2. The write seam hands back the number (`support::emit_error`),
            // and this branch supplies the sentence and the exit code.
            if let Some(code) = support::emit_error(format!("{line}\n").as_bytes()) {
                support::warn_line(&prog, "stdout", code);
                support::terminate(2);
            }
        }
    }
    support::terminate(0)
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
