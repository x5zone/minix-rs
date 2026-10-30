//! End-to-end face for the `fold` binary, checked against
//! `minix3/usr.bin/fold/fold.c` (option ring `:76-101`, operand loop
//! `:103-110`, `usage()` `:115-121`) and the libc `getopt` it drives
//! (`minix3/lib/libc/stdlib/getopt.c:101-104`, `:132-136`).
//!
//! C's option string is `"0123456789bsw:"`: only `w` takes a value, `0-9` are
//! **legal letters** for the legacy width form, and the string does not begin
//! with `:` so `opterr` is in effect. Hence the three exits differ in **line
//! count**, which is what this gate pins down:
//! (1) the two getopt reports (a letter outside the set, `-w` missing its
//! value) = one libc line + one usage line;
//! (2) a non-positive width from `atoi` = the command's own
//! `errx(1, "illegal width value")`, **that line only**;
//! (3) `usage()`'s program name follows `argv[0]` (`getprogname()`), exit 1.
//!
//! This face did not exist before: `fold`'s read seam used raw `open`/`read`,
//! so on the host it could read no input at all, and the coverage had to come
//! from library tests (the same shortfall `nl` had).

mod face_support;

use face_support::write_stdin;
use std::ffi::OsString;
use std::fs;
use std::os::unix::ffi::OsStringExt;
use std::process::{Command, Stdio};

const FOLD: &str = env!("CARGO_BIN_EXE_fold");
const USAGE: &str = "usage: fold [-bs] [-w width] [file ...]\n";

