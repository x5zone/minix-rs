//! End-to-end face for the `primes` binary: real process, real pipes, checked
//! against `minix3/games/primes/primes.c` (the operand handling `:129-172`,
//! `read_num_buf` `:176-205`, `usage` `:339-343`).
//!
//! Before this batch the shell had **no** diagnostic at all: every bad argument
//! reached `terminate(1)` in silence, so a typo looked identical to an empty
//! interval. C splits those failures by family, and the split is the content:
//! a word `strtoumax` cannot convert at all sets `errno`, so C prints
//! `primes: <word>: Invalid argument` (`primes.c:138`/`:145`/`:160` via
//! `err(1, "%s", argv[i])`), while digits followed by junk leave `errno` alone and
//! print `primes: <word>: illegal numeric format.` (`:140`, note the period) —
//! the first is the `err(3)` family, the second `errx(3)`.
//!
//! `usage()` is the other family detail worth pinning: C writes it as a literal
//! (`:341`), so renaming the binary moves the `err`/`errx` lines and leaves the
//! usage line alone — the opposite of `rm`/`cp`/`mv`.
//!
//! Two of C's faces are declared out of reach instead of faked: the `-d`/`-h`
//! output flags (this shell refuses them by name) and the zero-operand path that
//! reads a start value from standard input (`read_num_buf`), which this tree
//! replaces with the 2..cap interval.

use std::fs;
use std::os::unix::fs::PermissionsExt as _;
use std::path::Path;
use std::process::{Command, Stdio};

const USAGE: &str = "usage: primes [-dh] [start [stop]]\n";

fn run_at(program: &Path, args: &[&str]) -> (String, i32, String) {
    let output = Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("the primes binary should be runnable");
    (
        String::from_utf8_lossy(&output.stdout).into_owned(),
        output.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

fn run(args: &[&str]) -> (String, i32, String) {
    run_at(Path::new(env!("CARGO_BIN_EXE_primes")), args)
}

#[test]
fn a_bounded_run_prints_primes_and_stays_silent_on_stderr() {
    // The success path C shares with this tree: one prime per line from `start`
    // up to and including `stop` (`primes.c:173` → `primes(start, stop)`).
    let (out, status, err) = run(&["10", "20"]);
    assert_eq!(status, 0);
    assert_eq!(err, "", "正常路一声不出");
    assert_eq!(
        out, "11\n13\n17\n19\n",
        "区间的两端都算（`primes.c:173` 的注释）"
    );
}

#[test]
fn digits_with_trailing_junk_is_the_errx_line() {
    // `primes.c:140`: `errx(1, "%s: illegal numeric format.", argv[0])` — digits
    // converted, `errno` untouched, so the line ends with C's own period and no
    // strerror sentence.
    let (out, status, err) = run(&["12a"]);
    assert_eq!(out, "");
    assert_eq!(status, 1);
    assert_eq!(err, "primes: 12a: illegal numeric format.\n");
}

#[test]
fn a_word_that_cannot_convert_carries_the_number_sentence() {
    // `primes.c:138` with `_strtol.h:69` (`errno = EINVAL`): this one is
    // `err(1, "%s", argv[0])`, so the sentence belongs to the number.
    let (out, status, err) = run(&["abc"]);
    assert_eq!(out, "");
    assert_eq!(status, 1);
    assert_eq!(err, "primes: abc: Invalid argument\n");
}

#[test]
fn an_inverted_interval_is_refused_by_phrase() {
    // `primes.c:171-172`: `errx(1, "start value must be less than stop value.")`.
    let (out, status, err) = run(&["5", "4"]);
    assert_eq!(out, "");
    assert_eq!(status, 1);
    assert_eq!(err, "primes: start value must be less than stop value.\n");
}

#[test]
fn a_stop_beyond_the_cap_names_the_cap() {
    // `primes.c:149` prints C's `SPSPMAX`; this tree prints its own declared cap
    // (1,000,000 — the difference is registered in 22 篇, the shape is C's).
    let (out, status, err) = run(&["2", "2000000"]);
    assert_eq!(out, "");
    assert_eq!(status, 1);
    assert_eq!(err, "primes: 2000000: stop value too large (>1000000).\n");
}

#[test]
fn an_operand_starting_with_a_hyphen_is_the_negative_phrase() {
    // `primes.c:133`: the check runs on the operands *after* option scanning, so
    // `primes 5 -3` reaches it while `primes -5` does not (an option letter that
    // `getopt` does not know answers `usage()` first — see the next test).
    let (_, status, err) = run(&["5", "-3"]);
    assert_eq!(status, 1);
    assert_eq!(err, "primes: negative numbers aren't permitted.\n");
}

#[test]
fn unknown_letters_and_extra_operands_answer_the_hardcoded_usage() {
    // `primes.c:115-116` (`default: usage()`) and `:167-168`.
    let (out, status, err) = run(&["-q"]);
    assert_eq!(out, "");
    assert_eq!(status, 1);
    assert_eq!(err, USAGE);

    let (_, status, err) = run(&["1", "2", "3"]);
    assert_eq!(status, 1);
    assert_eq!(err, USAGE);

    // A refused option we know but do not implement is its own line, not usage.
    let (_, status, err) = run(&["-h"]);
    assert_eq!(status, 1);
    assert_eq!(
        err,
        "primes: -d/-h not wired (difference column, hexadecimal output)\n"
    );
}

#[test]
fn the_usage_line_ignores_argv_zero_while_the_error_lines_do_not() {
    // The two halves of C's habit in one command: `err`/`errx` print
    // `getprogname()` (`:341` is a literal), so a renamed copy moves one line and
    // not the other.
    let copied = std::env::temp_dir().join(format!("primes-face-copy-{}", std::process::id()));
    fs::copy(Path::new(env!("CARGO_BIN_EXE_primes")), &copied).expect("copy the binary");
    fs::set_permissions(&copied, fs::Permissions::from_mode(0o755)).expect("make it run");

    let name = copied.file_name().unwrap().to_str().unwrap().to_string();
    let (_, _, err) = run_at(&copied, &["abc"]);
    assert_eq!(err, format!("{name}: abc: Invalid argument\n"));

    let (_, _, err) = run_at(&copied, &["-q"]);
    assert_eq!(err, USAGE, "C 的 usage 是硬写字面量，不跟 `argv[0]`");

    let _ = fs::remove_file(&copied);
}
