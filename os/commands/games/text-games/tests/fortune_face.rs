//! End-to-end face for the `fortune` binary, checked against
//! `minix3/games/fortune/fortune/fortune.c`.
//!
//! This suite presupposes that **the host can open the operand file**: the
//! shell's file-name access goes through the twin faces of
//! `bin_support::slurp_operand` (host leg `std::fs`, target leg
//! `minix_sys::{open, read, close}`). Before that, `fortune` could not open a
//! file on the host at all, so no end-to-end gate could be written (the
//! criterion lives in `rewrite-notes/18-stage-commands/todo.md`).
//!
//! Four shapes and their readings:
//! ① Single-line database -- the extracted index is always 0 modulo the
//!    database length, so every roll hits the same line; it is written out
//!    verbatim and the exit status is 0.
//! ② Unopenable file -- reports this shell's line
//!    `fortune: <operand>: cannot open` and exits 1.
//! ③ Empty database -- `pick_quip` has nothing to roll; reports
//!    `fortune: <operand>: empty database` and exits 1.
//! ④ Standard output stops taking bytes -- C's output leg does not check for
//!    failure: `fputs(line, stdout)` (`:272`) is followed by
//!    `(void) fflush(stdout)` (`:274`), the return values are discarded, and
//!    the file never calls `ferror(stdout)` anywhere; the exit status is
//!    decided only by the two `exit` calls at `:202` and `:377`. So a failed
//!    write **adds no line and changes no status**: the shape matches ①,
//!    exit 0 with an empty stderr. The host injects it with `/dev/full`
//!    (writing there always returns "no space left on device").

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

fn fortune_bin() -> &'static str {
    env!("CARGO_BIN_EXE_fortune")
}

/// Build a database with the given contents in a temporary directory; the
/// path carries `tag` so parallel gates do not overwrite one another.
fn fixture(tag: &str, bytes: &[u8]) -> PathBuf {
    let path = std::env::temp_dir().join(format!("minix-fortune-face-{tag}.db"));
    std::fs::write(&path, bytes).expect("write fortune fixture");
    path
}

/// Run one leg: standard output is the caller's choice (a pipe or
/// `/dev/full`); all three channels are collected.
fn run(db: &Path, stdout: Stdio) -> (String, i32, String) {
    let child = Command::new(fortune_bin())
        .arg(db)
        .stdout(stdout)
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn fortune");
    let out = child.wait_with_output().expect("wait fortune");
    (
        String::from_utf8_lossy(&out.stdout).into_owned(),
        out.status.code().expect("exit code"),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

#[test]
fn a_one_line_database_prints_that_line_and_exits_zero() {
    let db = fixture("plain", b"only-quip\n");
    let (out, code, err) = run(&db, Stdio::piped());
    assert_eq!((out.as_str(), code, err.as_str()), ("only-quip\n", 0, ""));
    let _ = std::fs::remove_file(&db);
}

#[test]
fn an_unopenable_operand_reports_the_c_err_line_and_exits_one() {
    // C is `err(1, "Cannot open `%s'", fp->path)` (`minix3/games/fortune/fortune/fortune.c:1056`):
    // a three-part shape whose middle is the file name wrapped in a backtick
    // and a single quote, with the sentence for that errno at the tail and
    // exit status 1. The expected reading comes from a live run (the same
    // command with another file name reproduces it).
    let db = std::env::temp_dir().join("minix-fortune-face-does-not-exist.db");
    let _ = std::fs::remove_file(&db);
    let (out, code, err) = run(&db, Stdio::piped());
    assert_eq!(
        (out.as_str(), code, err.as_str()),
        (
            "",
            1,
            &format!(
                "fortune: Cannot open `{db}': No such file or directory\n",
                db = db.display()
            )[..]
        ),
        "stderr={err}"
    );
}

#[test]
fn an_empty_database_is_blamed_by_name_and_exits_one() {
    let db = fixture("empty", b"");
    let (out, code, err) = run(&db, Stdio::piped());
    assert_eq!(
        (out.as_str(), code, err.as_str()),
        (
            "",
            1,
            &format!("fortune: {}: empty database\n", db.display())[..]
        ),
        "stderr={err}"
    );
    let _ = std::fs::remove_file(&db);
}

#[test]
fn a_stdout_that_stops_taking_bytes_adds_no_word_and_no_status() {
    // The same database does write one line and exit 0 in
    // `a_one_line_database_prints_that_line_and_exits_zero`, so the 0 here is
    // not faked by "never hit anything".
    let db = fixture("full", b"only-quip\n");
    let file = std::fs::File::create("/dev/full").expect("open /dev/full");
    let (out, code, err) = run(&db, Stdio::from(file));
    assert_eq!(err, "", "标准输出收下不了字节时不该有任何一句话：{err}");
    assert_eq!(
        (out.as_str(), code),
        ("", 0),
        "C 的输出腿不查失手（`fortune.c:272-274`），退码不该因写不进去而变"
    );
    let _ = std::fs::remove_file(&db);
}

/// An operand that opens but cannot be read (a directory: `open` succeeds and
/// `read` returns "is a directory") means, in C, loading zero quips -- `fgets`
/// returns NULL just as at EOF, and `ferror` is never checked (`fortune.c:268`,
/// `:293`). This shell used to give it a separate `read error` line and exit 1;
/// that line and that status were invented. After the change it must take the
/// **same path** as an empty database: the same message (with the operand
/// swapped), the same status. This assertion does not guess C's exact wording;
/// it only pins that "a read failure is not a separate shape" -- both shapes
/// must agree on message and status.
#[test]
fn an_operand_that_opens_but_cannot_be_read_is_an_empty_database() {
    let dir = std::env::temp_dir().join("minix-fortune-face-dir-as-db.db");
    std::fs::create_dir_all(&dir).expect("make a directory operand");
    let (dir_out, dir_code, dir_err) = run(&dir, Stdio::piped());
    let empty = fixture("dirfold", b"");
    let (_empty_out, empty_code, empty_err) = run(&empty, Stdio::piped());
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::remove_file(&empty);
    assert_eq!(dir_out, "");
    assert_eq!(dir_code, empty_code, "读不断与空库的退码必须同形");
    assert_eq!(
        dir_err.replace(dir.display().to_string().as_str(), "<操作数>"),
        empty_err.replace(empty.display().to_string().as_str(), "<操作数>"),
        "读不断与空库的报文必须同形：目录得 {dir_err:?}，空库得 {empty_err:?}"
    );
}
