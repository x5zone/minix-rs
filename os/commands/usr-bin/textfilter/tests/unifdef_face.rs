//! End-to-end face for the `unifdef` binary, checked against
//! `minix3/usr.bin/unifdef/unifdef.c` (option ring `:273-323`, symbol test `addsym`
//! `:965-991`, operands and open `:327-341`, `usage()` `:373-379`) and the libc
//! `getopt` it drives (`minix3/lib/libc/stdlib/getopt.c:74-78`, `:81-92`,
//! `:95-104`, `:118-141`).
//!
//! `unifdef.c:273`'s optstring is `"i:D:U:I:o:cdeklst"`: `D`/`U`/`i`/`I`/`o` take a
//! value and `c`/`d`/`e`/`k`/`l`/`s`/`t` do not, and case is part of the letter set
//! (`-uD`'s `u` is off the string, as is `-tE`'s `E`). There are three exits, and
//! **this shell's usage exits 2** (`exit(2)`, not the 1 its family usually uses):
//! (1) an off-letter and a missing value each get one libc line first, then
//!     `default: usage()` adds one more;
//! (2) an `-i` whose following letter is not `D`/`U`, or a `-D`/`-U` symbol name
//!     that does not fit `addsym`'s shape — C **prints the usage line alone** in
//!     both places, with no sentence of its own (this shell used to invent
//!     `-i must be followed by D or U` and `bad symbol spec …` there, one line too
//!     many);
//! (3) after the ring ends, "no symbol at all and no `-s`" gets a `warnx` line plus
//!     two usage lines; this branch runs before the operand check, so
//!     `unifdef -- -z` reports it rather than "only one file".
//!
//! Three more places only show up end to end: `-D ''` and `-U ''` are **valid** empty
//! symbol names (`skipsym` consumes no character and returns to its start, so
//! `*val == '\0'`), and the whole file passes through unchanged with exit 0; only
//! one operand is allowed and the ring **does not rearrange** (`unifdef -DA in.c -c`
//! takes `-c` as a second operand and exits 2 instead of consuming it as a flag); an
//! operand that cannot be opened is the three-part `err(2, "can't open %s", …)`.
//! The `-i` branch consumes one byte first and then looks at the rest, so `-iDA`'s
//! symbol is `A` (byte-for-byte the same as `-D A`), not the whole string `DFOO`.
//!
//! Neither the control-symbol list `-s` prints nor `-d`'s debug lines are asserted by
//! this face: the first is the `printf` inside `findsym` (`unifdef.c:948-949`, the
//! only print point under `-s`) and the second is `debug()` flushing to stderr. Both
//! belong to the deciding half, registered as `UNIFDEF-SYMLIST-OUTPUT` and
//! `UNIFDEF-D-DEBUG-OUTPUT`. When a kept line contains raw bytes, the reference passes
//! them through while this shell returns replacement characters, registered as
//! `UNIFDEF-LINE-CONTENT-TEXTSEAM` (measured in `tmp/unifdef-471/content-seam.log`).

mod face_support;

use face_support::write_stdin;
use std::fs;
use std::path::PathBuf;
use std::process::{Command, Stdio};

const UNIFDEF: &str = env!("CARGO_BIN_EXE_unifdef");
const USAGE: &str = "usage: unifdef [-cdeklst] [-o output] [-Dsym[=val]] [-Usym] [-iDsym[=val]] [-iUsym] ... [file]\n";
const SOURCE: &[u8] = b"#ifdef A\nint a;\n#endif\n#ifdef B\nint b;\n#endif\n";
/// `-DA` resolves the A group: `#ifdef A` and its `#endif` disappear, `int a;`
/// stays, and the B group is left alone.
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

/// The same command seam as `run`, but **byte for byte**: `from_utf8_lossy` renders
/// a bare `c3` as the three-byte U+FFFD, so the text form cannot tell the
/// "one byte" mistake from the "two bytes" one (the same holds for `rev`/`paste`/`jot`).
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

/// The letter slot of the off-letter line is **one raw byte**: the reference binary
/// `tmp/unifdef-318/unifdef_ref` measured `-é` and `-cé` both as
/// `unifdef: unknown option -- ` + `c3` + newline followed by usage, exit 2
/// (`c` is a valid valueless letter in `"i:D:U:I:o:cdeklst"`, so the cluster keeps
/// consuming and reaches the first byte of `é`; `-Zé` stops at `Z`, because
/// `unifdef.c:322-323` calls `usage()` on `'?'`). The ASCII `-z` form and the
/// "cluster reports only the first letter" form already have their own owners in
/// this file (`an_unknown_letter_prints_the_libc_line_then_the_usage`,
/// `the_case_of_a_letter_is_part_of_the_option_set`), so the redundancy rule keeps
/// them from being duplicated here.
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

