//! Minix-RS arithmetic — interactive arithmetic quiz.
//!
//! Ground truth: `minix3/games/arithmetic/arithmetic.c`. Questions print as
//! `left op right =   ` (line 231) with operands in `0..range` (`-r`,
//! default 10, line 102) and operators from the key set (`-o`, default
//! `+-` of `+-x/`, line 101). Answers: non-numeric input asks again with
//! `Please type a number.` (line 246), a correct answer answers
//! `Right!` (line 250), a wrong one `What?` and the same problem is asked
//! again (lines 254 and following); end of input prints the score
//! `\n\nRights %d; Wrongs %d; Score %d%%` (line 169), which also fires
//! every twenty problems.
//!
//! Two deviations are declared rather than smuggled in: the operand
//! source is a seeded linear congruential generator (the C program uses
//! `random(3)`, whose sequence is time-seeded and therefore not an
//! observable contract either), and the wrong-answer penalty hints
//! (`penalise`, line 255) are not modelled yet — the same problem is
//! simply asked again.

#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

extern crate alloc;

use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
#[path = "../bin_support.rs"]
mod support;

use minix_stdio_games::quiz::{
    DEFAULT_RANGE, Drawn, Operator, Penalties, Question, Score, answer_of, charged_operands,
    parse_range, slot_of,
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
                        support::emit(b"arithmetic: invalid range.\n");
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
                            support::emit(b"arithmetic: unknown key.\n");
                            support::terminate(1);
                        }
                    }
                }
            }
            _ => {
                support::emit(b"usage: arithmetic [-o +-x/] [-r range]\n");
                support::terminate(1);
            }
        }
        index += 1;
    }

    let mut random = seed();
    let mut score = Score::default();
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
        loop {
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
        asked += 1;
        if asked % 20 == 0 {
            print_score(&score);
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
            support::warn(b"arithmetic: bug: inconsistent penalties.\n");
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

/// Prints `left op right =   ` (arithmetic.c:231).
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
    support::emit(&out[..length]);
}

/// Prints the running score (arithmetic.c:169).
fn print_score(score: &Score) {
    let mut out = [0u8; 64];
    let mut length = 0;
    for &byte in b"\n\nRights " {
        out[length] = byte;
        length += 1;
    }
    length += support::utoa(score.right, &mut out[length..]);
    for &byte in b"; Wrongs " {
        out[length] = byte;
        length += 1;
    }
    length += support::utoa(score.wrong, &mut out[length..]);
    for &byte in b"; Score " {
        out[length] = byte;
        length += 1;
    }
    length += support::utoa(score.percent(), &mut out[length..]);
    out[length] = b'%';
    length += 1;
    support::emit(&out[..length]);
    support::emit(b"\n");
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
