//! End-to-end face for the `rm` binary: real process, real pipes, checked against
//! `minix3/bin/rm/rm.c` (`rm_file` `:289-354`, `rm_tree` `:167-287`, `checkdot`
//! `:589-618`, `usage` `:620-628`).
//!
//! What this door exists to pin is the **shape of the failure line**. C's per-operand
//! report is `warn("%s", f)` (`rm.c:311`, `rm.c:348`), and `vwarn.c` renders that as
//! `<getprogname()>: <operand>: <strerror(errno)>`. Before this leg was wired the shell
//! printed `rm: <operand>` — right prefix, right operand, but the number's sentence was
//! thrown away along with the `Errno`, so a permission failure and a nonexistent operand
//! read identically.
//!
//! Three properties come with it and none of them is decoration:
//! - the program name is `argv[0]`'s last slash-segment (C `setprogname(argv[0])`,
//!   `rm.c:104`; the rule is `minix3/lib/csu/common/crt0-common.c:156-165`), so it is not
//!   a string baked into the shell — checked here by running a copy under another name;
//! - one operand failing does not stop the run (`rm_file`'s `while` loop keeps going and
//!   only remembers `eval = 1`), so N operands produce N lines and one exit status;
//! - `warnx` lines carry the program name but **no** sentence (`rm.c:323` "is a directory",
//!   `rm.c:610` the `.`/`..` phrase), so they must not grow a `: <sentence>` tail.
//!
//! Which number a host transport answers is environment fact, not C's contract: hosted
//! `lstat` fails with `EIO` here, so the sentence is asserted as "a sentence" (non-empty,
//! lower case, not an `EPERM`-style identifier) and `-f`'s silence — which in C hinges on
//! the `NONEXISTENT` triage of ENOENT/ENAMETOOLONG/ENOTDIR (`rm.c:89-90`) — is left to the
//! deciding-half unit tests rather than faked here.

use std::fs;
use std::os::unix::fs::PermissionsExt as _;
use std::path::Path;
use std::process::{Command, Stdio};