/// An off-letter: the libc line comes first, usage second, exit **2**
/// (`unifdef.c:322-323` plus `:378`).
#[test]
fn an_unknown_letter_prints_the_libc_line_then_the_usage() {
    let (out, status, err) = run(&["-z"], SOURCE);
    assert_eq!(out, "");
    assert_eq!(status, 2, "stderr 是 {err:?}");
    assert_eq!(err, format!("unifdef: unknown option -- z\n{USAGE}"));
}

/// A cluster reports only its first bad letter; letter case is part of the set, and
/// neither `u` nor `E` is in the string.
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

/// All five valued letters have the same missing-value form (including `-cD` and
/// `-lI`, where the cluster ends at the letter): the libc line plus usage, exit 2.
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

/// When `-i`'s following character is not `D`/`U`, C only calls `usage()`: no libc
/// line and no invented sentence. An empty value (`-i` with an empty token) has the
/// same shape, because `*optarg` is already `\0`.
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

/// `addsym`'s shape test (`unifdef.c:978-990`): `-D` allows `sym`, `sym=` and
/// `sym=val`, `-U` allows only `sym`; anything after the name that is neither the end
/// nor `=` is a bad shape, and a bad shape prints the usage line alone.
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

/// An empty symbol name is valid: `skipsym("")` returns in place, so `*val == '\0'`
/// takes the "empty value" branch. This test guards against the old shape that
/// rejected an empty name as bad — C accepts it and passes the whole file through
/// unchanged with exit 0.
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

/// No symbol at all: the `warnx` line plus usage, exit 2; this branch runs **before**
/// the operand check, so `-- -z` (both tokens after the ring) still reports it.
/// Giving `-s` removes the need for a symbol (list mode).
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
    // A lone `-s` is valid: this test asserts only that the warning is not taken;
    // the list content belongs to the deciding half's registered item.
    let (_, status, err) = run(&["-s"], SOURCE);
    assert!(!err.contains("must -D or -U"), "`-s` 不该撞那句: {err:?}");
    assert_eq!(status, 0);
}

/// Only one operand is allowed, and the ring **does not rearrange**: the `-c` after
/// the first non-option token is a second operand (the reference gives
/// `unifdef -DA in.c -c` → `can only do one file`), a single `errx` line.
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

/// An operand that cannot be opened is the three-part
/// `err(2, "can't open %s", filename)`: program name, operand text and `strerror`
/// sentence, all three required (the old shape dropped the third).
#[test]
fn an_unopenable_operand_is_a_three_part_error() {
    let (out, status, err) = run(&["-DA", "./nope-318.c"], SOURCE);
    assert_eq!((out.as_str(), status), ("", 2));
    assert_eq!(
        err, "unifdef: can't open ./nope-318.c: No such file or directory\n",
        "三段形：程序名、操数原文、`strerror` 句子"
    );
}

/// A bare `-` is an alias for standard input (`unifdef.c:333`'s
/// `strcmp(*argv, "-") != 0`) and behaves like no operand at all; this is the read
/// seam of this shell — it used to call bare `minix_sys::read`, which cannot read on
/// the host.
#[test]
fn a_dash_operand_reads_stdin_like_no_operand_at_all() {
    let (out, status, err) = run(&["-DA"], SOURCE);
    assert_eq!((out.as_str(), status), (RESOLVED_A, 1), "stderr 是 {err:?}");
    assert_eq!(err, "");

    let (out, status, err) = run(&["-DA", "-"], SOURCE);
    assert_eq!((out.as_str(), status), (RESOLVED_A, 1), "stderr 是 {err:?}");
    assert_eq!(err, "");
}

/// `-i` consumes one character before taking the symbol name (`opt = *optarg++`),
/// so `-iDA`'s symbol is `A`; two unrelated symbol names (`DFOO`) match neither
/// branch, so the whole file passes through with exit 0.
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

    // `-iD=1`'s symbol name is the **start** of that segment (here the start is `=`,
    // with no identifier character at all), so it matches nothing: the whole file
    // passes through with exit 0. The two sides agree byte for byte on this form
    // (measured against the reference).
    let (out, status, _err) = run(&["-iD=1"], SOURCE);
    assert_eq!(
        (out.as_str(), status),
        (std::str::from_utf8(SOURCE).unwrap(), 0),
        "`-iD=1` 是合法的「空名字挂个值」，不是坏形"
    );
}

/// Repeating a symbol neither errors nor stacks two entries: C's `findsym` finds the
/// same slot and overwrites its value/ignore.
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

/// `-o` writes the result into a file and the status still reports whether anything
/// changed; this test pins only that standard output stays empty and that the file
/// really has content. The write-failure sentence lacks its `strerror` segment,
/// registered as `UNIFDEF-O-ERRNO-LEG`.
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

