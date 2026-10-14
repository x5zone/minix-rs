//! End-to-end face for the `join` binary: real process, real pipes, checked against
//! `minix3/usr.bin/join/join.c` (the operand handling `:236-253`, the mutually
//! exclusive check `:235`, `usage` `:630-637`).
//!
//! `join` sits on the `err(3)`/`warnx(3)` side of the family boundary this crate keeps
//! having to relearn: the file leg is `err(1, "%s", *argv)` (`join.c:244`, `:249`), so
//! the line is `join: <operand>: <strerror(errno)>` — program name AND the number's
//! sentence. The shell printed `join: cannot open file 1`: the operand was gone, the
//! number was gone, and the words were invented. `usage()` is two lines in C with
//! `getprogname()` in the first one (`:632-635`); the shell had one line whose option
//! list did not match C either.
//!
//! One leg was missing outright: C checks `F1->fp == stdin && F2->fp == stdin` and
//! `errx(1, "only one input file may be stdin")` (`:250-251`). Without it the two
//! operands both read the same stream. Checked here before any bytes are compared,
//! because the whole point is that nothing is read.
//!
//! Unlike the file-ops shells, a hosted `join` can open real files, so the success path
//! is exercised too — but only as "stdout carries the join, stderr stays silent", which
//! is what the diagnostic legs owe.

use std::fs;
use std::os::unix::fs::PermissionsExt as _;
use std::path::Path;
use std::process::{Command, Stdio};

const USAGE_FIRST: &str = "usage: join [-a fileno | -v fileno] [-e string] [-j fileno field]\n";
const USAGE_SECOND: &str = "            [-o list] [-t char] [-1 field] [-2 field] file1 file2\n";

fn run_at(program: &Path, args: &[&str]) -> (String, i32, String) {
    let output = Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("the join binary should be runnable");
    (
        String::from_utf8_lossy(&output.stdout).into_owned(),
        output.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

fn run(args: &[&str]) -> (String, i32, String) {
    run_at(Path::new(env!("CARGO_BIN_EXE_join")), args)
}

/// C's `err(3)` line: `<prog>: <operand>: <sentence>`.
fn assert_err_line(err: &str, prog: &str, operand: &str) {
    let head = format!("{prog}: {operand}: ");
    assert!(err.starts_with(&head), "`{err}` should start with `{head}`");
    let sentence = &err[head.len()..err.len() - 1];
    assert!(
        !sentence.is_empty() && sentence.chars().any(|c| c.is_lowercase()),
        "缀的应是一句 strerror 文本而不是代号：`{sentence}`"
    );
}

/// Two throwaway files with a shared key column; returns their paths.
fn operand_files() -> (std::path::PathBuf, std::path::PathBuf) {
    let dir = std::env::temp_dir().join(format!("join-face-{}", std::process::id()));
    fs::create_dir_all(&dir).expect("temp dir");
    let one = dir.join("one");
    let two = dir.join("two");
    fs::write(&one, "a 1\nb 2\n").expect("write one");
    fs::write(&two, "a x\nb y\n").expect("write two");
    (one, two)
}

#[test]
fn a_successful_join_says_nothing_on_stderr() {
    // The diagnostic legs owe silence on the success path: C prints only the joined
    // lines (`join.c:253+`), and `stderr` stays empty.
    let (one, two) = operand_files();
    let (out, status, err) = run(&[one.to_str().unwrap(), two.to_str().unwrap()]);
    assert_eq!(status, 0, "两档都能开：`{err}`");
    assert_eq!(err, "", "成功路一声不出");
    assert!(
        out.contains('a') && out.contains('b'),
        "该有连接结果：`{out}`"
    );
    let _ = fs::remove_dir_all(one.parent().unwrap());
}

#[test]
fn an_unopenable_operand_reports_the_operand_and_the_number() {
    let (out, status, err) = run(&["/tmp/join-face-absent", "/tmp/join-face-other"]);
    assert_eq!(out, "", "C 的失败路不写标准输出");
    assert_eq!(status, 1, "`err(1, …)` 是退 1");
    assert_err_line(&err, "join", "/tmp/join-face-absent");
    assert!(
        !err.contains("cannot open"),
        "「cannot open」是本树自创的短语，C 打的是操数名：`{err}`"
    );
}

#[test]
fn two_stdin_operands_are_refused_before_anything_is_read() {
    // `join.c:250-251`: both `FILE*` end up as `stdin`, which C refuses outright —
    // `errx` shaped (program name, phrase, no sentence), and no usage line follows.
    let (out, status, err) = run(&["-", "-"]);
    assert_eq!(out, "", "这一支护住在读之前，不该有任何输出");
    assert_eq!(status, 1);
    assert_eq!(err, "join: only one input file may be stdin\n");
}

#[test]
fn the_mutually_exclusive_pair_is_errx_without_a_usage_line() {
    // `join.c:235`: `errx(1, "-a and -v options mutually exclusive")` — one line, no
    // usage. The other option errors in C are `warnx` + `usage()`; that wording needs
    // the option letter and value, which the deciding half's error type drops, and is
    // registered as the next slice rather than invented here.
    let (_, status, err) = run(&["-a", "1", "-v", "1", "/tmp/join-face-a", "/tmp/join-face-b"]);
    assert_eq!(status, 1);
    assert_eq!(err, "join: -a and -v options mutually exclusive\n");
}

#[test]
fn usage_is_two_lines_with_the_name_only_in_the_first() {
    // `join.c:630-637`: `fprintf(stderr, "usage: %s ...\n            ...\n",
    // getprogname())` — the second line is indented twelve spaces and carries no name.
    let expected = format!("{USAGE_FIRST}{USAGE_SECOND}");
    let (out, status, err) = run(&[]);
    assert_eq!(out, "");
    assert_eq!(status, 1);
    assert_eq!(err, expected, "两行 usage 逐字对 C");

    let (_, status, err) = run(&["/tmp/join-face-only"]);
    assert_eq!(status, 1, "一个操数也是 usage（`argc != 2`）");
    assert_eq!(err, expected);
}

#[test]
fn the_report_line_follows_argv_zero_like_getprogname_does() {
    // The program name is `argv[0]`'s last segment: a copy under another name moves the
    // `err(3)` line's head and the usage line's name, while the operand path stays as
    // given. That is what pins the name is not baked in as a literal.
    let dir = std::env::temp_dir().join(format!("join-face-copy-{}", std::process::id()));
    fs::create_dir_all(&dir).expect("temp dir");
    let copied = dir.join("renamed-join");
    fs::copy(Path::new(env!("CARGO_BIN_EXE_join")), &copied).expect("copy the binary");
    fs::set_permissions(&copied, fs::Permissions::from_mode(0o755)).expect("make it run");

    let (_, _, err) = run_at(&copied, &["/tmp/join-face-3", "/tmp/join-face-4"]);
    assert_err_line(&err, "renamed-join", "/tmp/join-face-3");

    let (_, _, err) = run_at(&copied, &[]);
    assert_eq!(
        err,
        format!(
            "usage: renamed-join [-a fileno | -v fileno] [-e string] [-j fileno field]\n{USAGE_SECOND}"
        )
    );

    let _ = fs::remove_dir_all(&dir);
}
