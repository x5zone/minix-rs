//! End-to-end face for the `unifdef` binary, checked against
//! `minix3/usr.bin/unifdef/unifdef.c` (option ring `:273-323`, symbol validation
//! `addsym` `:965-991`, operands and open `:327-341`, `usage()` `:373-379`) and
//! the libc `getopt` it drives (`minix3/lib/libc/stdlib/getopt.c:74-78`,
//! `:81-92`, `:95-104`, `:118-141`).
//!
//! The optstring at `unifdef.c:273` is `"i:D:U:I:o:cdeklst"`: `D`/`U`/`i`/`I`/`o`
//! take a value, `c`/`d`/`e`/`k`/`l`/`s`/`t` do not, and letter case counts (`u`
//! in `-uD` and `E` in `-tE` are outside the set). There are three exit shapes,
//! and **this arm's usage exits with 2** (`exit(2)`, not the 1 common to its
//! siblings): (1) an out-of-set letter and a missing value each get one line from
//! libc first, then one line from `default: usage()`; (2) when the letter after
//! `-i` is not `D`/`U`, or a `-D`/`-U` symbol name does not fit the shape `addsym`
//! accepts, C prints **only the usage line** with no diagnostic of its own (this
//! shell once invented `-i must be followed by D or U` and `bad symbol spec …`
//! there, one extra line); (3) after the ring ends it first checks "no symbol at
//! all and no `-s`" = one `warnx` line plus two usage lines, and that arm runs
//! before the operand check, so `unifdef -- -z` reports it rather than "can only
//! do one file".
//!
//! Three more show up only end to end: `-D ''` and `-U ''` are **valid** empty
//! symbol names (`skipsym` consumes no character and returns where it started,
//! so `*val == '\0'`), and the file passes through unchanged with status 0; only
//! one operand is accepted and the ring **does not reorder** (the `-c` in
//! `unifdef -DA in.c -c` is the second operand, status 2, rather than a flag
//! swallowed by the ring); an unopenable operand name takes the three-part
//! `err(2, "can't open %s", …)` shape. The `-i` arm eats one character before
//! looking at the rest, so the symbol of `-iDA` is `A` (byte-identical to `-D A`)
//! and not a whole string like `DFOO`.
//!
//! Neither the list of controlling symbols printed by `-s` nor the debug lines
//! of `-d` are asserted by this face: the former is `printf`ed by `findsym` while
//! it scans the table (`unifdef.c:930-931`), the latter is flushed to standard
//! error by `debug()`; both belong to the deciding half and are registered as
//! `UNIFDEF-SYMLIST-OUTPUT` and `UNIFDEF-D-DEBUG-OUTPUT`.

mod face_support;

use face_support::write_stdin;
use std::fs;
use std::path::PathBuf;
use std::process::{Command, Stdio};

const UNIFDEF: &str = env!("CARGO_BIN_EXE_unifdef");
const USAGE: &str = "usage: unifdef [-cdeklst] [-o output] [-Dsym[=val]] [-Usym] [-iDsym[=val]] [-iUsym] ... [file]\n";
const SOURCE: &[u8] = b"#ifdef A\nint a;\n#endif\n#ifdef B\nint b;\n#endif\n";
/// `-DA` resolves group A: `#ifdef A` and its `#endif` disappear, `int a;` stays,
/// group B is untouched.
const RESOLVED_A: &str = "int a;\n#ifdef B\nint b;\n#endif\n";

