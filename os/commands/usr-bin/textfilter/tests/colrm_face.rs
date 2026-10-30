//! End-to-end face for the `colrm` binary, checked against
//! `minix3/usr.bin/colrm/colrm.c` (option loop `:67-74`, operand section `:76-95`,
//! column walk and read/write `:97-121`, `check()` `:123-130`, `usage()` `:132-137`)
//! and the libc `getopt` it drives (`minix3/lib/libc/stdlib/getopt.c:74-78`,
//! `:81-92`, `:95-104`).
//!
//! `colrm.c:67`'s optstring is the **empty string**, structurally identical to
//! `rev`, so the three shapes of that loop carry over; but `colrm`'s operands
//! are **column numbers**, not file names, so the same bare `-` lands on a
//! completely different sentence in the two commands. The shapes measured one by
//! one with the reference binary `tmp/colrm-310/colrm_ref` (`colrm.c` ground truth
//! + Minix's `getopt.c` + a program-name shim, linked on the host) are:
//! (1) any token starting with `-` (except `--` and a bare `-`) = one libc line +
//!     one usage line, exit 1;
//! (2) `--` ends the option loop and is not an operand itself, while a bare `-`
//!     is an operand and is rejected as a column;
//! (3) no reordering: a `-Z` after an operand is also a column
//!     (`colrm: illegal column -- -Z`);
//! (4) when both column operands are bad, the **later-written** one is reported
//!     (C computes `argv[1]` first and then FALLTHROUGHs);
//! (5) columns go through `strtol`: leading blanks, a `+` sign and overflow
//!     saturation are accepted, trailing characters and 0 rejected, and a negative
//!     wraps through `u_long` into a huge value and is accepted (`colrm -- -5`
//!     exits 0 and passes every line through);
//! (6) the `err(1, ...)` line is the three-part form suffixed with `strerror`,
//!     while the `errx` line is the two-part form without it.
//!
//! This face did not exist before: `colrm`'s read seam used a bare
//! `minix_sys::read`, so the host could not read any input and its output could
//! only be covered by library tests (as with `nl`, `fold` and `rev`).

mod face_support;

use face_support::write_stdin;
use std::process::{Command, Stdio};
use std::os::unix::ffi::OsStringExt;

const COLRM: &str = env!("CARGO_BIN_EXE_colrm");
const USAGE: &str = "usage: colrm [start [stop]]\n";
/// Input used by the reference binary (`tmp/colrm-310/in.txt`); the goldens were
/// taken shape by shape from its readings.
const INPUT: &[u8] = b"abcdefg\nhi\n";

