//! End-to-end face for the `env` binary, checked against
//! `minix3/usr.bin/env/env.c`.
//!
//! The print branch is the pure half (`-i`, assignments, `NAME=VALUE`
//! lines). The command branch is the exec leg (`env.c:78-85`: `execvp`
//! under the modified environment, `err` with 127 for ENOENT and 126 for
//! every other failure). On a host build the exec syscall is the none-target
//! stub — the run therefore exercises the search and the failure reporting
//! exactly like C's failure path, while a successful exec (which replaces
//! the process and never returns) is only reachable on the target.

use std::process::{Command, Stdio};

fn run(args: &[&str], clear_env: bool) -> (String, i32, String) {
    let mut command = Command::new(env!("CARGO_BIN_EXE_env"));
    command.args(args).stdin(Stdio::null());
    if clear_env {
        command.env_clear();
    }
    let out = command.output().expect("spawn env");
    (
        String::from_utf8_lossy(&out.stdout).into_owned(),
        out.status.code().expect("exit code"),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

/// The print branch: a cleared environment plus assignments is exactly the
/// lines `env.c:87-88` prints.
#[test]
fn assignments_print_one_name_value_line_each() {
    let (out, code, err) = run(&["A=1", "B=2"], true);
    assert_eq!(out, "A=1\nB=2\n");
    assert_eq!((code, err.as_str()), (0, ""));
}

/// `-i` clears the inherited environment first (`env.c:66-68`).
#[test]
fn dash_i_clears_the_inherited_environment() {
    let (out, code, err) = run(&["-i", "A=1"], true);
    assert_eq!(out, "A=1\n");
    assert_eq!((code, err.as_str()), (0, ""));
}

/// A command operand now passes the option gate and reaches the exec leg.
/// On a host build the exec stub answers EIO, which C's mapping sends to
/// exit 126 (`errno != ENOENT`) with the `env: <command>: <sentence>` line
/// (`env.c:82-83`).
#[test]
fn a_command_operand_reaches_the_exec_leg_instead_of_a_refusal() {
    let (out, code, err) = run(&["FOO=1", "/no-such-command-x"], true);
    assert_eq!(out, "");
    assert_eq!(code, 126);
    assert_eq!(err, "env: /no-such-command-x: Input/output error\n");
    assert!(!err.contains("not wired"), "命令腿不再报未接线：{err}");
}

/// The empty command name is not looked up at all: `execvp` answers ENOENT
/// (`execvp.c:84-87`), which `env` maps to 127.
#[test]
fn an_empty_command_name_answers_the_not_found_exit_code() {
    let (out, code, err) = run(&[""], true);
    assert_eq!(out, "");
    assert_eq!(code, 127);
    assert_eq!(err, "env: : No such file or directory\n");
}
