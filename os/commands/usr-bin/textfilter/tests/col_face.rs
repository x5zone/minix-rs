//! End-to-end face for the `col` binary, checked against
//! `minix3/usr.bin/col/col.c` (the option loop `:133-158`, the `-l` `atoi`
//! check `:145-150`, the operands' veto `:157-158`, `usage()` `:520-526`) and
//! the libc `getopt` it drives (`minix3/lib/libc/stdlib/getopt.c:74-92`,
//! `:95-104`, `:132-136`).
//!
//! `col.c:133`'s optstring is `"bfhl:px"`: five valueless letters plus one
//! value-bearing `l`, and it accepts **no operand at all** (`optind != argc`
//! goes straight to usage). The reference binary `tmp/col-313/col_ref` (the
//! real `col.c` + Minix's `getopt.c` + a program-name shim, linked on the host)
//! measured the following shapes:
//! 1. out-of-set letter = one libc line + one usage line, exit 1 (`-Z`, the
//!    cluster `-Zb` reports `Z`, `--x` reports `-`);
//! 2. `-l` missing value = libc's `option requires an argument -- l` + two
//!    usage lines (the old form wrongly took the `bad -l argument` branch, a
//!    completely different sentence);
//! 3. `atoi(optarg) <= 0` is `col: bad -l argument <verbatim>.` on a
//!    **single line**, with no usage appended; the program name is hardcoded
//!    `col` and the sentence ends with a period; `atoi` ignores trailing junk,
//!    so `-l 3x` is a valid 3;
//! 4. `--` ends option scanning and is not itself an operand; a bare `-` or
//!    any file argument goes to usage;
//! 5. clustered letters apply one by one (`-bfpx`, `-xb`, `-hx` are all valid,
//!    later ones overriding earlier ones).
//!
//! Before this gate existed: `col`'s read seam used bare `minix_sys::read` and
//! could read no input on the host (the seventh instance of the family), so
//! the line-assembly state machine's output had never been tested at the
//! process level.

mod face_support;

use face_support::write_stdin;
use std::ffi::OsString;
use std::os::unix::ffi::OsStringExt;
use std::process::{Command, Stdio};

const COL: &str = env!("CARGO_BIN_EXE_col");
const USAGE: &str = "usage: col [-bfpx] [-l nline]\n";

