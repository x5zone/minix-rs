//! Minix-RS jot — print sequential or random data.
//!
//! Ground truth: `minix3/usr.bin/jot/jot.c` (NetBSD), usage
//! `jot [-cnr] [-b word] [-p precision] [-s string] [-w word]
//! [reps [begin [end [step | seed]]]]`. The deciding half (right-to-
//! left operand derivation, format selection, rendering) is the
//! library's `jot` module; this program parses the command line and
//! drains the value stream to stdout. The `-r` seed defaults to the
//! hosted time/pid mix until the target time seam lands; libc
//! `random()` is not reproduced bit-exactly (registered corner). The
//! hosted-versus-target seams are the echo template's
//! (`os/commands/bin/fileops/src/bin/echo.rs`): argv via
//! `std::env::args`, termination via the host runtime.
//!
//! The front door has four classes of exit, each with its own sentence owner:
//! the two libc `getopt` lines (option letter outside the set, value-requiring
//! letter without its value) come single-sourced from
//! [`minix_textfilter::GetoptReport`] and are always followed by one usage line
//! (the `default: usage()` of `jot.c:151-152`); a bad `-p` value, a bad
//! `reps`/`step`, and more than four operands are each a separate `errx(3)`
//! line, with the sentence written by `jot.c` itself and the program name from
//! `getprogname()`, and none of them is followed by usage; no operand at all
//! (including a bare `--`) is usage, without the libc line.

#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

extern crate alloc;

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
#[path = "../bin_support.rs"]
mod support;

use minix_textfilter::jot::{
    HAVE_BEGIN, HAVE_ENDER, HAVE_REPS, HAVE_STEP, Jot, JotArgError, JotError, get_prec,
    operand_to_number, parse_jot_args, parse_reps, parse_step, seed_from_step,
};

