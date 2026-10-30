//! End-to-end face for the `comm` binary, checked against
//! `minix3/usr.bin/comm/comm.c` (option ring `:80-95`, operand check `:97-102`,
//! `file()`'s open branch `:177-187`, `usage()` `:195-199`, `getnextln()`
//! `:201-215`) and the libc `getopt` it drives
//! (`minix3/lib/libc/stdlib/getopt.c:74-78`, `:81-92`, `:95-104`).
//!
//! `comm.c:80`'s optstring is `"123f"`: `1`/`2`/`3`/`f` are four **valueless**
//! letters. That one fact fixes the reachable exits of this face to two families — the
//! reference build (`comm.c` as source of truth + Minix's `getopt.c` + a program-name
//! shim, linked on the host) measured these shapes:
//! (1) out-of-set letter = one libc line + one usage line, exit 1 (`-Z`, cluster
//!     `-1Z` reports `Z`, `--x` reports `-`);
//! (2) an operand count other than two = **only the usage line**, exit 1 (one too few,
//!     one too many, and the `comm file -1 file` shape where "the option is written
//!     after an operand" all land here — the ring does not reorder, `-1` is the third
//!     file name);
//! (3) an unopenable file name = the three-part `err(1, "%s", name)` form, one line,
//!     **fatal**: the first name that fails exits 1 and the second is never tried
//!     (unlike `column`/`rev`'s warn+continue);
//! (4) the program name in the usage line is **hard-coded in the string**
//!     (`comm.c:197` has no `getprogname()`), while branch (1)'s prefix follows
//!     `getprogname()` — two lines of one command with different sources;
//! (5) a bare `-` is a legal operand (`comm.c:181`'s `!strcmp(name, "-")` →
//!     `stdin`), and `--` ends the ring without being an operand itself;
//! (6) a last line without a newline **is still a record** (`getnextln` returns
//!     `NULL` only at EOF with no character read) — the exact opposite of `column`,
//!     which treats a residual line as a "long line" and drops it;
//! (7) the tab prefix **follows the columns actually printed** (`comm.c:105-110`
//!     hands out `tabs[]` to the printed columns in turn), so after `-1` a common
//!     line gets the single tab of `tabs[1]`, not two — an earlier shell indexed
//!     `tabs[column - 1]` by column number, which this door caught on the spot.
//!
//! Before this door existed `comm`'s read seam went straight to bare
//! `minix_sys::open`/`read`, so no operand file could be read on the host (the eighth
//! instance of the same family), and the process-level three-column merge had never
//! been tested.

mod face_support;

use face_support::write_stdin;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

const COMM: &str = env!("CARGO_BIN_EXE_comm");
/// The `comm.c:197` line: the program name is hard-coded in the string.
const USAGE: &str = "usage: comm [-123f] file1 file2\n";

/// Same command seam as `run`, but both output streams come back **byte for
/// byte**: `from_utf8_lossy` renders a bare `c3` as the three-byte U+FFFD, so a text
/// form cannot tell "one byte" from "two bytes" apart.
fn run_bytes(args: &[&str], input: &[u8]) -> (Vec<u8>, i32, Vec<u8>) {
    let mut child = Command::new(COMM)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the comm binary should be runnable");
    face_support::write_stdin(&mut child, input);
    let output = child.wait_with_output().expect("the run should finish");
    (
        output.stdout,
        output.status.code().unwrap_or(-1),
        output.stderr,
    )
}

/// **Byte-form** fixture for argv shapes: a bare `ff` cannot be written as `&str`, so
/// the arguments go down through `OsStr::from_bytes` and what is compared is the byte
/// string the child actually receives (in a text-form fixture `from_utf8_lossy` turns a
/// bare `ff` and `ef bf bd` into the same name — precisely the seam under test).
fn run_argv(args: &[&[u8]], input: &[u8]) -> (Vec<u8>, i32, Vec<u8>) {
    use std::os::unix::ffi::OsStrExt as _;
    let mut child = Command::new(COMM)
        .args(args.iter().map(|a| std::ffi::OsStr::from_bytes(a)))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the comm binary should be runnable");
    write_stdin(&mut child, input);
    let output = child.wait_with_output().expect("the run should finish");
    (
        output.stdout,
        output.status.code().unwrap_or(-1),
        output.stderr,
    )
}

