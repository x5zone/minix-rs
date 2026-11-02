//! End-to-end face for the `grep` binary, covering only the **input seam**
//! that this crate shares with `sed`: which bytes reach the matcher, from
//! where, and what an unopenable operand costs. The selection rules
//! themselves (`-v`, `-c`, `-l`, name prefixes) keep their gates inside
//! `os/commands/usr-bin/regex/src/grep.rs`.
//!
//! Before the seam had two carriers, `minix_sys::open` was the only one and it
//! fails on the host, so every operand took the `file_err` arm and a normal run
//! could not be observed from a process test at all.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

const GREP: &str = env!("CARGO_BIN_EXE_grep");

fn run(args: &[&str], input: &[u8]) -> (Vec<u8>, i32, Vec<u8>) {
    let mut child = Command::new(GREP)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the grep binary should be runnable");
    {
        let mut pipe = child.stdin.take().expect("a standard input pipe");
        use std::io::Write as _;
        pipe.write_all(input)
            .expect("feeding the input should work");
    }
    let output = child.wait_with_output().expect("the run should finish");
    (
        output.stdout,
        output.status.code().unwrap_or(-1),
        output.stderr,
    )
}

fn written(tag: &str, bytes: &[u8]) -> PathBuf {
    let mut path = std::env::temp_dir();
    path.push(format!("grep-face-{}-{tag}", std::process::id()));
    fs::write(&path, bytes).expect("writing the scratch operand");
    path
}

fn text(path: &Path) -> String {
    path.to_str()
        .expect("a scratch path the child can take")
        .to_owned()
}

#[test]
fn a_named_operand_is_read() {
    // One operand prints the matching line with no name prefix, so the only
    // way to see the bytes is that they arrived at all.
    let path = written("named", b"abc\ndef\n");
    let (out, status, err) = run(&["a", &text(&path)], b"");
    assert_eq!(
        out,
        b"abc\n".to_vec(),
        "stderr was {:?}",
        String::from_utf8_lossy(&err)
    );
    assert_eq!(status, 0);
}

#[test]
fn standard_input_and_a_named_operand_agree() {
    // `grep.c` walks the same per-file routine for standard input as for a
    // named operand, so the two shapes must produce the same bytes.
    let path = written("agree", b"abc\ndef\n");
    let (from_file, file_status, file_err) = run(&["a", &text(&path)], b"");
    let (from_pipe, pipe_status, _) = run(&["a"], b"abc\ndef\n");
    assert_eq!(
        from_file,
        from_pipe,
        "stderr was {:?}",
        String::from_utf8_lossy(&file_err)
    );
    assert_eq!(file_status, pipe_status);
}

#[test]
fn several_operands_carry_their_name_prefixes() {
    // The prefix decision reads the **operand count** before anything is
    // opened (`grep.c:493-494`: `hflag` is set only for `argc == 0 || argc == 1`),
    // so with two operands each match is prefixed by its file name — observable
    // only once the operands really open.
    let first = written("first", b"abc\ndef\n");
    let second = written("second", b"xa\nyb\n");
    let (out, status, err) = run(&["a", &text(&first), &text(&second)], b"");
    assert_eq!(
        out,
        format!("{}:abc\n{}:xa\n", text(&first), text(&second)).into_bytes(),
        "stderr was {:?}",
        String::from_utf8_lossy(&err)
    );
    assert_eq!(status, 0);
}

#[test]
fn an_unopenable_operand_only_shows_up_in_the_exit_status() {
    // `procfile` warns and sets `file_err`, the run continues with the rest, and
    // the trouble reaches the exit status as 2
    // (`grep.c:505`: `exit(c ? (file_err ? (qflag ? 0 : 2) : 0) : (file_err ? 2 : 1))`).
    // The sentence's exact shape (C's `warn` appends a `strerror` clause neither
    // shell prints) is registered as `OPEN-WARN-SHAPE`, not pinned here.
    let good = written("after-bad", b"abc\n");
    let (out, status, err) = run(&["a", "/no-such-operand-here", &text(&good)], b"");
    assert_eq!(
        out,
        format!("{}:abc\n", text(&good)).into_bytes(),
        "stderr was {:?}",
        String::from_utf8_lossy(&err)
    );
    assert_eq!(status, 2);
    assert!(String::from_utf8_lossy(&err).contains("no-such-operand-here"));
}

#[test]
fn an_unescaped_parenthesis_in_a_basic_pattern_is_text() {
    // The basic dialect has no group operator, so `(a)` searches for the four
    // bytes `(a)` and a lone `)` searches for itself
    // (`regcomp.c:628-690`'s `default: ordinary`, and `:570-597`'s body stop is
    // the two-byte pair `\)`). The engine used to refuse `(a)` outright.
    let path = written("paren", b"(a)\nab\nx)\n(a)b\n");
    let name = text(&path);
    let (out, status, err) = run(&["-G", "(a)", &name], b"");
    assert_eq!(
        out,
        b"(a)\n(a)b\n".to_vec(),
        "stderr was {:?}",
        String::from_utf8_lossy(&err)
    );
    assert_eq!(status, 0);
    let (out, status, _) = run(&["-G", "x)", &name], b"");
    assert_eq!(out, b"x)\n".to_vec(), "a lone closer is text as well");
    assert_eq!(status, 0);
    // The escaped pair is the group, and it still groups.
    let (out, status, _) = run(&["-G", "y\\(a\\)b", &name], b"");
    assert_eq!(out, Vec::<u8>::new(), "no line holds `yab`");
    assert_eq!(status, 1);
}

#[test]
fn the_extended_dialect_groups_and_a_stray_closer_is_still_text() {
    // `regcomp.c:417-427` makes `(` open a group whose body ends at the bare `)`,
    // while a `)` with no group open is ordinary text because Minix compiles the
    // library with `-DPOSIX_MISTAKE` (`Makefile.inc:7`, removing the error arm at
    // `regcomp.c:428-438`).
    let path = written("ere-paren", b"abc\na(b)c\nx)\n");
    let name = text(&path);
    let (out, status, err) = run(&["-E", "a(b)c", &name], b"");
    assert_eq!(
        out,
        b"abc\n".to_vec(),
        "stderr was {:?}",
        String::from_utf8_lossy(&err)
    );
    assert_eq!(status, 0);
    let (out, status, _) = run(&["-E", "x)", &name], b"");
    assert_eq!(out, b"x)\n".to_vec(), "a stray closer must not be an error");
    assert_eq!(status, 0);
}