fn run(args: &[&str], input: &[u8]) -> (String, i32, String) {
    let mut child = Command::new(UNIFDEF)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the unifdef binary should be runnable");
    write_stdin(&mut child, input);
    let output = child.wait_with_output().expect("the run should finish");
    (
        String::from_utf8_lossy(&output.stdout).into_owned(),
        output.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

fn scratch(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("unifdef318-{tag}-{}", std::process::id()));
    fs::create_dir_all(&dir).expect("the scratch directory should be creatable");
    dir
}

/// The same command seam as `run`, but handing the bytes back **verbatim**:
/// `from_utf8_lossy` renders a bare `c3` as the three-byte U+FFFD, and the text
/// form cannot tell "one byte" from "two bytes", the two ways to get it wrong
/// (the `rev`/`paste`/`jot` arms work the same way).
fn run_bytes(args: &[&str], input: &[u8]) -> (Vec<u8>, i32, Vec<u8>) {
    let mut child = Command::new(UNIFDEF)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the unifdef binary should be runnable");
    write_stdin(&mut child, input);
    let output = child.wait_with_output().expect("the run should finish");
    (
        output.stdout,
        output.status.code().unwrap_or(-1),
        output.stderr,
    )
}

/// In the out-of-set-letter line the letter slot is **one bare byte**: measured
/// against the reference binary `tmp/unifdef-318/unifdef_ref`, both `-é` and
/// `-cé` yield `unifdef: unknown option -- ` + `c3` + newline, then the usage,
/// status 2 (`c` is a valid valueless letter of `"i:D:U:I:o:cdeklst"`, so the
/// cluster keeps eating and takes the first byte of `é`; `-Zé` stops at `Z` and
/// `unifdef.c:322-323` calls `usage()` on `'?'`). The ASCII `-z` form and the
/// "a cluster reports only the first bad letter" form already have owners in this
/// file (`an_unknown_letter_prints_the_libc_line_then_the_usage`,
/// `the_case_of_a_letter_is_part_of_the_option_set`), so the redundancy rule
/// says not to carry them again.
#[test]
fn a_multibyte_flag_letter_reaches_the_libc_line_as_one_byte() {
    let want: Vec<u8> = [
        b"unifdef: unknown option -- ".as_slice(),
        b"\xc3\n",
        USAGE.as_bytes(),
    ]
    .concat();
    for form in ["-\u{e9}", "-c\u{e9}"] {
        let (out, status, err) = run_bytes(&[form], SOURCE);
        assert_eq!(out, Vec::new(), "{form} 的报文只走 stderr");
        assert_eq!(status, 2, "{form} 的 stderr 是 {err:?}");
        assert_eq!(err, want, "{form} 的字母该是一个裸字节 c3");
    }

    let (_, status, err) = run_bytes(&["-Z\u{e9}"], SOURCE);
    assert_eq!(status, 2);
    assert_eq!(
        err,
        [
            b"unifdef: unknown option -- Z\n".as_slice(),
            USAGE.as_bytes(),
        ]
        .concat(),
        "环停在第一个坏字母上，`é` 根本不该被读到"
    );
}

/// Out-of-set letter: the libc line first, the usage after, status **2**
/// (`unifdef.c:322-323` plus `:378`).
#[test]
fn an_unknown_letter_prints_the_libc_line_then_the_usage() {
    let (out, status, err) = run(&["-z"], SOURCE);
    assert_eq!(out, "");
    assert_eq!(status, 2, "stderr 是 {err:?}");
    assert_eq!(err, format!("unifdef: unknown option -- z\n{USAGE}"));
}

/// A cluster reports only the first bad letter; and letter case is part of the
/// set: neither `u` nor `E` is in the string.
#[test]
fn the_case_of_a_letter_is_part_of_the_option_set() {
    for (args, letter) in [
        ("-cz", "z"),
        ("-ZD", "Z"),
        ("--x", "-"),
        ("-tE", "E"),
        ("-uD", "u"),
    ] {
        let (out, status, err) = run(&[args], SOURCE);
        assert_eq!(
            (out.as_str(), status),
            ("", 2),
            "`{args}` 的 stderr 是 {err:?}"
        );
        assert_eq!(err, format!("unifdef: unknown option -- {letter}\n{USAGE}"));
    }
}

/// All five value-taking letters without their value look alike (including the
/// clusters `-cD` and `-lI` that end at the string end): the libc line plus the
/// usage, status 2.
#[test]
fn each_value_letter_without_its_value_prints_the_libc_line() {
    for letter in ['D', 'U', 'i', 'I', 'o'] {
        let flag = format!("-{letter}");
        let (out, status, err) = run(&[&flag], SOURCE);
        assert_eq!(
            (out.as_str(), status),
            ("", 2),
            "{flag} 的 stderr 是 {err:?}"
        );
        assert_eq!(
            err,
            format!("unifdef: option requires an argument -- {letter}\n{USAGE}"),
            "缺值这一支是两句，程序名跟着 `getprogname()`"
        );
    }
    for (cluster, letter) in [("-cD", "D"), ("-lI", "I")] {
        let (_, status, err) = run(&[cluster], SOURCE);
        assert_eq!(status, 2);
        assert_eq!(
            err,
            format!("unifdef: option requires an argument -- {letter}\n{USAGE}")
        );
    }
}

/// When the character after `-i` is not `D`/`U`, C only calls `usage()`: no libc
/// line and no diagnostic of its own. An empty value (`-i` with an empty token)
/// looks the same, because `*optarg` is `\0` right there.
#[test]
fn a_rejected_ignore_form_is_the_usage_line_alone() {
    for args in [
        vec!["-iX"],
        vec!["-i", ""],
        vec!["-id"],
        vec!["-i", "-x"],
        vec!["-i", "-"],
    ] {
        let (out, status, err) = run(&args, SOURCE);
        assert_eq!(
            (out.as_str(), status),
            ("", 2),
            "`{args:?}` 的 stderr 是 {err:?}"
        );
        assert_eq!(err, USAGE, "只有 usage 一行");
    }
}

/// `addsym`'s shape check (`unifdef.c:978-990`): `-D` accepts `sym`, `sym=`, and
/// `sym=val`; `-U` accepts only `sym`; whatever follows the name other than the
/// end or `=` is a bad shape, and a bad shape prints only the usage line.
#[test]
fn a_rejected_symbol_form_is_the_usage_line_alone() {
    for args in [
        vec!["-UFOO=1"],
        vec!["-DFOO."],
        vec!["-D."],
        vec!["-U", "FOO."],
        vec!["-D", "FOO BAR"],
    ] {
        let (out, status, err) = run(&args, SOURCE);
        assert_eq!(
            (out.as_str(), status),
            ("", 2),
            "`{args:?}` 的 stderr 是 {err:?}"
        );
        assert_eq!(err, USAGE);
    }
}

/// An empty symbol name is valid: `skipsym("")` returns where it started and
/// `*val == '\0'`, so the "empty value" branch is taken. This test pins that C
/// accepts it and passes the whole file through unchanged with status 0.
#[test]
fn an_empty_symbol_name_is_accepted() {
    for flag in ["-D", "-U"] {
        let (out, status, err) = run(&[flag, ""], SOURCE);
        assert_eq!(
            (out.as_str(), status),
            ("#ifdef A\nint a;\n#endif\n#ifdef B\nint b;\n#endif\n", 0),
            "`{flag}` 配空符号名的 stderr 是 {err:?}"
        );
        assert_eq!(err, "");
    }
}

/// No symbol at all: the `warnx` line plus the usage, status 2; and this arm runs
/// **before** the operand check, so `-- -z` (both tokens after the ring) still
/// reports it. With `-s` no symbol is needed (list mode).
#[test]
fn a_missing_symbol_warns_then_prints_the_usage() {
    for args in [
        vec![],
        vec!["-c"],
        vec!["-l", "-t"],
        vec!["--"],
        vec!["--", "-z"],
    ] {
        let (out, status, err) = run(&args, SOURCE);
        assert_eq!(
            (out.as_str(), status),
            ("", 2),
            "`{args:?}` 的 stderr 是 {err:?}"
        );
        assert_eq!(
            err,
            format!("unifdef: must -D or -U at least one symbol\n{USAGE}")
        );
    }
    // A lone `-s` is valid: this test only asserts that the warning is not
    // emitted; the list content belongs to the registered deciding-half item.
    let (_, status, err) = run(&["-s"], SOURCE);
    assert!(!err.contains("must -D or -U"), "`-s` 不该撞那句: {err:?}");
    assert_eq!(status, 0);
}

/// Only one operand is allowed, and the ring **does not reorder**: a `-c` after
/// the first non-option token is the second operand (reference binary:
/// `unifdef -DA in.c -c` → `can only do one file`), and the text is `errx`'s
/// single line.
#[test]
fn only_one_operand_is_taken_and_the_ring_does_not_rearrange() {
    let dir = scratch("operands");
    let file = dir.join("in.c");
    fs::write(&file, SOURCE).expect("write in.c");
    let path = file.to_str().unwrap();
    for args in [
        vec!["-DA", path, "extra"],
        vec!["-DA", path, "-c"],
        vec!["-DA", path, path],
    ] {
        let (out, status, err) = run(&args, SOURCE);
        assert_eq!(
            (out.as_str(), status),
            ("", 2),
            "`{args:?}` 的 stderr 是 {err:?}"
        );
        assert_eq!(
            err, "unifdef: can only do one file\n",
            "只有这一行，不接 usage"
        );
    }
}

/// An unopenable operand name takes the three-part
/// `err(2, "can't open %s", filename)` shape: program name, the operand text as
/// given, and the `strerror` sentence — none of the three may be missing.
#[test]
fn an_unopenable_operand_is_a_three_part_error() {
    let (out, status, err) = run(&["-DA", "./nope-318.c"], SOURCE);
    assert_eq!((out.as_str(), status), ("", 2));
    assert_eq!(
        err, "unifdef: can't open ./nope-318.c: No such file or directory\n",
        "三段形：程序名、操数原文、`strerror` 句子"
    );
}

/// A bare `-` aliases standard input (`strcmp(*argv, "-") != 0` at
/// `unifdef.c:333`), with the same effect as no operand at all; this is the read
/// seam this arm goes through.
#[test]
fn a_dash_operand_reads_stdin_like_no_operand_at_all() {
    let (out, status, err) = run(&["-DA"], SOURCE);
    assert_eq!((out.as_str(), status), (RESOLVED_A, 1), "stderr 是 {err:?}");
    assert_eq!(err, "");

    let (out, status, err) = run(&["-DA", "-"], SOURCE);
    assert_eq!((out.as_str(), status), (RESOLVED_A, 1), "stderr 是 {err:?}");
    assert_eq!(err, "");
}

/// `-i` eats one character before taking the symbol name (`opt = *optarg++`), so
/// the symbol of `-iDA` is `A`; an unrelated symbol name (`DFOO`) matches
/// nothing, so the whole file passes through with status 0.
#[test]
fn the_ignore_letter_is_consumed_before_the_symbol_name() {
    let (out, status, err) = run(&["-iDA"], SOURCE);
    assert_eq!((out.as_str(), status), (RESOLVED_A, 1), "stderr 是 {err:?}");
    assert_eq!(err, "", "`-iDA` 与 `-D A` 同效：符号是 A，不是 DFOO");

    let (out, status, _err) = run(&["-iDFOO"], SOURCE);
    assert_eq!(
        (out.as_str(), status),
        (std::str::from_utf8(SOURCE).unwrap(), 0)
    );

    // For `-iD=1` the symbol name is the **start** of that segment (here the
    // start is `=`, not one identifier character), so it matches nothing: the
    // whole file passes through with status 0. Both sides agree byte for byte on
    // this form (measured against the reference binary).
    let (out, status, _err) = run(&["-iD=1"], SOURCE);
    assert_eq!(
        (out.as_str(), status),
        (std::str::from_utf8(SOURCE).unwrap(), 0),
        "`-iD=1` 是合法的「空名字挂个值」，不是坏形"
    );
}

/// Giving the same symbol twice neither errors nor doubles it: C's `findsym`
/// finds the same slot and overwrites the value/ignore.
#[test]
fn a_repeated_symbol_updates_the_same_slot() {
    let (out, status, err) = run(&["-DA", "-DA"], SOURCE);
    assert_eq!((out.as_str(), status), (RESOLVED_A, 1), "stderr 是 {err:?}");
    let (out, status, _err) = run(&["-DB", "-DA"], SOURCE);
    assert_eq!(
        (out.as_str(), status),
        ("int a;\nint b;\n", 1),
        "两个符号各管各的那一组"
    );
}

/// `-o` writes the result to a file and the exit status still reports whether
/// anything changed; this test only pins that standard output stays empty and the
/// file does hold content — the write-failure line lacks the `strerror` part and
/// is registered as `UNIFDEF-O-ERRNO-LEG`.
#[test]
fn the_output_operand_takes_the_text_and_stdout_stays_empty() {
    let dir = scratch("output");
    let out_path = dir.join("out.c");
    let path = out_path.to_str().unwrap();
    let (out, status, err) = run(&["-DA", "-o", path], SOURCE);
    assert_eq!((out.as_str(), status), ("", 1), "stderr 是 {err:?}");
    assert_eq!(err, "");
    assert_eq!(
        fs::read_to_string(&out_path).expect("read out.c"),
        RESOLVED_A
    );
}
