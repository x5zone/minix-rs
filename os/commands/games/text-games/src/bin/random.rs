//! Minix-RS random — the doing half over `minix_text_games::lottery`.
//!
//! Ground truth: `minix3/games/random/random.c`. `random` is a filter: it draws
//! once per line of standard input, independently, and writes the **whole
//! selected line out verbatim** (the `selected` test is at `:126`, the
//! per-character `putchar` loop at `:127-137`). The denominator comes from the
//! operands: none takes 2, one takes `strtod(*argv, &ep)`, two or more is usage
//! (`:89-104`). Seeding mixes in the clock and the process number (`mix_seed`'s
//! decisive half, `:106-107`).
//!
//! The operand step, the filter and the option ring are all shaped after C, and
//! the denominator has been switched to C's `double`: `strtod`'s decimal leg
//! (leading whitespace, sign, decimal point, exponent, `inf`/`nan`) and both
//! `errno == ERANGE` legs (overflow, underflow to 0) are wired. The standard
//! output leg also follows C: after each line a failure is checked once, and a
//! failure reports `err(2, "stdout")` and exits 2 (`random.c:131-133`). Still
//! open are the two shapes hexadecimal float `0x1p3` and a `nan(0x…)` payload;
//! the account lives in the `RANDOM-DENOM-LEGS` entry of
//! `rewrite-notes/18-stage-commands/todo.md`.

#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

extern crate alloc;

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

#[path = "../bin_support.rs"]
mod support;
use minix_text_games::TextGameError;
use minix_text_games::lottery::{
    NO_OPERAND_DENOMINATOR, NOT_VALID_SENTENCE, OperandLeg, Ring, USAGE, exit_status_value,
    judge_operands, line_selected, mix_seed, scan_ring, unknown_option_line,
};

fn run() -> ! {
    let argv: Vec<String> = support::args();
    let prog = support::prog_name(&argv);
    // C's `argc -= optind; switch (argc)` (`random.c:86-104`). This branch's
    // option ring is not wired yet, so the operands are just `argv[1..]`; the
    // input is read only after the count is judged.
    // The ring runs first, and the operands are whatever is left (C's
    // `argc -= optind`)
    let tokens: Vec<&str> = argv[1..].iter().map(String::as_str).collect();
    let (exit_status, operands) = match scan_ring(&tokens) {
        Ring::Done {
            exit_status,
            operands_at,
            ..
        } => (exit_status, &tokens[operands_at..]),
        Ring::Unknown(letter) => {
            // The libc line comes first and the `usage()` line second, and
            // whether each carries a name differs: the first goes through
            // `getprogname()`, the `random` in the second is hardcoded in the
            // string. The letter position uses `unknown_option_line`'s byte
            // exit: `getopt.c:79`'s `optopt = *place++` takes one byte at a
            // time, so taking `c3` as a char into a `String` would carry an
            // extra `83`.
            support::warn(&unknown_option_line(prog, letter));
            support::warn(USAGE.as_bytes());
            support::terminate(1);
        }
    };
    let operands: Vec<&str> = operands.to_vec();
    let denominator = match judge_operands(&operands) {
        OperandLeg::NoOperandDefault => NO_OPERAND_DENOMINATOR,
        OperandLeg::Accepted(value) => value,
        OperandLeg::TooMany => {
            // The `usage()` line carries no program-name prefix; `random` is
            // hardcoded in the string (`random.c:143-147`)
            support::warn(USAGE.as_bytes());
            support::terminate(1);
        }
        OperandLeg::NotValid => {
            // `errx(1, "denominator is not valid.")` (`:98-99`): the prefix
            // follows `argv[0]`, but the operand is not in the sentence —
            // `errx` has no `%s` slot
            support::warn(format!("{prog}: {NOT_VALID_SENTENCE}\n").as_bytes());
            support::terminate(1);
        }
        OperandLeg::OutOfRange => {
            // `err(1, "%s", *argv)` (`:96-97`): three-part shape — the slot
            // holds the operand, and the tail is the sentence for the errno
            support::warn_line(
                prog,
                operands[0],
                minix_sys::Errno::from_i32(TextGameError::OutOfRange.as_errno()),
            );
            support::terminate(1);
        }
    };

    let micros = support::epoch_micros().unwrap_or(0);
    let pid = minix_sys::getpid().unwrap_or(0) as u32;
    let mut state = mix_seed(micros / 1_000_000, (micros % 1_000_000) as u32, pid);

    if exit_status {
        // C's `if (random_exit) return ((denom * random()) / RANDOM_MAX);`
        // (`:109-111`): this branch returns **before reading standard input**,
        // so it prints nothing and the exit code is the drawn number
        state = state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        support::terminate(exit_status_value(denominator, (state >> 33) as u64));
    }

    let (bytes, read_failure) = support::read_stdin_error();
    if let Some(code) = read_failure {
        // The `ferror(stdin)` path is `err(2, "stdin")` (`random.c:135-136`):
        // three-part shape, the slot holds the word `stdin` rather than a file
        // name (C's `ifd` starts at 0, so standard input and a named file take
        // the same `read()` leg), and the exit code is 2. The thin shell here
        // used to print a homegrown `random: read error` and exit 1
        support::warn_line(prog, "stdin", code);
        support::terminate(2);
    }
    // C is one streaming pass (`random.c:126-137`): a selected line is written
    // out whole, and the next line draws again. Selecting no line is not an
    // error — the ring ends and it `exit(0)`s; in the reference build that
    // shape is empty output with exit 0
    let text = String::from_utf8_lossy(&bytes).into_owned();
    for line in text.lines() {
        // An LCG (Numerical Recipes constants), taking the high bits before the
        // modulus. C draws with libc's `random()`, so the bit sequence is not
        // reproduced (this carrier difference is recorded in this branch's
        // ledger); the `1/denom` criterion follows C
        state = state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        let draw = (state >> 33) & 0x7fff_ffff;
        if line_selected(denominator, draw) {
            // C checks `ferror(stdout)` once after writing each line and on
            // failure takes `err(2, "stdout")` (`random.c:131-133`): the
            // three-part shape's slot holds the word `stdout` rather than a file
            // name, and the exit code is 2. The write seam returns the errno
            // (`support::emit_error`); the sentence and exit code are this
            // branch's own choice
            if let Some(code) = support::emit_error(format!("{line}\n").as_bytes()) {
                support::warn_line(prog, "stdout", code);
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
