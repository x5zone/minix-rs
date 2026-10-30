//! End-to-end face for the `what` binary: real process, real pipes, checked against
//! `minix3/usr.bin/what/what.c` (the operand loop `:83-91`, `usage` `:122-128`).
//!
//! `what` reports with `perror(*argv)` (`what.c:85`), and Minix3's `perror`
//! (`minix3/lib/libc/stdio/perror.c`) prints `<operand>: <strerror(errno)>` — no
//! program name and no extra words. The shell used to print `what: <operand>: cannot
//! open`: a name C never writes, a phrase C never writes, and no number. That is the
//! same two-family boundary `link`/`unlink` and `truncate` documented: `perror(3)` and
//! `err(3)`/`warn(3)` are not one function, so they must not be levelled into one line.
//!
//! Two more properties are pinned because they are the interesting ones here:
//! the failure ends the **whole** run (`perror` then `exit`, so a second operand never
//! gets a line), and `usage()` is a hardcoded literal in this command (`what.c:126`) —
//! unlike `rm`/`cp`/`mv`, whose usage carries `getprogname()`. A copy under another name
//! therefore changes neither line: the usage stays "what", and the report never had a
//! name to begin with.
//!
//! Host `open` answers `EIO` through the transport seam, so the identifier-printing
//! path (`what.c:88-89` plus `search()`) is out of reach here; it is covered by the
//! deciding half's unit tests over synthetic images.

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
        .expect("the what binary should be runnable");
    (
        String::from_utf8_lossy(&output.stdout).into_owned(),
        output.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

fn run(args: &[&str]) -> (String, i32, String) {
    run_at(Path::new(env!("CARGO_BIN_EXE_what")), args)
}

/// C's `perror` line: `<operand>: <sentence>`, and nothing before the operand.
fn assert_perror_line(err: &str, operand: &str) {
    let head = format!("{operand}: ");
    assert!(err.starts_with(&head), "`{err}` should start with `{head}`");
    let sentence = &err[head.len()..err.len() - 1];
    assert!(
        !sentence.is_empty() && sentence.chars().any(|c| c.is_lowercase()),
        "缀的应是一句 strerror 文本而不是代号：`{sentence}`"
    );
    assert!(err.ends_with('\n'), "`perror` 自己补换行：`{err}`");
}

#[test]
fn an_unopenable_operand_prints_the_perror_line_without_a_program_name() {
    let (out, status, err) = run(&["/tmp/what-face-absent"]);
    assert_eq!(out, "", "C 的失败路在 `printf` 之前就走了");
    assert_eq!(status, 1, "`exit(matches ? EXIT_SUCCESS : 1)`，此处无匹配");
    assert_perror_line(&err, "/tmp/what-face-absent");
    assert!(
        !err.contains("what:") && !err.contains("cannot open"),
        "程序名与「cannot open」都是本树自创的，C 没有：`{err}`"
    );
}

#[test]
fn the_first_failure_ends_the_run_so_only_one_line_appears() {
    // `what.c:85-87` is a `perror` followed by `exit`, not a `continue`: the second
    // operand is never opened. This is the opposite of `mkfifo`, whose per-name loop
    // reports every failure and only then exits.
    let (_, status, err) = run(&["/tmp/what-face-a", "/tmp/what-face-b"]);
    assert_eq!(status, 1);
    let lines: Vec<&str> = err.lines().collect();
    assert_eq!(lines.len(), 1, "第一个失败就收：`{err}`");
    assert_perror_line(&format!("{}\n", lines[0]), "/tmp/what-face-a");
}

#[test]
fn usage_is_hardcoded_for_no_operand_and_for_an_unknown_letter() {
    // `what.c:122-128`: one literal line, no `getprogname()`. Both entry points are
    // the same answer: no operand (`:81`'s `argc < 1`) and getopt's `?` arm (`:74-76`).
    let (out, status, err) = run(&[]);
    assert_eq!(out, "");
    assert_eq!(status, 1);
    assert_eq!(err, "usage: what [-s] file ...\n");

    let (_, status, err) = run(&["-q", "/tmp/what-face-x"]);
    assert_eq!(status, 1);
    assert_eq!(err, "usage: what [-s] file ...\n");
}

#[test]
fn a_renamed_copy_changes_neither_line() {
    // The report line never had a program name to move, and the usage line is a
    // literal — together these pin that `what` sits on the `perror(3)` side of the
    // family boundary, unlike `rm`/`cp`/`mv`/`mkfifo` whose lines follow `argv[0]`.
    let copied = std::env::temp_dir().join(format!("what-face-copy-{}", std::process::id()));
    fs::copy(Path::new(env!("CARGO_BIN_EXE_what")), &copied).expect("copy the binary");
    fs::set_permissions(&copied, fs::Permissions::from_mode(0o755)).expect("make it run");
    let name = copied.file_name().unwrap().to_str().unwrap().to_string();

    let (_, _, err) = run_at(&copied, &["/tmp/what-face-2"]);
    assert_perror_line(&err, "/tmp/what-face-2");
    assert!(!err.contains(&name), "程序名不该出现在这一行里：`{err}`");

    let (_, _, err) = run_at(&copied, &[]);
    assert_eq!(err, "usage: what [-s] file ...\n", "usage 是硬写的");

    let _ = fs::remove_file(&copied);
}

#[test]
fn a_long_operand_survives_the_perror_line_untouched() {
    // C's `perror` writes the caller's whole string out through `fprintf`
    // (`minix3/lib/libc/stdio/perror.c:67`); only the sentence goes through
    // the fixed-size `strerror_r` table. This tree's `perror_line` used to
    // write into a 256-byte stack buffer first and truncate, so an overlong
    // operand was cut before the sentence; this test pins that the
    // `perror(3)` family does not truncate either (the same shape also
    // guards `truncate`'s two raw report lines).
    let long = format!("/tmp/{}", "w".repeat(600));
    let (out, status, err) = run(&[long.as_str()]);
    assert_eq!(out, "", "C 的失败路在 `printf` 之前就走了");
    assert_eq!(status, 1);
    assert_perror_line(&err, &long);
    assert!(
        err.contains(&long),
        "600 字节的操数得完整出现在报文里：`{}` 字节",
        err.len()
    );
}
