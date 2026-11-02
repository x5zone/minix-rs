//! End-to-end face for the `sed` binary: a real process, real pipes and real
//! files, checked against `minix3/usr.bin/sed/main.c` (which operand gets
//! read, in what order, and what a failure costs) and
//! `minix3/usr.bin/sed/process.c` (the per-line print rules).
//!
//! This file exists because the crate's input legs had only one carrier: both
//! standard input and the file operands went through `minix_sys::{open, read}`,
//! which on the host is a shim that hands back nothing. Measured before the
//! fix, `printf 'abc\ndef\n' | sed 's/a/A/'` printed **nothing** and exited 0
//! while the host's own sed printed two lines — so every input claim in
//! `rewrite-notes/18-stage-commands/08-grep-sed.md` was unobservable from a
//! process test. The legs are now `cfg` twins (`read_stdin` / `read_file` in
//! `os/commands/usr-bin/regex/src/bin_support.rs`), which is what these tests
//! pin; the transformation itself keeps its own gates inside the library.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

const SED: &str = env!("CARGO_BIN_EXE_sed");

/// Runs the binary with `input` on standard input, and hands back the three
/// channels as they came out: standard output stays bytes because a filter's
/// output is data, not a message.
fn run(args: &[&str], input: &[u8]) -> (Vec<u8>, i32, Vec<u8>) {
    let mut child = Command::new(SED)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the sed binary should be runnable");
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

/// A scratch operand under the system temp directory, named after the running
/// test process so parallel tests cannot collide.
fn written(tag: &str, bytes: &[u8]) -> PathBuf {
    let mut path = std::env::temp_dir();
    path.push(format!("sed-face-{}-{tag}", std::process::id()));
    fs::write(&path, bytes).expect("writing the scratch operand");
    path
}

fn text(path: &Path) -> String {
    path.to_str()
        .expect("a scratch path the child can take")
        .to_owned()
}

#[test]
fn standard_input_reaches_the_pattern_space() {
    // No operand means the one file C adds is `NULL`, and `mf_fgets` opens it
    // as `stdin` (`main.c:214`, `:361-363`). Each line is substituted and
    // auto-printed (`process.c:270` `new: if (!nflag && !pd) OUT()`).
    let (out, status, err) = run(&["s/a/A/"], b"abc\ndef\n");
    assert_eq!(
        out,
        b"Abc\ndef\n".to_vec(),
        "stderr was {:?}",
        String::from_utf8_lossy(&err)
    );
    assert_eq!(status, 0);
    assert!(err.is_empty());
}

#[test]
fn a_named_operand_is_read_and_agrees_with_the_pipe() {
    // C reads a named operand through the same line loop, so the two shapes
    // must give byte-identical output; only the opening differs.
    let path = written("named", b"abc\ndef\n");
    let (from_file, file_status, file_err) = run(&["s/a/A/", &text(&path)], b"");
    let (from_pipe, pipe_status, _) = run(&["s/a/A/"], b"abc\ndef\n");
    assert_eq!(
        from_file,
        from_pipe,
        "stderr was {:?}",
        String::from_utf8_lossy(&file_err)
    );
    assert_eq!(file_status, pipe_status);
    assert_eq!(file_status, 0);
}

#[test]
fn every_operand_is_transformed_in_order() {
    // C chains the operands into one file list and walks it (`main.c:203-214`),
    // so output is the first file's lines then the second's.
    let first = written("first", b"one\ntwo\n");
    let second = written("second", b"three\n");
    let (out, status, err) = run(&["s/o/0/", &text(&first), &text(&second)], b"");
    assert_eq!(
        out,
        b"0ne\ntw0\nthree\n".to_vec(),
        "stderr was {:?}",
        String::from_utf8_lossy(&err)
    );
    assert_eq!(status, 0);
}

#[test]
fn quiet_prints_only_what_the_p_flag_prints() {
    // `-n` suppresses the auto-print, so only `s///p`'s own `OUT()` inside
    // `substitute()` (`process.c:466-467`) survives: the matching line once,
    // the non-matching line not at all.
    let (out, status, err) = run(&["-n", "s/a/A/p"], b"abc\ndef\n");
    assert_eq!(
        out,
        b"Abc\n".to_vec(),
        "stderr was {:?}",
        String::from_utf8_lossy(&err)
    );
    assert_eq!(status, 0);
}

#[test]
fn an_unopenable_operand_warns_and_flags_the_exit_status() {
    // `main.c:449-452`: `warn("%s", fname)` then `rval = 1` then `continue`, so
    // the run goes on with the remaining operands and `exit(rval)` reports it.
    // The sentence's exact shape (C's `warn` goes through `vwarnc`, which
    // prints `<program name>: <text>: <strerror>` — this shell prints the first
    // two segments with a hardwired name) is registered as `OPEN-WARN-SHAPE`,
    // so this pins only that the operand is named and that the run is neither
    // silent nor fatal.
    let good = written("after-bad", b"abc\n");
    let (out, status, err) = run(&["s/a/A/", "/no-such-operand-here", &text(&good)], b"");
    assert_eq!(
        out,
        b"Abc\n".to_vec(),
        "stderr was {:?}",
        String::from_utf8_lossy(&err)
    );
    assert_eq!(status, 1);
    assert!(
        String::from_utf8_lossy(&err).contains("no-such-operand-here"),
        "stderr was {:?}",
        String::from_utf8_lossy(&err)
    );
}

#[test]
fn a_bare_dash_stays_an_operand_name() {
    // NetBSD sed opens operands through `fopen`, with no `-` special case
    // (unlike `split`), so `-` is looked up as a file in the working directory
    // and never read as standard input: the piped bytes must not appear.
    let (out, status, _) = run(&["s/a/A/", "-"], b"abc\n");
    assert!(out.is_empty(), "standard input must not have been read");
    assert_eq!(status, 1, "the dash operand should have failed to open");
}
