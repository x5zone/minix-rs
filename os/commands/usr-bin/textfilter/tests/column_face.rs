//! End-to-end face for the `column` binary, checked against
//! `minix3/usr.bin/column/column.c` (the option loop `:93-110`, the operand
//! section `:112-127`, `input()`'s long-line verdict `:270-297`, `usage()`
//! `:302-307`) and the libc `getopt` it drives
//! (`minix3/lib/libc/stdlib/getopt.c:74-78`, `:81-92`, `:95-104`, `:132-136`).
//!
//! The optstring at `column.c:93` is `"c:s:tx"`: two value-taking items plus
//! two flags, and the operands are a **file list** (open and read it if
//! possible, otherwise `warn(3)` and continue). The shapes measured one by one
//! against a reference build (`column.c`'s real source + Minix's `getopt.c` +
//! program-name/`fgetln` shims, linked on the host) are:
//! ① A letter outside the set = one libc line + one usage line, exit 1 (`-Z`,
//!    the cluster `-tZ` blames `Z`, `--x` blames `-`);
//! ② `-c`/`-s` missing a value = libc's
//!    `option requires an argument -- <letter>` + usage, exit 1 (the old
//!    spelling folded this arm into a self-invented usage and lost the libc
//!    line);
//! ③ The usage line is **capital `Usage`, the program name follows
//!    `getprogname()`, and the tail carries `[file ...]`**; the old spelling
//!    was a self-invented lowercase short form;
//! ④ `-c`'s value goes through `atoi`: `0`, `abc`, and `-5` are not errors;
//!    the width collapses to "one entry per line" and the exit status is 0;
//! ⑤ No reordering: the option loop ends at the first non-option token, and a
//!    `-t` after it is a **file name** (measured on the reference build:
//!    `column: Cannot open `-t': No such file or directory`), not a silently
//!    swallowed flag;
//! ⑥ An unopenable operand is the three-part `warn(3)` and is **not fatal**:
//!    skip this one, set exit status 1, continue with the next file;
//! ⑦ A bare `-` and the tokens after `--` are operands, while `--` itself is
//!    not (`getopt.c:81-85` advances first);
//! ⑧ When the last line has no newline, C calls it a "long line": that
//!    `warnx(3)` line has only two parts (no strerror attached), the line does
//!    not enter the entry pool, and the exit status is set to 1 -- an all-blank
//!    last line is still treated as an empty line, with no warning at all;
//! ⑨ An entry is the **whole line**: C stores the `buf` at the start of the
//!    line, not the pointer after skipping leading blanks, so leading blanks
//!    both enter the pool and occupy layout width.
//!    And it is **bytes**: a non-UTF-8 byte still enters the pool and still
//!    occupies layout width by `strlen`; the old spelling decoded UTF-8 once on
//!    the shell side, so a single bad byte made the **whole file** yield no
//!    entries at all;
//! ⑩ The last column of table mode is emitted verbatim (`printf("%s\n", …)` at
//!    `column.c:252`), with neither padding nor trimming, so a trailing
//!    carriage return stays at the end of the line.
//!
//! ⑧ and ⑨ were both caught the first time this new gate actually exercised
//! layout end to end: the old `c_columnate`/`r_columnate` returned entries,
//! tabs, and newlines as three separate pieces, and the shell added one
//! newline per piece, so neither the vertical nor the horizontal fill ever
//! printed a single proper line -- there was no process-level gate before, and
//! no test could see it.
//!
//! This gate did not exist before: `column`'s four exit classes had only a
//! self-invented usage line, and the long-line sentence did not even have a
//! shadow.

mod face_support;

use face_support::write_stdin;
use std::fs;
use std::process::{Command, Stdio};

const COLUMN: &str = env!("CARGO_BIN_EXE_column");
/// The line from `column.c:304-305`: capital `Usage`, the program name
/// following `getprogname()`.
const USAGE: &str = "Usage: column [-tx] [-c columns] [-s sep] [file ...]\n";
/// The layout the reference build produces at its default width of 80 when `-c`
/// is absent (five 5-byte entries fit on one line).
/// The fixture that compares all three channels byte by byte: the
/// `col`/`rev`/`colrm`/`column` family all needs to pin "the letter slot is one
/// raw byte", and `String::from_utf8_lossy` renders both a bare `c3` and
/// `c3 83` as the replacement character, so a text fixture cannot tell the two
/// failure modes apart.
fn run_bytes(args: &[&str], input: &[u8]) -> (Vec<u8>, i32, Vec<u8>) {
    let mut child = Command::new(COLUMN)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the column binary should be runnable");
    write_stdin(&mut child, input);
    let output = child.wait_with_output().expect("the run should finish");
    (
        output.stdout,
        output.status.code().unwrap_or(-1),
        output.stderr,
    )
}

