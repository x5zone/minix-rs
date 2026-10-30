//! End-to-end face for the `stat` binary (and the same code under the name
//! `readlink`): real process, real pipes, checked against `minix3/usr.bin/stat/stat.c`
//! (the operand loop `:320-350`, the personality test `:217-232`, `usage` `:357-365`,
//! the format refusals `:261-263` and `:289-291`).
//!
//! Two things this door exists to pin.
//!
//! The first is the **four-field** report line. C's failure leg is
//! `warn("%s: %s", name, usestat ? "stat" : "lstat")` (`stat.c:340-344`), and
//! `vwarnc.c` renders `warn` as `<getprogname()>: <format-output>: <strerror(errno)>`, so
//! the full line is `stat: <name>: lstat: <sentence>` — program name, operand, the word C
//! puts in the format string, and the sentence behind the number. Previously
//! the shell stopped after the third field, which makes a permission denial and a
//! vanished operand read alike; `-L` also has to say `stat`, not `lstat`, because
//! that word follows the option and not the syscall that finally answered.
//!
//! The second is the personality test. C compares `getprogname()` — `argv[0]` minus
//! its directories — to `"readlink"` with `strcmp` (`stat.c:220`), so a copy named
//! `my-readlink` is **not** `readlink`: it gets `stat`'s option face, `stat`'s warning,
//! and its own name in every line. An `ends_with` test passes that case and is wrong.
//!
//! The `%Y`/`%R` legs need a successful `stat` first, which a host build cannot get
//! (`lstat` answers `EIO` through the transport seam), so they are not covered here;
//! what is covered is the parse/format legs, the usage pair, and the quiet flag.

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
        .expect("the stat binary should be runnable");
    (
        String::from_utf8_lossy(&output.stdout).into_owned(),
        output.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

fn run(args: &[&str]) -> (String, i32, String) {
    run_at(Path::new(env!("CARGO_BIN_EXE_stat")), args)
}

/// The `<prog>: <operand>: <leg-name>: <sentence>` shape, checked field by
/// field.
fn assert_four_fields(err: &str, prog: &str, operand: &str, leg: &str) {
    let head = format!("{prog}: {operand}: {leg}: ");
    assert!(err.starts_with(&head), "`{err}` should start with `{head}`");
    let sentence = &err[head.len()..err.len() - 1];
    assert!(
        !sentence.is_empty() && sentence.chars().any(|c| c.is_lowercase()),
        "第四段应是 strerror 的句子而不是代号：`{sentence}`"
    );
    assert!(err.ends_with('\n'), "C 的一行以换行收尾：`{err}`");
}

#[test]
fn a_failed_lstat_prints_all_four_fields() {
    let (out, status, err) = run(&["/tmp/stat-face-absent"]);
    assert_eq!(out, "", "失败路不写标准输出");
    assert_eq!(
        status, 1,
        "`errs = 1` 之后是 `EXIT_FAILURE`（`stat.c:348-350`）"
    );
    assert_four_fields(&err, "stat", "/tmp/stat-face-absent", "lstat");
}

#[test]
fn the_leg_word_follows_the_option_not_the_syscall_that_answered() {
    // `stat.c:340-344` prints `usestat ? "stat" : "lstat"`. With `-L` C tries
    // `stat()` first and only falls back when the number is `ENOENT`, and the word it
    // prints is `stat` either way — pinning this keeps a future "helpful" rewrite from
    // reporting which call actually failed.
    let (_, status, err) = run(&["-L", "/tmp/stat-face-absent"]);
    assert_eq!(status, 1);
    assert_four_fields(&err, "stat", "/tmp/stat-face-absent", "stat");
}

#[test]
fn quiet_silences_the_leg_and_still_fails() {
    // `stat.c:339-345` gates the `warn` on `!quiet` only; `errs` is still set, so the
    // exit status is unchanged. That asymmetry is the whole point of `-q`.
    let (out, status, err) = run(&["-q", "/tmp/stat-face-absent"]);
    assert_eq!(out, "");
    assert_eq!(status, 1, "静默不等于成功");
    assert_eq!(err, "", "`-q` 该一声不出：`{err}`");
}

#[test]
fn the_format_refusals_and_the_bad_format_are_errx_lines() {
    // Three `errx` legs, none of which consults `errno`: `stat.c:261-263`
    // (`can't use format '%c' with '%c'`), `:289-291` (`... with -F`), and
    // `:555-556`/`:989` (`<sub-format>: bad format`). Each ends at the phrase.
    let (_, status, err) = run(&["-F", "-f", "%N", "/tmp/stat-face-x"]);
    assert_eq!(status, 1);
    assert_eq!(err, "stat: can't use format 'f' with -F\n");

    let (_, status, err) = run(&["-f", "%N", "-x", "/tmp/stat-face-x"]);
    assert_eq!(status, 1);
    assert_eq!(err, "stat: can't use format 'f' with 'x'\n");

    let (_, status, err) = run(&["-f", "%Q", "/tmp/stat-face-x"]);
    assert_eq!(status, 1);
    assert_eq!(err, "stat: %Q: bad format\n");
}

#[test]
fn the_usage_line_carries_the_program_name_inside_it() {
    // `stat.c:357-365`: `fprintf(stderr, "usage: %s %s\n", getprogname(), synopsis)`.
    // The name is an argument to the format string, not a prefix, and the synopsis is
    // the personality's (`stat.c:221-232`).
    let (_, status, err) = run(&["-Z"]);
    assert_eq!(status, 1);
    assert_eq!(
        err,
        "usage: stat [-FlLnqrsx] [-f format] [-t timefmt] [file ...]\n"
    );
}

#[test]
fn the_personality_is_the_whole_program_name_not_a_suffix() {
    // C: `strcmp(getprogname(), "readlink") == 0` (`stat.c:220`). Two copies in one
    // throwaway directory pin both halves of that rule, and the directory itself pins
    // a third thing: the name is `argv[0]`'s last segment, so the path in front of it
    // never reaches the line.
    let dir = std::env::temp_dir().join(format!("stat-face-{}", std::process::id()));
    fs::create_dir_all(&dir).expect("temp dir");
    let as_readlink = dir.join("readlink");
    let almost = dir.join("my-readlink");
    for name in [&as_readlink, &almost] {
        fs::copy(Path::new(env!("CARGO_BIN_EXE_stat")), name).expect("copy the binary");
        fs::set_permissions(name, fs::Permissions::from_mode(0o755)).expect("make it run");
    }

    // The real `readlink`: readlink's option face, default quiet (`stat.c:226`), so a
    // failed operand stays silent but still exits 1.
    let (_, status, err) = run_at(&as_readlink, &["/tmp/stat-face-y"]);
    assert_eq!(status, 1, "readlink 人格下失败仍退 1");
    assert_eq!(err, "", "readlink 默认 `quiet`，不该出声：`{err}`");

    let (_, status, err) = run_at(&as_readlink, &["-Z"]);
    assert_eq!(status, 1);
    assert_eq!(err, "usage: readlink [-fnqsv] [file ...]\n");

    // A name that merely ends with `readlink` is ordinary `stat`: the warning prints,
    // and the head of the line is that last segment, directories dropped.
    let (_, status, err) = run_at(&almost, &["/tmp/stat-face-z"]);
    assert_eq!(status, 1);
    assert_four_fields(&err, "my-readlink", "/tmp/stat-face-z", "lstat");

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn the_operand_without_slashes_still_keeps_its_own_name_in_the_line() {
    // `getprogname()` drops the directories of `argv[0]` but the operand is printed
    // exactly as given (`stat.c:341`, `argv[0]` of the operand list), including a bare
    // relative name — the two fields must not be confused with each other.
    let (out, status, err) = run(&["stat-face-bare"]);
    assert_eq!(out, "");
    assert_eq!(status, 1);
    assert_four_fields(&err, "stat", "stat-face-bare", "lstat");
}
