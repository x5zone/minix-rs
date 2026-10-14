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