/// The **byte-shape** fixture for argv: a bare `ff` cannot be written as
/// `&str`, so arguments go down through `OsStr::from_bytes` and the comparison
/// is against the bytes the child actually received (a text fixture would let
/// `String::from_utf8_lossy` turn `ff` and `ef bf bd` into the same string).
fn run_argv(args: &[&[u8]], input: &[u8]) -> (Vec<u8>, i32, Vec<u8>) {
    use std::os::unix::ffi::OsStrExt as _;
    let mut child = Command::new(COLUMN)
        .args(args.iter().map(|a| std::ffi::OsStr::from_bytes(a)))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the column binary should be runnable");
    write_stdin(&mut child, input);
    let output = child.wait_with_output().expect("the run should finish");
    (
        output.stdout,
        output.status.code().unwrap_or(-1),
        output.stderr,
    )
}

/// The command-line half is byte-shaped too: the value of `-s` is the output
/// separator, so it is the only leg in this branch that **reaches the data
/// plane**. The reference build measures two shapes -- under `-s<ff>`,
/// `a<ff>b` splits into two columns and prints `a  b`, while `a<ef bf bd>b`
/// (the three-byte U+FFFD in the input) stays one line as-is. A text seam gets
/// both shapes wrong at once: once `ff` is folded to `ef bf bd`, the separator
/// becomes "three bytes", so a real `ff` does not split and those three bytes
/// split instead.
/// A third shape pins the letter slot: `-\xff` returns that one bare byte
/// itself, the same string as usage, and exits 1.
#[test]
fn an_option_value_of_column_splits_on_the_raw_byte_it_carries() {
    let (out, code, err) = run_argv(&[b"-t", b"-s\xff"], b"a\xffb\tz\n");
    assert_eq!(
        (out.as_slice(), code, err.as_slice()),
        (&b"a  b\tz\n"[..], 0, &b""[..]),
        "分隔符是该字节本身"
    );

    let (out, code, err) = run_argv(&[b"-t", b"-s\xff"], b"a\xef\xbf\xbdb\n");
    assert_eq!(
        (out.as_slice(), code, err.as_slice()),
        (&b"a\xef\xbf\xbdb\n"[..], 0, &b""[..]),
        "U+FFFD 的三字节不是分隔符，不该被误切"
    );

    let (out, code, err) = run_argv(&[b"-\xff"], b"alpha\nbeta\n");
    assert_eq!(out, Vec::new(), "集外字母不打数据");
    assert_eq!(code, 1);
    let want: Vec<u8> = [
        b"column: unknown option -- ".as_slice(),
        b"\xff\n",
        USAGE.as_bytes(),
    ]
    .concat();
    assert_eq!(err, want, "字母位该是 argv 里那一枚原字节");
}

/// The letter slot in an out-of-set letter's line is **one raw byte**: the
/// reference build (`column.c`'s real source + Minix's `getopt.c`, started
/// through a symlink so the program name matches this shell) measures that
/// `column -é` and `column -té` both print `unknown option -- ` + `c3` +
/// newline + usage and exit 1 (the optstring is `"c:s:tx"`, `t` is legal, so
/// the cluster keeps consuming and reaches the first byte of `é`).
/// The ASCII `-Z` and the missing-value `-c` shapes are pinned at the same time:
/// changing the rendering must not move them.
#[test]
fn the_option_line_of_a_nonascii_letter_carries_one_raw_byte() {
    let want: Vec<u8> = [
        b"column: unknown option -- ".as_slice(),
        b"\xc3\n",
        USAGE.as_bytes(),
    ]
    .concat();
    for form in ["-\u{e9}", "-t\u{e9}"] {
        let (out, code, err) = run_bytes(&[form], FIVE);
        assert_eq!(out, Vec::new(), "{form} 的报文只走 stderr");
        assert_eq!(code, 1, "{form} 的 stderr 是 {err:?}");
        assert_eq!(err, want, "{form} 的字母该是一个裸字节 c3");
    }

    let (_out, code, err) = run_bytes(&["-Z"], FIVE);
    assert_eq!(code, 1);
    assert_eq!(
        err,
        b"column: unknown option -- Z\nUsage: column [-tx] [-c columns] [-s sep] [file ...]\n"
            .to_vec()
    );

    let (_out, code, err) = run_bytes(&["-c"], FIVE);
    assert_eq!(code, 1);
    assert_eq!(
        err,
        b"column: option requires an argument -- c\nUsage: column [-tx] [-c columns] [-s sep] [file ...]\n".to_vec()
    );
}

