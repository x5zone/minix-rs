//! End-to-end face for the `cmp` binary, checked against
//! `minix3/usr.bin/cmp/cmp.c` (option loop `:74-86`, `-l`/`-s` mutual exclusion
//! `:87-90`, operand count `:92-93`, open branch `:95-119`, skip parsing `:121-134`,
//! `usage()` `:158-164`) and the libc `getopt` it drives
//! (`minix3/lib/libc/stdlib/getopt.c:74-92`, `:95-104`).
//!
//! The optstring at `cmp.c:74` is `"ls"`: neither letter takes a value, so libc's
//! `option requires an argument` branch is unreachable here (the reachable exits
//! are fixed by the optstring, not filled in from a family rule). The reference
//! binary `tmp/cmp-316/cmp_ref` (`cmp.c` + `misc.c` + `regular.c` + `special.c`
//! sources + Minix `getopt.c` + a program-name shim, linked natively on the host)
//! measured these shapes one by one:
//! (1) out-of-set letter = one libc line + one usage line, exit **2** (`-q`,
//!     clustered `-lq` reporting `q`, `--x` reporting `-`);
//! (2) `-l` and `-s` given together (including clustered `-ls`, `-sl`) =
//!     `cmp: only one of -l and -s may be specified`
//!     **on a single line, without usage**, exit 2 (the two `errx` shapes);
//! (3) fewer than two or more than four operands = **only the usage line**, exit 2;
//! (4) an unopenable file name = `warn("%s", name)` three-part form, exit 2, and
//!     **the whole sentence is suppressed under `-s`**
//!     (`if (!sflag)` at `cmp.c:101-104`, `:113-117`);
//! (5) the loop does not permute: `cmp file1 -l file2` has **three operands**, and
//!     that `-l` is the second file name (reference measurement
//!     `cmp: -l: No such file or directory`); the old implementation swallowed it
//!     as a flag and then compared two files;
//! (6) a bare `-` aliases standard input, and **both operands may name it, once
//!     each** — when both do:
//!     `errx(ERR_EXIT, "standard input may only be specified once")`;
//! (7) messages carry the **operand text** (`cmp.c:96` rewrites `file1` to
//!     `"stdin"` only for the later reports, not for this line), and the exit code
//!     has three levels: 0 same, 1 differs, 2 error (`ERR_EXIT`).
//!
//! The skip section (`strtoq(..., 0)`) was measured too: `5x` takes 5 (trailing
//! junk ignored), `010` is octal 8, `0x2` is 2, `-1` is legal (past the end it
//! falls into the `EOF on <file>` branch **without** the char/line suffix), while
//! `x`, the empty string, and overflow each land on usage.

mod face_support;

use face_support::write_stdin;
use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

const CMP: &str = env!("CARGO_BIN_EXE_cmp");
/// That line of `cmp.c:161-162`: the program name is hard-coded in the string.
const USAGE: &str = "usage: cmp [-l | -s] file1 file2 [skip1 [skip2]]\n";

/// The same file names and contents as the reference (in a scratch directory, with
/// the command run from that directory as its working directory, so the file names
/// in messages are short names that compare verbatim).
/// The file-name label in reports: the `-` side is written as `stdin` (`cmp.c:96`,
/// `:111` rewrite `file1`/`file2` to `"stdin"`), while the other side remains the
/// operand text.
/// One more thing is **order**: C opens both files first and parses skip after
/// (`cmp.c:95-119` precedes `:121-134`), so with "file cannot be opened + skip is a
/// bad value" the open sentence is printed and `usage:` never appears.
#[test]
fn the_stdin_label_wins_in_the_report_and_the_open_failure_wins_over_a_bad_skip() {
    let dir = workspace("label");
    fs::write(dir.join("b2.txt"), b"abd\n").expect("write b2");
    let (out, status, err) = run(&dir, &["-", "b2.txt"], b"abc\n");
    assert_eq!(
        (out.as_str(), status, err.as_str()),
        ("stdin b2.txt differ: char 3, line 1\n", 1, "")
    );

    let (out, status, err) = run(&dir, &["nope.txt", "same2.txt", "x"], b"");
    assert_eq!((out.as_str(), status), ("", 2));
    assert_eq!(err, "cmp: nope.txt: No such file or directory\n");
    assert!(
        !err.contains("usage:"),
        "开档失败该压过坏 skip：stderr 是 {err:?}"
    );
}

