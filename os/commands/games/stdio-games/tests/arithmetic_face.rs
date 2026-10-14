//! End-to-end face for the `arithmetic` binary: real process, real pipes, checked
//! against `minix3/games/arithmetic/arithmetic.c` (the option scan `:119-139`,
//! `penalise` `:303-316`, `usage` `:378-385`).
//!
//! This door pins the diagnostic channel, which is the whole point of batch 二百五十一:
//! C's three option errors are `errx(1, …)` and `usage()` is a `fprintf(stderr, …)`,
//! yet the shell sent all three to **standard output** (and its usage line was a
//! lowercase literal with the name baked in). Channel is part of an `err(3)`-family
//! leg's shape: anything that runs the quiz and keeps the stderr stream clean used
//! to see the failures mixed into the quiz output.
//!
//! One thing is deliberately NOT "fixed": C's format strings themselves start with the
//! word `arithmetic: `, so `errx`'s own `getprogname()` prefix yields
//! `arithmetic: arithmetic: invalid range.` — a doubled name. That is what Minix3
//! prints, so it is what the shell prints, and the duplication is registered as an
//! upstream wart rather than quietly improved.

use std::path::Path;
use std::process::{Command, Stdio};

fn run(args: &[&str]) -> (String, i32, String) {
    let output = Command::new(Path::new(env!("CARGO_BIN_EXE_arithmetic")))
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("the arithmetic binary should be runnable");
    (
        String::from_utf8_lossy(&output.stdout).into_owned(),
        output.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

/// Read the **next** prompt out of the accumulated output, consuming everything up to and
/// including it. C writes `"%d %c %d =   "` and then flushes (`arithmetic.c:231-232`), so the
/// marker is three tokens plus five spaces; whatever came before it (`Right!\n`, the score
/// block) stays in the caller's buffer for the assertions.
///
/// Taking the *first* marker is load-bearing: under I/O load the child can flush an answer's
/// `Right!\n` together with the following prompt while the parent is still parsing, and a
/// search for the last marker would skip the question the child is actually waiting on — the
/// answer then goes to the wrong problem and the run records a `What?` (this harness did
/// exactly that once, which is why `Wrongs 0` is asserted below).
fn take_prompt(buffer: &mut Vec<u8>) -> Option<(u64, u8, u64)> {
    const MARKER: &[u8] = b" =   ";
    let at = buffer.windows(MARKER.len()).position(|w| w == MARKER)?;
    let text = std::str::from_utf8(&buffer[..at]).ok()?;
    let mut words = text.split_whitespace().rev();
    let right: u64 = words.next()?.parse().ok()?;
    let op = *words.next()?.as_bytes().first()?;
    let left: u64 = words.next()?.parse().ok()?;
    buffer.drain(..at + MARKER.len());
    match op {
        b'+' | b'-' | b'x' | b'/' => {}
        _ => return None,
    }
    Some((left, op, right))
}

/// The answer C built the problem to have (`:191-227`): subtraction and division are
/// constructed exact, so plain integer arithmetic here matches `answer_of`.
fn answer_of_prompt(left: u64, op: u8, right: u64) -> u64 {
    match op {
        b'+' => left + right,
        b'-' => left - right,
        b'x' => left * right,
        _ => left / right,
    }
}

#[test]
fn twenty_wrong_answers_ask_the_same_problem_and_print_no_score_block() {
    // C's every-twenty counter advances only when `problem()` returns, and that happens only
    // on a correct answer (`arithmetic.c:148-151` with the retry loop at `:236-266`). Twenty
    // wrong tries therefore print twenty `What?` and never reach `showstats`; the following
    // end of input prints a single newline and exits 0 (`:241-243`).
    let (out, status, err) = run_with_stdin(&[], "99999\n".repeat(20).as_bytes());
    assert_eq!(status, 0, "stderr was {err:?}");
    assert_eq!(err, "", "答题面一行也不写标准错误");
    assert_eq!(
        out.matches("What?\n").count(),
        20,
        "同一题重复问到答对为止：`{out:?}`"
    );
    assert!(
        !out.contains("Rights "),
        "没答对一题就不该走到分数块：`{out:?}`"
    );
    assert!(out.ends_with('\n'), "EOF 那一支只补一个换行");
}

#[test]
fn the_twentieth_correct_answer_prints_the_score_block_with_both_time_numbers() {
    use minix_stdio_games::quiz::seconds_per_problem;
    use std::io::{Read, Write};

    let mut child = Command::new(Path::new(env!("CARGO_BIN_EXE_arithmetic")))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the arithmetic binary should be runnable");
    let mut stdin = child.stdin.take().expect("stdin pipe");
    let mut stdout = child.stdout.take().expect("stdout pipe");

    let mut buffer: Vec<u8> = Vec::new();
    let mut byte = [0u8; 1];
    let mut answered = 0;
    let mut closed = false;
    while answered < 20 && !closed {
        if let Some((left, op, right)) = take_prompt(&mut buffer) {
            let answer = answer_of_prompt(left, op, right);
            writeln!(stdin, "{answer}").expect("write the answer");
            stdin.flush().expect("flush the answer");
            answered += 1;
            continue;
        }
        // No prompt in what has arrived yet: pull one more byte. A zero-length read is the
        // child closing its standard output, which ends the loop instead of spinning on it.
        match stdout.read(&mut byte) {
            Ok(0) => closed = true,
            Ok(_) => buffer.push(byte[0]),
            Err(e) => panic!("reading the prompt failed: {e}"),
        }
    }
    assert!(
        !closed,
        "子过程序在第 {} 题之前就关了标准输出：`{:?}`",
        answered,
        String::from_utf8_lossy(&buffer)
    );
    drop(stdin);
    let mut tail = Vec::new();
    stdout.read_to_end(&mut tail).expect("drain the output");
    buffer.extend_from_slice(&tail);
    let out = String::from_utf8_lossy(&buffer).into_owned();
    let status = child
        .wait()
        .expect("wait for arithmetic")
        .code()
        .unwrap_or(-1);
    assert_eq!(status, 0, "output was `{out:?}`");

    // The block C writes (`:168-173`): two leading newlines, the counters, and — because
    // `nright > 0` here — the time line ending in two newlines, then the `Press RETURN`
    // line and the closing newline that live outside both guards.
    assert!(
        out.contains("\n\nRights 20; Wrongs 0; Score 100%\nTotal time "),
        "分数块按 C 的形状：`{out:?}`"
    );
    assert!(
        out.contains(" seconds per problem\n\nPress RETURN to continue...\n\n"),
        "时间行后面接 `Press RETURN` 与收尾换行：`{out:?}`"
    );

    // Both numbers of the time line come from the same `qtime`: the seconds as `%ld` and the
    // per-problem value as `%.1f` of the *float* quotient. Parsing what was printed and
    // re-deriving it through the deciding half is what pins the wiring (the elapsed seconds
    // themselves depend on how long this run took, so the count is not asserted).
    let at = out.find("Total time ").expect("the time line") + "Total time ".len();
    let rest = &out[at..];
    let secs_end = rest.find(" seconds; ").expect("the seconds field");
    let secs: u64 = rest[..secs_end].parse().expect("seconds parse");
    let rate = &rest[secs_end + " seconds; ".len()..];
    let digits = rate.split(' ').next().expect("the rate field");
    let (whole, tenth) = digits.split_once('.').expect("one decimal point");
    let expected = seconds_per_problem(secs, 20).expect("a nonzero right count");
    assert_eq!(
        (
            whole.parse::<u64>().expect("whole"),
            tenth.parse::<u8>().expect("tenth")
        ),
        (expected.whole, expected.tenth),
        "`%.1f` 的那一位得与决定半同形：印的是 `{digits}`"
    );
}

#[test]
fn an_invalid_range_is_an_errx_line_on_standard_error() {
    // `arithmetic.c:132`: `errx(1, "arithmetic: invalid range.")`; the doubled
    // program name is C's own format string plus `vwarnx`'s prefix.
    let (out, status, err) = run(&["-r", "bogus"]);
    assert_eq!(out, "", "C 的 `errx` 一行也不写标准输出");
    assert_eq!(status, 1);
    assert_eq!(err, "arithmetic: arithmetic: invalid range.\n");
}

#[test]
fn an_unknown_key_is_an_errx_line_on_standard_error() {
    // `arithmetic.c:126`: `errx(1, "arithmetic: unknown key.")` — note the period.
    let (out, status, err) = run(&["-o", "q"]);
    assert_eq!(out, "");
    assert_eq!(status, 1);
    assert_eq!(err, "arithmetic: arithmetic: unknown key.\n");
}

#[test]
fn the_usage_line_is_capital_with_the_name_from_argv_zero() {
    // `arithmetic.c:378-385`: `fprintf(stderr, "Usage: %s [-o +-x/] [-r range]\n",
    // getprogname())`. Any leftover argument reaches it too (`:138-139`'s
    // `if (argc -= optind) usage();`).
    let (out, status, err) = run(&["-x"]);
    assert_eq!(out, "");
    assert_eq!(status, 1);
    assert_eq!(err, "Usage: arithmetic [-o +-x/] [-r range]\n");

    let (_, status, err) = run(&["-r", "5", "leftover"]);
    assert_eq!(status, 1, "C 对多余操数也是 usage");
    assert_eq!(err, "Usage: arithmetic [-o +-x/] [-r range]\n");
}

#[test]
fn the_quiz_face_writes_nothing_on_standard_error() {
    // The other half of the channel fix: the quiz itself (questions, `Right!`, `What?`)
    // is C's standard-output face (`arithmetic.c:231`, `:250`, `:254`), so a run that
    // answers problems must leave standard error untouched.
    // Not asserted: any score line. C's main loop exits at end of input *without*
    // calling `showstats` (`:148-150`, `if (problem() == EOF) exit(0);`), so EOF after
    // four right answers prints no `Rights …` line — that is this shell's behaviour too
    // (checked against C before writing this test, not assumed).
    let (out, status, err) = run_with_stdin(&["-r", "1"], b"1\n9\n");
    assert_eq!(status, 0, "无输入即结束：`{err}`");
    assert_eq!(err, "", "答题面一行也不写标准错误");
    assert!(
        out.contains("Right!"),
        "第一题按 `-r 1` 的唯一答案是 1：`{out}`"
    );
    assert!(out.contains("What?"), "答错的那题按 C 印 `What?`：`{out}`");
}

fn run_with_stdin(args: &[&str], input: &[u8]) -> (String, i32, String) {
    use std::io::Write;
    use std::process::Stdio;
    let mut child = Command::new(Path::new(env!("CARGO_BIN_EXE_arithmetic")))
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the arithmetic binary should be runnable");
    child
        .stdin
        .take()
        .expect("stdin pipe")
        .write_all(input)
        .expect("feed the answers");
    let output = child.wait_with_output().expect("wait for arithmetic");
    (
        String::from_utf8_lossy(&output.stdout).into_owned(),
        output.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}