fn run(program: &str, args: &[&str]) -> (String, i32, String) {
    let path = Path::new(env!("CARGO_BIN_EXE_rm")).with_file_name(program);
    let output = Command::new(&path)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("the rm binary should be runnable");
    (
        String::from_utf8_lossy(&output.stdout).into_owned(),
        output.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

/// C's `warn(3)` line: `<prog>: <operand>: <sentence>`, one line, newline-terminated.
fn assert_warn_line(line: &str, prog: &str, operand: &str) {
    let head = format!("{prog}: {operand}: ");
    assert!(
        line.starts_with(&head),
        "`{line}` should start with `{head}`"
    );
    let sentence = &line[head.len()..line.len() - 1];
    assert!(!sentence.is_empty(), "句子不该为空：`{line}`");
    assert!(
        sentence.chars().any(|c| c.is_lowercase()) && !sentence.contains(": "),
        "缀的应是一句 strerror 文本，不是 errno 代号也不是又一个字段：`{sentence}`"
    );
    assert!(line.ends_with('\n'), "C 的一行以换行收尾：`{line}`");
}

/// C's `warnx(3)` line: `<prog>: <phrase>` — no sentence after the phrase.
fn assert_warnx_line(line: &str, expected: &str) {
    assert_eq!(line, expected);
}

#[test]
fn a_failing_operand_reports_the_number_as_well_as_the_name() {
    let (out, status, err) = run("rm", &["/tmp/rm-face-absent"]);
    assert_eq!(out, "", "C 的失败路不写标准输出");
    assert_eq!(status, 1, "`eval = 1` 是 `rm.c:349` 那一支");
    let lines: Vec<&str> = err.lines().collect();
    assert_eq!(lines.len(), 1, "一个操作数出一行：`{err}`");
    assert_warn_line(&format!("{}\n", lines[0]), "rm", "/tmp/rm-face-absent");
}

#[test]
fn every_operand_gets_its_own_line_and_the_run_carries_on() {
    // `rm_file`'s loop hands each remaining name the same treatment; only the exit
    // status is accumulated (`rm.c:300-353`).
    let operands = ["/tmp/rm-face-one", "/tmp/rm-face-two", "/tmp/rm-face-three"];
    let (_, status, err) = run("rm", &operands);
    assert_eq!(status, 1);
    let lines: Vec<&str> = err.lines().collect();
    assert_eq!(lines.len(), 3, "三个操作数该出三行：`{err}`");
    for (line, operand) in lines.iter().zip(operands) {
        assert_warn_line(&format!("{line}\n"), "rm", operand);
    }
}

#[test]
fn the_program_name_comes_from_argv_zero_not_from_a_literal() {
    // C prefixes every `warn`/`warnx`/`err` line and the usage line with
    // `getprogname()`, i.e. the last segment of `argv[0]`. A copy under another name is
    // the only way to show the shell is not printing a hard-coded `"rm"`.
    let renamed = std::env::temp_dir().join(format!("rm-face-copy-{}", std::process::id()));
    let name = renamed.file_name().unwrap().to_str().unwrap().to_string();
    fs::copy(Path::new(env!("CARGO_BIN_EXE_rm")), &renamed).expect("copy the binary");
    fs::set_permissions(&renamed, fs::Permissions::from_mode(0o755)).expect("make it run");

    let (_, status, err) = run_at(&renamed, &["/tmp/rm-face-renamed"]);
    assert_eq!(status, 1);
    assert_warn_line(&err, &name, "/tmp/rm-face-renamed");

    let (_, status, err) = run_at(&renamed, &[]);
    assert_eq!(status, 1);
    assert_eq!(
        err,
        format!("usage: {name} [-f|-i] [-dPRrvWx] file ...\n"),
        "usage 行的程序名同样来自 `argv[0]`（`rm.c:624-625`）"
    );

    let _ = fs::remove_file(&renamed);
}

/// Run a binary at an explicit path, keeping the shape of [`run`].
fn run_at(path: &Path, args: &[&str]) -> (String, i32, String) {
    let output = Command::new(path)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("the copied binary should be runnable");
    (
        String::from_utf8_lossy(&output.stdout).into_owned(),
        output.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

#[test]
fn no_operand_is_usage_but_minus_f_is_silent_success() {
    // `rm.c:144-148`: an empty operand list with `-f` returns 0 before anything else
    // is consulted; without it, usage (exit 1).
    let (out, status, err) = run("rm", &[]);
    assert_eq!(status, 1);
    assert_eq!(out, "");
    assert_eq!(err, "usage: rm [-f|-i] [-dPRrvWx] file ...\n");

    let (out, status, err) = run("rm", &["-f"]);
    assert_eq!(status, 0, "`-f` 让空操作数列表静默成功");
    assert_eq!(out, "");
    assert_eq!(err, "");
}

#[test]
fn dot_operands_warn_once_and_reach_the_filesystem_not_at_all() {
    // `checkdot` (`rm.c:589-618`) drops every `.`/`..` operand and complains with
    // `warnx` **once** for the whole list (`if (!complained++)`); the surviving operands
    // are still processed afterwards, and the line has no errno sentence behind it.
    let (out, status, err) = run("rm", &[".", "..", "dir/.", "/tmp/rm-face-after-dots"]);
    assert_eq!(out, "");
    assert_eq!(status, 1);
    let lines: Vec<&str> = err.lines().collect();
    assert_eq!(
        lines.len(),
        2,
        "点只抱怨一次，剩下的操作数各出一行：`{err}`"
    );
    assert_warnx_line(lines[0], "rm: \".\" and \"..\" may not be removed");
    assert_warn_line(&format!("{}\n", lines[1]), "rm", "/tmp/rm-face-after-dots");
}

#[test]
fn the_recursive_walk_reports_the_same_line_for_an_unstatable_root() {
    // `-r`/`-R` takes `rm_tree`; its root cannot be stat'ed, which is C's `FTS_NS`
    // arm (`rm.c:211-224`) rendering `warnx("%s: %s", path, strerror(errno))` — the same
    // three fields `warn` produces, so the leg is shared rather than duplicated.
    let (out, status, err) = run("rm", &["-r", "/tmp/rm-face-absent-dir"]);
    assert_eq!(out, "");
    assert_eq!(status, 1);
    assert_eq!(err.lines().count(), 1, "根取不到就不再往下走：`{err}`");
    assert_warn_line(&err, "rm", "/tmp/rm-face-absent-dir");
}

#[test]
fn a_long_operand_is_written_out_wholly_in_the_report_line() {
    // C 的 `vwarnc` 拿 `vfprintf(stderr, fmt, ap)` 打调用者的串，长度不进方程
    // （`minix3/lib/libc/gen/vwarnc.c:62`），所以一个 600 字节的操数会整段出现在那一行里。
    // 本树的渲染一开始写进固定 256 字节的栈缓冲再裁，超过的部分静默消失——这一枚
    // 钉住它不会再发生（同一个形状也保 `cp`/`du`/`mv` 那几路 `warn(3)` 腿）。
    let long = "r".repeat(600);
    let (out, status, err) = run("rm", &[long.as_str()]);
    assert_eq!(out, "");
    assert_eq!(status, 1);
    let lines: Vec<&str> = err.lines().collect();
    assert_eq!(lines.len(), 1, "一个操数一行：`{err}`");
    assert_warn_line(&err, "rm", &long);
    assert!(err.contains(&long), "600 字节的操作数必须完整出现在报文里");
}
