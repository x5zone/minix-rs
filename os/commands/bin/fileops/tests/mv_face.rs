//! End-to-end face for the `mv` binary: real process, real pipes, checked against
//! `minix3/bin/mv/mv.c` (`do_move` `:153-253`, `fastcopy` `:256-354`, `usage`
//! `:401-409`).
//!
//! `mv`'s failure line is the one place in this crate where C puts the label **in
//! front of** the operand and adds a second operand in the middle: `warn("rename %s
//! to %s", from, to)` (`mv.c:225`) renders as `mv: rename <from> to <to>:
//! <strerror(errno)>` (`minix3/lib/libc/gen/vwarnc.c`). Before this round the shell
//! printed `mv: rename <from> to <to>` — the words were C's, the number was gone, so
//! a cross-device rename, a permission denial and a missing source all read the same.
//!
//! Two more shapes are pinned here because they are easy to "tidy" into the wrong
//! one: the usage pair carries `getprogname()` twice and C writes the option group
//! without spaces (`[-fiv]`, `mv.c:404-405`), and the questions (`overwrite %s? `,
//! `override …for %s? `) are bare `fprintf` calls that must NOT grow a program name.
//!
//! On a host build `access(2)`/`stat(2)`/`rename(2)` all fail through the transport
//! seam, so the ask ladder never fires and `fastcopy` is unreachable; the legs that
//! need a real filesystem are the deciding half's unit tests and the `C-57`/`E5`
//! machine gate, and this door says so instead of pretending to cover them.

use std::fs;
use std::os::unix::fs::PermissionsExt as _;
use std::path::Path;
use std::process::{Command, Stdio};

/// C's two usage lines (`mv.c:401-409`), with the seven-space indent of the second.
fn usage_of(name: &str) -> String {
    format!("usage: {name} [-fiv] source target\n       {name} [-fiv] source ... directory\n")
}

fn run(program: &str, args: &[&str]) -> (String, i32, String) {
    let path = Path::new(env!("CARGO_BIN_EXE_mv")).with_file_name(program);
    run_at(&path, args)
}

fn run_at(program: &Path, args: &[&str]) -> (String, i32, String) {
    let output = Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("the mv binary should be runnable");
    (
        String::from_utf8_lossy(&output.stdout).into_owned(),
        output.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

#[test]
fn a_failed_rename_names_both_operands_and_the_number() {
    // `mv.c:225`: `warn("rename %s to %s", from, to)` — label first, both operands,
    // then that errno's sentence, and the exit status is `eval = 1`.
    let (out, status, err) = run("mv", &["/tmp/mv-face-from", "/tmp/mv-face-to"]);
    assert_eq!(out, "", "C 的失败路不写标准输出");
    assert_eq!(status, 1);
    let head = "mv: rename /tmp/mv-face-from to /tmp/mv-face-to: ";
    assert!(err.starts_with(head), "`{err}` should start with `{head}`");
    let sentence = &err[head.len()..err.len() - 1];
    assert!(
        !sentence.is_empty() && sentence.chars().any(|c| c.is_lowercase()),
        "缀的应是一句 strerror 文本而不是代号：`{sentence}`"
    );
    assert!(err.ends_with('\n'));
}

#[test]
fn too_few_operands_answer_the_usage_pair() {
    // `mv.c:117-118` and `usage()`'s `exit(1)`: one operand, and no operand at all.
    let (out, status, err) = run("mv", &["only-one"]);
    assert_eq!(out, "");
    assert_eq!(status, 1);
    assert_eq!(err, usage_of("mv"));

    let (_, status, err) = run("mv", &[]);
    assert_eq!(status, 1);
    assert_eq!(err, usage_of("mv"));
}

#[test]
fn several_sources_without_a_directory_target_are_usage_too() {
    // `mv.c:113-118`: the N-to-one shape requires the last operand to be a
    // directory; on a host build nothing can be one, so this is the usage answer
    // rather than a filesystem report.
    let (_, status, err) = run(
        "mv",
        &["/tmp/mv-face-a", "/tmp/mv-face-b", "/tmp/mv-face-plain"],
    );
    assert_eq!(status, 1);
    assert_eq!(err, usage_of("mv"));
}

#[test]
fn an_unknown_letter_answers_the_usage_pair() {
    // `mv.c:127-130`: getopt's `?` arm falls through to `usage()`.
    let (_, status, err) = run("mv", &["-Z", "/tmp/mv-face-from", "/tmp/mv-face-to"]);
    assert_eq!(status, 1);
    assert_eq!(err, usage_of("mv"));
}

#[test]
fn the_program_name_comes_from_argv_zero_not_from_a_literal() {
    // C sets it once (`mv.c:85`) and both usage lines and every `warn` line read
    // it; a copy under another name is the only way to show nothing is hard-coded.
    let copied = std::env::temp_dir().join(format!("mv-face-copy-{}", std::process::id()));
    fs::copy(Path::new(env!("CARGO_BIN_EXE_mv")), &copied).expect("copy the binary");
    fs::set_permissions(&copied, fs::Permissions::from_mode(0o755)).expect("make it run");
    let name = copied.file_name().unwrap().to_str().unwrap().to_string();

    let (_, status, err) = run_at(&copied, &["/tmp/mv-face-2from", "/tmp/mv-face-2to"]);
    assert_eq!(status, 1);
    assert!(
        err.starts_with(&format!(
            "{name}: rename /tmp/mv-face-2from to /tmp/mv-face-2to: "
        )),
        "失败行的程序名取自 `argv[0]`（`mv.c:85`）而不是硬写的 `mv`：`{err}`"
    );

    let (_, _, err) = run_at(&copied, &[]);
    assert_eq!(err, usage_of(&name));

    let _ = fs::remove_file(&copied);
}
