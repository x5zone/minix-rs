//! Minix-RS arithmetic — interactive arithmetic quiz.
//!
//! Ground truth: `minix3/games/arithmetic/arithmetic.c`. Questions print as
//! `left op right =   ` (line 231) with operands in `0..range` (`-r`,
//! default 10, line 102) and operators from the key set (`-o`, default
//! `+-` of `+-x/`, line 101). Answers: non-numeric input asks again with
//! `Please type a number.` (line 246), a correct answer answers
//! `Right!` (line 250), a wrong one `What?` and the same problem is asked
//! again (lines 254 and following). End of input prints **only** a newline and
//! exits (`:241-243`); the score block is `showstats`, which fires every twenty
//! problems (`:148-151`) and on the interrupt (`:156-161`) — never at end of input.
//! Its shape is three pieces with two different guards, and it carries the
//! `Total time %ld seconds; %.1f seconds per problem` line (`:172-173`): `qtime`
//! accumulates, problem by problem, the **whole-second difference** between two
//! `time(2)` stamps, and that single decimal digit is `(float)qtime / nright`
//! rounded to nearest with ties to even by `gdtoa`.
//!
//! Two declared differences: the operand source is a seeded linear congruential
//! generator (the C program uses `random(3)`, whose sequence is time-seeded and
//! therefore not an observable contract either), and C's "no memory left, so
//! forget about the penalty" branch (`arithmetic.c:309-310`: a failed `malloc`
//! makes `penalise` a no-op) has no counterpart — this tree's penalty list is a
//! `Vec`, which aborts instead of returning null. The penalty arithmetic itself
//! is modelled (`Penalties`/`Drawn`/`charged_operands`) and C
//! emits no sound on a wrong answer, so there is no hint face to miss.
//!
//! The diagnostic face is C's and nothing else: all three option errors are
//! `errx(1, …)` (`arithmetic.c:126`, `:132`, `:362`, `:373`) and `usage()` is a
//! `fprintf(stderr, …)` with `getprogname()` and a capital `U` (`:378-385`). Two
//! details are reproduced rather than "cleaned up": the channel is standard error
//! (this shell used to `emit` them to standard output), and C's format strings
//! themselves begin with the word `arithmetic: `, so `errx`'s own prefix makes the
//! real line read `arithmetic: arithmetic: invalid range.` — the doubled name is
//! C's text, kept as the truth of the byte output (registered as an upstream wart
//! rather than silently improved).

#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

extern crate alloc;

use alloc::format;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
#[path = "../bin_support.rs"]
mod support;

use minix_stdio_games::quiz::{
    DEFAULT_RANGE, Drawn, Operator, Penalties, Question, Score, answer_of, charged_operands,
    elapsed_secs, parse_range, seconds_per_problem, shows_score, slot_of,
};
use support::LineReader;

const KEYS: [Operator; 4] = [
    Operator::Add,
    Operator::Subtract,
    Operator::Multiply,
    Operator::Divide,
];