/// Write a file under its **raw byte name** and return its absolute path bytes
/// (`PathBuf` cannot splice in a bare `ff` segment). The call site writes the folded-name
/// copy separately with the same content — the reason sits in the comment of the door
/// that uses it.
fn write_raw(dir: &Path, name: &[u8], body: &[u8]) -> Vec<u8> {
    use std::os::unix::ffi::OsStrExt as _;
    fs::write(dir.join(std::ffi::OsStr::from_bytes(name)), body).expect("按原字节名落盘");
    let mut full = dir.as_os_str().as_bytes().to_vec();
    full.push(b'/');
    full.extend_from_slice(name);
    full
}

/// The letter position of the out-of-set letter line is **one raw byte**: the reference
/// build measured `-é` and `-1é` both printing `comm: unknown option -- ` + `c3` +
/// newline followed by the usage line, exit 1 (`1` is a legal letter in `"123f"`, so the
/// cluster keeps scanning into the first byte of `é`; `-Zé` stops at `Z`).
/// This shape holds only when **both halves** agree: the fetch is byte-wise and the
/// message is written byte-wise, so this test and the library's
/// `the_unknown_letter_is_the_first_byte_not_the_first_character` pin one half each.
#[test]
fn the_libc_line_of_a_multibyte_flag_is_one_byte_after_the_cluster() {
    let want: Vec<u8> = [
        b"comm: unknown option -- ".as_slice(),
        b"\xc3\n",
        USAGE.as_bytes(),
    ]
    .concat();
    for form in ["-\u{e9}", "-1\u{e9}"] {
        let (out, status, err) = run_bytes(&[form, "/dev/null", "/dev/null"], b"");
        assert_eq!(out, Vec::new(), "{form} 的报文只走标准错误");
        assert_eq!(status, 1, "{form} 的 stderr 是 {err:?}");
        assert_eq!(err, want, "{form} 的字母该是一个裸字节 c3");
    }

    let (_out, status, err) = run_bytes(&["-Z\u{e9}", "/dev/null", "/dev/null"], b"");
    assert_eq!(status, 1);
    assert_eq!(
        err,
        [b"comm: unknown option -- Z\n".as_slice(), USAGE.as_bytes(),].concat(),
        "环停在第一个坏字母上，`é` 根本不该被读到"
    );
}

/// The argv half is byte-wise too: a bare `ff` is still that one byte when it reaches
/// the option ring, and the letter position reports it as it is. The reference build (run
/// through a symlink named `comm`, so `getprogname()` yields `comm`, the same name as
/// this shell) measured `-\xff` and the cluster `-1\xff` both returning
/// `comm: unknown option -- ` + that one `ff` + a newline, then the **hard-coded** usage
/// line, exit 1 (`comm.c:195-199`'s usage has no `getprogname()`, so the two lines of one
/// command take their program name from different sources: the first follows argv[0], the
/// second is the string itself).
///
/// The third shape also carries raw bytes in the **operands**. It has no discriminating
/// power over the argv seam — folding is idempotent, so folding once up front and folding
/// again when the open arm is reached yield the same name (which is why a negative control
/// can be green on both sides and still confirm this point) — but it has teeth against the
/// open arm's **folding style**: swapping `from_utf8_lossy` for
/// `from_utf8(...).unwrap_or("")` turns this shape red on the spot. The disk therefore
/// holds **both copies** (the raw byte name and its `from_utf8_lossy` folded name, same
/// content): the open leg still needs `&str`, so with only the raw name this shell would
/// necessarily report `ENOENT` while the reference reads it, and that difference would
/// measure the open leg, not this seam.
#[test]
fn comm_reports_the_raw_option_byte_before_the_hardcoded_usage_line() {
    let want: Vec<u8> = [
        b"comm: unknown option -- ".as_slice(),
        b"\xff\n",
        USAGE.as_bytes(),
    ]
    .concat();
    for form in [&b"-\xff"[..], &b"-1\xff"[..]] {
        let (out, status, err) = run_argv(&[form, b"/dev/null", b"/dev/null"], b"");
        assert_eq!(out, Vec::new(), "{form:?} 的报文只走标准错误");
        assert_eq!(status, 1, "{form:?} 的 stderr 是 {err:?}");
        assert_eq!(err, want, "字母位该是 argv 里那一枚原字节 ff");
    }

    let dir = scratch("rawff");
    let raw = write_raw(&dir, b"pair\xff", b"apple\nbanana\n");
    fs::write(dir.join("pair\u{fffd}"), b"apple\nbanana\n").expect("写折叠孪生件");
    let (out, status, err) = run_argv(&[&raw, &raw], b"");
    assert_eq!(
        (status, err.as_slice()),
        (0, &b""[..]),
        "两枚同内容的操作数不该走到错误支"
    );
    assert_eq!(
        out, b"\t\tapple\n\t\tbanana\n",
        "两行都落列三：形状与参照件 `[原字节名, 原字节名]` 的现量一致"
    );
}