/// The same command seam as `run_bytes`, but **argv is handed over as raw bytes**: a
/// bare `0xff` cannot enter a `&str`. It runs in its own scratch directory (the `-o`
/// form leaves a file on disk) and returns the exit status, stderr byte for byte,
/// stdout byte for byte, and the file names in the directory (as bytes).
fn run_raw(args: &[&[u8]], input: &[u8]) -> (i32, Vec<u8>, Vec<u8>, Vec<Vec<u8>>) {
    use std::ffi::OsStr;
    use std::os::unix::ffi::OsStrExt as _;
    static SEQ: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let seq = SEQ.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    let dir = std::env::temp_dir().join(format!("unifdef-raw-{}-{}", std::process::id(), seq));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("scratch dir");
    std::fs::write(dir.join("in.txt"), b"#ifdef A\nkeep\n#endif\n").expect("in.txt");
    let mut child = Command::new(UNIFDEF)
        .args(args.iter().map(|a| OsStr::from_bytes(a)))
        .current_dir(&dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the unifdef binary should be runnable");
    face_support::write_stdin(&mut child, input);
    let output = child.wait_with_output().expect("the run should finish");
    let mut names: Vec<Vec<u8>> = fs::read_dir(&dir)
        .expect("the scratch dir should be readable")
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.file_name().as_bytes().to_vec())
        .collect();
    names.sort();
    let read = (
        output.status.code().unwrap_or(-1),
        output.stderr,
        output.stdout,
        names,
    );
    let _ = std::fs::remove_dir_all(&dir);
    read
}

/// The byte readings on the argument seam (`tmp/unifdef-471/measure.log`: thirteen
/// forms; 9/13 matched the reference byte for byte before the fix and 11/13 after).
/// The two that came right are the letter slot and the `can't open` sentence for an
/// **input operand**; `-o`'s name slot now also carries bytes (the
/// `Undefined error: 0` segment is the existing difference where `write_file` has no
/// errno channel, registered as `UNIFDEF-O-ERRNO-LEG`). Three forms are **verified to
/// agree** now: `-D<ff>A` and `-i<ff>` both land on the usage line alone (C's
/// `skipsym` and `opt = *optarg++` accept ASCII only), and `-DA=<ff>` is accepted on
/// both sides. The last form pins the on-disk name: the reference writes
/// `out` + `ff` + `.c`, while this shell writes `out` + `ef bf bd` + `.c` (the
/// `&str` open entry, registered under `ARGV-BYTE-PATH-IN-LIBS`).
#[test]
fn the_operand_and_output_names_carry_the_argv_bytes() {
    let (status, err, out, names) = run_raw(&[b"-\xff"], b"");
    assert_eq!(
        status, 2,
        "unifdef 的 usage 退的是 2 而不是 1（`unifdef.c:378`）"
    );
    assert_eq!(
        err,
        [
            b"unifdef: unknown option -- \xff\n".as_slice(),
            USAGE.as_bytes()
        ]
        .concat()
    );
    assert_eq!(out, Vec::new());
    assert!(names.iter().all(|n| n == b"in.txt"), "报错时不落任何新档");

    let (status, err, _out, _names) = run_raw(&[b"-DA", b"no\xffx"], b"");
    assert_eq!(status, 2);
    assert_eq!(
        err,
        [
            b"unifdef: can't open no\xffx: ".as_slice(),
            b"No such file or directory\n".as_slice()
        ]
        .concat(),
        "操数进的是 `err(2, \"can't open %s\", filename)` 的名字位"
    );

    // `-o`'s name slot carries bytes; the sentence segment is still this layer's
    // missing-errno reading (see the test comment above).
    let (status, err, _out, _names) = run_raw(&[b"-DA", b"-o", b"d\xff/x", b"in.txt"], b"");
    assert_eq!(status, 2);
    assert_eq!(
        err,
        [
            b"unifdef: can't open d\xff/x: ".as_slice(),
            b"Undefined error: 0\n".as_slice()
        ]
        .concat()
    );

    // Three forms agree: the usage line alone, no libc line first, and no invented
    // sentence.
    for flag in [b"-D\xffA".as_slice(), b"-i\xff".as_slice()] {
        let (status, err, _out, _names) = run_raw(&[flag], b"");
        assert_eq!(status, 2, "{flag:?}");
        assert_eq!(
            err,
            USAGE.as_bytes(),
            "{flag:?} 只打 usage 一行（C 的 `skipsym` 与 `*optarg++` 都只认 ASCII）"
        );
    }
    let (status, err, out, _names) = run_raw(&[b"-DA=\xff", b"in.txt"], b"");
    assert_eq!(status, 1, "值位带裸字节不影响处理：`-DA=\\xff` 两侧都接受");
    assert_eq!(err, Vec::new());
    assert_eq!(out, b"keep\n");

    // The registered gap: the output name on disk is still the folded three bytes.
    let (status, _err, _out, names) = run_raw(&[b"-DA", b"-o", b"out\xff.c", b"in.txt"], b"");
    assert_eq!(status, 1);
    let mut want = names.clone();
    want.retain(|n| n != b"in.txt");
    assert_eq!(
        want,
        vec![b"out\xef\xbf\xbd.c".to_vec()],
        "参照件这一形落的是 out+ff+.c"
    );
}