fn run(args: &[&str], input: &[u8]) -> (String, i32, String) {
    let mut child = Command::new(COL)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the col binary should be runnable");
    write_stdin(&mut child, input);
    let output = child.wait_with_output().expect("the run should finish");
    (
        String::from_utf8_lossy(&output.stdout).into_owned(),
        output.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

/// Same as above but hands back **raw bytes**: the letter slot must pin "one
/// byte", and `String::from_utf8_lossy` renders both `c3` and `c3 83` as a
/// replacement character, so a text fixture cannot tell those two failure
/// modes apart.
fn run_bytes(args: &[&str], input: &[u8]) -> (Vec<u8>, i32, Vec<u8>) {
    let mut child = Command::new(COL)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the col binary should be runnable");
    write_stdin(&mut child, input);
    let output = child.wait_with_output().expect("the run should finish");
    (
        output.stdout,
        output.status.code().unwrap_or(-1),
        output.stderr,
    )
}

/// Out-of-set letter: the libc line first, `usage()` after, exit 1
/// (`col.c:154-156`). The old form omitted the libc line.
#[test]
fn an_unknown_letter_prints_the_libc_line_then_the_usage() {
    let (out, status, err) = run(&["-Z"], b"plain\n");
    assert_eq!(out, "");
    assert_eq!(status, 1, "stderr 是 {err:?}");
    assert_eq!(err, format!("col: unknown option -- Z\n{USAGE}"));
}

/// A cluster reports only its first character (reference: `-Zb` →
/// `unknown option -- Z`).
#[test]
fn a_cluster_reports_only_its_first_letter() {
    let (out, status, err) = run(&["-Zb"], b"plain\n");
    assert_eq!((out.as_str(), status), ("", 1));
    assert_eq!(err, format!("col: unknown option -- Z\n{USAGE}"));
}

/// `--x` reports the second hyphen as the letter: the optstring has no `-`
/// (reference: `unknown option -- -`).
#[test]
fn a_dash_followed_by_a_word_is_an_unknown_option_named_by_the_dash() {
    let (_, status, err) = run(&["--x"], b"plain\n");
    assert_eq!(status, 1);
    assert_eq!(err, format!("col: unknown option -- -\n{USAGE}"));
}

/// `-l` with nothing after it is libc's missing-argument sentence + usage,
/// **not** `bad -l argument`: the two sentences differ and so do the line
/// counts (the old form merged this branch into `bad -l`, losing the usage
/// line).
#[test]
fn a_missing_line_count_prints_the_libc_line_not_the_bad_argument() {
    let (out, status, err) = run(&["-l"], b"plain\n");
    assert_eq!((out.as_str(), status), ("", 1));
    assert_eq!(
        err,
        format!("col: option requires an argument -- l\n{USAGE}"),
        "缺值那支是两句、程序名跟着 `getprogname()`"
    );
    // A `-l` at the end of a cluster takes the same branch (`-bl`).
    let (_, status, err) = run(&["-bl"], b"plain\n");
    assert_eq!(status, 1);
    assert_eq!(
        err,
        format!("col: option requires an argument -- l\n{USAGE}")
    );
}

/// The `atoi` non-positive branch prints a single line: program name
/// hardcoded `col`, the operand text included, a period at the end, and
/// **no usage appended** (`col.c:145-150` is a bare `fprintf` plus `exit`).
#[test]
fn a_rejected_line_count_quotes_the_operand_and_skips_the_usage() {
    for (word, reported) in [("0", "0"), ("-2", "-2"), ("abc", "abc"), ("", "")] {
        let args = if word.is_empty() {
            vec!["-l".to_string(), "".to_string()]
        } else {
            vec!["-l".to_string(), word.to_string()]
        };
        let refs: Vec<&str> = args.iter().map(String::as_str).collect();
        let (out, status, err) = run(&refs, b"plain\n");
        assert_eq!((out.as_str(), status), ("", 1), "`-l {word}` 该被拒");
        assert_eq!(
            err,
            format!("col: bad -l argument {reported}.\n"),
            "句子该带操作数原文与句点，且不接 usage"
        );
    }
}

/// `atoi` ignores trailing junk, so `-l 3x` is a valid 3; the value may also
/// be attached to the option (`-l3`). The old form used Rust's
/// `parse::<i32>()`, which judged `3x` illegal and did not accept the
/// attached form either.
#[test]
fn the_line_count_accepts_attached_and_junk_tailed_forms() {
    for args in [&["-l3"][..], &["-l", "3"][..], &["-l", "3x"][..]] {
        let (out, status, err) = run(args, b"plain\n");
        assert_eq!(
            (out.as_str(), err.as_str(), status),
            ("plain\n", "", 0),
            "args 是 {args:?}"
        );
    }
}

/// The five valueless letters take effect one by one, later ones overriding
/// earlier ones: the `x` in `-bfpx` turns off space compression, and `-b`
/// keeps only the last of a backspace and an overstruck character.
#[test]
fn valueless_letters_apply_letter_by_letter_within_a_cluster() {
    // Default: backspace passes through verbatim (reference `a^Hb`).
    let (out, status, err) = run(&[], b"a\x08b\nline2\n");
    assert_eq!(
        (out.as_str(), err.as_str(), status),
        ("a\x08b\nline2\n", "", 0)
    );
    // `-b`: no backspace output; an overstruck pair keeps only the last
    // character.
    let (out, status, _) = run(&["-b"], b"a\x08b\nline2\n");
    assert_eq!((out.as_str(), status), ("b\nline2\n", 0));
    // By default leading spaces compress to a tab; `-x` flattens them into
    // spaces.
    let (out, _, _) = run(&[], b"x\ty\n");
    assert_eq!(out, "x\ty\n");
    let (out, status, _) = run(&["-x"], b"x\ty\n");
    assert_eq!(
        (out.as_str(), status),
        ("x       y\n", 0),
        "`-x` 该摊成空格"
    );
    // Cluster: `-xb` enables both.
    let (out, status, _) = run(&["-xb"], b"a\x08b\nx\ty\n");
    assert_eq!((out.as_str(), status), ("b\nx       y\n", 0));
}

/// Any operand is a veto: `col` reads only standard input, so a bare `-`, a
/// file argument, or anything after `--` all print the one usage line; a lone
/// `--` is legal (it does not count as an operand).
#[test]
fn any_operand_is_rejected_but_the_marker_alone_is_not() {
    let (out, status, err) = run(&["--"], b"plain\n");
    assert_eq!((out.as_str(), err.as_str(), status), ("plain\n", "", 0));

    for args in [&["-"][..], &["somefile"][..], &["--", "-b"][..]] {
        let (out, status, err) = run(args, b"plain\n");
        assert_eq!((out.as_str(), status), ("", 1), "args 是 {args:?}");
        assert_eq!(err, USAGE, "`{args:?}` 只该印 usage 一行");
    }
}

/// The output leg (the other half of why this gate exists): `col` used to read
/// through bare `minix_sys::read` and got no input on the host, so the
/// line-assembly state machine had never been tested at the process level.
/// The golden shapes are taken from the reference binary's readings
/// (`golden.log`).
#[test]
fn the_reverse_feed_filter_reaches_the_output_end_to_end() {
    // Reverse line feed (carriage return + backspace pullback) is absorbed;
    // both lines survive.
    let (out, status, err) = run(&[], b"l1\n\x08\x08l0\n");
    assert_eq!((out.as_str(), err.as_str(), status), ("l1\nl0\n", "", 0));
    // Blank lines come out as they are.
    let (out, status, _) = run(&[], b"\n\n\n");
    assert_eq!((out.as_str(), status), ("\n\n\n", 0));
    // An unknown escape sequence is swallowed (reference: `a\033Zb` yields
    // `ab`).
    let (out, status, _) = run(&[], b"a\x1bZb\n");
    assert_eq!((out.as_str(), status), ("ab\n", 0));
    let (out, status, _) = run(&["-p"], b"a\x1bZb\n");
    assert_eq!((out.as_str(), status), ("ab\n", 0));
    // A last line with no newline is not emitted (C flushes only at a
    // newline; the reference matches).
    let (out, status, err) = run(&[], b"abc");
    assert_eq!((out.as_str(), err.as_str(), status), ("", "", 0));
    // Empty input yields empty output.
    let (out, status, _) = run(&[], b"");
    assert_eq!((out.as_str(), status), ("", 0));
}

/// Measured against the reference binary `tmp/col-313/col_ref`: `col -bé`
/// prints `col: unknown option -- ` + **the raw byte `0xC3`** + newline,
/// exits 1, then the whole usage block. This layer once split by character
/// and so printed a full `é` (`c3 a9`) — the sentence looked right, the bytes
/// were not. The gate compares the code point U+00C3 (`char::from(0xC3)`,
/// printed as `c3 83`); taking by character would yield the whole `é` (U+00E9,
/// printed as `c3 a9`). **The last stretch is still one difference this family
/// shares**: C prints the single raw byte `c3`, while this tree going through
/// `char`'s `Display` emits two bytes — that is registered in the
/// `*-NONASCII-OPTLETTER` family to be collected with the shared seam, and
/// this gate pins only the judgment "the first byte is taken" (the same
/// treatment `column` already has).
#[test]
fn nonascii_option_letter_is_reported_as_one_byte() {
    // Measured against the reference binary `tmp/col-313/col`: all three
    // forms print **the one raw byte `c3`** + usage, exit 1: `-bé` (`b` is
    // valid, then the first byte of `é` is consumed), `-é`, `-xé`. This gate
    // once pinned the lossy code point U+00C3 — that corresponded to the
    // version that "takes bytes but still renders as `char`" (printing the two
    // bytes `c3 83`); after switching to the byte form the expectation became
    // byte-wise, because lossy reads both a raw `c3` and `c3 83` as the
    // replacement character, leaving a text fixture unable to distinguish
    // them.
    let want: Vec<u8> = [
        b"col: unknown option -- ".as_slice(),
        b"\xc3\n",
        USAGE.as_bytes(),
    ]
    .concat();
    for form in ["-b\u{e9}", "-\u{e9}", "-x\u{e9}"] {
        let (out, code, err) = run_bytes(&[form], b"a\n");
        assert_eq!(out, Vec::new(), "{form} 的报文只走 stderr");
        assert_eq!(code, 1, "{form} 的 stderr 是 {err:?}");
        assert_eq!(err, want, "{form} 的字母该是一个裸字节 c3");
    }

    // The ASCII form is identical through both entries: it reports `Z`
    // itself.
    let (_out, code, err) = run_bytes(&["-Z"], b"a\n");
    assert_eq!(code, 1);
    assert_eq!(
        err,
        b"col: unknown option -- Z\nusage: col [-bfpx] [-l nline]\n".to_vec()
    );
}

/// The `-l` missing-value form also goes through the byte form: `l` is ASCII,
/// so both entries agree (measured: `col: option requires an argument -- l` +
/// usage, exit 1). This gate guards against "the missing-value sentence's
/// wording drifting along when the rendering changes".
#[test]
fn the_missing_value_line_of_l_is_ascii_and_unchanged() {
    let (out, code, err) = run_bytes(&["-l"], b"a\n");
    assert_eq!(out, Vec::new());
    assert_eq!(code, 1);
    assert_eq!(
        err,
        b"col: option requires an argument -- l\nusage: col [-bfpx] [-l nline]\n".to_vec()
    );
}
/// After the argument seam hands back raw bytes, both of `col`'s faces fall
/// into place together: the letter slot is one raw `ff` (libc's `%c` prints
/// the `optopt` byte, `minix3/lib/libc/stdlib/getopt.c:103`), and the operand
/// text of the `bad -l argument` sentence is also one `ff`
/// (`col.c:145-150`'s `%s`). The old text seam folded both into the three-byte
/// U+FFFD and then took its first byte `ef`.
#[test]
fn a_raw_nonutf8_letter_and_line_count_keep_their_own_bytes() {
    let child = Command::new(COL)
        .arg(OsString::from_vec(b"-\xff".to_vec()))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the col binary should be runnable");
    let output = child.wait_with_output().expect("the run should finish");
    assert_eq!(
        output.stderr,
        b"col: unknown option -- \xff\nusage: col [-bfpx] [-l nline]\n".as_slice()
    );

    let child = Command::new(COL)
        .arg(OsString::from_vec(b"-l\xff".to_vec()))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the col binary should be runnable");
    let output = child.wait_with_output().expect("the run should finish");
    assert_eq!(
        output.stderr,
        b"col: bad -l argument \xff.\n".as_slice(),
        "句子带的是原字节，也不接 usage"
    );
}
