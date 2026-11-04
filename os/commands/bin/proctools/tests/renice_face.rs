//! End-to-end face for the `renice` binary: real process, real pipes, checked
//! against `minix3/usr.bin/renice/renice.c`.
//!
//! The reachable-on-host legs are the argument/error faces; the numeric
//! `donice` path needs a live PM (`getpriority` answers through the IPC
//! stubs on a host build), so those pins are the failure shapes both sides
//! share. The `-u` name resolution reads C `_PATH_PASSWD` (`/etc/passwd`);
//! a hosted run cannot read it (the VFS IPC stubs answer errors), which is
//! byte-identical to C's `getpwnam` answering NULL — the
//! `warnx("%s: unknown user")` line and the error count.

use std::process::{Command, Stdio};

fn run(args: &[&str]) -> (String, i32, String) {
    let output = Command::new(env!("CARGO_BIN_EXE_renice"))
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("the renice binary should be runnable");
    (
        String::from_utf8_lossy(&output.stdout).into_owned(),
        output.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

#[test]
fn the_usage_line_is_c_capital_with_brackets_and_the_program_name() {
    // `renice.c:158-162`: three fprintf pieces; `argc < 2` after the shift
    // (`:64`) fires it. One priority and no target is still `< 2`.
    let (out, status, err) = run(&["5"]);
    assert_eq!(out, "");
    assert_eq!(status, 1);
    assert_eq!(
        err,
        "Usage: renice [<priority> | -n <incr>] [[-p] <pids>...] [-g <pgrp>...] [-u <user>...]\n"
    );
}

#[test]
fn a_bad_priority_token_is_quoted_after_the_phrase() {
    // `getnum("priority", …)`'s `warnx("Bad %s argument: %s", com, str)`
    // (`renice.c:113-115`), then main `return 1` (`:72`).
    let (out, status, err) = run(&["xyz", "1"]);
    assert_eq!(out, "");
    assert_eq!(status, 1);
    assert_eq!(err, "renice: Bad priority argument: xyz\n");
}

#[test]
fn a_user_target_with_no_answer_is_an_unknown_user_error() {
    // `renice.c:96-105`: `getpwnam` answers NULL (here: the database is
    // unreadable on a hosted build — the same face as an absent account),
    // `warnx("%s: unknown user", *argv)` prints, `errs` counts one, and the
    // exit is `EXIT_FAILURE` (`:111`). A numeric target after it still gets
    // attempted (`continue`, not `exit`).
    let (out, status, err) = run(&["5", "-u", "nosuchuser"]);
    assert_eq!(out, "");
    assert_eq!(status, 1);
    assert_eq!(err, "renice: nosuchuser: unknown user\n");
}

#[test]
fn a_non_numeric_target_in_the_process_scope_is_a_bad_value() {
    // `renice.c:106-109`: the pid `getnum` fails →
    // `warnx("%s: bad value", *argv)`, one error, exit failure.
    let (out, status, err) = run(&["5", "notapid"]);
    assert_eq!(out, "");
    assert_eq!(status, 1);
    assert_eq!(err, "renice: notapid: bad value\n");
}

#[test]
fn the_numeric_leg_reports_the_priority_read_failure() {
    // On a host build `getpriority` answers through the IPC stubs, so
    // C `donice`'s failure arm (`renice.c:138-141`) fires: `warn("%d:
    // getpriority", who)`, one error, exit failure — the same sentence
    // shape a real EPERM would produce on the machine.
    let (out, status, err) = run(&["5", "-p", "1"]);
    assert_eq!(out, "");
    assert_eq!(status, 1);
    assert_eq!(err, "renice: 1: getpriority\n");
}