fn run() -> ! {
    let argv: Vec<Vec<u8>> = support::args_bytes();
    // `jot` reads neither standard input nor files (`jot.c` has not a single
    // `fopen`), so this is the only thin shell along the argument seam with **no
    // operand-side `&str` door**: apart from the program name, nothing is folded.
    let prog = String::from_utf8_lossy(support::prog_name_bytes(&argv));
    let args: Vec<&[u8]> = argv.iter().map(Vec::as_slice).collect();
    let mut seed_word: Option<&[u8]> = None;
    // Option loop (`"b:cnp:rs:w:"` at `jot.c:150`). The two libc lines and
    // `usage()` are **two lines**, while `-p`'s `errx` has only its own line —
    // the two branches are not a terse and a verbose form of one thing.
    //
    // Option letters go through `line_bytes`: `src/jot.rs` already takes them by
    // byte (`char::from(bytes[pos])`, and the missing-value branch uses
    // `char::from(ch)`), whereas a non-ASCII letter via `line` would be encoded
    // as two UTF-8 bytes. Measured against the C reference `jot`, both `-é` and
    // `-cé` yield `jot: unknown option -- ` + a single `c3` + a newline, the
    // mechanism being `optopt = *place++` at `getopt.c:79` and the `%c` at
    // `:103`.
    let (mut options, operands) = match parse_jot_args(&args[1..]) {
        Ok(parsed) => (parsed.options, parsed.operands),
        Err(JotArgError::Getopt(report)) => {
            support::warn(&report.line_bytes(&prog));
            usage(&prog);
        }
        Err(JotArgError::BadPrecision) => {
            // The sentence at `jot.c:164` is fixed; the program name follows
            // `getprogname()`.
            diagnose(&prog, b"Bad precision value");
            support::terminate(1);
        }
    };

    // Examine args right to left, falling through the cases
    // (jot.c:184-226). `-` keeps the default for that slot.
    // This `switch (argc)` sits before any operand is interpreted, so "no
    // operand at all" takes usage and "five or more" takes the `Too many
    // arguments` line; neither consults the first operand.
    if operands.is_empty() {
        usage(&prog);
    }
    if operands.len() > 4 {
        // `jot.c:223-225`: from the fifth operand on there is only one `errx`,
        // reporting the **fifth** (`argv[4]`); none of the first four is
        // interpreted.
        too_many_arguments(&prog, operands[4]);
    }
    let mut inferred_begin = 0usize;
    let mut inferred_ender = 0usize;
    if let Some(reps_arg) = operands.first().filter(|s| **s != b"-") {
        match parse_reps(reps_arg) {
            Ok(r) => {
                options.reps = r;
                options.have |= HAVE_REPS;
            }
            Err(_) => bad_reps(&prog, reps_arg),
        }
    }
    if let Some(begin_arg) = operands.get(1).filter(|s| **s != b"-") {
        // An empty operand makes C compute the index as `-1`, an out-of-bounds
        // read (`jot.c:196`) whose value depends on what precedes that memory;
        // the hosted reference measures `jot 3 '' 5` returning `1 3 5`, i.e.
        // that slot keeps its default, so this layer keeps the default per that
        // measurement instead of copying undefined behavior (registered as
        // `JOT-EMPTY-OPERAND-READS-OOB`).
        if let Some(value) = operand_to_number(begin_arg) {
            options.begin = value;
        }
        options.have |= HAVE_BEGIN;
        if options.prec.is_none() {
            inferred_begin = get_prec(begin_arg);
            options.prec = Some(inferred_begin);
        }
    }
    if let Some(ender_arg) = operands.get(2).filter(|s| **s != b"-") {
        if let Some(value) = operand_to_number(ender_arg) {
            options.ender = value;
        }
        options.have |= HAVE_ENDER;
        inferred_ender = get_prec(ender_arg);
    }
    if let Some(step_arg) = operands.get(3).filter(|s| **s != b"-") {
        match parse_step(step_arg) {
            Ok(s) => {
                options.step = s;
                options.have |= HAVE_STEP;
                if options.randomize {
                    seed_word = Some(step_arg);
                }
            }
            Err(_) => bad_step(&prog, step_arg),
        }
    }
    // Maximum precision of begin/ender (jot.c:206-209).
    options.prec = Some(
        options
            .prec
            .unwrap_or(0)
            .max(inferred_begin)
            .max(inferred_ender),
    );

    let seed: u64 = match seed_word {
        // `jot.c:118` is `srandom((unsigned long) step)`: C's truncation first,
        // then unsigned wraparound. So **negative numbers and fractions are both
        // valid seeds** (`-5` works, `-0.5` truncates to 0); this layer's former
        // `v >= 0.0` test reported shapes the reference accepts as
        // `Bad step value`, a criterion of its own invention. The whole-string
        // check of `strtod` (`parse_step`) is untouched, so bad values still get
        // the same sentence.
        Some(s) => match parse_step(s) {
            Ok(v) => seed_from_step(v),
            Err(_) => bad_step(&prog, s),
        },
        None => {
            // Clock + pid mix (C jot seeds from time(2)); the clock half
            // rides the `support::epoch_micros` twin, the pid half the
            // kernel getpid wrapper both sides.
            support::epoch_micros().unwrap_or(0) ^ ((minix_sys::getpid().unwrap_or(0) as u64) << 16)
        }
    };

    match Jot::new(options, seed) {
        Ok(mut jot) => {
            let mut out: Vec<u8> = Vec::new();
            while let Some(item) = jot.next() {
                out.extend_from_slice(&item);
                match jot.separator_after() {
                    Some(sep) => out.extend_from_slice(sep),
                    None => break,
                }
                // Unbounded runs (reps == 0) drain until the write
                // side fails; flush periodically.
                if out.len() >= 4096 && !support::write_ok(&out) {
                    support::terminate(1);
                }
                if out.len() >= 4096 {
                    out.clear();
                }
            }
            if !jot.nofinalnl() {
                out.push(b'\n');
            }
            support::emit(&out);
            support::terminate(0);
        }
        Err(err) => {
            support::warn(&error_bytes(&prog, &err));
            support::terminate(1);
        }
    }
}

