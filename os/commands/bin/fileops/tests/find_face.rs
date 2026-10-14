//! End-to-end face for the `find` binary: real process, real pipes, checked against
//! `minix3/usr.bin/find/{main.c,option.c,function.c,operator.c}`.
//!
//! `find`'s parse-side reports are all `errx(1, …)` lines: program name from
//! `getprogname()`, C's phrase, and **no** `strerror` sentence (C never consulted
//! `errno` there). Until this batch the shell carried its own invented wording — e.g.
//! ``find: unknown option `-nope'`` where C says ``find: -nope: unknown option``
//! (`option.c:131`), and an invented `find: (\` unmatched` where C says
//! `find: (: missing closing ')'` (`operator.c:96-98`). Every line here is therefore
//! checked byte-for-byte against the C source, including the punctuation C chose.
//!
//! These legs need no filesystem, so all of them are reachable on a host build —
//! unlike the walk warnings (`find.c:230`'s `warnx` with `strerror`), whose number is
//! still dropped by the probe trait; that gap is registered in the shell's header and
//! in `rewrite-notes/18-stage-commands/todo.md`, and this door does not cover it.

use std::fs;
use std::os::unix::fs::PermissionsExt as _;
use std::path::Path;
use std::process::{Command, Stdio};

fn run_at(program: &Path, args: &[&str]) -> (String, i32, String) {
    let output = Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("the find binary should be runnable");
    (
        String::from_utf8_lossy(&output.stdout).into_owned(),
        output.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

fn run(args: &[&str]) -> (String, i32, String) {
    run_at(Path::new(env!("CARGO_BIN_EXE_find")), args)
}

/// One `errx`-shaped report: exactly the line C prints, exit 1, nothing on stdout.
fn assert_errx_line(args: &[&str], expected: &str) {
    let (out, status, err) = run(args);
    assert_eq!(out, "", "`{args:?}` 的报错路不写标准输出");
    assert_eq!(status, 1, "`errx(1, …)` 是退 1：`{args:?}`");
    assert_eq!(err, format!("{expected}\n"), "报文逐字对 C");
}

#[test]
fn the_usage_line_carries_the_program_name_and_a_capital_usage() {
    // `main.c:159-163`: `fprintf(stderr, "Usage: %s [-H | -L | -P] [-dEhsXx] [-f
    // file] file [file ...] [expression]\n", getprogname())`. Capital `U`, name
    // inside the line, and it fires when no starting path is given (`:147-149`).
    assert_errx_line(
        &[],
        "Usage: find [-H | -L | -P] [-dEhsXx] [-f file] file [file ...] [expression]",
    );
    assert_errx_line(
        &["-print"],
        "Usage: find [-H | -L | -P] [-dEhsXx] [-f file] file [file ...] [expression]",
    );
}

#[test]
fn an_unknown_option_puts_the_name_first_with_no_backquotes() {
    // `option.c:131`: `errx(1, "%s: unknown option", *argv)`.
    assert_errx_line(&[".", "-nope"], "find: -nope: unknown option");
}

#[test]
fn a_missing_option_argument_names_the_option() {
    // `option.c:134`: `errx(1, "%s: requires additional arguments", *--argv)`.
    assert_errx_line(
        &[".", "-name"],
        "find: -name: requires additional arguments",
    );
}

#[test]
fn the_exec_terminator_line_quotes_both_terminators() {
    // `function.c:647-651`: the scan gives up on `!*ap` with
    // `errx(1, "%s: no terminating \";\" or \"+\"", isok ? "-ok" : "-exec")` — the
    // `-ok` spelling is C's too, because the `+` here does not follow `{}`.
    assert_errx_line(
        &[".", "-exec", "echo"],
        "find: -exec: no terminating \";\" or \"+\"",
    );
    assert_errx_line(
        &[".", "-ok", "echo", "+"],
        "find: -ok: no terminating \";\" or \"+\"",
    );
}

#[test]
fn plus_after_braces_is_permitted_for_exec_and_refused_for_ok() {
    // `function.c:666-669`: `errx(1, "-ok: terminating \"+\" not permitted.")` —
    // note the trailing period C writes, and that `-exec … {} +` is legal.
    assert_errx_line(
        &[".", "-ok", "echo", "{}", "+"],
        "find: -ok: terminating \"+\" not permitted.",
    );
    // C accepts `-exec … {} +` (it sets `F_PLUSSET` and goes on), so the run must not
    // produce a parse-side refusal for it. The host walk itself fails here, so the
    // exit status is the traversal's answer, not this leg's: check the line instead.
    let (_, _, err) = run(&[".", "-exec", "echo", "{}", "+"]);
    assert!(
        !err.contains("no terminating") && !err.contains("not permitted"),
        "`-exec … {{}} +` 是被收下的形状，不该出报文：`{err}`"
    );
}

#[test]
fn the_paren_lines_are_c_phrases_not_invented_ones() {
    // Three separate C lines, three separate shapes:
    // `operator.c:96-98` `err(1, "(: missing closing ')'")`, `operator.c:106-108`
    // `errx(1, "(): empty inner expression")`, `operator.c:149`
    // `errx(1, "): no beginning '('")`. The first is `err` in C — its sentence would
    // be whatever `errno` the last call left, so the shell stops at the phrase and
    // says so in its header rather than inventing a number.
    assert_errx_line(&[".", "(", "-print"], "find: (: missing closing ')'");
    assert_errx_line(&[".", "(", ")"], "find: (): empty inner expression");
    assert_errx_line(&[".", "-print", ")"], "find: ): no beginning '('");
}

#[test]
fn a_bang_without_an_operand_uses_c_phrase() {
    // `operator.c:197-199`: `errx(1, "!: no following expression")`.
    assert_errx_line(&[".", "!"], "find: !: no following expression");
}

#[test]
fn every_line_follows_argv_zero_like_getprogname_does() {
    // The prefix is C's `getprogname()` (seeded by `crt0-common.c:156-165`), so a
    // copy under another name moves all of them — including the usage line, which
    // embeds the name inside it (`Usage: %s …`) rather than prefixing it.
    let copied = std::env::temp_dir().join(format!("find-face-copy-{}", std::process::id()));
    fs::copy(Path::new(env!("CARGO_BIN_EXE_find")), &copied).expect("copy the binary");
    fs::set_permissions(&copied, fs::Permissions::from_mode(0o755)).expect("make it run");
    let name = copied.file_name().unwrap().to_str().unwrap().to_string();

    let (_, _, err) = run_at(&copied, &[]);
    assert_eq!(
        err,
        format!("Usage: {name} [-H | -L | -P] [-dEhsXx] [-f file] file [file ...] [expression]\n")
    );

    let (_, _, err) = run_at(&copied, &[".", "-nope"]);
    assert_eq!(err, format!("{name}: -nope: unknown option\n"));

    let _ = fs::remove_file(&copied);
}