fn workspace(tag: &str) -> PathBuf {
    static SEQ: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let n = SEQ.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    let dir = std::env::temp_dir().join(format!("cmp-face-{}-{tag}-{n}", std::process::id()));
    fs::create_dir_all(&dir).expect("scratch dir");
    fs::write(dir.join("same1.txt"), b"abcde\n").expect("write same1");
    fs::write(dir.join("same2.txt"), b"abcde\n").expect("write same2");
    fs::write(dir.join("diff2.txt"), b"abXde\n").expect("write diff2");
    fs::write(dir.join("short3.txt"), b"abc\n").expect("write short3");
    fs::write(dir.join("empty.txt"), b"").expect("write empty");
    dir
}

fn run(dir: &Path, args: &[&str], input: &[u8]) -> (String, i32, String) {
    let mut child = Command::new(CMP)
        .args(args)
        .current_dir(dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the cmp binary should be runnable");
    write_stdin(&mut child, input);
    let output = child.wait_with_output().expect("the run should finish");
    (
        String::from_utf8_lossy(&output.stdout).into_owned(),
        output.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

/// Same command seam as `run`, but both output streams come back **byte for
/// byte**: `from_utf8_lossy` renders a bare `c3` as the three-byte U+FFFD, so the
/// text form cannot distinguish "one byte" from "two bytes" mistakes.
fn run_bytes(dir: &Path, args: &[&str], input: &[u8]) -> (Vec<u8>, i32, Vec<u8>) {
    let mut child = Command::new(CMP)
        .args(args)
        .current_dir(dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the cmp binary should be runnable");
    write_stdin(&mut child, input);
    let output = child.wait_with_output().expect("the run should finish");
    (
        output.stdout,
        output.status.code().unwrap_or(-1),
        output.stderr,
    )
}

/// Byte-shaped argv fixture: a bare `ff` cannot be written as `&str`, so arguments
/// are passed down as bytes (`OsStr::from_bytes`) and the comparison is against the
/// byte string the child process actually receives. Same working directory as `run`.
fn run_argv(dir: &Path, args: &[&[u8]], input: &[u8]) -> (Vec<u8>, i32, Vec<u8>) {
    use std::os::unix::ffi::OsStrExt as _;
    let mut child = Command::new(CMP)
        .args(args.iter().map(|a| OsStr::from_bytes(a)))
        .current_dir(dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the cmp binary should be runnable");
    write_stdin(&mut child, input);
    let output = child.wait_with_output().expect("the run should finish");
    (
        output.stdout,
        output.status.code().unwrap_or(-1),
        output.stderr,
    )
}

/// Drop a scratch file **whose name contains a bare `ff`** into `dir`, returning the
/// byte string argv must carry.
///
/// The on-disk name is the folded one: the open leg still requires `&str` (a known
/// libs gap), and writing under the raw byte name cannot be opened on the host at
/// all, so neither the message leg nor the echo leg would be reached. Writing the
/// file under the folded name separates the two legs — this gate pins "the open leg
/// folded once; the echo must not fold along".
/// Once that libs gap closes, this should switch to the raw byte name; the
/// assertions themselves stay unchanged.
fn write_raw_named(dir: &Path, name: &[u8], body: &[u8]) -> Vec<u8> {
    let folded = String::from_utf8_lossy(name).into_owned();
    fs::write(dir.join(folded), body).expect("折叠名的临时档该落得下去");
    name.to_vec()
}

/// C's `diffmsg` is `printf("%s %s differ: char %lld, line %lld\n", file1, file2,
/// ...)` (`minix3/usr.bin/cmp/misc.c:78-85`); both name slots are the byte strings
/// from argv.
#[test]
fn a_raw_byte_in_operand_names_is_echoed_as_those_bytes_by_cmp() {
    let dir = workspace("raw-differ");
    let first = write_raw_named(&dir, b"one\xff", b"abc\n");
    let second = write_raw_named(&dir, b"two\xff", b"abd\n");
    let (out, code, err) = run_argv(&dir, &[&first, &second], b"");
    assert_eq!(err, b"", "两份都开得动，报文腿不该出声，实读 {err:?}");
    assert_eq!(code, 1, "有差异退 DIFF_EXIT");
    let mut want = b"one".to_vec();
    want.extend_from_slice(b"\xff two");
    want.extend_from_slice(b"\xff differ: char 3, line 1\n");
    assert_eq!(out, want, "两处名字位都该是 argv 里那串原字节");
}

/// C's `eofmsg` is `warnx("EOF on %s: ...")` (`misc.c:59-73`), and the sentence
/// goes through the `warn(3)` prefix `cmp: `; without `-l` there is only the
/// `EOF on <name>` part, whose name slot is likewise the raw argv bytes.
#[test]
fn a_raw_byte_in_the_eof_report_name_is_echoed_as_that_byte_by_cmp() {
    let dir = workspace("raw-eof");
    // The first file is entirely a prefix of the second: the common part matches
    // throughout, and EOF is only reached at the end of the shorter one.
    let short = write_raw_named(&dir, b"short\xff", b"abc\n");
    let long = write_raw_named(&dir, b"longer\xff", b"abc\nd");
    let (out, code, err) = run_argv(&dir, &[&short, &long], b"");
    assert_eq!(out, b"", "共同前缀一样，就没有 differ 行");
    assert_eq!(code, 1, "EOF 也算差异");
    let mut want = b"cmp: EOF on short".to_vec();
    want.extend_from_slice(b"\xff\n");
    assert_eq!(err, want, "报文里的名字位该是 argv 里那一串原字节");
}

/// The letter slot of that out-of-set-letter line is **one bare byte**: reference
/// measurement `tmp/cmp-410/cmp_ref` shows `-é` and `-lé` both print
/// `cmp: unknown option -- ` + `c3` + newline followed by usage, exit **2**
/// (`ERR_EXIT`; `l` is a legal valueless letter in `"ls"`, so clustering continues
/// to the first byte of `é`; `-Zé` stops at `Z`). This shape must match **both
/// halves at once**: byte-wise parsing (library gate
/// `the_unknown_letter_is_the_first_byte_not_the_first_character`) and byte-wise
/// message writing are pinned separately, and changing only one half turns it red —
/// this branch has the same shape as `comm` (the "one branch, two halves" rhythm).
#[test]
fn the_libc_line_of_a_multibyte_flag_is_one_byte_for_cmp() {
    let dir = std::env::temp_dir();
    let want: Vec<u8> = [
        b"cmp: unknown option -- ".as_slice(),
        b"\xc3\n",
        USAGE.as_bytes(),
    ]
    .concat();
    for form in ["-\u{e9}", "-l\u{e9}"] {
        let (out, status, err) = run_bytes(&dir, &[form, "a.txt", "b.txt"], b"");
        assert_eq!(out, Vec::new(), "{form} 的报文只走标准错误");
        assert_eq!(status, 2, "{form} 的 stderr 是 {err:?}");
        assert_eq!(err, want, "{form} 的字母该是一个裸字节 c3");
    }

    let (_out, status, err) = run_bytes(&dir, &["-Z\u{e9}", "a.txt", "b.txt"], b"");
    assert_eq!(status, 2);
    assert_eq!(
        err,
        [b"cmp: unknown option -- Z\n".as_slice(), USAGE.as_bytes(),].concat(),
        "环停在第一个坏字母上，`é` 根本不该被读到"
    );
}

/// Out-of-set letter: the libc line first, usage after, exit 2 (the old
/// implementation omitted the libc line).
#[test]
fn an_unknown_letter_prints_the_libc_line_then_the_usage() {
    let dir = workspace("unknown");
    let (out, status, err) = run(&dir, &["-q", "same1.txt", "same2.txt"], b"");
    assert_eq!((out.as_str(), status), ("", 2), "stderr 是 {err:?}");
    assert_eq!(err, format!("cmp: unknown option -- q\n{USAGE}"));

    // Clustering reports only the first out-of-set letter (earlier legal letters do
    // not change the exit).
    let (_, status, err) = run(&dir, &["-lq", "same1.txt", "same2.txt"], b"");
    assert_eq!(status, 2);
    assert_eq!(err, format!("cmp: unknown option -- q\n{USAGE}"));

    // `--x` reports the second hyphen as the letter: the optstring has no `-`.
    let (_, status, err) = run(&dir, &["--x", "same1.txt", "same2.txt"], b"");
    assert_eq!(status, 2);
    assert_eq!(err, format!("cmp: unknown option -- -\n{USAGE}"));
}

/// Giving `-l` and `-s` together is one `errx`: program name + sentence, with
/// **no** period at the end, and **no usage** appended (the old implementation
/// judged the clustered form as an out-of-set letter and printed the two-line
/// usage).
#[test]
fn the_two_flags_conflict_in_one_line_without_the_usage() {
    let dir = workspace("conflict");
    for word in ["-ls", "-sl"] {
        let (out, status, err) = run(&dir, &[word, "same1.txt", "same2.txt"], b"");
        assert_eq!(
            (out.as_str(), status),
            ("", 2),
            "`{word}` 的 stderr 是 {err:?}"
        );
        assert_eq!(
            err, "cmp: only one of -l and -s may be specified\n",
            "`{word}` 在 C 是逐字母聚簇，两面旗标都立起来才落这一支"
        );
    }
    // Writing the two flags separately conflicts the same way (C merely assigns 1
    // again, so `-l -l` is actually legal — see the next gate).
    let (_, status, err) = run(&dir, &["-l", "-s", "same1.txt", "same2.txt"], b"");
    assert_eq!(status, 2);
    assert_eq!(err, "cmp: only one of -l and -s may be specified\n");
}

/// Fewer than two or more than four operands: only the usage line, exit 2 (the
/// reference measured the same for all three shapes).
#[test]
fn an_operand_count_out_of_range_is_the_usage_line_only() {
    let dir = workspace("count");
    for args in [
        vec![],
        vec!["same1.txt"],
        vec!["same1.txt", "same2.txt", "0", "0", "0"],
    ] {
        let (out, status, err) = run(&dir, &args, b"");
        assert_eq!((out.as_str(), status), ("", 2), "args 是 {args:?}");
        assert_eq!(err, USAGE);
    }
}

/// Repeating the same flag is legal (C just assigns 1 again); `-l -l` reads the
/// same as `-l`.
#[test]
fn a_repeated_flag_stays_legal_and_idempotent() {
    let dir = workspace("repeat");
    let (out, status, err) = run(&dir, &["-l", "-l", "same1.txt", "diff2.txt"], b"");
    assert_eq!((out.as_str(), status), ("     3 143 130\n", 1));
    assert_eq!(err, "");
}

/// The loop does not permute: once `same1.txt` appears the loop ends, and that
/// `-l` is the **second file name** (reference measurement
/// `cmp: -l: No such file or directory`, exit 2). The old implementation scanned
/// to the end, swallowed `-l` as a flag, then compared two files — different exit
/// code and sentence.
#[test]
fn the_option_ring_stops_at_the_first_operand() {
    let dir = workspace("reorder");
    let (out, status, err) = run(&dir, &["same1.txt", "-l", "same2.txt"], b"");
    assert_eq!((out.as_str(), status), ("", 2));
    assert_eq!(err, "cmp: -l: No such file or directory\n");

    // `--` ends the loop and is not itself an operand: `-- same1 same2` reads the
    // same as without `--`.
    let (out, status, err) = run(&dir, &["--", "same1.txt", "same2.txt"], b"");
    assert_eq!((out.as_str(), status), ("", 0));
    assert_eq!(err, "");
}

/// An unopenable file name produces the three-part `warn(3)` form, and under `-s`
/// **the whole sentence is suppressed** (`if (!sflag)` at `cmp.c:101-104`) — the
/// exit code is still 2.
#[test]
fn an_unopenable_operand_warns_in_three_parts_and_silence_suppresses_it() {
    let dir = workspace("open");
    let (out, status, err) = run(&dir, &["nope.txt", "same2.txt"], b"");
    assert_eq!((out.as_str(), status), ("", 2));
    assert_eq!(err, "cmp: nope.txt: No such file or directory\n");

    let (out, status, err) = run(&dir, &["-s", "nope.txt", "same2.txt"], b"");
    assert_eq!(
        (out.as_str(), status, err.as_str()),
        ("", 2, ""),
        "`-s` 把开档失败的那一句整个压掉，只留退出码"
    );

    // The second file name failing to open behaves the same, and the message names
    // that file itself.
    let (out, status, err) = run(&dir, &["same1.txt", "nope2.txt"], b"");
    assert_eq!((out.as_str(), status), ("", 2));
    assert_eq!(err, "cmp: nope2.txt: No such file or directory\n");
}

/// A bare `-` aliases standard input; writing `-` on both sides is one `errx`
/// (reference measurement: exit 2).
#[test]
fn a_dash_operand_reads_stdin_and_only_one_operand_may() {
    let dir = workspace("stdin");
    let (out, status, err) = run(&dir, &["-", "same1.txt"], b"abcde\n");
    assert_eq!((out.as_str(), status, err.as_str()), ("", 0, ""));
    let (out, status, err) = run(&dir, &["same1.txt", "-"], b"abcde\n");
    assert_eq!((out.as_str(), status, err.as_str()), ("", 0, ""));
    let (out, status, err) = run(&dir, &["-", "-"], b"abcde\n");
    assert_eq!((out.as_str(), status), ("", 2));
    assert_eq!(
        err, "cmp: standard input may only be specified once\n",
        "`cmp.c:106-110` 的第二支 `errx`，不接 usage"
    );
}

/// The three exit codes and both reports: same -> 0 and nothing printed; differs ->
/// 1, with the report on **stdout**, in the form `%s %s differ: char %lld, line
/// %lld`; `-l` switches to a six-wide byte table.
#[test]
fn the_three_exit_codes_and_both_reports_follow_the_c() {
    let dir = workspace("report");
    let (out, status, err) = run(&dir, &["same1.txt", "same2.txt"], b"");
    assert_eq!((out.as_str(), status, err.as_str()), ("", 0, ""));
    let (out, status, err) = run(&dir, &["same1.txt", "diff2.txt"], b"");
    assert_eq!(
        (out.as_str(), status, err.as_str()),
        ("same1.txt diff2.txt differ: char 3, line 1\n", 1, "")
    );
    let (out, status, _) = run(&dir, &["-l", "same1.txt", "diff2.txt"], b"");
    assert_eq!((out.as_str(), status), ("     3 143 130\n", 1));
    let (out, status, err) = run(&dir, &["-s", "same1.txt", "diff2.txt"], b"");
    assert_eq!(
        (out.as_str(), status, err.as_str()),
        ("", 1, ""),
        "`-s` 什么都不打"
    );
    // Two empty files also exit 0 (`empty.txt` against `empty.txt`).
    let (out, status, _) = run(&dir, &["empty.txt", "empty.txt"], b"");
    assert_eq!((out.as_str(), status), ("", 0));
}

/// One side hits the end first: `regular.c:111` names the **shorter side**. Without
/// `-l` the difference is hit first (after `c`, one side has `d` and the other a
/// newline), so the `differ` branch applies; only with `-l` does it reach `eofmsg`,
/// in the form `cmp: EOF on <short file>: char N, line M`.
#[test]
fn an_early_eof_names_the_shorter_file_with_its_own_shape() {
    let dir = workspace("eof");
    let (out, status, err) = run(&dir, &["same1.txt", "short3.txt"], b"");
    assert_eq!(
        (out.as_str(), status, err.as_str()),
        ("same1.txt short3.txt differ: char 4, line 1\n", 1, "")
    );
    let (out, status, err) = run(&dir, &["-l", "same1.txt", "short3.txt"], b"");
    assert_eq!((out.as_str(), status), ("     4 144  12\n", 1));
    assert_eq!(err, "cmp: EOF on short3.txt: char 5, line 1\n");
}

/// The skip section is the reading of `strtoq(..., 0)`: `5x` takes 5 and ignores the
/// tail, `0x2` is 2, `010` is octal 8 (past the end of the file ->
/// `cmp: EOF on same1.txt`, **without** the char/line suffix), `-1` is legal and
/// lands in the same branch; inputs that convert no digit at all (`x`, the empty
/// string) and overflow each exit 2 through usage.
#[test]
fn the_skip_operands_are_read_the_way_strtoq_reads_them() {
    let dir = workspace("skip");
    let cases = [
        ("2", "same1.txt same2.txt differ: char 1, line 1\n"),
        ("0x2", "same1.txt same2.txt differ: char 1, line 1\n"),
        ("5x", "same1.txt same2.txt differ: char 1, line 1\n"),
    ];
    for (word, expected) in cases {
        let (out, status, err) = run(&dir, &["same1.txt", "same2.txt", word], b"");
        assert_eq!(
            (out.as_str(), status, err.as_str()),
            (expected, 1, ""),
            "`{word}` 的读数不对"
        );
    }
    for word in ["010", "-1"] {
        let (out, status, err) = run(&dir, &["same1.txt", "same2.txt", word], b"");
        assert_eq!(
            (out.as_str(), status, err.as_str()),
            ("", 1, "cmp: EOF on same1.txt\n"),
            "`{word}` 越过档尾该报不带 char/line 的那一支"
        );
    }
    for word in ["x", "", "99999999999999999999999"] {
        let (out, status, err) = run(&dir, &["same1.txt", "same2.txt", word], b"");
        assert_eq!(
            (out.as_str(), status, err.as_str()),
            ("", 2, USAGE),
            "`{word}` 该落 usage 一支"
        );
    }
    // Both skips given: after `2 2` the difference remains.
    let (out, status, _) = run(&dir, &["same1.txt", "diff2.txt", "2", "2"], b"");
    assert_eq!(
        (out.as_str(), status),
        ("same1.txt diff2.txt differ: char 1, line 1\n", 1)
    );
}
