//! End-to-end face for the `touch` binary's `-h` gate, checked against
//! `minix3/usr.bin/touch/touch.c`.
//!
//! The write class of this crate runs on the none-target syscall faces, so
//! the host observes the failure shape rather than a successful time write.
//! The pin here: `-h` passes the option gate (no "not wired" refusal) and the
//! walk reaches the per-operand warn with status 1 — the same shape the
//! plain form shows when its own syscall leg is unavailable.

use std::process::{Command, Stdio};

fn touch_bin() -> &'static str {
    env!("CARGO_BIN_EXE_touch")
}

fn run(args: &[&str]) -> (String, i32, String) {
    let out = Command::new(touch_bin())
        .args(args)
        .stdin(Stdio::null())
        .output()
        .expect("spawn touch");
    (
        String::from_utf8_lossy(&out.stdout).into_owned(),
        out.status.code().expect("exit code"),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

#[test]
fn h_passes_the_option_gate_and_never_creates() {
    // C's `cflag` leg (`touch.c:162-178`): with `-h` a missing path is
    // skipped silently — no creation, no warning, and the status stays 0.
    let missing = format!("/tmp/touch-face-h-absent-{}", std::process::id());
    let (out, status, err) = run(&["-h", missing.as_str()]);
    assert_eq!((out.as_str(), status, err.as_str()), ("", 0, ""));
    assert!(!err.contains("not wired"), "-h 不该再报未接线：{err}");
}

/// `-d` parses through the `parsedate` port and then reaches the per-operand
/// write leg, which is unavailable on the host — the same shape the plain
/// form shows.
#[test]
fn d_passes_the_option_gate_to_the_operand_walk() {
    let (out, status, err) = run(&["-d", "now", "/tmp/touch-face-d-absent"]);
    assert_eq!(out.as_str(), "");
    assert_eq!(status, 1);
    assert_eq!(err, "touch: /tmp/touch-face-d-absent\n");
    assert!(!err.contains("not wired"), "-d 不该再报未接线：{err}");
}

/// `stime_arg0` (`touch.c:214-222`): an unparsable argument, and a parser
/// answer of -1 (the real epoch second), both answer the `Could not parse`
/// line with C's backtick/apostrophe quoting.
#[test]
fn d_rejects_unparsable_and_minus_one() {
    for bad in ["not a date", "@-1"] {
        let (_, status, err) = run(&["-d", bad, "x"]);
        assert_eq!(status, 1, "{bad}");
        assert_eq!(err, format!("touch: Could not parse `{bad}'\n"), "{bad}");
    }
}

/// The GNU long spellings are not in C's getopt string; `--date` therefore
/// hits the unknown-option path and answers usage.
#[test]
fn a_double_dash_word_answers_usage() {
    let (_, status, err) = run(&["--date", "now", "x"]);
    assert_eq!(status, 1);
    assert_eq!(
        err,
        "Usage: touch [-acfhm] [-d|--date datetime] [-r|--reference file] [-t time] file ...\n"
    );
}
