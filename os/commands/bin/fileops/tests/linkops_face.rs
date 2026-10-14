//! End-to-end face for the `link` and `unlink` binaries: real process, real pipes,
//! checked against `minix3/usr.sbin/link/link.c` (`:53-61`) and
//! `minix3/usr.sbin/unlink/unlink.c` (`:52-61`).
//!
//! Both C programs are a bare argument-count check plus one syscall, and the failure
//! leg is `perror("link")` / `perror("unlink")`. Minix3's `perror`
//! (`minix3/lib/libc/stdio/perror.c`) prints `<caller string>: <strerror(errno)>` —
//! **no program-name prefix, and the operands never appear**. That second half is what
//! this gate exists to pin: the old shells printed `unlink: <path>`, which reads like
//! the C line but reports the wrong thing.
//!
//! The number a host transport answers is environment fact, not C's contract, so the
//! sentence is checked as "a sentence" (non-empty, has lower case, no extra field) while
//! the field order, the absence of the operand, and the exit status are pinned exactly.

use std::path::Path;
use std::process::{Command, Stdio};

fn run(program: &str, args: &[&str]) -> (String, i32, String) {
    let path = Path::new(env!("CARGO_BIN_EXE_unlink")).with_file_name(program);
    let output = Command::new(path)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("both binaries should be runnable");
    (
        String::from_utf8_lossy(&output.stdout).into_owned(),
        output.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

/// C's `perror` line for a given caller string: exactly two fields, the second a sentence.
fn assert_perror_line(err: &str, label: &str, forbidden: &[&str]) {
    let head = format!("{label}: ");
    assert!(err.starts_with(&head), "`{err}` should start with `{head}`");
    let sentence = err[head.len()..].trim_end_matches('\n');
    assert!(!sentence.is_empty(), "句子不该为空：`{err}`");
    assert!(
        sentence.chars().any(|c| c.is_lowercase()) && !sentence.contains(": "),
        "缀的应是一句 `<label>: <句子>`，不是代号也不是又一个字段：`{sentence}`"
    );
    for word in forbidden {
        assert!(!err.contains(word), "C 的 perror 不打操作数：`{err}`");
    }
}

#[test]
fn a_failed_unlink_reports_the_syscall_not_the_path() {
    let (out, status, err) = run("unlink", &["/tmp/linkops-face-absent"]);
    assert_eq!(out, "", "C 的失败路不写标准输出");
    assert_eq!(status, 1, "`perror` 之后是 `exit(EXIT_FAILURE)`");
    assert_perror_line(&err, "unlink", &["/tmp/linkops-face-absent"]);
}

#[test]
fn a_failed_link_reports_the_syscall_not_either_operand() {
    let (out, status, err) = run("link", &["/tmp/linkops-face-old", "/tmp/linkops-face-new"]);
    assert_eq!(out, "");
    assert_eq!(status, 1);
    assert_perror_line(
        &err,
        "link",
        &["/tmp/linkops-face-old", "/tmp/linkops-face-new"],
    );
}

#[test]
fn the_operand_counts_are_checked_before_any_syscall_is_tried() {
    // C 的两个程序只有一条 `argc != 2` / `argc != 3`（`unlink.c:52`、`link.c:53`），
    // 没有 getopt：任何形状不对都直接 usage，且 usage 行是硬写死的字符串（不带程序名）。
    let (_, status, err) = run("unlink", &[]);
    assert_eq!(status, 1);
    assert_eq!(err, "usage: unlink file\n");
    let (_, status, err) = run("unlink", &["a", "b"]);
    assert_eq!(status, 1);
    assert_eq!(err, "usage: unlink file\n");
    let (_, status, err) = run("link", &["only-one"]);
    assert_eq!(status, 1);
    assert_eq!(err, "usage: link file1 file2\n");
    let (_, status, err) = run("link", &["a", "b", "c"]);
    assert_eq!(status, 1);
    assert_eq!(err, "usage: link file1 file2\n");
}

#[test]
fn an_option_looking_operand_is_still_just_a_name() {
    // 没有 getopt，`-x` 就是文件名；C 走 `unlink("-x")` 失败后同样只报号。
    let (_, status, err) = run("unlink", &["-x"]);
    assert_eq!(status, 1);
    assert_perror_line(&err, "unlink", &[]);
    assert_ne!(err, "usage: unlink file\n", "C 这里不出 usage 行");
}
