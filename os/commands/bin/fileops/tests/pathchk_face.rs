//! End-to-end face for the `pathchk` binary: real process, real pipes, checked
//! against `minix3/usr.bin/pathchk/pathchk.c` (`check` `:91-167`, `portable`
//! `:173-191`, `usage` `:82-88`).
//!
//! The goldens below were read off a C reference binary built from that source
//! with the host compiler, so the two refusal shapes are byte-for-byte what Minix
//! prints — including the program name that `warnx` and `warn` put there
//! (`minix3/lib/libc/gen/vwarnx.c`): this shell used to print those lines without
//! it, which makes `<operand>: <component>: …` look like another program's output.
//!
//! The default mode's two `pathconf` questions cannot be asked of a MINIX file
//! service on a host, so the shell answers `ENOSYS` and takes C's own fallback to
//! `NAME_MAX`/`PATH_MAX` (`pathchk.c:107`, `:155`) — the same pair a MINIX file
//! service answers, since there both are header constants. That makes the limit
//! numbers below deterministic; the live-probe branch itself is target-only.

use std::os::unix::fs::PermissionsExt as _;
use std::path::Path;
use std::process::{Command, Stdio};

const PATHCHK: &str = env!("CARGO_BIN_EXE_pathchk");

fn run_at(program: &Path, args: &[&str]) -> (String, i32) {
    let output = Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("the pathchk binary should be runnable");
    (
        String::from_utf8_lossy(&output.stderr).into_owned(),
        output.status.code().unwrap_or(-1),
    )
}

fn run(args: &[&str]) -> (String, i32) {
    run_at(Path::new(PATHCHK), args)
}

#[test]
fn the_warnx_line_opens_with_the_program_name_and_repeats_the_operand() {
    // `check` prints the operand and then the component, so the two are equal for a
    // single-component path — that doubling is C's own text (pathchk.c:131-132).
    let (err, status) = run(&["-p", "abc+def"]);
    assert_eq!(
        (status, err.as_str()),
        (
            1,
            "pathchk: abc+def: abc+def: component contains non-portable character `+'\n"
        )
    );
    // A path that is fine in every way says nothing at all and exits 0.
    let (err, status) = run(&["-p", "usr/bin/grep"]);
    assert_eq!((status, err.as_str()), (0, ""));
}

#[test]
fn a_stat_failure_that_is_not_enoent_is_a_warn_line_with_the_cut_prefix() {
    // `pathchk.c:124-128` warns with `%.*s` over the copy truncated after the
    // component, minus one byte more — which for `/etc/hostname/x` is exactly
    // `/etc/hostname`, and the number's sentence rides on the end (glibc and the
    // MINIX table agree on `ENOTDIR`).
    let (err, status) = run(&["/etc/hostname/x"]);
    assert_eq!(status, 1, "stderr was `{err:?}`");
    assert_eq!(
        err,
        "pathchk: /etc/hostname/x: /etc/hostname: Not a directory\n"
    );
}

#[test]
fn a_missing_file_is_not_a_failure_at_all() {
    // C's test is `errno != ENOENT` (`:124`), so every component of an absent path is
    // waved through and only the limits remain: `pathchk /does/not/exist` exits 0.
    let (err, status) = run(&["/tmp/pathchk-face-absent/deeper"]);
    assert_eq!((status, err.as_str()), (0, ""), "缺文件不是拒因");
}

#[test]
fn the_fallback_limits_are_the_ones_c_documents() {
    // A probe that fails with an errno set takes C's fallback, and on a host build
    // the probe does fail (`ENOSYS`): so a 512-byte component is refused at 511 —
    // `NAME_MAX`, the number a MINIX file service would also answer. The
    // just-inside case is pinned by the library test instead (`check_system` with a
    // probe that answers 511), because on a host file system a 511-byte name is
    // itself refused by `stat` with `ENAMETOOLONG` — which is C's own behaviour too.
    let too_long = "a".repeat(512);
    let (err, status) = run(&[too_long.as_str()]);
    assert_eq!(status, 1);
    assert_eq!(
        err,
        format!("pathchk: {too_long}: {too_long}: component too long (limit 511)\n")
    );
}

#[test]
fn a_renamed_copy_moves_the_diagnostic_name_and_only_that() {
    // `warnx`/`warn` print `getprogname()`, while `usage()` at `pathchk.c:86` is a
    // literal naming `pathchk`. A renamed copy therefore changes the report lines and
    // leaves the usage line alone.
    let copied = std::env::temp_dir().join(format!("pathchk-face-copy-{}", std::process::id()));
    std::fs::copy(PATHCHK, &copied).expect("copy the binary");
    std::fs::set_permissions(&copied, std::fs::Permissions::from_mode(0o755)).expect("make it run");
    let name = copied.file_name().unwrap().to_str().unwrap().to_string();

    let (err, _) = run_at(&copied, &["-p", "bad name"]);
    assert_eq!(
        err,
        format!("{name}: bad name: bad name: component contains non-portable character ` '\n")
    );
    let (err, status) = run_at(&copied, &["-q"]);
    assert_eq!(
        (status, err.as_str()),
        (1, "usage: pathchk [-p] pathname...\n"),
        "usage 行不跟 `argv[0]` 走"
    );

    let _ = std::fs::remove_file(&copied);
}

#[test]
fn every_operand_is_checked_and_any_failure_sets_the_status() {
    // `main` ORs the per-operand results (`pathchk.c:75-79`) and never stops at the
    // first bad name — unlike `what`, whose `perror` leg leaves the program.
    let output = Command::new(PATHCHK)
        .args(["-p", "fine", "bad+name", "worse name"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("the pathchk binary should be runnable");
    let err = String::from_utf8_lossy(&output.stderr).into_owned();
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(
        err.lines().count(),
        2,
        "两个坏操数各一行，好的那一个不出声：`{err:?}`"
    );
    assert!(err.contains("`+") && err.contains("` '"), "`{err:?}`");
}
