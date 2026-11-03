//! End-to-end face for the `rev` binary, checked against
//! `minix3/usr.bin/rev/rev.c` (the option loop `:70-75`, `argv += optind`
//! `:77-78`, the operand loop and `warn` `:83-106`, `usage()` `:110-115`) and
//! the libc `getopt` it drives (`minix3/lib/libc/stdlib/getopt.c:74-78`,
//! `:81-92`, `:95-104`).
//!
//! C's optstring is the **empty string**, which is easy to misread as "no
//! option loop", so this shell used to treat `-Z` as a file operand. The
//! reference binary `tmp/rev-309/rev_ref` (the `rev.c` ground truth + Minix's
//! `getopt.c` + `fgetwln`/`setprogname` shims, linked against the host)
//! measured these shapes:
//! 1. Any token starting with `-` (except `--` and a bare `-`) = one libc line
//!    + one usage line, status 1;
//! 2. `--` ends the option loop and is not an operand itself, while a bare `-`
//!    is an operand;
//! 3. No reordering: a `-Z` after a non-option token is an operand;
//! 4. An operand that cannot be opened gets the three-part `warn(3)` shape and
//!    is **non-fatal** (the run continues with later files, exit code 1).
//!
//! This face did not exist before: `rev`'s read seam took a bare
//! `open`/`read`, so on the host it could read no input at all, and its output
//! was previously covered only by library tests (as with `nl` and `fold`
//! earlier).

mod face_support;

use face_support::write_stdin;
use std::fs;
use std::process::{Command, Stdio};

const REV: &str = env!("CARGO_BIN_EXE_rev");
const USAGE: &str = "usage: rev [file ...]\n";

/// Run one shape with **raw bytes** as argv: a `&str` cannot carry a bare `ff`
/// argument (`"-\xff"` is a Unicode escape, and encoding it as UTF-8 yields
/// `ef bf bd`); only what `OsString::from_vec` hands over is the byte string
/// the kernel sees.
fn run_raw(args: &[Vec<u8>], input: &[u8]) -> (Vec<u8>, i32, Vec<u8>) {
    use std::os::unix::ffi::OsStringExt as _;
    let argv: Vec<std::ffi::OsString> = args
        .iter()
        .map(|a| std::ffi::OsString::from_vec(a.clone()))
        .collect();
    let mut child = Command::new(REV)
        .args(&argv)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the rev binary should be runnable");
    write_stdin(&mut child, input);
    let output = child.wait_with_output().expect("the run should finish");
    (
        output.stdout,
        output.status.code().unwrap_or(-1),
        output.stderr,
    )
}

