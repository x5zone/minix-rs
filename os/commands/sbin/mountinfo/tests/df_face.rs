//! End-to-end face for the `df` binary's host-safe shapes, checked against
//! `minix3/bin/df/df.c`.
//!
//! The mount-table legs are the none-target syscall faces, so the host sees
//! the failure sentences; the option parsing runs before any of that and is
//! pinned directly:
//! ① `-P -h` is refused by name (C reaches an `assert(hflag == 0)` in the
//!    `-P` arm, `df.c:455` — an abort this shell turns into a sentence);
//! ② `-h` passes the option gate and reaches the mount-table fetch, which
//!    fails on the host with C's sentence (`df.c:160-161`);
//! ③ an unknown letter still answers the not-wired refusal.

use std::process::{Command, Stdio};

fn df_bin() -> &'static str {
    env!("CARGO_BIN_EXE_df")
}

fn run(args: &[&str]) -> (String, i32, String) {
    let out = Command::new(df_bin())
        .args(args)
        .stdin(Stdio::null())
        .output()
        .expect("spawn df");
    (
        String::from_utf8_lossy(&out.stdout).into_owned(),
        out.status.code().expect("exit code"),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

#[test]
fn p_and_h_cannot_be_combined() {
    let (out, status, err) = run(&["-P", "-h"]);
    assert_eq!(out.as_str(), "");
    assert_eq!(status, 1);
    assert_eq!(err, "df: -P and -h cannot be combined\n");
}

#[test]
fn h_passes_the_option_gate_to_the_mount_table() {
    let (out, status, err) = run(&["-h"]);
    assert_eq!(out.as_str(), "");
    assert_eq!(status, 1);
    assert_eq!(
        err,
        "df: retrieving mounted file system information failed\n"
    );
}

#[test]
fn an_unknown_letter_still_answers_the_refusal() {
    let (_, status, err) = run(&["-Z"]);
    assert_eq!(status, 1);
    assert_eq!(err, "df: option not wired (POSIX -Pk view only)\n");
}

/// `-P -i` has a real C sentence (`df.c:143-144`), unlike the -P/-h pair
/// whose C path only asserts.
#[test]
fn p_and_i_together_are_refused_with_c_sentence() {
    let (out, status, err) = run(&["-P", "-i"]);
    assert_eq!(out.as_str(), "");
    assert_eq!(status, 1);
    assert_eq!(err, "only one of -P and -i may be specified\n");
}

/// `-i` passes the option gate and reaches the mount-table fetch.
#[test]
fn i_passes_the_option_gate_to_the_mount_table() {
    let (out, status, err) = run(&["-i"]);
    assert_eq!(out.as_str(), "");
    assert_eq!(status, 1);
    assert_eq!(
        err,
        "df: retrieving mounted file system information failed\n"
    );
}
