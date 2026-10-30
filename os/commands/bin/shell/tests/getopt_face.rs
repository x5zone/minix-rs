//! End-to-end face for the `getopt` binary: real process, real pipes, checked
//! against `minix3/usr.bin/getopt/getopt.c` (the print loop `:24-41`) driving
//! `minix3/lib/libc/stdlib/getopt.c`.
//!
//! This door exists to pin the **output face**, which the `emit` seam fix first
//! made observable at all. `getopt`'s whole product is one line on
//! standard output, and the shell crate's `bin_support::emit` had no hosted twin: on a
//! host build the bytes went into the substitute transport, so the line was empty — and
//! because `emit` exits 1 when its write fails, the command also failed. Measured before
//! the fix: `getopt "ab:" -a` produced 0 bytes on stdout and exit status 1; afterwards it
//! produces ` -a --\n` and exit 0. That asymmetry is the reason the seam carries two
//! implementations rather than one: the diagnostics face (`warn`) already had its twin,
//! the output face had not.
//!
//! The expected bytes come from C's own `printf` calls, not from this tree's habit:
//! `" -%c %s"` per option-with-value, `" -%c"` per flag, then `" --"`, then one
//! `" %s"` per remaining operand, then `"\n"` — and on the unknown-option path C still
//! prints the tail (`status = 1` only changes the exit code), which is why the
//! `-q` case below expects ` --\n` on stdout *together with* exit 1.

use std::path::Path;
use std::process::{Command, Stdio};

fn run(args: &[&str]) -> (String, i32, String) {
    let output = Command::new(Path::new(env!("CARGO_BIN_EXE_getopt")))
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("the getopt binary should be runnable");
    (
        String::from_utf8_lossy(&output.stdout).into_owned(),
        output.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

#[test]
fn the_parsed_line_reaches_standard_output() {
    // `getopt.c:33`: `printf(" -%c", c)` for an option without an argument, then
    // `printf(" --")` (`:36`) and the terminating newline (`:39`).
    let (out, status, err) = run(&["ab:", "-a"]);
    assert_eq!(status, 0, "`getopt(3)` 一路认得这些字母：`{err}`");
    assert_eq!(err, "");
    assert_eq!(
        out, " -a --\n",
        "宿主面必须看得见这一行（改前这里是空字节且退 1）"
    );
}

#[test]
fn values_and_trailing_operands_are_laid_out_as_c_prints_them() {
    // `getopt.c:31` prints `" -%c %s"` for an option with an argument; `:37-38`
    // print each leftover operand after the ` --` marker (`:36`), in order.
    let (out, status, _) = run(&["ab:", "-a", "-b", "zzz"]);
    assert_eq!(status, 0);
    assert_eq!(out, " -a -b zzz --\n");

    let (out, status, _) = run(&["ab:", "-b", "zzz", "foo", "bar"]);
    assert_eq!(status, 0);
    assert_eq!(out, " -b zzz -- foo bar\n", "操作数按原序跟在 `--` 之后");
}

#[test]
fn no_option_at_all_still_prints_the_marker_line() {
    // With nothing to report, C's loop prints nothing and the tail (`" --"` at
    // `:36`, then `"\n"` at `:39`) is the whole line — the command still succeeds.
    let (out, status, err) = run(&["ab:"]);
    assert_eq!(status, 0);
    assert_eq!(err, "");
    assert_eq!(out, " --\n");
}

#[test]
fn an_unknown_option_keeps_the_tail_and_fails() {
    // `getopt.c:26-28` sets `status = 1` on `?` and breaks out; the tail (`:36-39`)
    // is still printed before `exit(status)` (`:40`), and `getopt(3)` itself owns the message on
    // standard error. A host build that could not write stdout would have shown
    // only half of this.
    let (out, status, err) = run(&["ab:", "-q"]);
    assert_eq!(status, 1, "`status = 1` 只改退码，不改那一行");
    assert_eq!(out, " --\n");
    assert!(
        err.contains("q"),
        "`getopt(3)` 的报错该带上那个字母：`{err}`"
    );
}
