//! End-to-end face for the `hostname` and `domainname` binaries: real process, real
//! pipes, checked against `minix3/bin/hostname/hostname.c` (`:81`/`:84`) and
//! `minix3/bin/domainname/domainname.c` (`:80`/`:83`).
//!
//! Both C programs report a failed syscall through `err(1, "<operation>")`, whose line
//! is fixed by `minix3/lib/libc/gen/verrc.c`: `getprogname()`, `": "`, the operation the
//! caller wrote, `": "`, then `strerror(errno)` and a newline, leaving with status 1.
//! The program name comes from `argv[0]` minus its directories
//! (`minix3/lib/csu/common/crt0-common.c:156-165`), so what is asserted here is
//! `hostname: gethostname: ` and not a hard-coded path.
//!
//! The number itself is the host transport's answer and is deliberately not pinned:
//! which errno a wrapped `gethostname(2)` yields outside a running Minix is not part of
//! C's contract. What is part of it — the prefix, the operation, the separator, one
//! non-empty sentence, and status 1 — is checked in full.

use std::path::Path;
use std::process::{Command, Stdio};

fn run(program: &str, args: &[&str]) -> (String, i32, String) {
    let path = Path::new(env!("CARGO_BIN_EXE_hostname")).with_file_name(program);
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

/// The `err(3)` line, up to the sentence: `prog: operation: ` and then exactly one more
/// field, which must be a non-empty sentence rather than an identifier or an empty tail.
fn assert_err_line(err: &str, prog: &str, operation: &str) {
    let head = format!("{prog}: {operation}: ");
    assert!(err.starts_with(&head), "`{err}` should start with `{head}`");
    let sentence = &err[head.len()..];
    // 句子必须是一个句子：里头不再有一个字段，也不能是大写代号（`EIO`、`EINVAL` 这类
    // 标识符一个 lowercase 都没有），并且以换行收尾。
    assert!(
        !sentence.is_empty() && !sentence.contains(": "),
        "不该再有一个字段：`{sentence}`"
    );
    assert!(
        sentence.chars().any(|c| c.is_lowercase()),
        "打回的应是句子而不是大写代号：`{sentence}`"
    );
    assert!(
        sentence.ends_with('\n'),
        "C 的那一行以换行收尾：`{sentence}`"
    );
}

#[test]
fn a_failed_sethostname_reports_the_c_err_line_and_leaves_one() {
    // C: `if (sethostname(*argv, strlen(*argv))) err(1, "sethostname")`.
    let (out, status, err) = run("hostname", &["some.host"]);
    assert_eq!(out, "", "C 的失败路不写标准输出");
    assert_eq!(status, 1);
    assert_err_line(&err, "hostname", "sethostname");
}

#[test]
fn a_failed_gethostname_reports_the_c_err_line_and_leaves_one() {
    // C: `if (gethostname(hostname, sizeof(hostname))) err(1, "gethostname")`.
    let (out, status, err) = run("hostname", &[]);
    assert_eq!(out, "");
    assert_eq!(status, 1);
    assert_err_line(&err, "hostname", "gethostname");
}

#[test]
fn domainname_follows_the_same_err_line_on_both_arms() {
    let (out, status, err) = run("domainname", &[]);
    assert_eq!(out, "");
    assert_eq!(status, 1);
    assert_err_line(&err, "domainname", "getdomainname");
    let (_, status, err) = run("domainname", &["some.domain"]);
    assert_eq!(status, 1);
    assert_err_line(&err, "domainname", "setdomainname");
}

#[test]
fn an_option_like_argument_answers_the_usage_line_on_stderr() {
    // C's `usage()` (`hostname.c:95-99`, `domainname.c:91-95`) writes
    // `usage: %s …` with `getprogname()` and exits 1 — the program name in that line is
    // the same last-slash segment, so both arms agree on it.
    let (out, status, err) = run("hostname", &["-x"]);
    assert_eq!(out, "");
    assert_eq!(status, 1);
    assert_eq!(err, "usage: hostname [-s] [name-of-host]\n");
    let (_, status, err) = run("domainname", &["-x"]);
    assert_eq!(status, 1);
    assert_eq!(err, "usage: domainname [name-of-domain]\n");
}
