//! The argv seam face for the whole crate: every shell gets its arguments through
//! `bin_support::args_bytes()`, where the host leg and the real-machine leg hand
//! back the same raw byte string.
//!
//! The ground truth is a direct reading: the host leg once used
//! `std::env::args()` (unwrapping a non-UTF-8 argument panicked with exit 134),
//! while the reference binary `tmp/random-421/random` printed
//! `unknown option -- \xff` (C passes argv as bytes and never validates it). The
//! two legs first converged on "keep the raw bytes, fold each side lossily"; this
//! face pins the stronger contract that a raw byte must come back as itself: the
//! shells now use `args_bytes()`, lossy folding is no longer part of the seam, and
//! the face below expects a byte-for-byte match with C's answer (the reading from
//! `tmp/random-421/random`).
//!
//! Each shell's own message text is pinned by its own face (`random_face`'s byte
//! form, `wtf_face`'s usage form, `fortune_face`'s three-part `Cannot open`
//! sentence); this one pins only the three forms that must agree **across shells**
//! on the argv seam, plus "no line panics" — the old `env::args()` unwrap panic
//! with exit 134 on the host leg is exactly why this face exists.

use std::ffi::OsStr;
use std::io::Write as _;
use std::os::unix::ffi::OsStrExt as _;
use std::process::{Command, Stdio};

fn bin(name: &str) -> &'static str {
    match name {
        "random" => env!("CARGO_BIN_EXE_random"),
        "wtf" => env!("CARGO_BIN_EXE_wtf"),
        "fortune" => env!("CARGO_BIN_EXE_fortune"),
        _ => unreachable!(),
    }
}

fn run(name: &str, args: &[&OsStr], input: &[u8]) -> (i32, Vec<u8>, Vec<u8>) {
    let mut child = Command::new(bin(name))
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn");
    feed_stdin(&mut child, input);
    let out = child.wait_with_output().expect("wait");
    (
        out.status.code().expect("exit code"),
        out.stdout,
        out.stderr,
    )
}

/// Feed standard input: only `BrokenPipe` is tolerable (a child that exits before
/// reading its input is a normal outcome for several forms; see the same rule in
/// `random_face.rs::feed_stdin`). Any other write error still fails on the spot.
fn feed_stdin(child: &mut std::process::Child, input: &[u8]) {
    let mut stdin = child.stdin.take().expect("stdin pipe");
    if let Err(err) = stdin.write_all(input) {
        assert_eq!(
            err.kind(),
            std::io::ErrorKind::BrokenPipe,
            "写标准输入只该因为「子进程已退出」而失败，实际是 {err}"
        );
    }
}

/// A raw byte argument must behave **byte-for-byte like C's answer** (the reading
/// from `tmp/random-421/random`; the leg that folded `\xff` into `ef bf bd`
/// through lossy conversion is gone), and none of the three shells may exit by
/// signal (`code()` returning None means a crash, such as the old `env::args()`
/// unwrap panic on the host leg, which exited 134 and is exactly why this face
/// exists).
#[test]
fn a_raw_nonutf8_argument_reaches_the_report_as_itself() {
    let raw = OsStr::from_bytes(b"-\xff");
    // random: `-\xff` goes through the option ring, the letter slot of the libc
    // line is **that byte** (the reference reading is `unknown option -- \xff`),
    // and the usage line hardwires `random`.
    let (code, out, err) = run("random", &[raw], b"aa\nbb\n");
    assert_ne!(code, 134, "random 以信号退出：{err:?}");
    assert_eq!(
        err,
        b"random: unknown option -- \xff\nusage: random [-er] [denominator]\n".to_vec()
    );
    assert_eq!(out, Vec::<u8>::new());
    // wtf: `-\xff` in the flag position (starts with `-` and longer than one)
    // only reaches usage, and wtf's usage goes to **standard output** with exit 1
    // — a raw byte and a folded string get the same verdict here (the test is
    // "longer than one"), and the message carries no byte slot at all.
    let (code, out, err) = run("wtf", &[raw], b"");
    assert_eq!(code, 1);
    assert_eq!(out, b"usage: wtf [-o] [-f dbfile] [is] term ...\n".to_vec());
    assert_eq!(err, Vec::<u8>::new());
    // The other way round: a raw byte in the query-word position must reach the
    // message **as itself** — `wtf -- -\xff` takes that byte as the query word
    // (the flag ring stops after `--`), and the name in "I don't know what …" is
    // that byte; the folded `ef bf bd` no longer appears. The query has to be
    // reached at all, so a database must be openable first: the default dictionary
    // is not in the image (the same residual as look), so hand in a scratch
    // database with `-f`.
    let mut db = std::env::temp_dir();
    db.push(format!("argv-seam-db-{}", std::process::id()));
    std::fs::write(&db, b"Y2K: the year two thousand problem\n").expect("writing the scratch db");
    let db_os = OsStr::from_bytes(db.as_os_str().as_bytes());
    let (code, _out, err) = run(
        "wtf",
        &[OsStr::new("-f"), db_os, OsStr::new("--"), raw],
        b"",
    );
    assert_eq!(code, 1);
    assert_eq!(
        err,
        // The whole token is the query word: the `-` is part of it and appears as is.
        b"wtf: I don't know what `-\xff' means!\n".to_vec()
    );
    let _ = std::fs::remove_file(&db);
    // fortune: `-\xff` cannot be opened as a file name, and the file-name slot of
    // the three-part `Cannot open` sentence is the raw byte.
    let (code, out, err) = run("fortune", &[raw], b"");
    assert_eq!(code, 1);
    assert_eq!(out, Vec::<u8>::new());
    assert_eq!(
        err,
        b"fortune: Cannot open `-\xff': No such file or directory\n".to_vec()
    );
}
