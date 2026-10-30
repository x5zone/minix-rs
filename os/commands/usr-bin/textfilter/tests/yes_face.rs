//! The end-to-end gate for `yes`: the byte string on the command line is
//! the content of every line.
//!
//! The ground truth is `minix3/usr.bin/yes/yes.c:50-56` — `yes = (argc > 1)
//! ? argv[1] : "y"` stores a `const char *`, and `while (puts(yes) >= 0)`
//! emits that byte string verbatim plus a newline on every round.
//! This branch has no option loop and no file name: the argument is neither
//! a flag nor a path, it is the **output content** that gets repeated
//! forever. So any folding at the argument seam changes, not one message
//! line, but the bytes of every line.
//!
//! What the gate can read is constrained by two facts: first, `yes` loops
//! forever, so the fixture must take the first bytes and then kill the
//! child; second, the loop exits on a write failure (C's `puts < 0` implies
//! `EXIT_FAILURE`), so "read enough, then kill" is this fixture's normal
//! wrap-up, not a defect in the code under test.

use std::ffi::OsString;
use std::io::Read as _;
use std::os::unix::ffi::OsStringExt as _;
use std::process::{Command, Stdio};

const YES: &str = env!("CARGO_BIN_EXE_yes");

/// Start `yes`, take back the first `want` bytes, then kill it. argv is
/// handed over byte by byte (`OsString::from_vec` performs no UTF-8
/// validation), because these gates pin down exactly whether raw bytes
/// can be repeated verbatim.
fn head_bytes(args: &[Vec<u8>], want: usize) -> Vec<u8> {
    let mut child = Command::new(YES)
        .args(args.iter().map(|raw| OsString::from_vec(raw.clone())))
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the yes binary should be runnable");
    let mut got: Vec<u8> = Vec::new();
    {
        let mut pipe = child.stdout.take().expect("stdout is piped");
        let mut byte = [0u8; 1];
        while got.len() < want {
            match pipe.read(&mut byte) {
                Ok(1) => got.push(byte[0]),
                _ => break,
            }
        }
    }
    let _ = child.kill();
    let _ = child.wait();
    got
}

/// With a bare `ff` argument every line is `ff` plus a newline: before the
/// change this shell first folded argv into `ef bf bd` and then printed it,
/// so three of the four bytes were that replacement character.
#[test]
fn the_repeated_word_is_the_raw_bytes_of_the_argument() {
    let arg: Vec<u8> = b"\xff".to_vec();
    assert_eq!(head_bytes(&[arg], 4), b"\xff\n\xff\n");
}

/// With valid and invalid bytes mixed in one string, the invalid byte must
/// not replace the whole string: a line is four bytes (`a`, `ff`, `b`,
/// newline).
#[test]
fn a_nonutf8_byte_inside_the_word_does_not_disturb_its_neighbours() {
    let arg: Vec<u8> = [b"a".as_slice(), b"\xff", b"b"].concat();
    assert_eq!(head_bytes(&[arg], 8), b"a\xffb\na\xffb\n");
}

/// With no argument the default lone `y` still holds (the other arm of the
/// ternary at `yes.c:52`).
#[test]
fn no_argument_repeats_a_lone_y() {
    assert_eq!(head_bytes(&[], 3), b"y\ny");
}
