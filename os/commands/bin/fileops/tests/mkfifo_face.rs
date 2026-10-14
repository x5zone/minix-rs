//! End-to-end face for the `mkfifo` binary: real process, real pipes, checked against
//! `minix3/usr.bin/mkfifo/mkfifo.c` (the option scan `:71-91`, the per-name loop
//! `:93-99`, `usage()` `:102-107`).
//!
//! C's per-name failure leg is `warn("%s", *argv)` (`mkfifo.c:95`), and `vwarn.c`
//! renders that line as `getprogname(): <name>: <strerror(errno)>`. Two properties make
//! this leg worth a gate of its own: the run **continues** to the remaining names
//! (`exitval = 1` is remembered, not taken), and the line carries both the program name
//! derived from `argv[0]` and the sentence that errno's number maps to.
//!
//! Which number a host transport answers for `mknod` is not part of C's contract, so the
//! assertions pin the line's shape rather than its sentence — including that the tail is
//! a sentence and not an errno identifier.

use std::process::{Command, Stdio};

fn run(args: &[&str]) -> (String, i32, String) {
    let output = Command::new(env!("CARGO_BIN_EXE_mkfifo"))
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("the mkfifo binary should be runnable");
    (
        String::from_utf8_lossy(&output.stdout).into_owned(),
        output.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

/// One `warn(3)` line: `mkfifo: <label>: <sentence>`, with a sentence that is a sentence.
fn one_warn_line(err: &str, label: &str) {
    let head = format!("mkfifo: {label}: ");
    assert!(err.starts_with(&head), "`{err}` should start with `{head}`");
    let sentence = &err[head.len()..err.len() - 1];
    assert!(!sentence.is_empty(), "句子不该为空：`{err}`");
    assert!(
        sentence.chars().any(|c| c.is_lowercase()),
        "缀的应是句子而不是大写代号：`{sentence}`"
    );
    assert!(err.ends_with('\n'), "C 的一行以换行收尾：`{err}`");
}

#[test]
fn a_failed_name_warns_the_c_line_and_the_loop_moves_on() {
    // C 的循环把每个名字都走一遍，失败只把退出码记成 1（`mkfifo.c:93-99`）。
    let (out, status, err) = run(&["/tmp/mkfifo-face-a", "/tmp/mkfifo-face-b"]);
    assert_eq!(out, "", "C 的失败路不写标准输出");
    assert_eq!(status, 1);
    let lines: Vec<&str> = err.lines().collect();
    assert_eq!(lines.len(), 2, "两个名字该出两行：`{err}`");
    one_warn_line(&format!("{}\n", lines[0]), "/tmp/mkfifo-face-a");
    one_warn_line(&format!("{}\n", lines[1]), "/tmp/mkfifo-face-b");
}

#[test]
fn a_bad_mode_stops_before_any_name_is_touched() {
    // `err(1, "Cannot set file mode \`%s'")` (`mkfifo.c:74-76`) leaves at once, so no
    // per-name line may appear behind it. The quoted text is the operand as given.
    let (out, status, err) = run(&["-m", "bogus", "/tmp/mkfifo-face-c"]);
    assert_eq!(out, "");
    assert_eq!(status, 1);
    assert_eq!(err, "mkfifo: Cannot set file mode `bogus'\n");
}

#[test]
fn no_names_at_all_answers_the_usage_line_on_stderr() {
    // C checks `argv[0] == NULL` right after the option scan (`mkfifo.c:90-91`) and
    // `usage()` writes one line then exits 1 (`:102-107`).
    let (out, status, err) = run(&[]);
    assert_eq!(out, "");
    assert_eq!(status, 1);
    assert_eq!(err, "usage: mkfifo [-m mode] fifoname ...\n");
    // A `-m`-taking option that is not filled in is the same answer (`case '?'`).
    let (_, status, err) = run(&["-q"]);
    assert_eq!(status, 1);
    assert_eq!(err, "usage: mkfifo [-m mode] fifoname ...\n");
}

#[test]
fn the_program_name_loses_its_directories_like_getprogname_does() {
    // C 的运行时刻把 `argv[0]` 切成最后一段（`minix3/lib/csu/common/crt0-common.c:156-165`），
    // 所以以带目录的路径调用时，报的第一段仍是不含目录的程序名。这里刻意把一个副本放进
    // 临时目录再按那条路径调用：`argv[0]` 带斜杠，输出的前缀不带。
    let dir = std::env::temp_dir().join("mkfifo-face-progname");
    let _ = std::fs::create_dir_all(&dir);
    let copy = dir.join("mkfifo");
    std::fs::copy(env!("CARGO_BIN_EXE_mkfifo"), &copy).expect("copying the binary should work");
    let output = Command::new(&copy)
        .arg("/tmp/mkfifo-face-d")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("the copied binary should be runnable");
    assert_eq!(output.status.code().unwrap_or(-1), 1);
    let err = String::from_utf8_lossy(&output.stderr).into_owned();
    let first = err.lines().next().unwrap_or("");
    // 报告的前缀是不带目录的程序名，后面紧跟的才是那个操作数。
    let prog = first.split(": ").next().unwrap_or("");
    assert_eq!(prog, "mkfifo", "程序名那段不该带目录：`{first}`");
    assert!(
        first.starts_with("mkfifo: /tmp/mkfifo-face-d: "),
        "整行应是 `mkfifo: <名字>: <句子>`：`{first}`"
    );
    let _ = std::fs::remove_dir_all(&dir);
}