fn run() -> ! {
    let argv: Vec<String> = support::args();
    // C's `arithmetic` does not call `setprogname`; `getprogname()` is still set
    // up from `argv[0]` by `crt0-common.c:156-165`, and that is what `usage()`
    // uses.
    let prog = support::prog_name(&argv);
    let mut range = DEFAULT_RANGE;
    let mut keys: Vec<usize> = vec![0, 1];
    let mut index = 1;
    while index < argv.len() {
        match argv[index].as_str() {
            "-r" if index + 1 < argv.len() => {
                index += 1;
                match parse_range(&argv[index]) {
                    Ok(value) => range = value,
                    Err(_) => {
                        // `arithmetic.c:132`: `errx(1, "arithmetic: invalid range.")`—
                        // the duplicated prefix comes from C's own format string.
                        support::warn(b"arithmetic: arithmetic: invalid range.\n");
                        support::terminate(1);
                    }
                }
            }
            "-o" if index + 1 < argv.len() => {
                index += 1;
                keys.clear();
                for key in argv[index].bytes() {
                    match KEYS.iter().position(|op| sign_of(*op) == key) {
                        Some(position) => keys.push(position),
                        None => {
                            // `arithmetic.c:126`, same as above.
                            support::warn(b"arithmetic: arithmetic: unknown key.\n");
                            support::terminate(1);
                        }
                    }
                }
            }
            _ => {
                // `arithmetic.c:378-385`: `Usage: %s [-o +-x/] [-r range]\n`, capital U,
                // the name embedded in the string, to stderr.
                support::warn(format!("Usage: {prog} [-o +-x/] [-r range]\n").as_bytes());
                support::terminate(1);
            }
        }
        index += 1;
    }

    let mut random = seed();
    let mut score = Score::default();
    let mut qtime = 0u64;
    let mut penalties = Penalties::default();
    let mut input = [0u8; 64];
    let mut reader = LineReader::new(support::stdin_chunk, &mut input);
    let mut asked = 0u32;
    loop {
        let question = next_question(&mut random, range, &keys, &mut penalties);
        let answer = answer_of(question);
        // The prompt goes out once; a wrong answer does not reprint it, C just reads the
        // next line (`arithmetic.c:231-266`). End of input prints a newline and the program
        // exits — the score is shown every twenty problems and on the interrupt only.
        ask(&question);
        // C stamps `time(&start)` right after the prompt and its flush (`arithmetic.c:232`),
        // so the wrong attempts below are inside the charged window.
        let start = support::epoch_micros();
        loop {
            // C's arithmetic never checks `ferror` (no such test anywhere in it);
            // end of file and a read error both just wrap up and exit 0.
            let has_line = reader.next_line().unwrap_or(false);
            if !has_line {
                support::emit(b"\n");
                support::terminate(0);
            }
            // Skip leading whitespace, take the decimal digit run (`atoi` semantics: trailing
            // text is ignored), and an empty run re-prompts (`:238-247`).
            let line = reader.line();
            let start = match line.iter().position(|b| !b.is_ascii_whitespace()) {
                Some(start) if line[start].is_ascii_digit() => start,
                _ => {
                    support::emit(b"Please type a number.\n");
                    continue;
                }
            };
            let mut end = start;
            while end < line.len() && line[end].is_ascii_digit() {
                end += 1;
            }
            let word = core::str::from_utf8(&line[start..end]).unwrap_or("0");
            let given: u64 = word.parse().unwrap_or(u64::MAX);
            if given == answer as u64 {
                score.right += 1;
                support::emit(b"Right!\n");
                break;
            }
            score.wrong += 1;
            support::emit(b"What?\n");
            for (charge_slot, operand, value) in
                charged_operands(question.op, question.left, question.right, answer)
            {
                penalties.add(charge_slot, operand, value);
            }
        }
        // …and `qtime += finish - start` once the answer is right (`:271-272`). A clock face
        // that refuses has no counterpart in C (its `time(2)` cannot fail here), so it charges
        // nothing rather than inventing a duration.
        if let (Some(start), Some(finish)) = (start, support::epoch_micros()) {
            qtime += elapsed_secs(start, finish);
        }
        asked += 1;
        if asked % 20 == 0 {
            show_stats(&mut reader, &score, qtime);
        }
    }
}

/// The operator's printed sign (C: `keylist` `+-x/`).
fn sign_of(op: Operator) -> u8 {
    match op {
        Operator::Add => b'+',
        Operator::Subtract => b'-',
        Operator::Multiply => b'x',
        Operator::Divide => b'/',
    }
}