fn run(args: &[&str], input: &[u8]) -> (String, i32, String) {
    let mut child = Command::new(REV)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the rev binary should be runnable");
    write_stdin(&mut child, input);
    let output = child.wait_with_output().expect("the run should finish");
    (
        String::from_utf8_lossy(&output.stdout).into_owned(),
        output.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

/// Same as above, but all three channels come back as **raw bytes**.
///
/// This is the only way to pin a bare byte in a report: `String::from_utf8_lossy`
/// renders both `c3` and `c3 83` as the replacement character U+FFFD, so the
/// fixture above cannot tell those two mistakes apart.
fn run_bytes(args: &[&str], input: &[u8]) -> (Vec<u8>, i32, Vec<u8>) {
    let mut child = Command::new(REV)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the rev binary should be runnable");
    write_stdin(&mut child, input);
    let output = child.wait_with_output().expect("the run should finish");
    (
        output.stdout,
        output.status.code().unwrap_or(-1),
        output.stderr,
    )
}

/// An unknown letter: the libc line first, `usage()` after, status 1 (the
/// `case '?': default` at `rev.c:72-74`). The reference read
/// `rev: unknown option -- Z` + `usage: rev [file ...]`.
#[test]
fn an_unknown_letter_prints_the_libc_line_then_the_usage() {
    let (out, status, err) = run(&["-Z"], b"abc");
    assert_eq!(out, "", "报文只走 stderr");
    assert_eq!(status, 1, "stderr 是 {err:?}");
    assert_eq!(err, format!("rev: unknown option -- Z\n{USAGE}"));
}

/// A cluster reports only its **first** character: the caller gets `'?'` and
/// exits through `usage()` without returning to getopt to read the second
/// letter (reference: `-bZ` → `unknown option -- b`).
#[test]
fn a_cluster_reports_only_its_first_letter() {
    let (out, status, err) = run(&["-bZ"], b"abc");
    assert_eq!(out, "");
    assert_eq!(status, 1);
    assert_eq!(err, format!("rev: unknown option -- b\n{USAGE}"));
}

/// A non-ASCII unknown letter prints **one bare byte**: the reference measured
/// `rev -é` → `rev: unknown option -- ` + `c3` + newline, then one usage line,
/// status 1. This shell got this shape wrong twice: taking a character reported
/// `c3 a9` (the whole `é`), and taking a byte but still rendering it as `char`
/// reported `c3 83` (the UTF-8 of U+00C3) — both are one byte longer than C,
/// and the lossy `String` fixture reads both as the replacement character, so
/// this test compares stderr byte by byte. That there is only one libc report
/// is part of the assertion too: `rev.c:72-74` exits on `'?'` and does not
/// report `a9` a second time the way an accumulating `getopt` caller would.
#[test]
fn an_unknown_nonascii_letter_prints_one_raw_byte() {
    let want: Vec<u8> = [
        b"rev: unknown option -- ".as_slice(),
        b"\xc3\n",
        USAGE.as_bytes(),
    ]
    .concat();
    let (out, status, err) = run_bytes(&["-\u{e9}"], b"abc");
    assert_eq!(out, Vec::new(), "报文只走 stderr");
    assert_eq!(status, 1, "stderr 是 {err:?}");
    assert_eq!(err, want, "字母该是一个裸字节 c3");

    // Non-ASCII in second position of a cluster: the report still lands on the
    // first byte, and it is still **one line**.
    let (out, status, err) = run_bytes(&["-x\u{e9}"], b"abc");
    assert_eq!(out, Vec::new());
    assert_eq!(status, 1);
    assert_eq!(
        err,
        b"rev: unknown option -- x\nusage: rev [file ...]\n".to_vec()
    );
}

/// A **single invalid-UTF-8 byte** as the option letter (`-` followed by a bare
/// `ff`): the reference measured `rev: unknown option -- ` + the bare byte `ff`,
/// status 1. C's `getopt` steps byte by byte
/// (`minix3/lib/libc/stdlib/getopt.c:optopt = *place++`), so the letter is that
/// one byte; there is no "decode as text first, then report" step.
#[test]
fn a_raw_nonutf8_option_letter_is_reported_as_that_byte() {
    let want: Vec<u8> = [
        b"rev: unknown option -- ".as_slice(),
        b"\xff\n",
        USAGE.as_bytes(),
    ]
    .concat();
    let (out, status, err) = run_raw(&[b"-\xff".to_vec()], b"abc");
    assert_eq!(out, Vec::new(), "报文只走 stderr");
    assert_eq!(status, 1, "stderr 是 {err:?}");
    assert_eq!(err, want, "字母该是裸字节 ff，不是折叠后的 efbfbd");
}

/// `--x` reports the second hyphen as the letter: `-` is not in the optstring,
/// so it is an unknown character too (reference: `unknown option -- -`). A
/// plain `--` takes the other branch — it ends the option loop and reports
/// nothing.
#[test]
fn a_dash_followed_by_a_word_is_an_unknown_option_named_by_the_dash() {
    let (out, status, err) = run(&["--x"], b"abc");
    assert_eq!(out, "");
    assert_eq!(status, 1);
    assert_eq!(err, format!("rev: unknown option -- -\n{USAGE}"));
}

/// `--` is consumed and everything after it counts as an operand
/// (`getopt.c:81-85` does `++optind` first): `--` alone still reads standard
/// input, while `-- <file>` reads that file instead.
#[test]
fn the_end_of_options_marker_is_not_an_operand() {
    let (out, status, err) = run(&["--"], b"abc\ndefg\n");
    assert_eq!((out.as_str(), err.as_str(), status), ("cba\ngfed\n", "", 0));
    let dir = scratch();
    let file = dir.join("in");
    fs::write(&file, b"xy\nz\n").expect("write operand");
    let (out, status, err) = run(&["--", file.to_str().unwrap()], b"");
    assert_eq!(
        (out.as_str(), err.as_str(), status),
        ("yx\nz\n", "", 0),
        "`--` 后面的具名文件该被读"
    );
    let _ = fs::remove_dir_all(dir);
}

/// A bare `-` is an operand, not an option: `-` is not in the optstring, and
/// getopt **does not advance** `optind` when it ends the option loop
/// (`getopt.c:87-92`), so this token falls into the `argv += optind` segment
/// and reaches C's `fopen("-")` → `warn` branch (reference:
/// `rev: -: No such file or directory`, **one line, no usage**, status 1).
#[test]
fn a_solitary_dash_is_an_operand_and_warns_in_the_err3_shape() {
    let (out, status, err) = run(&["-"], b"abc");
    assert_eq!(out, "", "`-` 不是「读标准输入」的别名");
    assert_eq!(status, 1, "stderr 是 {err:?}");
    let head = "rev: -: ";
    assert!(
        err.starts_with(head),
        "该以 `rev: -: ` 开头（程序名 + 操作数），实际 {err:?}"
    );
    let line = err[head.len()..].trim_end();
    assert!(
        !line.is_empty() && !line.contains('\n'),
        "缀的应是一句 strerror 文本且只有这一行：{err:?}"
    );
    assert!(!err.contains("usage:"), "操作数的 warn 不接 usage：{err:?}");
}

/// No reordering: the option loop ends as soon as a non-option token arrives
/// (`getopt.c:74-78`), so the `-Z` in `rev <file> -Z` is a **file name** (the
/// reference warns about it and processes the earlier file as usual).
#[test]
fn an_option_looking_token_after_an_operand_is_an_operand() {
    let dir = scratch();
    let file = dir.join("in");
    fs::write(&file, b"abc\n").expect("write operand");
    let (out, status, err) = run(&[file.to_str().unwrap(), "-Z"], b"");
    assert_eq!(out, "cba\n", "具名文件该被处理：{out:?}");
    assert_eq!(status, 1, "stderr 是 {err:?}");
    assert!(
        err.starts_with("rev: -Z: ") && !err.contains("usage:"),
        "`-Z` 该按打不开的操作数报 warn 三段形，而不是选项错：{err:?}"
    );
    let _ = fs::remove_dir_all(dir);
}

/// An operand that cannot be opened is **non-fatal** (`rev.c:85-89` is `warn` +
/// `++argv` + `continue`): the bad file reports one line, the good files are
/// processed as usual, exit code 1. The old implementation got both parts
/// wrong: the sentence lacked the `strerror` part, and there was no option
/// loop.
#[test]
fn an_unopenable_operand_warns_and_the_run_continues() {
    let dir = scratch();
    let good = dir.join("good");
    fs::write(&good, b"abc\ndefg\n").expect("write good");
    let missing = dir.join("absent");
    let (out, status, err) = run(&[missing.to_str().unwrap(), good.to_str().unwrap()], b"");
    assert_eq!(out, "cba\ngfed\n", "后面的好文件该照做：{out:?}");
    assert_eq!(status, 1, "stderr 是 {err:?}");
    let head = format!("rev: {}: ", missing.to_str().unwrap());
    assert!(
        err.starts_with(&head),
        "该以 `rev: <操作数>: ` 开头，实际 {err:?}"
    );
    assert_eq!(
        err.lines().count(),
        1,
        "只报 warn 那一行，不接 usage：{err:?}"
    );
    let _ = fs::remove_dir_all(dir);
}

/// The output leg (the other half of why this face exists): `rev` used to use a
/// bare `open`/`read`, so on the host it **could read no input**, and these
/// three shapes had never been tested at process level. The golden samples were
/// checked shape by shape against the reference (`tmp/rev-309/golden.log`).
#[test]
fn the_reversal_reaches_the_output_end_to_end() {
    let (out, status, err) = run(&[], b"abc\ndefg\n");
    assert_eq!((out.as_str(), err.as_str(), status), ("cba\ngfed\n", "", 0));
    // A final line without a newline is still reversed, and one newline is
    // added (`rev.c:94-99`).
    let (out, status, _) = run(&[], b"abc");
    assert_eq!((out.as_str(), status), ("cba\n", 0));
    // Empty input produces no lines.
    let (out, status, _) = run(&[], b"");
    assert_eq!((out.as_str(), status), ("", 0));
    // Character-level reversal: multibyte characters are not torn apart (C goes
    // through wide characters, the same guarantee).
    let (out, _, _) = run(&[], "aé中b\n".as_bytes());
    assert_eq!(out, "b中éa\n");
}

/// When standard input cannot be read, C reports the **stream name**, not an
/// operand: `rev.c:80-81` initialises `filename` to `"stdin"`, so
/// `if (ferror(fp)) warn("%s", filename)` at `:101-104` prints
/// `rev: stdin: <strerror>` and sets the exit code to 1. The easiest read-error
/// carrier on the host is a file descriptor for a directory as standard input:
/// the open succeeds and the read answers `EISDIR`. The reference's reading of
/// this shape (`tmp/rev-331/measure.log`): `rev_ref: stdin: Is a directory`,
/// status 1, no output. This shell used to treat a read failure as end of file,
/// so this shape **said nothing and exited 0**. The `Is a directory` sentence
/// comes from the host `strerror` table (the on-target libc read leg is
/// tracked separately as E5).
#[test]
fn a_stdin_that_cannot_be_read_names_the_stream_and_exits_one() {
    let dir = scratch();
    let stream = dir.join("not-a-stream");
    fs::create_dir(&stream).expect("create the directory to hand over as stdin");
    let handle = fs::File::open(&stream).expect("a directory opens read-only here");
    let output = Command::new(REV)
        .stdin(Stdio::from(handle))
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the rev binary should be runnable")
        .wait_with_output()
        .expect("the run should finish");
    let _ = fs::remove_dir_all(&dir);
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "",
        "读不到内容就不该有输出"
    );
    assert_eq!(
        String::from_utf8_lossy(&output.stderr),
        "rev: stdin: Is a directory\n",
        "报的该是流名 `stdin`，一行、三段形"
    );
    assert_eq!(
        output.status.code(),
        Some(1),
        "读失败是致命的吗？不，但要把退码置起来"
    );
}

/// The other half of the same `warn("%s", filename)`: an operand replaces
/// `filename`'s initial value (`rev.c:91`), so an operand that cannot be opened
/// or read reports **that operand**, not `stdin`. This shape is already
/// correct here (it goes through `read_file`'s error channel); it stays so the
/// previous test's "the stream name follows the stream" has a contrast, and the
/// two cases do not collapse into one generic report.
#[test]
fn an_unreadable_operand_names_itself_rather_than_stdin() {
    let dir = scratch();
    let stream = dir.join("directory-operand");
    fs::create_dir(&stream).expect("create the directory operand");
    let (out, status, err) = run(&[stream.to_str().unwrap()], b"");
    let _ = fs::remove_dir_all(&dir);
    assert_eq!(out, "");
    assert_eq!(status, 1);
    assert_eq!(
        err,
        format!("rev: {}: Is a directory\n", stream.to_str().unwrap())
    );
}

fn scratch() -> std::path::PathBuf {
    // Each test gets its own directory: these tests all end with
    // `remove_dir_all`, and cargo runs tests on multiple threads, so sharing
    // one directory named after the process id would let whichever finishes
    // first delete files another test is still using.
    static SEQ: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let seq = SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("rev-face-{}-{}", std::process::id(), seq));
    fs::create_dir_all(&dir).expect("scratch dir");
    dir
}

/// The adjudicated error leg (`REV-MULTIBYTE-ERROR-LEG`): C's `fgetwln`
/// raises `EILSEQ` on the first undecodable line — the decoded prefix stays
/// printed, the warn names the stream, and the exit status is 1
/// (`rev.c:92-103`). The sentence is the errno table's answer for 85, which
/// sits past `sys_nerr`: Minix's own `strerror` answers `Unknown error: 85`
/// (the glibc reference prints the long sentence; the table is the truth
/// this tree follows).
#[test]
fn an_undecodable_line_keeps_the_prefix_names_the_stream_and_exits_one() {
    let mut child = Command::new(REV)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn rev");
    write_stdin(&mut child, b"abc\ndef\n\xff\nzzz\n");
    let output = child.wait_with_output().expect("wait rev");
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "cba\nfed\n",
        "已解码前缀照印，坏行与其后不出现"
    );
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(
        String::from_utf8_lossy(&output.stderr),
        "rev: stdin: Unknown error: 85\n",
        "warn 走流名与表内句子"
    );
}