fn run(args: &[&str], input: &[u8]) -> (String, i32, String) {
    let mut child = Command::new(FOLD)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the fold binary should be runnable");
    write_stdin(&mut child, input);
    let output = child.wait_with_output().expect("the run should finish");
    (
        String::from_utf8_lossy(&output.stdout).into_owned(),
        output.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

/// A letter outside the set: libc's line first, `usage()` after, exit 1
/// (`fold.c:99-101`'s `default`).
#[test]
fn an_unknown_letter_prints_the_libc_line_then_the_usage() {
    let (out, status, err) = run(&["-Z"], b"abc");
    assert_eq!(out, "", "报文只走 stderr");
    assert_eq!(status, 1, "stderr 是 {err:?}");
    assert_eq!(err, format!("fold: unknown option -- Z\n{USAGE}"));
}

/// The same command seam as `run`, but handing bytes back **verbatim**:
/// `from_utf8_lossy` renders a bare `c3` as a three-byte U+FFFD, so the text
/// form cannot tell the two failure modes "one byte" and "two bytes" apart
/// (the `rev`/`paste`/`jot` arms share this property).
fn run_bytes(args: &[&str], input: &[u8]) -> (Vec<u8>, i32, Vec<u8>) {
    let mut child = Command::new(FOLD)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the fold binary should be runnable");
    write_stdin(&mut child, input);
    let output = child.wait_with_output().expect("the run should finish");
    (
        output.stdout,
        output.status.code().unwrap_or(-1),
        output.stderr,
    )
}

/// The letter slot on the unknown-letter line is **one bare byte**: a reference
/// build (`fold.c` plus Minix's `getopt.c` plus a program-name shim) measured
/// both `-é` and `-bé` as `fold: unknown option -- ` + `c3` + newline followed
/// by the usage, exit 1 (`fold.c:76`'s option string is `"0123456789bsw:"`;
/// `b` is a legal letter that takes no value, so the cluster keeps going and
/// eats the first byte of `é`; `-Zé` stops at `Z`, where `fold.c:99-101`'s
/// `default` goes straight to `usage()` with no message accumulated).
/// The ASCII `-Z` already has an owner in this file
/// (`an_unknown_letter_prints_the_libc_line_then_the_usage`), so under the
/// redundancy criterion it is not carried twice.
#[test]
fn the_libc_line_of_a_multibyte_flag_folds_to_one_byte() {
    let want: Vec<u8> = [
        b"fold: unknown option -- ".as_slice(),
        b"\xc3\n",
        USAGE.as_bytes(),
    ]
    .concat();
    for form in ["-\u{e9}", "-b\u{e9}"] {
        let (out, status, err) = run_bytes(&[form], b"abc");
        assert_eq!(out, Vec::new(), "{form} 的报文只走标准错误");
        assert_eq!(status, 1, "{form} 的 stderr 是 {err:?}");
        assert_eq!(err, want, "{form} 的字母该是一个裸字节 c3");
    }

    let (_out, status, err) = run_bytes(&["-Z\u{e9}"], b"abc");
    assert_eq!(status, 1);
    assert_eq!(
        err,
        [b"fold: unknown option -- Z\n".as_slice(), USAGE.as_bytes(),].concat(),
        "环停在第一个坏字母上，`é` 根本不该被读到"
    );
}

/// Once the argument seam hands back bytes, a letter slot in a cluster that is
/// **not a valid UTF-8 prefix** is one byte too. The previous gate used the
/// valid sequence `c3 a9`, which lossy leaves untouched, so the old seam passed
/// it as well; this one uses a bare `ff`: the old `String::from_utf8_lossy` seam
/// rendered that as a three-byte U+FFFD, the ring's first byte became `ef`, one
/// position more than the reference build shows. `-b` is a legal letter taking
/// no value (option string `"0123456789bsw:"`), so the ring moves on to the next
/// position and this form takes the cluster leg, not the same seam as the
/// single-letter leg in the cross-shell table.
#[test]
fn a_raw_nonutf8_letter_after_a_cluster_flag_is_one_byte() {
    let want: Vec<u8> = [
        b"fold: unknown option -- ".as_slice(),
        b"\xff\n",
        USAGE.as_bytes(),
    ]
    .concat();
    let (out, status, err) = run_raw(&[b"-b\xff".to_vec()], b"abc");
    assert_eq!(out, Vec::new(), "报文只走 stderr");
    assert_eq!(status, 1, "stderr 是 {err:?}");
    assert_eq!(err, want, "字母该是原字节 ff，不是 lossy 的第一字节 ef");
}

/// Like `run_bytes`, but the arguments are handed back as **raw bytes**: a
/// `&str` fixture cannot write a bare `ff` — written as `\u{fffd}` it would be
/// encoded as `ef bf bd`, and then this would not be the byte under test.
fn run_raw(args: &[Vec<u8>], input: &[u8]) -> (Vec<u8>, i32, Vec<u8>) {
    let mut child = Command::new(FOLD)
        .args(args.iter().map(|word| OsString::from_vec(word.clone())))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the fold binary should be runnable");
    write_stdin(&mut child, input);
    let output = child.wait_with_output().expect("the run should finish");
    (
        output.stdout,
        output.status.code().unwrap_or(-1),
        output.stderr,
    )
}

/// `-w` followed by no value is libc's other sentence; it has a **different
/// shape** from the "value is not positive" sentence, and a different line
/// count.
#[test]
fn a_missing_width_value_prints_the_libc_line_then_the_usage() {
    let (out, status, err) = run(&["-w"], b"abc");
    assert_eq!(out, "");
    assert_eq!(status, 1, "stderr 是 {err:?}");
    assert_eq!(
        err,
        format!("fold: option requires an argument -- w\n{USAGE}")
    );
}

/// A non-positive width parsed by `atoi` goes through the command's own
/// `errx`: that line only, **no usage**, exit 1 (`fold.c:84-86`). Both
/// spellings (letter and digits) must give the same sentence.
#[test]
fn a_nonpositive_width_is_one_line_without_the_usage() {
    // Pin only the two value-carrying arms. The legacy `-0` is **not an error**
    // in C: the digit case sets width to 0 and carries on (after
    // `fold.c:91-98` the only backstop is `if (width == -1)`); this shell
    // rejects it under positive-integer validation, and that form is already
    // recorded as a declared deviation, so it is not signed off here as C's
    // shape.
    for args in [vec!["-w", "0"], vec!["-w", "-3"]] {
        let (out, status, err) = run(&args, b"abc");
        assert_eq!(out, "", "{args:?}  printed {out:?}");
        assert_eq!(status, 1, "{args:?} 的 stderr 是 {err:?}");
        assert_eq!(err, "fold: illegal width value\n", "{args:?}");
    }
}

/// The width can be given three ways: default 80, `-w N`, and the legacy `-N`
/// or a digit inside a cluster (`-b9`). In C the cluster-digit form reads
/// `argv[optind - 1]`, so its result follows getopt's internal position and no
/// reference build can take evidence for it; this shell treats the digit run at
/// that position as the width and records that as a declared deviation (see the
/// `rewrite-notes/18-stage-commands/todo.md` ledger). What is pinned here is the
/// half where it is **not taken for an unknown letter**.
#[test]
fn the_width_can_be_given_three_ways() {
    let (out, _, _) = run(&[], b"abcdefghij");
    assert_eq!(out, "abcdefghij\n", "默认宽 80 不折");
    let (out, _, err) = run(&["-9"], b"abcdefghij");
    assert_eq!((out.as_str(), err.as_str()), ("abcdefghi\nj\n", ""));
    let (out, _, err) = run(&["-w", "4"], b"abcdefghij");
    assert_eq!((out.as_str(), err.as_str()), ("abcd\nefgh\nij\n", ""));
    let (out, _, err) = run(&["-b9"], b"abcdefghij");
    assert_eq!(
        (out.as_str(), err.as_str()),
        ("abcdefghi\nj\n", ""),
        "聚簇里的数字不该被当成集外字母"
    );
}

/// Operands follow C's `for (; *argv; ++argv) freopen(...) … fold(width)`:
/// every file **restarts** the column state, so two short files do not join
/// into one wide line.
#[test]
fn each_operand_restarts_the_column_state() {
    let dir = std::env::temp_dir().join(format!("fold-face-{}", std::process::id()));
    fs::create_dir_all(&dir).expect("scratch dir");
    let one = dir.join("one");
    let two = dir.join("two");
    fs::write(&one, b"abc").expect("write one");
    fs::write(&two, b"de").expect("write two");
    let (out, status, err) = run(
        &["-w", "4", one.to_str().unwrap(), two.to_str().unwrap()],
        b"",
    );
    assert_eq!(status, 0, "stderr 是 {err:?}");
    assert_eq!(out, "abc\nde\n", "两个文件各自从第一列起：{out:?}");
    let _ = fs::remove_dir_all(dir);
}

/// An unreadable operand is C's `err(1, "%s", *argv)`: program name, operand
/// and the `strerror` sentence are all three present before exit
/// (`fold.c:107-109`). The old spelling printed only the first two.
#[test]
fn an_unopenable_operand_carries_the_number_sentence() {
    let missing = format!("/tmp/fold-face-definitely-absent-{}", std::process::id());
    let (out, status, err) = run(&[missing.as_str()], b"abc");
    assert_eq!(out, "", "报文只走 stderr");
    assert_eq!(status, 1, "stderr 是 {err:?}");
    let head = format!("fold: {missing}: ");
    assert!(
        err.starts_with(&head),
        "该以 `fold: <操作数>: ` 开头，实际 {err:?}"
    );
    let sentence = err[head.len()..].trim_end();
    assert!(
        !sentence.is_empty() && sentence.chars().any(|c| c.is_lowercase()),
        "缀的应是一句 strerror 文本而不是代号：{sentence:?}"
    );
}
