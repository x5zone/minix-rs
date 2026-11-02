//! End-to-end face for the `paste` binary, checked against
//! `minix3/usr.bin/paste/paste.c` (the option loop `:70-83`, the operand
//! descent `:85-92`, `parallel()` `:95-155`, `sequential()` `:161-195`, `tr()`
//! `:199-226`, `usage()` `:229-233`) and the libc `getopt` it drives
//! (`minix3/lib/libc/stdlib/getopt.c:74-78`, `:81-85`, `:95-104`, `:112-141`).
//!
//! The optstring at `paste.c:70` is `"d:s"`: only `d` takes a value. The
//! reference build `tmp/paste-321/paste_ref` (the real `paste.c` source +
//! Minix's `getopt.c` + a program-name shim + a BSD `fgetln` shim, linked on
//! the host) measured these shapes one by one:
//! ① the loop exits at the **first** bad letter (`case '?': default: usage();`
//!    sits inside the `switch`), so both `paste -sZ` and `paste -ZQ` print only
//!    one libc line plus one usage line and exit 1—the exact opposite of its
//!    sibling `seq` (which keeps scanning after BADCH and accumulates the
//!    messages); `jot` has the same shape as this branch, and its `-ZQ`
//!    reference reports only `Z`;
//! ② the **order** of errors is the order of scanning: `paste -d '' -Z` reports
//!    "no delimiters" (`tr()` runs right inside `case 'd'`), while
//!    `paste -Z -d ''` reports the out-of-set letter;
//! ③ the program name in the usage line is hardcoded in the string
//!    (`paste.c:231`), while the libc line follows `getprogname()`;
//! ④ operands have **no count check**: `paste` with no operands reads no
//!    standard input, prints nothing, and exits 0;
//! ⑤ the loop does not reorder: `-s` in `paste a.txt -s` is the second file
//!    name, so the parallel mode's `err(1, "%s", …)` is fatal;
//! ⑥ an unopenable file name takes two different exits depending on mode:
//!    parallel is **fatal, exit 1**; serial `warn`s, continues, and exits **0**
//!    overall;
//! ⑦ `-d`'s table counts by byte; `\n`/`\t`/`\0` are the three escapes, and an
//!    unrecognized escape **drops the backslash** (the opposite of `seq`); a
//!    slot whose value is 0 occupies a position but prints nothing; in parallel
//!    mode each line's cursor starts at the **NUL at the table's tail**, and
//!    when one column's stream runs out, the later columns **add one extra
//!    delimiter** for it (`paste -d '+:' a b a` prints `3+:3` on its third
//!    line).
//!
//! **The eleventh "thin shell's raw `open`/`read` cannot read input on the
//! host" case**: this branch used to read operand files through raw
//! `minix_sys::open`, and on the host no file name could be opened ⇒ parallel
//! always exited 1 and serial always warned, so the operand shapes were never
//! tested at the process level. It now goes through `bin_support`'s twin face
//! (`read_file`/`read_stdin`).

mod face_support;

use face_support::write_stdin;
use std::ffi::OsString;
use std::fs;
use std::os::unix::ffi::OsStringExt as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

const PASTE: &str = env!("CARGO_BIN_EXE_paste");
/// The line from `paste.c:231`: the program name is hardcoded in the string,
/// with no interpolation.
const USAGE: &str = "paste: [-s] [-d delimiters] file ...\n";