/// C's `problem()` (`arithmetic.c:191-232`): draw the operator from the key set, then the
/// operands through the penalty lists, and build the problem so the answer is exact.
///
/// The ranges are C's, inclusive in `rangemax`: `getrandom(rangemax + 1, …)` yields
/// `0..=rangemax`, while division draws its divisor from `rangemax` and adds one so it is
/// never zero, then hides a remainder inside the dividend. Subtraction is built from the
/// answer, so the left operand is the sum. A construction that would overflow is redrawn,
/// matching C's `goto retry` guard against negative values.
fn next_question(
    random: &mut u32,
    range: u32,
    keys: &[usize],
    penalties: &mut Penalties,
) -> Question {
    loop {
        let slot = keys[next_random(random) as usize % keys.len()];
        let op = KEYS[slot];
        debug_assert_eq!(slot, slot_of(op), "the key order is C's `keylist`");
        let mut right = 0u32;
        if op != Operator::Divide {
            right = draw(penalties, random, slot, 1, range + 1);
        }
        let question = match op {
            Operator::Add => Question {
                left: draw(penalties, random, slot, 0, range + 1),
                right,
                op,
            },
            Operator::Subtract => {
                let answer = draw(penalties, random, slot, 0, range + 1);
                match right.checked_add(answer) {
                    Some(left) => Question { left, right, op },
                    None => continue,
                }
            }
            Operator::Multiply => Question {
                left: draw(penalties, random, slot, 0, range + 1),
                right,
                op,
            },
            Operator::Divide => {
                let divisor = draw(penalties, random, slot, 1, range).saturating_add(1);
                let answer = draw(penalties, random, slot, 0, range + 1);
                let remainder = next_random(random) % divisor;
                match divisor
                    .checked_mul(answer)
                    .and_then(|product| product.checked_add(remainder))
                {
                    Some(dividend) => Question {
                        left: dividend,
                        right: divisor,
                        op,
                    },
                    None => continue,
                }
            }
        };
        return question;
    }
}

/// One operand value: the generator's raw output through the penalty list for that slot.
/// C's `getrandom()` can only fail to find a listed penalty if its two bookkeeping arrays
/// disagree, and it calls that a bug (`:358-362`).
fn draw(
    penalties: &mut Penalties,
    random: &mut u32,
    slot: usize,
    operand: usize,
    maxval: u32,
) -> u32 {
    match penalties.draw(slot, operand, maxval, next_random(random)) {
        Drawn::Direct(value) | Drawn::Penalised(value) => value,
        Drawn::Inconsistent => {
            // `arithmetic.c:362`: `errx(1, "arithmetic: bug: inconsistent penalties.")`,
            // the duplicated prefix again as in the two cases above.
            support::warn(b"arithmetic: arithmetic: bug: inconsistent penalties.\n");
            support::terminate(1)
        }
    }
}

/// The generator step C performs with `random(3)`; this tree keeps a documented
/// congruential stand-in so the run is reproducible on the host.
fn next_random(random: &mut u32) -> u32 {
    let value = *random;
    *random = value.wrapping_mul(1_103_515_245).wrapping_add(12_345);
    value
}

/// Prints `left op right =   ` — C's `"%d %c %d =   "` (`arithmetic.c:231`), three spaces
/// after the sign; the shell used to write two, which the interactive door caught because the
/// prompt never finished the way C's does.
fn ask(question: &Question) {
    let mut out = [0u8; 24];
    let mut length = 0;
    length += support::utoa(question.left, &mut out[length..]);
    out[length] = b' ';
    length += 1;
    out[length] = sign_of(question.op);
    length += 1;
    out[length] = b' ';
    length += 1;
    length += support::utoa(question.right, &mut out[length..]);
    out[length] = b' ';
    length += 1;
    out[length] = b'=';
    length += 1;
    out[length] = b' ';
    length += 1;
    out[length] = b' ';
    length += 1;
    out[length] = b' ';
    length += 1;
    support::emit(&out[..length]);
    // …and C flushes it (`arithmetic.c:232`): the prompt ends without a newline, so a
    // buffered stdout would leave the player staring at nothing.
    support::flush_stdout();
}