/// Write the line "program name + `: ` + the sentence". It does **not** exit —
/// C's `errx` calls `exit` itself; the exit point here belongs to the caller,
/// and both branches (`Bad precision value` and the render-failure family) share
/// this one output.
fn diagnose(prog: &str, message: &[u8]) {
    let mut line = Vec::with_capacity(prog.len() + message.len() + 3);
    line.extend_from_slice(prog.as_bytes());
    line.extend_from_slice(b": ");
    line.extend_from_slice(message);
    line.push(b'\n');
    support::warn(&line);
}

/// Join a sentence's prefix and suffix with argv's raw bytes (shared by C's four
/// `%s` sentences: `jot.c:189`, `:215`, `:225`, `:385`/`:396`). The value part
/// goes through no character decoding — a bare `ff` is a single `ff` in the
/// sentence; a folded form would add two more bytes.
fn quoted(prefix: &[u8], value: &[u8], suffix: &[u8]) -> Vec<u8> {
    let mut message = Vec::with_capacity(prefix.len() + value.len() + suffix.len());
    message.extend_from_slice(prefix);
    message.extend_from_slice(value);
    message.extend_from_slice(suffix);
    message
}

/// The whole `errx(3)` line: the program name follows `getprogname()`, and the
/// phrase is taken from C's sentence per branch in the match below (the two
/// double-space sentences, the backtick sentence and the question-mark sentence
/// all match verbatim).
fn error_bytes(prog: &str, err: &JotError) -> Vec<u8> {
    let message = match err {
        JotError::BadPrecision => b"Bad precision value".to_vec(),
        JotError::BadReps(s) => quoted(b"Bad reps value:  ", s, b""),
        JotError::BadStep(s) => quoted(b"Bad step value:  ", s, b""),
        JotError::InvalidFormat(word) => quoted(b"unknown or invalid format `", word, b"'"),
        JotError::FormatWantsString => b"cannot convert numeric data to strings".to_vec(),
        JotError::ImpossibleStepsize => b"Impossible stepsize".to_vec(),
        JotError::MustSpecifyBegin => b"Must specify begin if reps == 0".to_vec(),
        JotError::InfiniteUnbounded => b"Infinite sequences cannot be bounded".to_vec(),
        JotError::TooManyArgs(s) => quoted(b"Too many arguments.  What do you mean by ", s, b"?"),
    };
    let mut line = Vec::with_capacity(prog.len() + message.len() + 3);
    line.extend_from_slice(prog.as_bytes());
    line.extend_from_slice(b": ");
    line.extend_from_slice(&message);
    line.push(b'\n');
    line
}

fn bad_reps(prog: &str, arg: &[u8]) -> ! {
    support::warn(&error_bytes(prog, &JotError::BadReps(arg.to_vec())));
    support::terminate(1);
}

fn bad_step(prog: &str, arg: &[u8]) -> ! {
    support::warn(&error_bytes(prog, &JotError::BadStep(arg.to_vec())));
    support::terminate(1);
}

/// `jot.c:223-225`: from the fifth operand on there is only one `errx`,
/// reporting the **fifth** operand (`argv[4]`, i.e. index 4 after
/// `argv += optind`); none of the first four is interpreted.
fn too_many_arguments(prog: &str, fifth: &[u8]) -> ! {
    support::warn(&error_bytes(prog, &JotError::TooManyArgs(fifth.to_vec())));
    support::terminate(1);
}

fn usage(prog: &str) -> ! {
    support::warn(
        format!(
            "usage: {prog} [-cnr] [-b word] [-p precision] [-s string] [-w word] \
             [reps [begin [end [step | seed]]]]\n"
        )
        .as_bytes(),
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