fn run(args: &[&str], input: &[u8]) -> (String, i32, String) {
    let mut child = Command::new(PASTE)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the paste binary should be runnable");
    write_stdin(&mut child, input);
    let output = child.wait_with_output().expect("the run should finish");
    (
        String::from_utf8_lossy(&output.stdout).into_owned(),
        output.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

/// The same command seam as `run`, but handing back **raw bytes**: the textual
/// form goes through `from_utf8_lossy`, which turns a bare `c3` into the
/// three-byte U+FFFD, so "one byte" and "two bytes" are indistinguishable in a
/// `String`. This file previously had no such need—every expectation was an
/// ASCII message.
fn run_bytes(args: &[&str], input: &[u8]) -> (Vec<u8>, i32, Vec<u8>) {
    let mut child = Command::new(PASTE)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the paste binary should be runnable");
    write_stdin(&mut child, input);
    let output = child.wait_with_output().expect("the run should finish");
    (
        output.stdout,
        output.status.code().unwrap_or(-1),
        output.stderr,
    )
}

/// The same command seam, but handing argv back **raw bytes**:
/// `OsString::from_vec` skips UTF-8 validation, so a bare `ff` reaches `paste`'s
/// `argv` unchanged—the only observable face of the `bin_support::args_bytes`
/// leg (the `&str` forms of `run`/`run_bytes` can only hand over valid bytes).
fn run_raw(args: &[Vec<u8>], input: &[u8]) -> (Vec<u8>, i32, Vec<u8>) {
    let mut child = Command::new(PASTE)
        .args(args.iter().map(|raw| OsString::from_vec(raw.clone())))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the paste binary should be runnable");
    write_stdin(&mut child, input);
    let output = child.wait_with_output().expect("the run should finish");
    (
        output.stdout,
        output.status.code().unwrap_or(-1),
        output.stderr,
    )
}

fn scratch(tag: &str) -> PathBuf {
    let mut dir = std::env::temp_dir();
    dir.push(format!("paste321-{}-{}", tag, std::process::id()));
    fs::create_dir_all(&dir).expect("scratch dir");
    dir
}

/// Three operand files: three lines, two lines, and an empty file (the same
/// content the reference build uses).
fn operands(tag: &str) -> (String, String, String) {
    let dir = scratch(tag);
    let a = dir.join("a.txt");
    let b = dir.join("b.txt");
    let c = dir.join("c.txt");
    fs::write(&a, b"1\n2\n3\n").expect("write a.txt");
    fs::write(&b, b"a\nb\n").expect("write b.txt");
    fs::write(&c, b"").expect("write c.txt");
    (path(&a), path(&b), path(&c))
}

fn path(file: &Path) -> String {
    file.to_str().expect("utf-8 temp path").to_string()
}

#[test]
fn off_letter_gets_one_libc_line_and_the_hardcoded_usage() {
    for letter in ['Z', 'h', '0'] {
        let flag = format!("-{letter}");
        let (out, code, err) = run(&[flag.as_str()], b"");
        assert_eq!(code, 1, "{flag} 退 1（`usage()` 自己 `exit(1)`）");
        assert_eq!(out, "", "{flag}");
        assert_eq!(
            err,
            format!("paste: unknown option -- {letter}\n{USAGE}"),
            "{flag}"
        );
    }
}

/// The letter slot of the out-of-set line is **one raw byte**: the reference
/// build `tmp/paste-321/paste_ref` measures both `paste -é` and `paste -sé` as
/// `paste: unknown option -- ` + `c3` + newline, followed by the hardcoded usage
/// line, exit 1 (the mechanism is `optopt = *place++` at `getopt.c:79` and the
/// `%c` at `:103`). `s` is a legal letter of the `paste.c:70` optstring `"d:s"`,
/// so the cluster keeps consuming and eats é's first byte; `-Zé` instead stops
/// at `Z`—`paste.c:79-81`'s `default: usage()` sits inside the loop, so the
/// second letter never gets a turn, and that shape is also the positive evidence
/// that "a multibyte flag never reaches the letter slot at all". The ASCII `-Z`
/// and the missing-value `-d` shapes already have their own owners in this file
/// (`off_letter_gets_one_libc_line_and_the_hardcoded_usage`,
/// `missing_value_for_d_is_a_libc_line_then_usage`), so this test does not carry
/// the same fact twice.
#[test]
fn the_libc_line_of_a_multibyte_flag_ends_at_its_first_byte() {
    let multibyte: Vec<u8> = [
        b"paste: unknown option -- ".as_slice(),
        b"\xc3\n",
        USAGE.as_bytes(),
    ]
    .concat();
    for form in ["-\u{e9}", "-s\u{e9}"] {
        let (out, code, err) = run_bytes(&[form], b"");
        assert_eq!(out, Vec::new(), "{form} 的报文只走 stderr");
        assert_eq!(code, 1, "{form} 的 stderr 是 {err:?}");
        assert_eq!(err, multibyte, "{form} 的字母该是一个裸字节 c3");
    }

    let (_, code, err) = run_bytes(&["-Z\u{e9}"], b"");
    assert_eq!(code, 1);
    assert_eq!(
        err,
        [b"paste: unknown option -- Z\n".as_slice(), USAGE.as_bytes(),].concat(),
        "环停在第一个坏字母上，`é` 根本不该被读到"
    );
}

#[test]
fn the_ring_stops_at_the_first_bad_letter() {
    // The opposite shape to `seq`: `usage()` is called directly inside the loop,
    // so messages are **not accumulated**.
    let (_, code, err) = run(&["-ZQ"], b"");
    assert_eq!(code, 1);
    assert_eq!(err, format!("paste: unknown option -- Z\n{USAGE}"));
    let (_, code, err) = run(&["-sZ", "a.txt"], b"");
    assert_eq!(code, 1);
    assert_eq!(
        err,
        format!("paste: unknown option -- Z\n{USAGE}"),
        "s 先被吃掉"
    );
}

#[test]
fn missing_value_for_d_is_a_libc_line_then_usage() {
    let (out, code, err) = run(&["-d"], b"");
    assert_eq!((code, out), (1, String::new()));
    assert_eq!(
        err,
        format!("paste: option requires an argument -- d\n{USAGE}")
    );
    // Same inside a cluster: `s` is eaten first, and the one missing a value is
    // `d`.
    let (_, code, err) = run(&["-sd"], b"");
    assert_eq!(code, 1);
    assert_eq!(
        err,
        format!("paste: option requires an argument -- d\n{USAGE}")
    );
}

#[test]
fn error_precedence_follows_the_scan() {
    // `-d ''` hits `tr()` inside the loop ⇒ "no delimiters" is reported first,
    // and the later out-of-set letter never gets a turn.
    let (out, code, err) = run(&["-d", "", "-Z"], b"");
    assert_eq!((code, out), (1, String::new()));
    assert_eq!(
        err, "paste: no delimiters specified.\n",
        "句子自带句号且不带 strerror"
    );
    // Conversely, when the bad letter comes first, `-d` never gets a turn.
    let (_, code, err) = run(&["-Z", "-d", ""], b"");
    assert_eq!(code, 1);
    assert_eq!(err, format!("paste: unknown option -- Z\n{USAGE}"));
    // An attached `-d` value counts too (an empty value likewise exits 1).
    let (_, code, err) = run(&["-d", "\\q"], b"");
    assert_eq!((code, err), (0, String::new()), "零个操作数 ⇒ 不打任何东西");
}

#[test]
fn no_operand_reads_nothing_and_exits_zero() {
    // With zero operands C neither reads standard input nor reports an error
    // (the reference build prints nothing for stdin `L\n` and exits 0).
    let (out, code, err) = run(&[], b"L\n");
    assert_eq!((out, code, err), (String::new(), 0, String::new()));
    let (out, code, err) = run(&["--"], b"L\n");
    assert_eq!((out, code, err), (String::new(), 0, String::new()));
    let (out, code, err) = run(&["-s"], b"L\n");
    assert_eq!((out, code, err), (String::new(), 0, String::new()));
}

#[test]
fn operands_are_not_reordered() {
    let (a, _, _) = operands("reorder");
    // The loop closes at the first non-option token without advancing ⇒ `-s` is
    // the **second file name**, and the parallel open check is fatal.
    let (out, code, err) = run(&[a.as_str(), "-s"], b"");
    assert_eq!((code, out), (1, String::new()));
    assert_eq!(err, "paste: -s: No such file or directory\n");
    // `-s` after `--` is likewise a file name.
    let (out, code, err) = run(&["--", "-s"], b"");
    assert_eq!((code, out), (1, String::new()));
    assert_eq!(err, "paste: -s: No such file or directory\n");
}

#[test]
fn unopenable_operand_is_fatal_in_parallel_and_a_warning_in_serial() {
    let (_, b, _) = operands("openface");
    // Parallel: `err(1, "%s", argv[cnt])` three-part form, fatal, exit 1.
    let (out, code, err) = run(&["nope.txt", b.as_str()], b"");
    assert_eq!(
        (code, out),
        (1, String::new()),
        "第一个开不了就退出，后面的档名不读"
    );
    assert_eq!(err, "paste: nope.txt: No such file or directory\n");
    // Serial: `warn("%s", p)` three-part form, skip and continue, exit 0
    // overall.
    let (out, code, err) = run(&["-s", "nope.txt", b.as_str()], b"");
    assert_eq!(code, 0);
    assert_eq!(err, "paste: nope.txt: No such file or directory\n");
    assert_eq!(out, "a\tb\n", "坏档名跳过，好档名照旧并成一行");
    // In serial mode a bad file name in the middle does not change the later
    // output.
    let (a, _, _) = operands("openface");
    let (out, code, err) = run(&["-s", b.as_str(), "nope.txt", a.as_str()], b"");
    assert_eq!(code, 0);
    assert_eq!(err, "paste: nope.txt: No such file or directory\n");
    assert_eq!(out, "a\tb\n1\t2\t3\n");
}

#[test]
fn dash_operand_names_stdin_and_is_shared_per_open() {
    // Both columns are `-` ⇒ the same stream, the same cursor: each line's two
    // columns take one line each (the reference build has the same shape).
    let (out, code, err) = run(&["-", "-"], b"1\n2\n3\n");
    assert_eq!((code, err), (0, String::new()));
    assert_eq!(out, "1\t2\n3\t\n");
    // A single-column `-` is straight pass-through.
    let (out, code, err) = run(&["-"], b"p\nq\n");
    assert_eq!((code, err), (0, String::new()));
    assert_eq!(out, "p\nq\n");
    // In serial mode the second `-` has no lines left to take.
    let (out, code, err) = run(&["-s", "-", "-"], b"1\n2\n");
    assert_eq!((code, err), (0, String::new()));
    assert_eq!(out, "1\t2\n");
}

#[test]
fn delimiter_escapes_and_nul_slots() {
    let (a, b, _) = operands("escapes");
    // `\q`: an unrecognized escape **drops the backslash** (the opposite of
    // seq's unescape).
    let (out, code, err) = run(&["-d", "\\q", a.as_str(), b.as_str()], b"");
    assert_eq!((code, err), (0, String::new()));
    assert_eq!(out, "1qa\n2qb\n3q\n");
    let (out, _, _) = run(&["-d", "\\n", a.as_str(), b.as_str()], b"");
    assert_eq!(out, "1\na\n2\nb\n3\n\n");
    // `\0`: occupies a slot but prints nothing.
    let (out, _, _) = run(&["-d", "\\0", a.as_str(), b.as_str()], b"");
    assert_eq!(out, "1a\n2b\n3\n");
    // `a\0b` has three slots: the delimiter between the two columns is slot 0,
    // 'a'.
    let (out, _, _) = run(&["-d", "a\\0b", a.as_str(), b.as_str()], b"");
    assert_eq!(out, "1aa\n2ab\n3a\n");
    // A value attached to this token (`-d,`) behaves the same as `-d` taking the
    // next token.
    let (out, _, _) = run(&["-d,", a.as_str(), b.as_str()], b"");
    assert_eq!(out, "1,a\n2,b\n3,\n");
    // In `-ds`, `s` is the **value**, not a flag: two parallel columns with `s`
    // as the delimiter.
    let (out, code, err) = run(&["-ds", a.as_str(), b.as_str()], b"");
    assert_eq!((code, err), (0, String::new()));
    assert_eq!(out, "1sa\n2sb\n3s\n");
}

#[test]
fn parallel_catches_up_for_a_finished_stream() {
    let (a, b, _) = operands("catchup");
    // The third column is a.txt again (a second open ⇒ an independent cursor);
    // b has only two lines, so its third line gets an added `+`.
    let (out, code, err) = run(&["-d", "+:", a.as_str(), b.as_str(), a.as_str()], b"");
    assert_eq!((code, err), (0, String::new()));
    assert_eq!(out, "1+a:1\n2+b:2\n3+:3\n");
    // An empty file name is also a column: every line gets a delimiter added for
    // it.
    let (_, _, c) = operands("catchup");
    let (out, _, _) = run(&[c.as_str(), a.as_str()], b"");
    assert_eq!(out, "\t1\n\t2\n\t3\n");
}

/// Non-UTF-8 bytes in operand files and standard input go through both merge
/// legs **byte for byte**: C's line legs (`fgetln` `paste.c:121`, `fgets`
/// `:171`) never decode. The expectations for each shape are current
/// measurements from the reference build `tmp/paste-321/paste_ref`
/// (`tmp/paste-comm-434/ab.log`); the old shell erased the whole file to empty
/// at the `from_utf8(...).unwrap_or("")` line.
#[test]
fn an_operand_line_that_is_not_utf8_keeps_every_byte() {
    let dir = scratch("nonutf8");
    let raw = dir.join("raw.txt");
    let ok = dir.join("ok.txt");
    fs::write(&raw, b"a\xffb\nc\n").expect("write raw.txt");
    fs::write(&ok, b"x\ny\n").expect("write ok.txt");
    let raw = path(&raw);
    let ok = path(&ok);

    // The same file opened twice: each column reads that one ff.
    let (out, status, err) = run_bytes(&[raw.as_str(), raw.as_str()], b"");
    assert_eq!((status, err), (0, Vec::new()));
    assert_eq!(out, b"a\xffb\ta\xffb\nc\tc\n");

    // A bad file beside a good one: the bad file's lines must not blank out the
    // good file's lines.
    let (out, _, _) = run_bytes(&[ok.as_str(), raw.as_str()], b"");
    assert_eq!(out, b"x\ta\xffb\ny\tc\n");

    // Serial mode likewise counts by byte.
    let (out, _, _) = run_bytes(&["-s", raw.as_str()], b"");
    assert_eq!(out, b"a\xffb\tc\n");

    // The standard-input leg is the same: a last line without a newline still
    // counts as a line.
    let (out, _, _) = run_bytes(&["-", raw.as_str()], b"p\n\xffq\n");
    assert_eq!(out, b"p\ta\xffb\n\xffq\tc\n");
}

/// The letter slot of the out-of-set line is **that raw byte** `ff` from argv:
/// `bin_support::args_bytes` hands argv back unchanged, `src/paste.rs` takes
/// that byte, and `line_bytes` writes it out byte by byte. The reference build
/// `tmp/paste-321/paste_ref` measures the same shape
/// (`tmp/argv-441/measure-ref.log`); the old shell handed back the folded first
/// byte `ef` (`tmp/argv-441/before-probe.log`).
#[test]
fn a_raw_ff_option_letter_is_reported_as_that_byte() {
    let flag: Vec<u8> = [b"-".as_slice(), b"\xff"].concat();
    let (out, code, err) = run_raw(&[flag], b"");
    assert_eq!((out, code), (Vec::new(), 1));
    assert_eq!(
        err,
        [
            b"paste: unknown option -- ".as_slice(),
            b"\xff",
            b"\n".as_slice(),
            USAGE.as_bytes()
        ]
        .concat()
    );
}

/// The non-UTF-8 byte in `-d`'s **value** is **one slot** of the delimiter
/// table: before the change it was folded by lossy into the three bytes
/// `ef bf bd`, so `paste -d\xff a.txt b.txt` printed `1\xefa` (`ef` used on the
/// first line, `bf` and `bd` on the two after it); the reference build measures
/// one `ff` per line (`tmp/argv-441/measure-ref.log`).
#[test]
fn a_raw_nonutf8_delimiter_byte_is_one_slot() {
    let (a, b, _) = operands("delimbyte");
    let attached: Vec<u8> = [b"-d".as_slice(), b"\xff"].concat();
    let (out, code, err) = run_raw(
        &[attached, a.as_bytes().to_vec(), b.as_bytes().to_vec()],
        b"",
    );
    assert_eq!((code, err), (0, Vec::new()));
    assert_eq!(out, b"1\xffa\n2\xffb\n3\xff\n");

    // Serial mode uses the same table: two `ff`s span three lines (a one-slot
    // table cycles per slot).
    let attached: Vec<u8> = [b"-d".as_slice(), b"\xff"].concat();
    let (out, code, err) = run_raw(
        &[
            b"-s".to_vec(),
            attached,
            a.as_bytes().to_vec(),
            b.as_bytes().to_vec(),
        ],
        b"",
    );
    assert_eq!((code, err), (0, Vec::new()));
    assert_eq!(out, b"1\xff2\xff3\na\xffb\n");

    // A value taken from the next token likewise counts by byte: `ff` and `+`
    // are two slots. With two columns, each line's second column lands on the
    // table's first slot (the parallel cursor starts at the NUL at the table's
    // tail each line, `paste.c:112`), so the second line is `ff` too; only the
    // third column reaches `+`—both shapes are current measurements from the
    // reference build (`tmp/argv-441/measure-three.log`).
    let value: Vec<u8> = [b"\xff".as_slice(), b"+"].concat();
    let (out, code, err) = run_raw(
        &[
            b"-d".to_vec(),
            value.clone(),
            a.as_bytes().to_vec(),
            b.as_bytes().to_vec(),
        ],
        b"",
    );
    assert_eq!((code, err), (0, Vec::new()));
    assert_eq!(out, b"1\xffa\n2\xffb\n3\xff\n");
    let (out, _, _) = run_raw(
        &[
            b"-d".to_vec(),
            value,
            a.as_bytes().to_vec(),
            b.as_bytes().to_vec(),
            a.as_bytes().to_vec(),
        ],
        b"",
    );
    assert_eq!(out, b"1\xffa+1\n2\xffb+2\n3\xff+3\n");
}

#[test]
fn serial_mode_joins_each_operand_onto_one_line() {
    let (a, b, _) = operands("serial");
    let (out, code, err) = run(&["-s", a.as_str()], b"");
    assert_eq!((code, err), (0, String::new()));
    assert_eq!(out, "1\t2\t3\n");
    let (out, _, _) = run(&["-s", a.as_str(), b.as_str()], b"");
    assert_eq!(out, "1\t2\t3\na\tb\n", "每个操作数一行，顺序输出");
    let (out, _, _) = run(&["-s", "-d", "+:", a.as_str()], b"");
    assert_eq!(out, "1+2:3\n", "顺序模式的游标从表头起，按位循环");
}

/// 情形四（跨板裁决 §2.8）：C 的发送腿以「串尾空字节」发送（`paste.c:180`
/// 把换行改写为串尾），数据内含 NUL 即在那里截断。本壳按字节向量发送，
/// 整行原样保留——这一枚把「保留完整字节」钉住。
#[test]
fn a_nul_byte_in_the_data_is_kept_not_truncated() {
    let (out, status, err) = run_bytes(&["-"], b"a\x00b\nc\n");
    assert_eq!((out, status, err), (b"a\x00b\nc\n".to_vec(), 0, Vec::new()));
}
