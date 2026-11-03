//! End-to-end face for the `cp` binary: real process, real pipes, checked against
//! `minix3/bin/cp/cp.c` (the target look `:237-268`, the shape refusals `:275-281`)
//! and `minix3/bin/cp/utils.c` (`copy_file` `:93-301`, `usage` `:422-430`).
//!
//! `cp` has more diagnostic legs than any other command in this crate — C counts
//! nineteen of them across the two files — and they come in two families that must
//! not be merged: `warn(3)` lines carry the number's sentence (`utils.c:103`, `:137`,
//! `:159`, …), while `warnx`/`errx` lines carry a phrase and no sentence at all
//! (`cp.c:277`, `:280`, `:426`, `:435`, `:458`). Before this round every one of them
//! printed `cp: <name>[: <label>]` with the `Errno` thrown away.
//!
//! On a host build the very first filesystem call — the `stat` of the target — already
//! fails, and C treats anything but `ENOENT` there as fatal (`cp.c:242`), so the
//! reachable shapes are the fatal one and the ones that never touch the filesystem:
//! the usage pair, the refused-letter line, and what `argv[0]` contributes. The deeper
//! legs (`copy_file`'s open/read/write family, `setfile`, the `-R` walk) are pinned by
//! the deciding half's unit tests and by the real-machine gate `E5`, and this
//! door does not pretend to cover them.

use std::fs;
use std::os::unix::fs::PermissionsExt as _;
use std::path::Path;
use std::process::{Command, Stdio};

/// C's second usage line is indented with seven spaces so that it lines up under
/// the program name of the first line.
fn usage_of(name: &str) -> String {
    format!(
        "usage: {name} [-R [-H | -L | -P]] [-f | -i] [-alNpv] src target\n\
         {sep}{name} [-R [-H | -L | -P]] [-f | -i] [-alNpv] src1 ... srcN directory\n",
        sep = " ".repeat(7)
    )
}

fn run(program: &str, args: &[&str]) -> (String, i32, String) {
    let path = Path::new(env!("CARGO_BIN_EXE_cp")).with_file_name(program);
    run_at(&path, args)
}

fn run_at(program: &Path, args: &[&str]) -> (String, i32, String) {
    let output = Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("the cp binary should be runnable");
    (
        String::from_utf8_lossy(&output.stdout).into_owned(),
        output.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

/// The name a binary at `path` reports for itself: `getprogname()`'s last segment.
fn name_of(path: &Path) -> String {
    path.file_name().unwrap().to_str().unwrap().to_string()
}

/// C's `err(3)`/`warn(3)` line: `<prog>: <label>: <sentence>` with a real sentence.
fn assert_warn_line(line: &str, prog: &str, label: &str) {
    let head = format!("{prog}: {label}: ");
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

#[test]
fn a_target_that_cannot_be_stat_is_fatal_and_carries_the_number() {
    // `cp.c:237-244`: `if (r == -1 && errno != ENOENT) err(EXIT_FAILURE, "%s",
    // to.p_path)`. Two things are pinned: the run leaves at once (so the source is
    // never named), and the line is the three-field `err(3)` shape.
    let (out, status, err) = run("cp", &["/tmp/cp-face-src", "/tmp/cp-face-dst"]);
    assert_eq!(out, "", "C 的失败路不写标准输出");
    assert_eq!(status, 1);
    let lines: Vec<&str> = err.lines().collect();
    assert_eq!(lines.len(), 1, "目标取不到就当场离开：`{err}`");
    assert_warn_line(&format!("{}\n", lines[0]), "cp", "/tmp/cp-face-dst");
    assert!(
        !err.contains("/tmp/cp-face-src"),
        "C 的这一行只带目标名，不带源名：`{err}`"
    );
}

#[test]
fn the_usage_pair_carries_the_program_name_twice() {
    // `utils.c:422-430`: one `fprintf` with two `getprogname()` slots, the second
    // line indented by seven spaces so it lines up under the first.
    let (out, status, err) = run("cp", &["only-one"]);
    assert_eq!(out, "");
    assert_eq!(status, 1, "`usage()` 走 `exit(1)`");
    assert_eq!(err, usage_of("cp"), "两行 usage 的缩进与字母表都要逐字对上");
}

#[test]
fn a_refused_letter_is_named_in_a_line_without_a_sentence() {
    // The `-r`/`-l`/`-N`/`-H`/`-L` refusals are this tree's own up-front stop,
    // shaped like C's `warnx`: program name, the phrase, no number behind it
    // (C has no such line because those letters are wired there).
    let (out, status, err) = run("cp", &["-H", "/tmp/cp-face-src", "/tmp/cp-face-dst"]);
    assert_eq!(out, "");
    assert_eq!(status, 1);
    assert_eq!(
        err, "cp: -H not wired (the -r/-H/-L traversal ladder, hard-link copy)\n",
        "拒绝行不该缀句子"
    );
}

#[test]
fn an_unknown_letter_answers_the_usage_pair_not_a_refusal() {
    // `cp.c:160-166`: getopt's `?` arm calls `usage()`, so an unlisted letter gets
    // the two usage lines rather than the "-<c> not wired" sentence.
    let (_, status, err) = run("cp", &["-Z", "/tmp/cp-face-src", "/tmp/cp-face-dst"]);
    assert_eq!(status, 1);
    assert_eq!(err, usage_of("cp"));
}

#[test]
fn the_program_name_comes_from_argv_zero_not_from_a_literal() {
    // C sets it once (`cp.c:113`) and both usage lines and every `warn`/`err` line
    // read it, so a copy under another name has to follow — the only way to show
    // the shell is not printing a hard-coded `"cp"`.
    let copied = std::env::temp_dir().join(format!("cp-face-copy-{}", std::process::id()));
    fs::copy(Path::new(env!("CARGO_BIN_EXE_cp")), &copied).expect("copy the binary");
    fs::set_permissions(&copied, fs::Permissions::from_mode(0o755)).expect("make it run");
    let name = name_of(&copied);

    let (_, status, err) = run_at(&copied, &["/tmp/cp-face-2src", "/tmp/cp-face-2dst"]);
    assert_eq!(status, 1);
    assert_warn_line(&err, &name, "/tmp/cp-face-2dst");

    let (_, _, err) = run_at(&copied, &[]);
    assert_eq!(err, usage_of(&name), "两行 usage 的程序名都来自 `argv[0]`");

    let _ = fs::remove_file(&copied);
}
