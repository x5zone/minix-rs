//! End-to-end face for the `vis` binary: real process, real pipes and real
//! files, checked against `minix3/usr.bin/vis/vis.c` and the vis(3) engine in
//! `minix3/lib/libc/gen/vis.c`.
//!
//! There is no differential leg here: a Linux host has no `vis`, and the
//! byte-exact expectations below are what the C source says. Output is compared
//! as bytes because `-n` copies the input through, including bytes above 0x7F,
//! which no `String` could carry unchanged.

use std::fs;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

const VIS: &str = env!("CARGO_BIN_EXE_vis");

fn run(args: &[&str], input: &[u8]) -> (Vec<u8>, i32, String) {
    let mut child = Command::new(VIS)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the vis binary should be runnable");
    child
        .stdin
        .as_mut()
        .expect("stdin piped")
        .write_all(input)
        .expect("writing standard input");
    drop(child.stdin.take());
    let output = child.wait_with_output().expect("the run should finish");
    (
        output.stdout,
        output.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

fn written(tag: &str, bytes: &[u8]) -> PathBuf {
    let mut path = std::env::temp_dir();
    path.push(format!("vis-face-{}-{tag}", std::process::id()));
    fs::write(&path, bytes).expect("writing the scratch file");
    path
}

fn text(path: &Path) -> String {
    path.to_str().unwrap().to_owned()
}

/// `a`, a backslash, `b`, 0x01 (control), 0xFF (high bit), newline.
fn sample() -> Vec<u8> {
    vec![b'a', b'\\', b'b', 0x01, 0xFF, b'\n']
}

#[test]
fn default_encodes_only_the_non_graphic_bytes() {
    // `do_svis` passes graphic bytes (backslash included) and space/tab/newline
    // through; 0x01 becomes `\^A` and 0xFF becomes `\M^?` — `do_mbyte` emits the
    // `M` and then the caret form for the stripped 0x7F, with no `-` in between
    // (lib/libc/gen/vis.c:271-287).
    let (out, status, err) = run(&[], &sample());
    assert_eq!(out, b"a\\b\\^A\\M^?\n".to_vec(), "stderr was {err:?}");
    assert_eq!(status, 0);
    assert_eq!(err, "");
}

#[test]
fn no_slash_drops_the_escape_prefix() {
    // `-b` is C's `VIS_NOSLASH` (vis.c:81), not `-n`.
    let (out, status, err) = run(&["-b"], &sample());
    assert_eq!(out, b"a\\b^AM^?\n".to_vec(), "stderr was {err:?}");
    assert_eq!(status, 0);
}

#[test]
fn mark_eol_ends_each_line_with_a_backslash_dollar() {
    // `vis.c`'s `else if (markeol && c == '\n')` arm writes `\`, `$`, newline —
    // and the backslash itself disappears under `-b`.
    let (out, status, err) = run(&["-l"], &sample());
    assert_eq!(out, b"a\\b\\^A\\M^?\\$\n".to_vec(), "stderr was {err:?}");
    assert_eq!(status, 0);
    let (out, _, _) = run(&["-b", "-l"], &sample());
    assert_eq!(out, b"a\\b^AM^?$\n".to_vec());
}

#[test]
fn none_copies_the_stream_and_doubles_backslashes() {
    // `vis.c`'s `if (none)` arm: 0xFF must come back as the byte 0xFF, not as
    // two UTF-8 bytes, and this is the letter that is *not* the no-slash flag.
    let (out, status, err) = run(&["-n"], &sample());
    assert_eq!(out, vec![b'a', b'\\', b'\\', b'b', 0x01, 0xFF, b'\n']);
    assert_eq!(status, 0, "stderr was {err:?}");
    // `-n` is tested before the marker in C's loop, so it wins.
    let (out, _, _) = run(&["-n", "-l"], &sample());
    assert_eq!(out, vec![b'a', b'\\', b'\\', b'b', 0x01, 0xFF, b'\n']);
}

#[test]
fn octal_and_cstyle_still_reach_the_same_stream() {
    // `-o` renders every encoded byte as three octal digits.
    let (out, _, _) = run(&["-o"], &sample());
    assert_eq!(out, b"a\\b\\001\\377\n".to_vec());
    // `-c` cannot reach the tab or the newline at all: `do_svis` passes
    // whitespace through before the flag-specific branch runs, so the only
    // escape here is 0x01, and that one has no named form (`\a` is 0x07), so
    // it renders in the caret form.
    let (out, _, _) = run(&["-c"], b"a\t\n\x01");
    assert_eq!(out, b"a\t\n\\^A".to_vec());
}

#[test]
fn operands_are_encoded_into_one_stream_and_a_bad_one_only_warns() {
    let one = written("op-a", b"a\n");
    let two = written("op-b", b"\x01");
    let missing = format!("vis-face-{}-missing", std::process::id());
    let (out, status, err) = run(&[&text(&one), &missing, &text(&two)], b"");
    assert_eq!(out, b"a\n\\^A".to_vec());
    assert_eq!(status, 1, "stderr was {err:?}");
    assert!(err.contains(&missing), "stderr was {err:?}");

    // A bare `-` is a literal file name for `vis` (its operands go through
    // `fopen`), not a stdin alias.
    let (out, status, err) = run(&["-"], b"a\n");
    assert_eq!(out, Vec::<u8>::new());
    assert_eq!(status, 1);
    assert!(err.contains("vis: -"), "stderr was {err:?}");

    for path in [&one, &two] {
        let _ = fs::remove_file(path);
    }
}

#[test]
fn unmodelled_letters_are_refused_not_ignored() {
    // The letters left in the deciding half are `-d -e -F -f -h -M -m -s -t -w`;
    // silently ignoring any of them would print an encoding the user did not ask
    // for. (`-d` is C's debug counter, compiled out of that build, so C rejects
    // it too — through `default:`, same verdict.)
    for flag in ["-d", "-e", "-F", "-f", "-h", "-M", "-m", "-s", "-t", "-w"] {
        let (out, status, err) = run(&[flag], b"a\n");
        assert_eq!(out, Vec::<u8>::new(), "flag {flag} printed {out:?}");
        assert_eq!(status, 1, "flag {flag} stderr was {err:?}");
        assert!(
            err.contains("not modelled"),
            "flag {flag} stderr was {err:?}"
        );
    }
}
