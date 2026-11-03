//! End-to-end face for the `date` binary's host-safe set-path shapes,
//! checked against `minix3/bin/date/date.c`.
//!
//! The clock-mutating leg itself is not exercised here: it would move the
//! host's real clock. The two shapes that never reach `clock_settime` are
//! deterministic and pinned instead —
//! ① `-j` with a fully specified twelve-digit operand interprets and prints
//!    it without touching the clock (`date.c:330-333`);
//! ② a rejected operand prints C's `badcanotime` (`date.c:174-182`): the
//!    sentence line, the operand, the caret, and the usage line, exit 1.

use std::process::{Command, Stdio};

fn date_bin() -> &'static str {
    env!("CARGO_BIN_EXE_date")
}

fn run(args: &[&str]) -> (String, i32, String) {
    let out = Command::new(date_bin())
        .args(args)
        .stdin(Stdio::null())
        .output()
        .expect("spawn date");
    (
        String::from_utf8_lossy(&out.stdout).into_owned(),
        out.status.code().expect("exit code"),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

#[test]
fn a_fully_specified_operand_with_j_prints_the_composed_time() {
    // "202412310000" is twelve digits: CC=20 YY=24 MM=12 DD=31 HH=00 MM=00 —
    // every field is given, so the printout cannot depend on the clock.
    let (out, code, err) = run(&["-j", "202412310000"]);
    assert_eq!(
        (out.as_str(), code, err.as_str()),
        ("Tue Dec 31 00:00:00 UTC 2024\n", 0, "")
    );
}

#[test]
fn a_rejected_operand_prints_the_badcanotime_block_and_exits_one() {
    // Month 13 rides in the eight-digit form; the caret sits under the
    // second month digit (C's `p - op - 1` with `p` after the pair).
    let (out, code, err) = run(&["13101530"]);
    assert_eq!(out.as_str(), "");
    assert_eq!(code, 1);
    assert_eq!(
        err,
        "date: Month out of range in canonical time\n13101530\n ^\n\
         Usage: date [-ajnu] [-d date] [-r seconds] [+format] [[[[[[CC]yy]mm]dd]HH]MM[.SS]]\n"
    );
}

/// The `-a` gradual-adjust leg on the host: the transport is the trap face,
/// which fails — C's `err(EXIT_FAILURE, "adjtime")` (`date.c:350`) then
/// prints the three-part line and exits 1. The exact errno behind the
/// sentence belongs to the transport, so the pin is the shape, not the
/// number.
#[test]
fn the_adjust_leg_reports_the_adjtime_sentence_and_exits_one() {
    let out = Command::new(date_bin())
        .args(["-a", "12101030"])
        .stdin(Stdio::null())
        .output()
        .expect("spawn date");
    let err = String::from_utf8_lossy(&out.stderr).into_owned();
    assert!(
        out.status.code() == Some(1),
        "exit={:?} stderr={err}",
        out.status.code()
    );
    assert!(
        err.starts_with("date: adjtime: ") && !err.ends_with(": \n"),
        "该是 err(3) 三段形：{err}"
    );
}
