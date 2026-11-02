//! End-to-end face for the `du` binary: real process, real pipes, checked against
//! `minix3/usr.bin/du/du.c` (the option scan `:75-160`, the walk `:197-253`,
//! `usage` `:356-362`).
//!
//! C's report for a position it cannot stat or read is
//! `warnx("%s: %s", p->fts_path, strerror(p->fts_errno))` (`du.c:233`) — three
//! fields, program name first, and the sentence behind the number. The shell printed
//! `du: <path>`: the right prefix and the wrong (absent) tail, so an unreadable
//! directory and a vanished operand read alike. A failed directory read is also
//! fatal in C (`if (errno) err(1, "fts_read")`, `:251-253`) and the totals gathered
//! so far are never printed; the shell used to report the directory and keep going.
//!
//! The usage line is the counter-example worth pinning: unlike `rm`/`cp`/`mv`, C's
//! `usage()` here is one hardcoded string (`du.c:359-360`), so renaming the binary
//! changes the report lines but NOT the usage line. Both halves of that difference
//! are checked below.

use std::fs;
use std::os::unix::fs::PermissionsExt as _;
use std::path::Path;
use std::process::{Command, Stdio};

const USAGE: &str = "usage: du [-H | -L | -P] [-a | -d depth | -s] [-cghikmnrx] [file ...]\n";

fn run_at(program: &Path, args: &[&str]) -> (String, i32, String) {
    let output = Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("the du binary should be runnable");
    (
        String::from_utf8_lossy(&output.stdout).into_owned(),
        output.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

fn run(args: &[&str]) -> (String, i32, String) {
    run_at(Path::new(env!("CARGO_BIN_EXE_du")), args)
}

/// C's `warnx("%s: %s", path, strerror(errno))` line: `<prog>: <path>: <sentence>`.
fn assert_three_fields(err: &str, prog: &str, path: &str) {
    let head = format!("{prog}: {path}: ");
    assert!(err.starts_with(&head), "`{err}` should start with `{head}`");
    let sentence = &err[head.len()..err.len() - 1];
    assert!(!sentence.is_empty(), "句子不该为空：`{err}`");
    assert!(
        sentence.chars().any(|c| c.is_lowercase()) && !sentence.contains(": "),
        "缀的应是一句 strerror 文本而不是代号：`{sentence}`"
    );
}

#[test]
fn an_unstatable_operand_loses_its_sentence_no_longer() {
    let (out, status, err) = run(&["/tmp/du-face-absent"]);
    assert_eq!(out, "", "取不到状态就没有总数可印");
    assert_eq!(status, 1, "`du.c:234` 的 `rval = 1`");
    assert_three_fields(&err, "du", "/tmp/du-face-absent");
}

#[test]
fn every_operand_gets_its_own_line_and_one_exit_status() {
    // C walks the operand list and only accumulates `rval`; N unstatable operands
    // produce N lines and a single exit status.
    let (out, status, err) = run(&["/tmp/du-face-a", "/tmp/du-face-b"]);
    assert_eq!(out, "");
    assert_eq!(status, 1);
    let lines: Vec<&str> = err.lines().collect();
    assert_eq!(lines.len(), 2, "两个操作数出两行：`{err}`");
    assert_three_fields(&format!("{}\n", lines[0]), "du", "/tmp/du-face-a");
    assert_three_fields(&format!("{}\n", lines[1]), "du", "/tmp/du-face-b");
}

#[test]
fn a_refused_letter_keeps_the_program_name_and_drops_the_sentence() {
    // `-H`/`-L`/`-x` are this tree's up-front refusals (the logical walk and the
    // mount-boundary test are not wired); the line is `warnx` shaped — program name
    // plus a phrase — because there is no number behind it. Note that `-h` is NOT
    // in that set: it renders through `humanize`, and the refusal phrase used to
    // claim otherwise.
    let (_, status, err) = run(&["-H", "/tmp/du-face-x"]);
    assert_eq!(status, 1);
    assert_eq!(
        err, "du: -H not wired (logical walk, FTS_XDEV)\n",
        "拒收行不该缀句子"
    );
}

#[test]
fn the_usage_line_is_hardcoded_while_the_report_line_follows_argv_zero() {
    // C's `usage()` (`du.c:356-362`) prints a literal `du`, unlike `rm`/`cp`/`mv`
    // whose usage carries `getprogname()`. Renaming a copy therefore moves the
    // report line and leaves the usage line alone — both halves are pinned.
    let copied = std::env::temp_dir().join(format!("du-face-copy-{}", std::process::id()));
    fs::copy(Path::new(env!("CARGO_BIN_EXE_du")), &copied).expect("copy the binary");
    fs::set_permissions(&copied, fs::Permissions::from_mode(0o755)).expect("make it run");
    let name = copied.file_name().unwrap().to_str().unwrap().to_string();

    let (_, status, err) = run_at(&copied, &["-q"]);
    assert_eq!(status, 1);
    assert_eq!(err, USAGE, "usage 行不跟着 `argv[0]` 走（C 也是硬写的）");

    let (_, _, err) = run_at(&copied, &["/tmp/du-face-renamed"]);
    assert_three_fields(&err, &name, "/tmp/du-face-renamed");

    let _ = fs::remove_file(&copied);
}

/// `-x` left the not-wired set when the device boundary landed (`du.c:144-146`,
/// fts's `FTS_XDEV`): the flag now passes the option gate, so on the host —
/// where the walk's syscall legs are the none-target faces — a missing
/// operand reaches the stat-error line instead of the refusal. The
/// cross-device prune decision itself is pinned by the lib's
/// `crosses_device` truth table (`du.rs` tests); the host face cannot mount a
/// second filesystem.
#[test]
fn x_passes_the_option_gate_to_the_walk() {
    let missing = format!("/tmp/du-face-x-absent-{}", std::process::id());
    let (out, status, err) = run(&["-x", missing.as_str()]);
    assert_eq!(out.as_str(), "");
    assert_eq!(status, 1);
    let head = format!("du: {missing}: ");
    assert!(
        err.starts_with(&head),
        "`{err}` 该走 stat 错误行而不是未接线拒绝"
    );
    assert!(!err.contains("not wired"), "-x 不该再报未接线：{err}");
}
