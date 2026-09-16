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

#[path = "../bin_support.rs"]
mod support;

use minix_stdio_games::quiz::{answer_of, parse_range, Operator, Question, Score, DEFAULT_RANGE};
use minix_sys::read;
use support::LineReader;

const KEYS: [Operator; 4] = [Operator::Add, Operator::Subtract, Operator::Multiply, Operator::Divide];

fn main() {
    let argv: Vec<String> = std::env::args().collect();
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
    let mut input = [0u8; 64];
    let mut reader = LineReader::new(
        |chunk| read(support::STDIN, chunk).map_err(|_| ()),
        &mut input,
    );
    let mut asked = 0u32;
    loop {
        let (question, seed_next) = next_question(&mut random, range, &keys);
        random = seed_next;
        ask(&question);
        loop {
            let has_line = reader.next_line().unwrap_or(false);
            if !has_line {
                support::emit(b"\n");
                print_score(&score);
                support::terminate(0);
            }
            // C: skip leading whitespace, take the decimal digit run
            // (`atoi` semantics — trailing text is ignored), an empty run
            // re-prompts (arithmetic.c:238-247).
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
            let word = std::str::from_utf8(&line[start..end]).unwrap_or("0");
            let given: u64 = word.parse().unwrap_or(u64::MAX);
            if given == answer_of(question) as u64 {
                score.right += 1;
                support::emit(b"Right!\n");
            } else {
                score.wrong += 1;
                support::emit(b"What?\n");
            }
            break;
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

/// Picks the next problem: operands uniform in `0..range`, the operator
/// uniform over the key set, and division constructed divisible
/// (`answer_of` never sees a remainder case this way).
fn next_question(random: &mut u32, range: u32, keys: &[usize]) -> (Question, u32) {
    let op = KEYS[keys[*random as usize % keys.len()]];
    *random = random.wrapping_mul(1_103_515_245).wrapping_add(12_345);
    let left = *random % range;
    *random = random.wrapping_mul(1_103_515_245).wrapping_add(12_345);
    let right = *random % range;
    let question = match op {
        Operator::Divide => {
            // right in 1..=range so the divisor is never zero; the dividend
            // is built as right * quotient, resampled on overflow.
            let divisor = right % range.max(2) + 1;
            for _ in 0..8 {
                *random = random.wrapping_mul(1_103_515_245).wrapping_add(12_345);
                let quotient = *random % range + 1;
                if let Some(dividend) = divisor.checked_mul(quotient) {
                    return (
                        Question {
                            left: dividend,
                            right: divisor,
                            op,
                        },
                        *random,
                    );
                }
            }
            Question {
                left: divisor,
                right: divisor,
                op,
            }
        }
        _ => Question { left, right, op },
    };
    (question, *random)
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

/// Seeds the generator from the host clock (std seam; the target build
/// seeds from a kernel clock read once the clock face lands).
fn seed() -> u32 {
    use std::time::SystemTime;
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.subsec_nanos() ^ (d.as_secs() as u32))
        .unwrap_or(1)
        | 1
}