fn run(args: &[&str], input: &[u8]) -> (String, i32, String) {
    let mut child = Command::new(COLRM)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the colrm binary should be runnable");
    write_stdin(&mut child, input);
    let output = child.wait_with_output().expect("the run should finish");
    (
        String::from_utf8_lossy(&output.stdout).into_owned(),
        output.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

/// Same as above but returns **raw bytes**: the letter slot must pin "one byte",
/// and `String::from_utf8_lossy` renders both `c3` and `c3 83` as the replacement
/// character, so a text fixture cannot tell those two errors apart.
fn run_bytes(args: &[&str], input: &[u8]) -> (Vec<u8>, i32, Vec<u8>) {
    let mut child = Command::new(COLRM)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the colrm binary should be runnable");
    write_stdin(&mut child, input);
    let output = child.wait_with_output().expect("the run should finish");
    (
        output.stdout,
        output.status.code().unwrap_or(-1),
        output.stderr,
    )
}

/// Off-string letter: the libc line first, `usage()` after, exit 1
/// (`colrm.c:67-72`'s `case '?': default`).
/// The reference reading is `colrm: unknown option -- Z` + usage. The old form
/// was missing the libc line.
#[test]
fn an_unknown_letter_prints_the_libc_line_then_the_usage() {
    let (out, status, err) = run(&["-Z"], INPUT);
    assert_eq!(out, "", "报文只走 stderr");
    assert_eq!(status, 1, "stderr 是 {err:?}");
    assert_eq!(err, format!("colrm: unknown option -- Z\n{USAGE}"));
}

/// A non-ASCII off-string letter prints **one raw byte**: the reference binary
/// `tmp/colrm-310/colrm` measured `colrm -é` → `colrm: unknown option -- ` +
/// `c3` + newline + usage, exit 1.
/// Missing either half — taking the byte right after the dash (`getopt.c:79`) or
/// rendering it as a byte rather than two UTF-8 bytes — still diverges: fixing
/// only the take prints `c3 83`, fixing only the rendering prints `c3 a9`.
#[test]
fn the_option_line_of_a_nonascii_letter_carries_one_raw_byte() {
    let want: Vec<u8> = [
        b"colrm: unknown option -- ".as_slice(),
        b"\xc3\n",
        USAGE.as_bytes(),
    ]
    .concat();
    let (out, status, err) = run_bytes(&["-\u{e9}"], INPUT);
    assert_eq!(out, Vec::new(), "报文只走 stderr");
    assert_eq!(status, 1, "stderr 是 {err:?}");
    assert_eq!(err, want, "字母该是一个裸字节 c3");

    // A non-ASCII letter in second position of a cluster: the error still names
    // the first byte, and there is still only one libc report (`colrm.c:68-72`'s
    // `default: usage()` exits on '?' without returning to getopt).
    let (out, status, err) = run_bytes(&["-x\u{e9}"], INPUT);
    assert_eq!(out, Vec::new());
    assert_eq!(status, 1);
    assert_eq!(
        err,
        b"colrm: unknown option -- x\nusage: colrm [start [stop]]\n".to_vec()
    );

    // For the ASCII shape the two entry points are byte-identical.
    let (_out, status, err) = run_bytes(&["-Z"], INPUT);
    assert_eq!(status, 1);
    assert_eq!(
        err,
        b"colrm: unknown option -- Z\nusage: colrm [start [stop]]\n".to_vec()
    );
}

/// A cluster reports only its **first** character: the caller gets `'?'`, runs
/// `usage()` and exits without returning to getopt for the second one (the
/// reference: `-bZ` → `unknown option -- b`).
#[test]
fn a_cluster_reports_only_its_first_letter() {
    let (out, status, err) = run(&["-bZ"], INPUT);
    assert_eq!((out.as_str(), status), ("", 1));
    assert_eq!(err, format!("colrm: unknown option -- b\n{USAGE}"));
}

/// The letter reported for `--x` is the second hyphen: optstring has no `-`, so it
/// is an off-string character too (the reference: `unknown option -- -`). A pure
/// `--` takes the other branch.
#[test]
fn a_dash_followed_by_a_word_is_an_unknown_option_named_by_the_dash() {
    let (out, status, err) = run(&["--x"], INPUT);
    assert_eq!((out.as_str(), status), ("", 1));
    assert_eq!(err, format!("colrm: unknown option -- -\n{USAGE}"));
}

/// `--` itself is consumed and everything after it counts as operands
/// (`getopt.c:81-85` does `++optind` first): `--` alone still passes every line
/// through, while `-- 2 4` actually deletes columns 2..4.
#[test]
fn the_end_of_options_marker_is_not_an_operand() {
    let (out, status, err) = run(&["--"], INPUT);
    assert_eq!(
        (out.as_str(), err.as_str(), status),
        ("abcdefg\nhi\n", "", 0)
    );
    let (out, status, err) = run(&["--", "2", "4"], INPUT);
    assert_eq!(
        (out.as_str(), err.as_str(), status),
        ("aefg\nh\n", "", 0),
        "`--` 后面的列号该生效"
    );
}

/// A bare `-` is an operand, not an option: optstring has no `-`, and getopt ends
/// the option loop **without advancing** `optind` (`getopt.c:87-92`), so it lands
/// in the `argv += optind` section and is rejected as a column (the reference:
/// `colrm: illegal column -- -`, one line, no usage, exit 1).
/// The same token in `rev` takes the `fopen` failure branch, a completely
/// different sentence.
#[test]
fn a_solitary_dash_is_an_operand_rejected_as_a_column() {
    let (out, status, err) = run(&["-"], INPUT);
    assert_eq!(out, "", "`-` 不是「读标准输入」的别名");
    assert_eq!(status, 1, "stderr 是 {err:?}");
    assert_eq!(err, "colrm: illegal column -- -\n");
    assert!(!err.contains("usage:"), "列号的 errx 不接 usage：{err:?}");
}

/// No reordering: the option loop ends as soon as a non-option token arrives
/// (`getopt.c:74-78`), so the `-Z` in `colrm 2 -Z` is a **column** (the
/// reference: `illegal column -- -Z`, not `unknown option`).
#[test]
fn an_option_looking_token_after_an_operand_is_a_column() {
    let (out, status, err) = run(&["2", "-Z"], INPUT);
    assert_eq!(out, "");
    assert_eq!(status, 1, "stderr 是 {err:?}");
    assert_eq!(err, "colrm: illegal column -- -Z\n");
}

/// When both are bad, the **later-written** one is reported: C's `case 2:` runs
/// `strtol` on `argv[1]` and checks it first, and only then does
/// `/* FALLTHROUGH */` reach `argv[0]` (`colrm.c:78-87`). The reference:
/// `abc def` → `def`.
#[test]
fn the_stop_column_is_checked_before_the_start_column() {
    let (out, status, err) = run(&["abc", "def"], INPUT);
    assert_eq!((out.as_str(), status), ("", 1));
    assert_eq!(err, "colrm: illegal column -- def\n");
    // When only start is bad, start is what gets reported.
    let (_, status, err) = run(&["abc", "2"], INPUT);
    assert_eq!(
        (err.as_str(), status),
        ("colrm: illegal column -- abc\n", 1)
    );
}

/// The accepted surface for a column is `strtol`, not "a run of digits": leading
/// blanks, an explicit `+` and leading zeros are accepted; zero and trailing
/// characters are rejected (C tests `*p`). The reference measured every shape.
#[test]
fn column_numbers_take_the_strtol_forms() {
    for accepted in ["3", " 3", "+3", "03"] {
        let (out, status, err) = run(&[accepted], INPUT);
        assert_eq!(
            (out.as_str(), err.as_str(), status),
            ("ab\nhi\n", "", 0),
            "`{accepted}` 该被当作列号 3"
        );
    }
    for rejected in ["0", "+0", "3x", "0x10", "1e2", "3 "] {
        let (out, status, err) = run(&[rejected], INPUT);
        assert_eq!(out, "", "报文只走 stderr");
        assert_eq!(status, 1, "`{rejected}` 该被拒收：stderr 是 {err:?}");
        assert_eq!(
            err,
            format!("colrm: illegal column -- {rejected}\n"),
            "`{rejected}` 的句子该带操作数原文"
        );
    }
}

/// Neither overflow nor negatives are **rejected**: `strtol` saturates to a `long`
/// endpoint and `colrm.c:63` stores it in a `u_long`, so C's `<= 0` only blocks 0;
/// a negative wraps into a huge value. Both are larger than any real column, so
/// the whole line passes through and the exit is 0 (the reference:
/// `colrm 99999999999999999999999` and `colrm -- -5` both pass everything).
#[test]
fn an_overflowing_or_negative_column_passes_everything_through() {
    for huge in ["99999999999999999999999", "100"] {
        let (out, status, err) = run(&[huge], INPUT);
        assert_eq!(
            (out.as_str(), err.as_str(), status),
            ("abcdefg\nhi\n", "", 0),
            "列号 `{huge}` 该被接受并透传"
        );
    }
    let (out, status, err) = run(&["--", "-5"], INPUT);
    assert_eq!(
        (out.as_str(), err.as_str(), status),
        ("abcdefg\nhi\n", "", 0),
        "负列号经 `u_long` 回绕成极大值，C 不拒它"
    );
}

/// More than two operands prints only usage (`colrm.c:90-91`'s
/// `default: usage()`), without the libc line and without a column sentence.
#[test]
fn too_many_operands_print_only_the_usage() {
    let (out, status, err) = run(&["2", "4", "6"], INPUT);
    assert_eq!(out, "");
    assert_eq!(status, 1, "stderr 是 {err:?}");
    assert_eq!(err, USAGE);
}

/// The start-after-stop branch is `err(1, ...)`, not `errx`: the three-part form
/// suffixes `strerror(errno)`, and `errno` is 0 here, so the sentence body
/// depends on each platform's table (the reference prints `Success` on this host).
/// This shell's gate therefore pins only the shape: program name + sentence + a
/// non-empty suffix, one line, no usage, exit 1.
#[test]
fn start_after_stop_prints_the_err_line_without_the_usage() {
    let (out, status, err) = run(&["4", "2"], INPUT);
    assert_eq!(out, "");
    assert_eq!(status, 1, "stderr 是 {err:?}");
    let head = "colrm: illegal start and stop columns: ";
    assert!(err.starts_with(head), "该以 `{head}` 开头，实际 {err:?}");
    let tail = err[head.len()..].trim_end();
    assert!(
        !tail.is_empty() && !tail.contains('\n'),
        "缀的应是一句 strerror 文本：{err:?}"
    );
    assert!(!err.contains("usage:"), "这一支不接 usage：{err:?}");
}

/// The output leg (the other half of why this gate exists): `colrm` used to read
/// through a bare `minix_sys::read`, so the host **could not read input** and the
/// column walk had never been tested at process level; the old form also ran the
/// input through a UTF-8 check and dropped non-text bytes wholesale. The goldens
/// were taken shape by shape from the reference readings
/// (`tmp/colrm-310/golden.log`).
#[test]
fn the_column_walk_reaches_the_output_end_to_end() {
    // Start column only: delete from that column to end of line (stop stays 0).
    assert_eq!(run(&["3"], INPUT).0, "ab\nhi\n");
    // Range: columns 2..4 are deleted.
    assert_eq!(run(&["2", "4"], INPUT).0, "aefg\nh\n");
    assert_eq!(run(&["1", "1"], INPUT).0, "bcdefg\ni\n");
    // A tab advances the column to the next multiple of 8: deleting 2..8 removes
    // the first tab's landing column 8, while the second lands at 16, outside the
    // range, and stays.
    let (out, status, _) = run(&["2", "8"], b"a\tb\tc\n");
    assert_eq!((out.as_str(), status), ("ab\tc\n", 0));
    let (out, status, _) = run(&["2", "9"], b"a\tb\tc\n");
    assert_eq!((out.as_str(), status), ("a\tc\n", 0));
    // A backspace pulls the column back; bytes whose landing column is then
    // outside the range stay.
    let (out, status, _) = run(&["2"], b"ab\x08\x08X\n");
    assert_eq!((out.as_str(), status), ("a\x08\x08X\n", 0));
    // The last line is deleted even without a trailing newline, and no newline is
    // added (C only ever calls putchar and never adds one).
    let (out, status, _) = run(&["2", "3"], b"abcdef");
    assert_eq!((out.as_str(), status), ("adef", 0));
    // Empty input yields empty output.
    let (out, status, err) = run(&["2"], b"");
    assert_eq!((out.as_str(), err.as_str(), status), ("", "", 0));
    // Non-UTF-8 bytes are not dropped wholesale: C walks columns byte by byte and
    // this shell must too (the old form validated UTF-8 first and treated the whole
    // input as empty when that failed). Here `2 3` deletes exactly those two
    // unprintable bytes.
    let (out, status, _) = run(&["2", "3"], b"a\xff\xbccdef\n");
    assert_eq!((out.as_str(), status), ("acdef\n", 0));
}

/// The carrier for both the read and write legs: standard output wired to
/// `/dev/full` (a character device whose writes always return `ENOSPC`).
/// On this shape the reference binary `tmp/colrm-310/colrm_ref` reports only after
/// crossing the stdio buffer — the measured threshold lies
/// between 4096 and 4505 bytes: 4 KiB of output exits 0 without a word, and from
/// 4505 bytes it reports `colrm_ref: stdout: No space left on device` and exits 1.
/// This shell has no stdio buffer, so it reports immediately at any size (the two
/// gates below pin one side each).
fn run_into_sink(args: &[&str], input: &[u8]) -> (i32, String) {
    let sink = std::fs::OpenOptions::new()
        .write(true)
        .open("/dev/full")
        .expect("/dev/full carries a failing write");
    let mut child = Command::new(COLRM)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::from(sink))
        .stderr(Stdio::piped())
        .spawn()
        .expect("the colrm binary should be runnable");
    write_stdin(&mut child, input);
    let output = child.wait_with_output().expect("the run should finish");
    (
        output.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

/// The read leg: hand standard input the fd of a **directory**, which opens fine
/// but returns `EISDIR` on read.
/// C's `check(stdin)` (`colrm.c:123-130`) checks `feof` and then `ferror`, so it
/// takes `err(1, "stdin")` — the reference prints
/// `colrm_ref: stdin: Is a directory`, exits 1 and produces no output.
/// Before the change this shell treated a read failure as end of file, said nothing
/// and exited 0. The sentence comes from Minix's errno table.
#[test]
fn a_stdin_that_cannot_be_read_names_the_stream_and_exits_one() {
    let dir = std::env::temp_dir().join(format!("colrm-stdin-dir-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("scratch dir");
    let handle = std::fs::File::open(&dir).expect("a directory opens read-only here");
    let output = Command::new(COLRM)
        .arg("2")
        .stdin(Stdio::from(handle))
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the colrm binary should be runnable")
        .wait_with_output()
        .expect("the run should finish");
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "",
        "读不到字节就不该有任何输出"
    );
    assert_eq!(
        String::from_utf8_lossy(&output.stderr),
        "colrm: stdin: Is a directory\n",
        "报文该是程序名、流名 stdin、句子三段的 err 形"
    );
    assert_eq!(output.status.code(), Some(1), "读错要把退码置起来");
}

/// The write leg (the shape that crosses the reference threshold, the same on both
/// sides): 8 KiB passed through and written to `/dev/full`.
#[test]
fn a_stdout_that_cannot_be_written_names_the_stream_and_exits_one() {
    let payload = vec![b'y'; 8192];
    let (status, err) = run_into_sink(&["--", "-5"], &payload);
    assert_eq!(
        (status, err.as_str()),
        (1, "colrm: stdout: No space left on device\n"),
        "写错报的是流名 stdout，与读错分得开"
    );
}

/// A small output on the same shape: the reference swallows it in glibc's buffer
/// (exits 0, no report), while this shell has no buffer and therefore **reports**.
/// This gate pins a deliberate divergence, not a regression — copying the buffer
/// size into the command layer would write a libc mechanism into OS semantics, and
/// on real hardware Minix stdio's buffer is yet another number, so copying it would
/// still be wrong.
#[test]
fn a_small_write_failure_is_reported_instead_of_swallowed_by_a_buffer() {
    let (status, err) = run_into_sink(&["--", "-5"], b"abcdefg\nhi\n");
    assert_eq!(
        (status, err.as_str()),
        (1, "colrm: stdout: No space left on device\n"),
        "本壳写不进去就报，不论尺寸"
    );
}

/// Both legs broken at once: the reference measures **stdin**, not stdout — C never
/// reads a single byte, so the loop never calls `putchar` and the write error has no
/// chance to be observed at all. This shell likewise gets zero bytes of input and
/// writes zero bytes (an empty write does not fail), so it reports the read leg.
/// This gate pins the **observation** of "which stream is reported when both legs
/// break".
///
/// This comment used to say "the ordering rule itself cannot be proven on this
/// shape ... the host cannot build a carrier where bytes are in flight and the read
/// also fails at the same time"; that half has since been **disproved**: after the
/// slave closes, a pty master first hands over buffered bytes and only returns `EIO`
/// on the next read, which is exactly a real read source the host can build. The
/// four-shape matrix readings and the predicate argument are in
/// `tools/atf-c-compat/probes/colrm_leg_order.py`. The ordering rule itself is still
/// not pinned, but the reason changed: it is not that no carrier can be built, but
/// that pinning it would require replicating libc's buffer threshold in the command
/// layer (Minix's `BUFSIZ` = 1024, see `minix3/include/stdio.h:182` and
/// `minix3/lib/libc/stdio/makebuf.c:170`), and whether to replicate it is a
/// conflict of principles that needs a human decision — until that decision lands,
/// this gate stays as it is.
#[test]
fn both_streams_broken_reports_the_stream_c_reaches_first() {
    let dir = std::env::temp_dir().join(format!("colrm-both-broken-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("scratch dir");
    let handle = std::fs::File::open(&dir).expect("a directory opens read-only here");
    let sink = std::fs::OpenOptions::new()
        .write(true)
        .open("/dev/full")
        .expect("/dev/full carries a failing write");
    let output = Command::new(COLRM)
        .arg("2")
        .stdin(Stdio::from(handle))
        .stdout(Stdio::from(sink))
        .stderr(Stdio::piped())
        .spawn()
        .expect("the colrm binary should be runnable")
        .wait_with_output()
        .expect("the run should finish");
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(
        String::from_utf8_lossy(&output.stderr),
        "colrm: stdin: Is a directory\n",
        "两腿同坏时报文该跟着 C 先观察到的那条流"
    );
    assert_eq!(output.status.code(), Some(1));
}

/// The raw form of argument bytes: `OsString::from_vec` can yield a bare `ff`, the
/// `&str` leg cannot.
fn run_raw(args: &[Vec<u8>], input: &[u8]) -> (Vec<u8>, i32, Vec<u8>) {
    let mut child = Command::new(COLRM)
        .args(
            args.iter()
                .map(|word| std::ffi::OsString::from_vec(word.clone())),
        )
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the colrm binary should be runnable");
    write_stdin(&mut child, input);
    let output = child.wait_with_output().expect("the run should finish");
    (
        output.stdout,
        output.status.code().unwrap_or(-1),
        output.stderr,
    )
}

/// Operand echo uses the original bytes: the reference binary
/// `tmp/colrm-310/colrm_ref` measured `colrm 1<ff>3` printing
/// `colrm: illegal column -- 1<ff>3`, where `<ff>` is one byte, not a three-byte
/// replacement character.
#[test]
fn a_raw_nonutf8_operand_is_echoed_as_its_own_bytes() {
    let (out, status, err) = run_raw(&[b"1\xff3".to_vec()], INPUT);
    assert_eq!(out, b"", "报文只走 stderr");
    assert_eq!(status, 1, "stderr 是 {err:?}");
    assert_eq!(
        err,
        [
            b"colrm: illegal column -- 1".as_slice(),
            &[0xff],
            b"3\n".as_slice()
        ]
        .concat()
    );
}

/// The letter slot of the off-string-option line is a bare `ff`: once the argument
/// seam has folded `ff` into `ef bf bd`, "taking the first byte" gets `ef`, so this
/// gate pins both the take and the seam.
#[test]
fn a_raw_nonutf8_option_letter_is_named_as_one_byte() {
    let (out, status, err) = run_raw(&[b"-\xff".to_vec(), b"3".to_vec()], INPUT);
    assert_eq!(out, b"", "报文只走 stderr");
    assert_eq!(status, 1, "stderr 是 {err:?}");
    assert_eq!(
        err,
        [
            b"colrm: unknown option -- ".as_slice(),
            &[0xff],
            b"\n".as_slice(),
            USAGE.as_bytes()
        ]
        .concat()
    );
}