const FIVE: &[u8] = b"alpha\nbeta\ngamma\ndelta\nepsilon\n";
const FIVE_LINE: &str = "alpha\tbeta\tgamma\tdelta\tepsilon\n";

fn run(args: &[&str], input: &[u8]) -> (String, i32, String) {
    let mut child = Command::new(COLUMN)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the column binary should be runnable");
    write_stdin(&mut child, input);
    let output = child.wait_with_output().expect("the run should finish");
    (
        String::from_utf8_lossy(&output.stdout).into_owned(),
        output.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

/// Out-of-set letter: the libc line first, `usage()` after it, exit 1
/// (`column.c:107-109`). The old spelling had only the usage line; the libc
/// line was missing entirely.
#[test]
fn an_unknown_letter_prints_the_libc_line_then_the_usage() {
    let (out, status, err) = run(&["-Z"], FIVE);
    assert_eq!((out.as_str(), status), ("", 1), "stderr 是 {err:?}");
    assert_eq!(err, format!("column: unknown option -- Z\n{USAGE}"));

    // The cluster reports only the first out-of-set letter (reference build:
    // `-tZ` -> `Z`).
    let (_, status, err) = run(&["-tZ"], FIVE);
    assert_eq!(status, 1);
    assert_eq!(err, format!("column: unknown option -- Z\n{USAGE}"));

    // `--x` blames the second hyphen: the optstring has no `-`.
    let (_, status, err) = run(&["--x"], FIVE);
    assert_eq!(status, 1);
    assert_eq!(err, format!("column: unknown option -- -\n{USAGE}"));
}

/// A value-taking item with no following segment gets libc's missing-value
/// sentence + usage, **not** a sentence invented by this command
/// (`getopt.c:132-136` prints first, `column.c:107-109` then prints usage).
#[test]
fn a_missing_option_value_prints_the_libc_line_then_the_usage() {
    for letter in ['c', 's'] {
        let (out, status, err) = run(&[&format!("-{letter}")], FIVE);
        assert_eq!((out.as_str(), status), ("", 1));
        assert_eq!(
            err,
            format!("column: option requires an argument -- {letter}\n{USAGE}"),
            "缺值那支是两句、程序名跟着 `getprogname()`"
        );
    }
    // A `-c` that ends up at the tail of a cluster takes the same arm.
    let (_, status, err) = run(&["-tc"], FIVE);
    assert_eq!(status, 1);
    assert_eq!(
        err,
        format!("column: option requires an argument -- c\n{USAGE}")
    );
}

/// `-c`'s value goes through C's `atoi` (`column.c:96`, unguarded): non-positive
/// or non-numeric is not an error, the width just collapses to "one entry per
/// line", and the exit status stays 0. The old spelling judged all three as
/// usage and exited 1.
#[test]
fn an_unparsable_width_is_not_an_error() {
    let each_line = "alpha\nbeta\ngamma\ndelta\nepsilon\n";
    for word in ["0", "abc", "-5", ""] {
        let args = if word.is_empty() {
            vec!["-c", ""]
        } else {
            vec!["-c", word]
        };
        let (out, status, err) = run(&args, FIVE);
        assert_eq!(
            (out.as_str(), status),
            (each_line, 0),
            "`-c {word}` 的 stderr 是 {err:?}"
        );
        assert_eq!(err, "");
    }
    // Trailing junk is ignored by `atoi`: `-c20x` is width 20 (the reference
    // build agrees).
    let (out, status, _) = run(&["-c20x"], FIVE);
    assert_eq!(status, 0);
    assert_eq!(out, "alpha\tdelta\nbeta\tepsilon\ngamma\n");
}

/// Flags act letter by letter and values may be attached to the same string:
/// `-tx` and `-t -c12 -s,` must not degrade into operands.
#[test]
fn flags_cluster_and_values_attach() {
    // At width 12, `numcols = 12 / 8 = 1`, so `-x`'s horizontal fill is also
    // one entry per line.
    let (out, status, err) = run(&["-tx", "-c", "12"], FIVE);
    assert_eq!(
        (out.as_str(), status),
        ("alpha\nbeta\ngamma\ndelta\nepsilon\n", 0)
    );
    assert_eq!(err, "");
    // `-s`'s separator only affects `-t`'s field splitting: the attached and
    // separate forms resolve identically.
    let tabbed = b"aa,bb,cc\ndddddee,ff,ggg\n";
    let (attached, _, _) = run(&["-t", "-s,"], tabbed);
    let (separate, _, _) = run(&["-t", "-s", ","], tabbed);
    assert_eq!(attached, separate);
    assert_eq!(attached, "aa       bb  cc\ndddddee  ff  ggg\n");
}

/// No reordering: the option loop ends the moment the first non-option token
/// arrives, and a `-t` after it is a **file name**. The reference build
/// measures `column: Cannot open `-t': No such file or directory`, exit 1.
#[test]
fn the_option_ring_stops_at_the_first_operand() {
    let dir = scratch("reorder");
    let file = dir.join("in.txt");
    fs::write(&file, FIVE).expect("write the operand file");
    let (out, status, err) = run(&[file.to_str().unwrap(), "-t"], b"should not be read\n");
    assert_eq!(status, 1);
    assert_eq!(err, "column: Cannot open `-t': No such file or directory\n");
    // The entry pool holds only the five entries of that file (`-t` was not
    // eaten as a flag, so the layout is still vertical fill).
    assert_eq!(out, FIVE_LINE);
}

/// An unopenable operand is the three-part `warn(3)` and **not a fatal
/// error**: skip it, continue with the next file, and set exit status 1
/// (`column.c:119-122`).
#[test]
fn an_unopenable_operand_warns_and_the_run_continues() {
    let dir = scratch("continue");
    let file = dir.join("good.txt");
    fs::write(&file, b"alpha\nbeta\n").expect("write the operand file");
    let (out, status, err) = run(&[file.to_str().unwrap(), "no-such-file"], b"ignored\n");
    assert_eq!(status, 1);
    assert_eq!(
        err, "column: Cannot open `no-such-file': No such file or directory\n",
        "三段形：程序名、操数原文、`strerror` 句子"
    );
    assert_eq!(out, "alpha\tbeta\n");

    // A bad file first behaves the same: the good file's entries still enter
    // the pool.
    let (out, status, _) = run(&["no-such-file", file.to_str().unwrap()], b"");
    assert_eq!((out.as_str(), status), ("alpha\tbeta\n", 1));
}

/// A bare `-` is a file name, not an alias for standard input (`column.c:115`
/// calls `fopen` directly): the reference build measures
/// `column: Cannot open `-': No such file or directory`, and standard input is
/// not read.
#[test]
fn a_solitary_dash_is_a_file_name_not_stdin() {
    let (out, status, err) = run(&["-"], FIVE);
    assert_eq!((out.as_str(), status), ("", 1));
    assert_eq!(err, "column: Cannot open `-': No such file or directory\n");
}

/// `--` ends the option loop without being an operand itself: `column`
/// therefore falls back to reading standard input at that `!*argv` test (the
/// reference build with `--` exits 0 and prints the standard-input layout);
/// only tokens after `--` are file names.
#[test]
fn the_double_dash_ends_the_ring_without_becoming_an_operand() {
    let (out, status, err) = run(&["--"], FIVE);
    assert_eq!((out.as_str(), status), (FIVE_LINE, 0));
    assert_eq!(err, "");

    let dir = scratch("dashdash");
    let file = dir.join("named.txt");
    fs::write(&file, b"one\ttwo\n").expect("write the operand file");
    // `-c` after `--` is a literal file name (unopenable -> warn); standard
    // input is not read.
    let (out, status, err) = run(&["--", "-c", file.to_str().unwrap()], FIVE);
    assert_eq!(status, 1);
    assert_eq!(err, "column: Cannot open `-c': No such file or directory\n");
    // Without `-t`, entries enter the layout as-is: the tab inside is not
    // replaced by padding spaces.
    assert_eq!(out, "one\ttwo\n");
}

/// Last line without a newline: C calls it a "long line", drops that entry,
/// prints one two-part `warnx` line, and sets exit status 1
/// (`column.c:277-283`). An all-blank last line is treated as an empty line,
/// with no warning at all.
#[test]
fn an_unterminated_last_line_warns_and_is_dropped() {
    let (out, status, err) = run(&[], b"one\ntwo");
    assert_eq!((out.as_str(), status), ("one\n", 1));
    assert_eq!(
        err, "column: line too long\n",
        "`warnx` 不缀 strerror，所以只有两段"
    );

    // Blank last line: no entry is dropped, so there is no warning.
    let (out, status, err) = run(&[], b"one\n   ");
    assert_eq!((out.as_str(), status, err.as_str()), ("one\n", 0, ""));

    // Input ending with a newline behaves normally.
    let (out, status, err) = run(&[], b"one\ntwo\n");
    assert_eq!((out.as_str(), status, err.as_str()), ("one\ttwo\n", 0, ""));
}

/// An empty entry pool returns straight from `eval` (`column.c:125-127`):
/// all-blank input plus one bad file yields only the warn line, no output, and
/// exit status 1.
#[test]
fn an_empty_pool_keeps_only_the_exit_status() {
    let (out, status, err) = run(&["no-such-file"], b"\n  \n\t\n");
    assert_eq!((out.as_str(), status), ("", 1));
    assert_eq!(
        err,
        "column: Cannot open `no-such-file': No such file or directory\n"
    );

    let (out, status, err) = run(&[], b"\n  \n");
    assert_eq!((out.as_str(), status, err.as_str()), ("", 0, ""));
}

/// The layout follows the width: once the longest entry rounded up to 8
/// reaches the width, it falls back to one entry per line; otherwise it fills
/// vertically (`column.c:128-137`).
#[test]
fn the_layout_follows_the_width() {
    // Width 8 with entries of length 5 (rounded to 8): `maxlength >= termwidth`
    // gives one entry per line.
    let (out, status, err) = run(&["-c", "8"], FIVE);
    assert_eq!(status, 0);
    assert_eq!(err, "");
    assert_eq!(out, "alpha\nbeta\ngamma\ndelta\nepsilon\n");
    // `-x` switches to horizontal fill.
    let (out, _, _) = run(&["-x", "-c", "24"], FIVE);
    assert_eq!(out, "alpha\tbeta\tgamma\ndelta\tepsilon\n");
}

/// An entry is the whole line: leading blanks enter the pool and the layout
/// too. The reference build measures `  lead` + two padding tabs + the entry
/// that starts with a tab + one padding tab + `plain`, so one of the three
/// consecutive tabs belongs to the entry itself -- stripping leading blanks
/// would undercount the width by one cell.
#[test]
fn an_entry_keeps_its_leading_blanks() {
    let (out, status, err) = run(&[], b"  lead\n\ttablead\nplain\n");
    assert_eq!(
        (out.as_str(), status),
        ("  lead\t\t\ttablead\tplain\n", 0),
        "stderr 是 {err:?}"
    );
    assert_eq!(err, "");
}

fn scratch(tag: &str) -> std::path::PathBuf {
    static SEQ: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let n = SEQ.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    let dir = std::env::temp_dir().join(format!("column-face-{}-{tag}-{n}", std::process::id()));
    fs::create_dir_all(&dir).expect("scratch dir");
    dir
}

/// Long-tail-line warnings are counted **per source**: two sources each with a
/// long line => two sentences; the same source given twice => still two
/// sentences (one open, one count each); a good source plus a long-line source
/// => one sentence; an unopenable source like `-` prints one `Cannot open`
/// first and then adds one sentence for its long line. The sentence counts and
/// exit statuses of the four sections below are all measured on the reference
/// build.
#[test]
fn each_source_with_a_long_tail_line_warns_once() {
    let dir = std::env::temp_dir().join(format!("column-face-tail-{}", std::process::id()));
    fs::create_dir_all(&dir).expect("临时目录该能建");
    let long = "z".repeat(70);
    let one = dir.join("tail1.txt");
    let two = dir.join("tail2.txt");
    let good = dir.join("ok.txt");
    fs::write(&one, format!("a b c\n{long}")).expect("写第一档");
    fs::write(&two, format!("d e f\n{long}")).expect("写第二档");
    fs::write(&good, "a b\nc d\n").expect("写好档");
    let one = one.to_string_lossy().into_owned();
    let two = two.to_string_lossy().into_owned();
    let good = good.to_string_lossy().into_owned();

    // Two files with one long line each => two sentences, exit 1
    let (_, code, err) = run(&["-c", "20", one.as_str(), two.as_str()], b"x y\n");
    assert_eq!(code, 1);
    assert_eq!(
        err.matches("column: line too long\n").count(),
        2,
        "两个来源各该报一句，实际 stderr={err:?}"
    );
    // The same file given twice => two opens, still two sentences
    let (_, _, err) = run(&["-c", "20", one.as_str(), one.as_str()], b"x y\n");
    assert_eq!(err.matches("column: line too long\n").count(), 2);
    // Good file first, long-line file second => only one sentence
    let (_, code, err) = run(&["-c", "20", good.as_str(), one.as_str()], b"x y\n");
    assert_eq!(code, 1);
    assert_eq!(
        err.matches("column: line too long\n").count(),
        1,
        "实际 {err:?}"
    );
    // `-` is not an alias for stdin: C first reports that it cannot be opened,
    // then adds one sentence for its long line
    let (_, code, err) = run(&["-c", "20", "-", one.as_str()], b"x y\n");
    assert_eq!(code, 1);
    assert_eq!(
        err.lines().next().unwrap_or(""),
        "column: Cannot open `-': No such file or directory"
    );
    assert_eq!(
        err.matches("column: line too long\n").count(),
        1,
        "实际 {err:?}"
    );
    let _ = fs::remove_dir_all(&dir);
}

/// Both the entry pool and the layout are bytes: the fixture `raw.txt` =
/// `61 ff 62 0a 7a 0a`; the reference build (started through a same-named
/// symlink so the program name matches) measures that all three layouts keep
/// that one `ff`, while the old shell decoded UTF-8 on the shell side => the
/// whole file returned nothing. The expected values are taken byte for byte
/// from that measurement.
#[test]
fn an_entry_that_is_not_utf8_keeps_its_bytes_through_every_layout() {
    let dir = std::env::temp_dir().join(format!("column-435-nonutf8-{}", std::process::id()));
    fs::create_dir_all(&dir).expect("scratch dir");
    let raw = dir.join("raw.txt");
    let ok = dir.join("ok.txt");
    fs::write(&raw, b"a\xffb\nz\n").expect("write raw.txt");
    fs::write(&ok, b"x\ny\n").expect("write ok.txt");
    let raw = raw.to_str().expect("utf-8 temp path");
    let ok = ok.to_str().expect("utf-8 temp path");

    let (out, status, err) = run_bytes(&[raw], b"");
    assert_eq!((status, err), (0, Vec::new()));
    assert_eq!(out, b"a\xffb\tz\n", "缺省的纵向填满");

    let (out, _, _) = run_bytes(&["-x", raw], b"");
    assert_eq!(out, b"a\xffb\tz\n", "横向填满同形");

    let (out, _, _) = run_bytes(&[ok, raw], b"");
    assert_eq!(
        out, b"x\ty\ta\xffb\tz\n",
        "好档与坏档共用一个池，坏档不把自己的行抹掉"
    );

    // The `-t` shape needs two entry lines to make padding visible: `a\xffb`
    // is 3 bytes, 2 bytes longer than `z`
    let tb = dir.join("tb.txt");
    fs::write(&tb, b"a\xffb\tq\nz\tww\n").expect("write tb.txt");
    let (out, _, _) = run_bytes(&["-t", tb.to_str().expect("utf-8 temp path")], b"");
    assert_eq!(out, b"a\xffb  q\nz    ww\n", "表模式的补齐宽度按字节量");

    // Table mode's last column is emitted verbatim: a trailing carriage return
    // stays at the end of the line (the reference build reads the same)
    let cr = dir.join("cr.txt");
    fs::write(&cr, b"a\tb\r\nzz\tq\r\n").expect("write cr.txt");
    let (out, _, _) = run_bytes(&["-t", cr.to_str().expect("utf-8 temp path")], b"");
    assert_eq!(out, b"a   b\r\nzz  q\r\n");
}