/// C's `showstats(0)` (`arithmetic.c:166-181`) — the every-twenty-problems call. Three
/// pieces, and the guards are the shape: the score block needs `nright + nwrong > 0` (`:168`),
/// the time block separately needs `nright > 0` (`:171`, which is also why C never divides by
/// zero), and the `Press RETURN` line plus the closing newline sit **outside** both guards.
/// The score line's own format ends at the percent sign: the newline that follows belongs to
/// the time line, so a run with no correct answer ends with no newline of its own either.
fn show_stats<R: FnMut(&mut [u8]) -> Result<usize, minix_sys::Errno>>(
    reader: &mut LineReader<R>,
    score: &Score,
    qtime: u64,
) {
    if shows_score(*score) {
        let mut out = [0u8; 64];
        let mut length = 0;
        length = append(&mut out, length, b"\n\nRights ");
        length += support::utoa(score.right, &mut out[length..]);
        length = append(&mut out, length, b"; Wrongs ");
        length += support::utoa(score.wrong, &mut out[length..]);
        length = append(&mut out, length, b"; Score ");
        length += support::utoa(score.percent(), &mut out[length..]);
        length = append(&mut out, length, b"%");
        support::emit(&out[..length.min(out.len())]);
        if let Some(rate) = seconds_per_problem(qtime, score.right) {
            // `"\nTotal time %ld seconds; %.1f seconds per problem\n\n"` with `(long)qtime`
            // and `(float)qtime / nright` (`:172-173`).
            let mut line = [0u8; 96];
            let mut n = 0;
            n = append(&mut line, n, b"\nTotal time ");
            n = put_udec(&mut line, n, qtime);
            n = append(&mut line, n, b" seconds; ");
            n = put_udec(&mut line, n, rate.whole);
            line[n] = b'.';
            n += 1;
            line[n] = b'0' + rate.tenth;
            n += 1;
            n = append(&mut line, n, b" seconds per problem\n\n");
            support::emit(&line[..n.min(line.len())]);
        }
    }
    support::emit(b"Press RETURN to continue...\n");
    // C's `while(!getchar());` (`arithmetic.c:177-179`) **eats one byte** here:
    // a NUL keeps it waiting, while any other byte, end of file, or read failure
    // ends the loop (it checks neither `feof` nor `ferror`). Without eating it,
    // the next `fgets` would read that character into the problem and shift the
    // 21st answer by one.
    while reader.next_byte() == Some(0) {}
    support::emit(b"\n");
}

/// Copy `text` into `out` at `at` and report the new end; the callers clip with
/// `out.len()` at the write, so a mis-sized buffer shortens the line instead of panicking.
fn append(out: &mut [u8], at: usize, text: &[u8]) -> usize {
    let mut at = at;
    for &byte in text {
        if at < out.len() {
            out[at] = byte;
        }
        at += 1;
    }
    at
}

/// `utoa` widened to `u64`: the time line's first field is C's `%ld` on a `time_t` sum, and
/// the seconds-per-problem number is that divided by a count, so neither is bounded like the
/// two counters above.
fn put_udec(out: &mut [u8], at: usize, value: u64) -> usize {
    let mut digits = [0u8; 20];
    let mut filled = 0usize;
    let mut rest = value;
    loop {
        digits[filled] = b'0' + (rest % 10) as u8;
        filled += 1;
        rest /= 10;
        if rest == 0 {
            break;
        }
    }
    let mut at = at;
    while filled > 0 {
        filled -= 1;
        at = append(out, at, &digits[filled..filled + 1]);
    }
    at
}

/// Seeds the generator from the clock face (`support::epoch_micros`
/// twin: host clock hosted, kernel `SYS_TIMES` face on target — the C
/// seeds `random(3)` from `time(2)`, whose sequence not the clock is the
/// observable contract).
fn seed() -> u32 {
    (support::epoch_micros().unwrap_or(1) as u32) | 1
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
