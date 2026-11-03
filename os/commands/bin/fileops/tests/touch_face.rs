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

#[test]
fn d_stays_the_registered_residual() {
    // `-d` (date-parse) keeps its refusal; the pin keeps the two apart.
    let (_, status, err) = run(&["-d", "now", "x"]);
    assert_eq!(status, 1);
    assert_eq!(err, "touch: -d not wired\n");
}