fn run(args: &[&str], input: &[u8]) -> (String, i32, String) {
    let mut child = Command::new(COMM)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the comm binary should be runnable");
    write_stdin(&mut child, input);
    let output = child.wait_with_output().expect("the run should finish");
    (
        String::from_utf8_lossy(&output.stdout).into_owned(),
        output.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

/// Two sorted operand files (the same pair of contents the reference uses); returns their
/// absolute paths.
fn operands(tag: &str) -> (PathBuf, String, String) {
    let dir = scratch(tag);
    let a = dir.join("a.txt");
    let b = dir.join("b.txt");
    fs::write(&a, b"apple\nbanana\ncherry\n").expect("write a.txt");
    fs::write(&b, b"banana\ndate\nfig\n").expect("write b.txt");
    (
        dir,
        a.to_str().unwrap().to_string(),
        b.to_str().unwrap().to_string(),
    )
}

/// Out-of-set letter: the libc line first, the hard-coded usage after, exit 1 (an earlier
/// shell dropped the libc line).
#[test]
fn an_unknown_letter_prints_the_libc_line_then_the_usage() {
    let (_dir, a, b) = operands("unknown");
    let (out, status, err) = run(&["-Z", &a, &b], b"");
    assert_eq!((out.as_str(), status), ("", 1), "stderr 是 {err:?}");
    assert_eq!(err, format!("comm: unknown option -- Z\n{USAGE}"));

    // A cluster reports only the first out-of-set letter; legal letters before it do not
    // change the exit.
    let (_, status, err) = run(&["-1Z", &a, &b], b"");
    assert_eq!(status, 1);
    assert_eq!(err, format!("comm: unknown option -- Z\n{USAGE}"));

    // `--x` reports the second hyphen as the letter (the optstring has no `-`).
    let (_, status, err) = run(&["--x", &a, &b], b"");
    assert_eq!(status, 1);
    assert_eq!(err, format!("comm: unknown option -- -\n{USAGE}"));
}

/// An operand count other than two = **only the usage line**: no libc line, and no `err`
/// line either. "The option written after an operand" also counts as three operands — the
/// ring does not reorder.
#[test]
fn an_operand_count_other_than_two_is_the_usage_line_only() {
    let (_dir, a, b) = operands("count");
    for (args, why) in [
        (vec![], "没有操作数"),
        (vec![a.as_str()], "只有一个操作数"),
        (vec![a.as_str(), b.as_str(), "extra"], "三个操作数"),
        (
            vec![a.as_str(), "-1", b.as_str()],
            "选项写在操作数之后（不重排）",
        ),
    ] {
        let (out, status, err) = run(&args, b"apple\nbanana\n");
        assert_eq!((out.as_str(), status), ("", 1), "{why}：stderr 是 {err:?}");
        assert_eq!(err, USAGE, "{why} 只该有 usage 一行");
    }
}

/// The default three-column merge: column one has no prefix, column two one tab, column
/// three two (`comm.c:56`'s `tabs[]`).
#[test]
fn the_default_merge_writes_three_columns_with_tab_offsets() {
    let (_dir, a, b) = operands("merge");
    let (out, status, err) = run(&[&a, &b], b"");
    assert_eq!(status, 0);
    assert_eq!(err, "");
    assert_eq!(out, "apple\n\t\tbanana\ncherry\n\tdate\n\tfig\n");
}

/// `-1`/`-2`/`-3` each drop one column, **and the column prefixes shift with them**: after
/// `-1` the former column two becomes the unprefixed one (`comm.c:105-110` assigns `tabs`
/// offsets to the printed columns in turn).
#[test]
fn each_suppression_flag_drops_its_column_and_retitles_the_rest() {
    let (_dir, a, b) = operands("suppress");
    let (out, _, _) = run(&["-1", &a, &b], b"");
    assert_eq!(out, "\tbanana\ndate\nfig\n");
    let (out, _, _) = run(&["-2", &a, &b], b"");
    assert_eq!(out, "apple\n\tbanana\ncherry\n");
    let (out, _, _) = run(&["-3", &a, &b], b"");
    assert_eq!(out, "apple\ncherry\n\tdate\n\tfig\n");
    // A cluster takes effect letter by letter: `-12` leaves only the common column, and it
    // gets `tabs[0]` (no prefix).
    let (out, _, _) = run(&["-12", &a, &b], b"");
    assert_eq!(out, "banana\n");
    // Suppressing all three columns prints nothing, and the exit code is still 0.
    let (out, status, err) = run(&["-123", &a, &b], b"");
    assert_eq!((out.as_str(), status, err.as_str()), ("", 0, ""));
}

/// `-f` switches the comparison to `strcasecmp` (`comm.c:88-89`): the same file pair
/// yields no common lines without folding, while `Apple`/`banana`-style pairs match once
/// folded.
#[test]
fn the_fold_flag_compares_case_insensitively() {
    let dir = scratch("fold");
    let up = dir.join("up.txt");
    let b = dir.join("b.txt");
    fs::write(&up, b"Apple\nBANANA\nCherry\n").expect("write up.txt");
    fs::write(&b, b"banana\ndate\nfig\n").expect("write b.txt");
    let (out, status, err) = run(&["-f", up.to_str().unwrap(), b.to_str().unwrap()], b"");
    assert_eq!(
        (out.as_str(), status),
        ("Apple\n\t\tBANANA\nCherry\n\tdate\n\tfig\n", 0)
    );
    assert_eq!(err, "");
    // Without folding the same pair follows byte order: all upper-case lines sort before
    // `banana`, so all three land in column one, then the first side runs out and the whole
    // second side goes to column two (the reference reads exactly this).
    let (plain, status, _) = run(&[up.to_str().unwrap(), b.to_str().unwrap()], b"");
    assert_eq!(status, 0);
    assert_eq!(plain, "Apple\nBANANA\nCherry\n\tbanana\n\tdate\n\tfig\n");
}

/// A bare `-` is a **legal operand** meaning standard input (`comm.c:181`); `--` ends the
/// ring and is not an operand itself, so a `-1` after it is a literal file name.
#[test]
fn a_dash_names_stdin_and_the_double_dash_ends_the_ring() {
    let (_dir, a, _b) = operands("dash");
    // First operand is `-`: the standard-input copy and the file copy run side by side. The
    // first side runs out first, so **the remaining lines are the second side's and go to
    // column two** (the reference reads `\t\tapple`, `\t\tbanana`, `\tcherry`).
    let (out, status, err) = run(&["-", &a], b"apple\nbanana\n");
    assert_eq!(
        (out.as_str(), status),
        ("\t\tapple\n\t\tbanana\n\tcherry\n", 0)
    );
    assert_eq!(err, "");

    // A second operand that cannot be opened = the three-part err(1) form, one line, fatal
    // (no usage).
    let (out, status, err) = run(&["-", "/nonexistent-second"], b"apple\n");
    assert_eq!((out.as_str(), status), ("", 1));
    assert_eq!(
        err, "comm: /nonexistent-second: No such file or directory\n",
        "开不了的档名是 err(1) 三段形且致命"
    );

    let dir = scratch("dashdash");
    let one = dir.join("one.txt");
    let two = dir.join("two.txt");
    fs::write(&one, b"alpha\nbeta\n").expect("write one.txt");
    fs::write(&two, b"beta\ngamma\n").expect("write two.txt");
    let (out, status, err) = run(&["--", one.to_str().unwrap(), two.to_str().unwrap()], b"");
    assert_eq!((out.as_str(), status), ("alpha\n\t\tbeta\n\tgamma\n", 0));
    assert_eq!(err, "");
    // A `-1` after `--` is not a flag: both are literal file names, and the first cannot be
    // opened → fatal.
    let (_, status, err) = run(&["--", "-1", "-2"], b"");
    assert_eq!(status, 1);
    assert_eq!(err, "comm: -1: No such file or directory\n");
}

/// An unopenable file name is the three-part `err(1, "%s", name)` form and **fatal**: when
/// the first name fails the second is never tried (the reference prints one line and exits
/// 1 in both branches); when the second fails, the first has already been read
/// successfully.
#[test]
fn an_unopenable_operand_is_fatal_and_prints_one_three_part_line() {
    let (_dir, a, b) = operands("fatal");
    let (out, status, err) = run(&["no-such-file", &b], b"");
    assert_eq!((out.as_str(), status), ("", 1));
    assert_eq!(
        err, "comm: no-such-file: No such file or directory\n",
        "三段形：程序名、档名、`strerror` 句子，且**不接** usage"
    );

    let (out, status, err) = run(&[&a, "no-such-file"], b"");
    assert_eq!((out.as_str(), status), ("", 1));
    assert_eq!(err, "comm: no-such-file: No such file or directory\n");
}

/// A last line without a newline **is still a record** (`getnextln` returns `NULL` only at
/// "EOF with no character read") — the opposite of `column`, which treats a residual line as
/// a "long line", drops it and warns.
#[test]
fn an_unterminated_last_line_is_still_a_record() {
    let dir = scratch("tail");
    let a = dir.join("a.txt");
    let nonl = dir.join("nonl.txt");
    fs::write(&a, b"apple\nbanana\ncherry\n").expect("write a.txt");
    fs::write(&nonl, b"apple\nbanana\ncherry").expect("write nonl.txt");
    let (out, status, err) = run(&[a.to_str().unwrap(), nonl.to_str().unwrap()], b"");
    assert_eq!(status, 0);
    assert_eq!(err, "");
    assert_eq!(out, "\t\tapple\n\t\tbanana\n\t\tcherry\n");
}

/// An empty file is a legal second input: the other side's lines all land in their own
/// column, and when both are empty nothing is printed.
#[test]
fn an_empty_operand_drains_the_other_side_into_its_own_column() {
    let dir = scratch("empty");
    let empty = dir.join("empty.txt");
    let b = dir.join("b.txt");
    fs::write(&empty, b"").expect("write empty.txt");
    fs::write(&b, b"banana\ndate\nfig\n").expect("write b.txt");
    let (out, status, err) = run(&[empty.to_str().unwrap(), b.to_str().unwrap()], b"");
    assert_eq!((out.as_str(), status), ("\tbanana\n\tdate\n\tfig\n", 0));
    assert_eq!(err, "");
    let (out, status, err) = run(&[empty.to_str().unwrap(), empty.to_str().unwrap()], b"");
    assert_eq!((out.as_str(), status, err.as_str()), ("", 0, ""));
}

/// Both legs byte-wise: (1) a non-UTF-8 line is still a record and is printed back byte for
/// byte (C's `getnextln` reads with `getc`, `comm.c:204-215`); (2) `-f`'s folding is a
/// per-byte ctype table, so `c3 89` (É) and `c3 a9` (é) are **not** equal (`strcasecmp`
/// `minix3/common/lib/libc/string/strcasecmp.c:58-67`).
/// Every expected value is taken from the reference build's measured output.
#[test]
fn a_nonutf8_line_is_a_record_and_the_fold_is_per_byte() {
    let dir = scratch("nonutf8");
    let a = dir.join("a.txt");
    let b = dir.join("b.txt");
    let upper = dir.join("upper.txt");
    let lower = dir.join("lower.txt");
    fs::write(&a, b"a\xffb\ncommon\nz\n").expect("write a.txt");
    fs::write(&b, b"a\xffb\nother\n").expect("write b.txt");
    fs::write(&upper, b"\xc3\x89\nApple\n").expect("write upper.txt");
    fs::write(&lower, b"\xc3\xa9\napple\n").expect("write lower.txt");

    let (out, status, err) = run_bytes(&[a.to_str().unwrap(), b.to_str().unwrap()], b"");
    assert_eq!((status, err), (0, Vec::new()));
    assert_eq!(
        out, b"\t\ta\xffb\ncommon\n\tother\nz\n",
        "共有的那一行带 ff，两半都该留住"
    );

    // No folding: in byte order `c3 89` and `c3 a9` differ in the second byte ⇒ each goes to
    // its own column
    let (out, status, err) = run_bytes(&[upper.to_str().unwrap(), lower.to_str().unwrap()], b"");
    assert_eq!((status, err), (0, Vec::new()));
    assert_eq!(out, b"\xc3\x89\nApple\n\t\xc3\xa9\n\tapple\n");

    // After `-f` the **shape does not change**: C's ctype table folds only `A`–`Z` and
    // returns non-ASCII bytes as they are, while the ASCII pair `Apple`/`apple` folds equal
    // and still lands in column one (because `A` < `c3`). An earlier shell folded É/é into
    // the same character via `str::to_lowercase`, so the two lines were judged a common line
    // and went to column three.
    let (out, status, err) = run_bytes(
        &["-f", upper.to_str().unwrap(), lower.to_str().unwrap()],
        b"",
    );
    assert_eq!((status, err), (0, Vec::new()));
    assert_eq!(out, b"\xc3\x89\nApple\n\t\xc3\xa9\n\tapple\n");
}

fn scratch(tag: &str) -> PathBuf {
    static SEQ: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let n = SEQ.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    let dir = std::env::temp_dir().join(format!("comm-face-{}-{tag}-{n}", std::process::id()));
    fs::create_dir_all(&dir).expect("scratch dir");
    dir
}
