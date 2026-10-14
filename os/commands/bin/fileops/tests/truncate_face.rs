//! End-to-end face for the `truncate` binary: real process, real pipes, checked
//! against `minix3/minix/commands/truncate/truncate.c` (the option scan `:56-81`,
//! the reference look `:91-96`, the per-operand loop `:108-141`, `usage` `:202-208`).
//!
//! This door exists because `truncate` reports with **`perror(3)`**, not with
//! `warn(3)`: `perror(fname)` (`truncate.c:111`, `:118`, `:125`, `:135`) goes
//! through `minix3/lib/libc/stdio/perror.c`, which prints `<operand>:
//! <strerror(errno)>` with **no program name** — the opposite of the `err(3)`/
//! `warn(3)` family the neighbouring `rm`/`cp`/`mv` shells use. Before this round
//! the shell printed `truncate: <path>`: a prefix C never writes, and no number.
//!
//! The other shape worth pinning is the missing newline. C's two `fprintf` lines
//! (`:67-70` the bad `-s` text, `:92-94` the reference name it cannot stat) have no
//! terminator at all — the program calls `exit` immediately — so those two answers
//! are byte-exact here rather than "a line".
//!
//! On a host build `open`/`stat` answer `EIO` through the transport seam, which is
//! enough to reach the `perror` leg and both `fprintf` legs; `ENOENT`'s silence
//! (`truncate.c:109-114`) and a real `ftruncate` need the writable root image, so
//! they stay with the deciding half's unit tests and the `C-57`/`E5` machine gate.

use std::fs;
use std::os::unix::fs::PermissionsExt as _;
use std::path::Path;
use std::process::{Command, Stdio};

/// C's two usage lines (`truncate.c:202-208`) in one write: the second starts with
/// seven spaces, so it must be spelled inside the literal — a `\` line continuation
/// would strip exactly those spaces.
const USAGE: &str = "usage: truncate [-c] -s [+|-]size[K|k|M|m|G|g] file ...\n       truncate [-c] -r rfile file ...\n";

fn run_at(program: &Path, args: &[&str]) -> (String, i32, String) {
    let output = Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("the truncate binary should be runnable");
    (
        String::from_utf8_lossy(&output.stdout).into_owned(),
        output.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

fn run(args: &[&str]) -> (String, i32, String) {
    run_at(Path::new(env!("CARGO_BIN_EXE_truncate")), args)
}

#[test]
fn a_failed_operand_prints_the_perror_line_without_a_program_name() {
    // `truncate.c:109-114` + Minix3's `perror.c`: `<operand>: <sentence>`, no
    // `truncate:` prefix. The prefix is what this door exists to keep out.
    let (out, status, err) = run(&["-s", "10", "/tmp/truncate-face-absent"]);
    assert_eq!(out, "", "C 的失败路不写标准输出");
    assert_eq!(status, 1, "`error++` 之后是 `EXIT_FAILURE`");
    assert!(
        err.starts_with("/tmp/truncate-face-absent: "),
        "`perror` 那一行以操作数开头：`{err}`"
    );
    assert!(
        !err.starts_with("truncate:"),
        "Minix3 的 `perror` 不缀程序名，这里也不该有：`{err}`"
    );
    let sentence = &err["/tmp/truncate-face-absent: ".len()..err.len() - 1];
    assert!(
        !sentence.is_empty() && sentence.chars().any(|c| c.is_lowercase()),
        "缀的应是一句 strerror 文本：`{sentence}`"
    );
    assert!(err.ends_with('\n'), "`perror` 自己补换行：`{err}`");
}

#[test]
fn a_bad_size_argument_prints_no_newline_and_names_no_program() {
    // `truncate.c:67-70`: `fprintf(stderr, "invalid size argument `%s'")` then
    // `exit(EXIT_FAILURE)` — the quoted text is the operand as given, and the two
    // things C never does are writing a newline and naming itself.
    let (out, status, err) = run(&["-s", "bogus", "/tmp/truncate-face-x"]);
    assert_eq!(out, "");
    assert_eq!(status, 1);
    assert_eq!(err, "invalid size argument `bogus'");
}

#[test]
fn an_unstatable_reference_prints_the_bare_name_and_no_operand_line() {
    // `truncate.c:92-94`: the reference name alone, then exit. Because the size is
    // settled before the loop, no operand line may follow it.
    let (out, status, err) = run(&["-r", "/tmp/truncate-face-ref", "/tmp/truncate-face-x"]);
    assert_eq!(out, "");
    assert_eq!(status, 1);
    assert_eq!(err, "/tmp/truncate-face-ref");
}

#[test]
fn missing_options_answer_the_two_hardcoded_usage_lines() {
    // `truncate.c:202-208`: `usage()` prints two literal lines — this command does
    // not use `getprogname()` at all, unlike `rm`/`cp`/`mv` — with the second line
    // indented seven spaces.
    let (out, status, err) = run(&[]);
    assert_eq!(out, "");
    assert_eq!(status, 1);
    assert_eq!(err, USAGE, "两行 usage 逐字对 C（含第二行的七个空格）");

    let (_, status, err) = run(&["-q", "/tmp/truncate-face-x"]);
    assert_eq!(status, 1);
    assert_eq!(err, USAGE, "未列字母同样走 usage");
}

#[test]
fn renaming_the_binary_does_not_give_the_line_a_program_name() {
    // The strongest form of the `perror`-vs-`warn` distinction: run a copy under
    // another name and check that nothing about the report changes. `warn(3)` would
    // have printed the new name; `perror(3)` prints none at all.
    let copied = std::env::temp_dir().join(format!("trunc-face-copy-{}", std::process::id()));
    fs::copy(Path::new(env!("CARGO_BIN_EXE_truncate")), &copied).expect("copy the binary");
    fs::set_permissions(&copied, fs::Permissions::from_mode(0o755)).expect("make it run");

    let (_, status, err) = run_at(&copied, &["-s", "5", "/tmp/truncate-face-renamed"]);
    assert_eq!(status, 1);
    assert!(
        err.starts_with("/tmp/truncate-face-renamed: "),
        "改名后仍是 `<操作数>: <句子>`：`{err}`"
    );
    let name = copied.file_name().unwrap().to_str().unwrap();
    assert!(!err.contains(name), "程序名不该出现在这一行里：`{err}`");

    let _ = fs::remove_file(&copied);
}
