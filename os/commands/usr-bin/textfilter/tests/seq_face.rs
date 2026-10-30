//! End-to-end face for the `seq` binary, checked against
//! `minix3/usr.bin/seq/seq.c` (the option loop `:101-125`, the `errflg` exit
//! `:132-136`, operands and the two checks `:139-159`, the two `-f` checks
//! `:161-167`, `numeric()` `:195-232`, `valid_format()` `:231-287`,
//! `unescape()` `:293-376`, `e_atof()` `:383-400`) and the libc `getopt` it
//! drives (`minix3/lib/libc/stdlib/getopt.c:74-78`, `:81-85`, `:87-92`,
//! `:95-104`, `:112-141`).
//!
//! The loop at `seq.c:101` reads `while ((optind < argc) && !numeric(argv[optind])
//! && (c = getopt(argc, argv, "f:hs:t:w")) != -1)`. The reference binary
//! `tmp/seq-320/seq_ref` (the `seq.c` ground truth + Minix's `getopt.c` + a
//! program-name shim, linked against the host) measured these shapes:
//! 1. Three sentence families, two exit codes — `errflg` and the command's own
//!    `errx` exit **1**, while operands that fail `strtod` exit **2**
//!    (`seq 3x` is `seq: invalid floating point argument: 3x`, and `seq 1e310`
//!    is the three-part `seq: 1e310: <strerror(ERANGE)>`, whose last part per
//!    Minix's `sys_errlist` is `Result too large or too small` (this host's
//!    glibc table spells it `Numerical result out of range`, the same
//!    same-origin difference recorded earlier);
//! 2. usage is **one line**, and the program name follows `getprogname()`
//!    (`seq.c:133-136`);
//! 3. The loop **does not stop at the first bad letter**: `seq -+- 3` prints
//!    two libc lines (`+` and `-`) and then one usage line;
//! 4. `h` is in the optstring ⇒ libc prints nothing for it, and C's
//!    `case 'h'` has no `break`, falling straight into `default: errflg++` ⇒
//!    both `seq -h` and `seq -hw 3` give **only the usage line**;
//! 5. The `!numeric(argv[optind])` guard before the loop only stops operands
//!    starting with a dash (`seq -- -5` gets through), while `-5e310` counts
//!    as non-numeric ("a sign must follow `e` to be a number"), so inside the
//!    loop it splits into **five** unknown letters, one line each; a token not
//!    starting with `-` makes libc end the loop without advancing `optind`
//!    (`getopt.c:74-78`), so `seq 1e2` prints no line and is taken as an
//!    operand printing 1..100, and an empty token even counts as a number in C
//!    (`seq ""` prints `1\n0\n`);
//! 6. Operand parsing order is `last`→`first`→`incr`, so a `-w` left in place
//!    by the non-reordering loop is named first (`seq 3 -w` and
//!    `seq -w 3 -s ,` both exit 2);
//! 7. The zero-stride sentence's name follows `first < last`, and
//!    `first == last` falls into the `decrement` branch (`seq 2 0 2` →
//!    `zero decrement`);
//! 8. `-f`'s accepted set is counter-intuitive: only `A a E e F G g` count as
//!    conversions, so `%d` and `%d\n` are **both invalid**, while `abc` and
//!    `%%` are legal (zero conversions) and `%g%g` is invalid (two); the
//!    sentence is ``seq: invalid format string: `%q'`` (backtick plus single
//!    quote, status 1, no usage after it);
//! 9. `-w` has two levels: one occurrence pads with `0` (`seq -w 1 10` prints
//!    `01`…`10`), from two occurrences on it pads with spaces and right-aligns
//!    (`seq -w -w -1 1` prints `-1`/` 0`/` 1`); `-s`/`-t` values go through
//!    `unescape` (`\t`, `\101`→`A`, and an unrecognized `\q` keeps its
//!    backslash).
//!
//! **This side has no read/write seam defect** (it neither reads standard input
//! nor opens files; the same family's ten instances stop at `units`).
//! Float rendering and the output once `-f` takes effect remain an adjudicated
//! gap: the reference's `seq -f 'abc' 3` prints three `abc` lines, while this
//! shell returns one self-admitting explanation and exits 1 (registered as
//! `SEQ-FORMAT-RENDER`), so the two assertions below about accepted formats pin
//! **this shell's shape**, and where it differs is written in the comments.

mod face_support;

use face_support::write_stdin;
use std::process::{Command, Stdio};

const SEQ: &str = env!("CARGO_BIN_EXE_seq");
/// The line from `seq.c:133-136`; the program name follows `getprogname()`.
const USAGE: &str = "usage: seq [-w] [-f format] [-s string] [-t string] [first [incr]] last\n";

fn run(args: &[&str]) -> (String, i32, String) {
    let mut child = Command::new(SEQ)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the seq binary should be runnable");
    write_stdin(&mut child, b"");
    let output = child.wait_with_output().expect("the run should finish");
    (
        String::from_utf8_lossy(&output.stdout).into_owned(),
        output.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

/// The same command seam as `run`, but stderr comes back **byte by byte**:
/// `from_utf8_lossy` renders a bare `c3` as the three-byte U+FFFD, so the text
/// form cannot tell the "one byte" and "two bytes" mistakes apart.
fn run_bytes(args: &[&str]) -> (Vec<u8>, i32, Vec<u8>) {
    let mut child = Command::new(SEQ)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the seq binary should be runnable");
    write_stdin(&mut child, b"");
    let output = child.wait_with_output().expect("the run should finish");
    (
        output.stdout,
        output.status.code().unwrap_or(-1),
        output.stderr,
    )
}

/// Every unknown-letter line has **one bare byte** in its letter slot, and
/// `seq`'s loop accumulates ⇒ a multibyte letter comes back as **several
/// lines**. Expected values come shape by shape from the reference
/// `tmp/seq-320/seq_ref` (started through the same-named symlink `seq` this
/// round; its identity was verified earlier: it answers `-Z` with
/// `unknown option -- Z`, i.e. Minix's `getopt.c`): `-é` (`c3 a9`) gives
/// `seq: unknown option -- ` + `303` + newline, then another line with the same
/// sentence carrying `251`, then that one usage line, status 1; `-Zé` is three
/// lines (`Z`, `303`, `251`); `-aé` is three lines too (`a` is not in the
/// option string, so the loop keeps scanning). This is the same family
/// misjudgment corrected for `jot` — `seq` is the only accumulating loop in
/// this family, so do not carry over the "stops at the first bad letter"
/// conclusion. The ASCII shapes (`-Z`, `-+-`, `-ABC`, `-wZ`) already have
/// owners in this file and are not repeated.
#[test]
fn the_libc_lines_of_a_multibyte_flag_are_one_byte_each_for_seq() {
    let usage = USAGE.as_bytes();
    let cases: [(&str, &[&[u8]]); 3] = [
        ("-\u{e9}", &[b"\xc3", b"\xa9"]),
        ("-Z\u{e9}", &[b"Z", b"\xc3", b"\xa9"]),
        ("-a\u{e9}", &[b"a", b"\xc3", b"\xa9"]),
    ];
    for (form, letters) in cases {
        let mut want: Vec<u8> = Vec::new();
        for letter in letters {
            want.extend_from_slice(b"seq: unknown option -- ");
            want.extend_from_slice(letter);
            want.push(b'\n');
        }
        want.extend_from_slice(usage);
        let (out, code, err) = run_bytes(&[form]);
        assert_eq!(out, Vec::new(), "{form} 的报文只走标准错误");
        assert_eq!(code, 1, "{form} 的 stderr 是 {err:?}");
        assert_eq!(err, want, "{form} 的每一行字母位都该是一个裸字节");
    }
}

#[test]
fn off_letter_gets_one_libc_line_and_the_single_usage_line() {
    let (out, code, err) = run(&["-Z"]);
    assert_eq!(code, 1, "errflg 一族退 1（`seq.c:136`），参照件同值");
    assert_eq!(err, format!("seq: unknown option -- Z\n{USAGE}"));
    assert_eq!(out, "");
}

#[test]
fn the_ring_keeps_going_and_accumulates_the_libc_lines() {
    // The old implementation exited at the first bad letter and could print only
    // one libc line; the reference gives two lines for `-+- 3`.
    let (out, code, err) = run(&["-+-", "3"]);
    assert_eq!(code, 1);
    assert_eq!(out, "", "errflg 置起来后仍要先过操作数判据，不打任何序列");
    assert_eq!(
        err,
        format!("seq: unknown option -- +\nseq: unknown option -- -\n{USAGE}")
    );
    // In a cluster the valid letters are consumed first, and the bad one is
    // reported
    let (_, code, err) = run(&["-wZ"]);
    assert_eq!(code, 1);
    assert_eq!(err, format!("seq: unknown option -- Z\n{USAGE}"));
    let (_, code, err) = run(&["-Zw"]);
    assert_eq!(code, 1);
    assert_eq!(err, format!("seq: unknown option -- Z\n{USAGE}"));
    // Three bad letters ⇒ three lines (`tmp/seq-320/measure4.log`); usage is
    // still one line
    let (_, code, err) = run(&["-ABC", "3"]);
    assert_eq!(code, 1);
    assert_eq!(
        err,
        format!(
            "seq: unknown option -- A\nseq: unknown option -- B\nseq: unknown option -- C\n{USAGE}"
        )
    );
    // A value-taking letter without a value after a bad letter: both sentences
    // are libc's, in scan order (the reference measured the same for `-Zf`)
    let (_, code, err) = run(&["-Zf"]);
    assert_eq!(code, 1);
    assert_eq!(
        err,
        format!("seq: unknown option -- Z\nseq: option requires an argument -- f\n{USAGE}")
    );
}

#[test]
fn missing_value_is_reachable_for_f_s_and_t_only() {
    for letter in ['f', 's', 't'] {
        let flag = format!("-{letter}");
        let (out, code, err) = run(&[flag.as_str()]);
        assert_eq!(code, 1, "{flag}");
        assert_eq!(out, "", "{flag}");
        assert_eq!(
            err,
            format!("seq: option requires an argument -- {letter}\n{USAGE}"),
            "{flag}"
        );
    }
    // In the cluster `w` is consumed first, so the missing value is reported for
    // `f`
    let (_, code, err) = run(&["-wf"]);
    assert_eq!(code, 1);
    assert_eq!(
        err,
        format!("seq: option requires an argument -- f\n{USAGE}")
    );
    // `w`/`h` take no value ⇒ this branch is unreachable for them: `-w 3`
    // produces the sequence normally
    let (out, code, err) = run(&["-w", "3"]);
    assert_eq!((code, err), (0, String::new()));
    assert_eq!(out, "1\n2\n3\n");
}

#[test]
fn help_letter_is_in_the_set_and_prints_no_libc_line() {
    for args in [&["-h"][..], &["-h", "3"][..], &["-hw", "3"][..]] {
        let (out, code, err) = run(args);
        assert_eq!(code, 1, "{args:?}");
        assert_eq!(out, "", "{args:?}");
        assert_eq!(err, USAGE, "{args:?} 该只有 usage 一行，libc 那句到不了");
    }
}

#[test]
fn operand_count_face_is_usage_alone() {
    for args in [&[] as &[&str], &["-w"][..], &["1", "2", "3", "4"][..]] {
        let (out, code, err) = run(args);
        assert_eq!(code, 1, "{args:?}");
        assert_eq!(out, "", "{args:?}");
        assert_eq!(err, USAGE, "{args:?}");
    }
}

#[test]
fn every_byte_of_a_nonascii_cluster_is_reported() {
    // `-é` is two bytes (`c3 a9`), and the reference prints one line per
    // **byte** (`%c` prints the bare byte; `tmp/seq-320/measure3.log` shows
    // `\303` and `\251`). This shell was moved to that shape as well: it used to
    // render by `char` into the two printable characters `Ã`/`©` (two bytes
    // each); that sentence was registered as `SEQ-NONASCII-OPTLETTER` and is
    // **closed now**, so this test's expectation is **deliberately flipped** to
    // the per-byte `c3`/`a9` (the difference before and after the flip is the
    // whole point of this family: in text form `c3` becomes the replacement
    // character under `from_utf8_lossy` and no byte can be asserted, hence
    // `run_bytes` here).
    let (out, code, err) = run_bytes(&["-\u{e9}"]);
    assert_eq!(out, Vec::new());
    assert_eq!(code, 1);
    assert_eq!(
        err,
        [
            b"seq: unknown option -- \xc3\n".as_slice(),
            b"seq: unknown option -- \xa9\n".as_slice(),
            USAGE.as_bytes(),
        ]
        .concat()
    );
}

#[test]
fn a_token_without_a_leading_dash_ends_the_ring_without_any_line() {
    // The `!numeric(...)` guard before the loop only acts on tokens starting
    // with a dash: the guard judges `1e2` non-numeric (no sign after `e`), but
    // libc sees a first character that is not `-`, ends the loop without
    // advancing `optind` (`getopt.c:74-78`), and so it is an operand — the
    // reference's `seq 1e2` prints all of 1..100.
    let (out, code, err) = run(&["1e2"]);
    assert_eq!((code, err), (0, String::new()));
    let expected: String = (1..=100).map(|n| format!("{n}\n")).collect();
    assert_eq!(out, expected, "参照件 `seq 1e2` 打满 1..100");
    // An empty token is a "number" in C too (the `while (*s)` loop takes no step
    // and the criterion is `*s == '\0'`): `seq ""` means last=0 ⇒ it prints 1
    // and 0; the reference's byte output is `1\n0\n`
    let (out, code, err) = run(&[""]);
    assert_eq!((code, err), (0, String::new()));
    assert_eq!(out, "1\n0\n");
    let (out, code, err) = run(&["", "3"]);
    assert_eq!((code, err), (0, String::new()));
    assert_eq!(out, "0\n1\n2\n3\n");
}

#[test]
fn the_numeric_guard_lets_negative_operands_through() {
    // The guard fails to recognize `-5e310` (no sign after `e`), so it enters
    // the loop and is split into unknown letters; the reference has the same
    // shape with **one line per letter, five lines**
    // (`tmp/seq-320/measure4.log`)
    let (_, code, err) = run(&["-5e310"]);
    assert_eq!(code, 1);
    assert_eq!(
        err,
        format!(
            "seq: unknown option -- 5\nseq: unknown option -- e\nseq: unknown option -- 3\n\
             seq: unknown option -- 1\nseq: unknown option -- 0\n{USAGE}"
        )
    );
    // `-5` is a number ⇒ a plain operand: C prints 1 0 -1 -2 -3 -4 -5
    let (out, code, err) = run(&["-5"]);
    assert_eq!((code, err), (0, String::new()));
    assert_eq!(out, "1\n0\n-1\n-2\n-3\n-4\n-5\n");
    // `--` is consumed, leaving `-- -Z` with one bad operand
    let (_, code, err) = run(&["--", "-Z"]);
    assert_eq!(code, 2);
    assert_eq!(err, "seq: invalid floating point argument: -Z\n");
}

#[test]
fn operand_errors_take_the_last_operand_first_and_exit_two() {
    // No reordering in the loop: both tokens of `seq 3 -w` are operands, and C
    // computes `last` first ⇒ it names `-w`.
    for (args, word) in [
        (&["3", "-w"][..], "-w"),
        (&["-w", "3", "-s", ","][..], ","),
        (&["3x"][..], "3x"),
    ] {
        let (out, code, err) = run(args);
        assert_eq!(code, 2, "{args:?} 走 e_atof 那一族");
        assert_eq!(out, "", "{args:?}");
        assert_eq!(
            err,
            format!("seq: invalid floating point argument: {word}\n"),
            "{args:?}"
        );
    }
    // The ERANGE arm is the three-part `err(2, "%s", num)` shape, with strerror:
    // overflow and "underflow to 0" share one exit (the reference exits 2 for
    // each of `1e310` and `1e-40000`, `measure3.log`)
    for word in ["1e310", "1e-40000"] {
        let (out, code, err) = run(&[word]);
        assert_eq!(code, 2, "{word}");
        assert_eq!(out, "", "{word}");
        assert_eq!(
            err,
            // The last part comes from Minix's `sys_errlist` (entry 34 in
            // `compat_errlist.c` is `Result too large or too small`); the
            // host-linked reference goes through glibc's table and prints
            // `Numerical result out of range` — the same same-origin difference
            // recorded earlier.
            format!("seq: {word}: Result too large or too small\n"),
            "{word}"
        );
    }
}

#[test]
fn zero_stride_sentence_follows_the_direction() {
    // `(first < last) ? "in" : "de"`: equality also takes the de branch (the
    // reference agrees for `seq 2 0 2`)
    for (args, name) in [
        (&["1", "0", "3"][..], "increment"),
        (&["5", "0", "1"][..], "decrement"),
        (&["2", "0", "2"][..], "decrement"),
    ] {
        let (out, code, err) = run(args);
        assert_eq!(code, 1, "{args:?}");
        assert_eq!(out, "", "{args:?}");
        assert_eq!(err, format!("seq: zero {name}\n"), "{args:?}");
    }
    // The two wrong-direction checks come after the zero-stride one
    let (_, code, err) = run(&["1", "-1", "5"]);
    assert_eq!(
        (code, err),
        (1, "seq: needs positive increment\n".to_string())
    );
    let (_, code, err) = run(&["5", "1", "1"]);
    assert_eq!(
        (code, err),
        (1, "seq: needs negative decrement\n".to_string())
    );
}

#[test]
fn format_validation_rejects_integer_conversions_with_the_named_sentence() {
    // Reference measurements: `%d`, `%d\n`, `%d%d`, `%g%g`, `%q` are all
    // invalid; the sentence carries the original text and is **not** followed by
    // usage
    for fmt in ["%d", "%d\n", "%d%d", "%g%g", "%q", "%"] {
        let (out, code, err) = run(&["-f", fmt, "3"]);
        assert_eq!(code, 1, "{fmt:?}");
        assert_eq!(out, "", "{fmt:?}");
        assert_eq!(
            err,
            format!("seq: invalid format string: `{fmt}'\n"),
            "{fmt:?}"
        );
    }
}

#[test]
fn accepted_formats_reach_the_float_render_gap_the_engine_declares() {
    // The reference produces numbers as usual for `%g`/`abc`/`%%` (`abc` prints
    // three literal lines); this shell handles integers only. That difference is
    // registered as SEQ-FORMAT-RENDER, and these assertions pin this shell's
    // self-admitted shape.
    let (out, code, err) = run(&["-f", "%g", "3"]);
    assert_eq!(code, 1);
    assert_eq!(out, "");
    assert_eq!(err, "seq: -f formats wait for the float renderer\n");
    let (_, code, _) = run(&["-f", "abc", "3"]);
    assert_eq!(code, 1, "零转换的串在 C 里合法，走到同一处空白");
}

#[test]
fn equalize_has_two_levels_and_the_second_switches_to_spaces() {
    let (out, code, err) = run(&["-w", "1", "10"]);
    assert_eq!((code, err), (0, String::new()));
    assert_eq!(out, "01\n02\n03\n04\n05\n06\n07\n08\n09\n10\n");
    // The space level is `% 2g`: a non-negative number **still occupies a sign
    // slot**, so `10` prints as the three-column `" 10"` (over-wide values are
    // not truncated) rather than right-aligned to two columns.
    // `tmp/seq-320/measure6.log` measured this byte by byte.
    let (out, code, err) = run(&["-w", "-w", "1", "10"]);
    assert_eq!((code, err), (0, String::new()));
    assert_eq!(out, " 1\n 2\n 3\n 4\n 5\n 6\n 7\n 8\n 9\n 10\n");
    let (out, code, err) = run(&["-w", "-w", "8", "10"]);
    assert_eq!((code, err), (0, String::new()));
    assert_eq!(out, " 8\n 9\n 10\n");
    // The width comes from the **actually printed** first and last values:
    // `seq -w 1 5 10` prints only 1 and 6 ⇒ width 1
    let (out, code, err) = run(&["-w", "1", "5", "10"]);
    assert_eq!((code, err), (0, String::new()));
    assert_eq!(out, "1\n6\n");
    let (out, code, err) = run(&["-w", "-w", "1", "5", "10"]);
    assert_eq!((code, err), (0, String::new()));
    assert_eq!(out, " 1\n 6\n");
    // Descending works the same: `seq -w 10 -3 1` prints 10 7 4 1, and the
    // width comes from first=10 ⇒ zero-padded to two columns
    let (out, code, err) = run(&["-w", "10", "-3", "1"]);
    assert_eq!((code, err), (0, String::new()));
    assert_eq!(out, "10\n07\n04\n01\n");
    let (out, code, err) = run(&["-w", "-w", "10", "-3", "1"]);
    assert_eq!((code, err), (0, String::new()));
    assert_eq!(out, " 10\n 7\n 4\n 1\n");
    // Sign slots at each level across a range spanning zero: `0` is `00` under
    // `%02g` and `" 0"` under `% 2g`
    let (out, code, err) = run(&["-w", "-5", "5"]);
    assert_eq!((code, err), (0, String::new()));
    assert_eq!(out, "-5\n-4\n-3\n-2\n-1\n00\n01\n02\n03\n04\n05\n");
    let (out, code, err) = run(&["-w", "-w", "-5", "5"]);
    assert_eq!((code, err), (0, String::new()));
    assert_eq!(out, "-5\n-4\n-3\n-2\n-1\n 0\n 1\n 2\n 3\n 4\n 5\n");
    // The three-wide space level: `-9` yields `" -9"` (two columns) while `0`
    // yields `"  0"` (three columns) — the width comes from `-10`, and the sign
    // slot and the alignment padding are two different things
    let (out, code, err) = run(&["-w", "-w", "-10", "10"]);
    assert_eq!((code, err), (0, String::new()));
    assert_eq!(
        out,
        "-10\n -9\n -8\n -7\n -6\n -5\n -4\n -3\n -2\n -1\n  0\n  1\n  2\n  3\n  4\n  5\n  6\n  7\n  8\n  9\n 10\n"
    );
    // A third level is still spaces, with the sign taking one column
    let (out, code, err) = run(&["-w", "-w", "-w", "-1", "1"]);
    assert_eq!((code, err), (0, String::new()));
    assert_eq!(out, "-1\n 0\n 1\n");
}

#[test]
fn separator_and_terminator_values_pass_through_unescape() {
    let (out, code, err) = run(&["-s", "\\t", "1", "3"]);
    assert_eq!((code, err), (0, String::new()));
    assert_eq!(
        out, "1\t2\t3\t",
        "C 的 `sep = unescape(optarg)`，每个数之后都放分隔符"
    );
    let (out, code, err) = run(&["-t", "\\a", "3"]);
    assert_eq!((code, err), (0, String::new()));
    assert_eq!(out, "1\n2\n3\n\u{7}", "terminator 只加在最后一个之后");
    // An unrecognized escape keeps the backslash; octal `\101` is `A`
    let (out, _, _) = run(&["-s", "\\q", "3"]);
    assert_eq!(out, "1\\q2\\q3\\q");
    let (out, _, _) = run(&["-s", "\\101", "3"]);
    assert_eq!(out, "1A2A3A");
}

#[test]
fn plain_sequences_keep_the_direction_following_default_stride() {
    for (args, want) in [
        (vec!["3"], "1\n2\n3\n"),
        (vec!["1", "3"], "1\n2\n3\n"),
        (vec!["1", "2", "6"], "1\n3\n5\n"),
        (vec!["3", "1"], "3\n2\n1\n"),
        (vec!["0"], "1\n0\n"),
    ] {
        let (out, code, err) = run(&args);
        assert_eq!(
            (code, err.clone()),
            (0, String::new()),
            "{args:?} → stderr={err}"
        );
        assert_eq!(out, want, "{args:?}");
    }
}

/// The same process seam as `run_bytes`, but with **raw bytes** as arguments: a
/// `&str` cannot hand over a bare `ff`, so the shape after argv byte-ification
/// can only be measured this way. Standard input is fed per the fixture
/// convention; a failed write is not counted as a defect of the code under test.
fn run_raw(args: &[Vec<u8>], input: &[u8]) -> (Vec<u8>, i32, Vec<u8>) {
    use std::ffi::OsString;
    use std::os::unix::ffi::OsStringExt as _;
    let mut child = Command::new(SEQ)
        .args(args.iter().map(|word| OsString::from_vec(word.clone())))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the seq binary should be runnable");
    {
        use std::io::Write as _;
        if let Some(mut stdin) = child.stdin.take() {
            let _ = stdin.write_all(input);
        }
    }
    let output = child.wait_with_output().expect("the run should finish");
    (
        output.stdout,
        output.status.code().expect("被信号杀掉就是双生面又分叉了"),
        output.stderr,
    )
}

/// `-s`/`-t` values are **output content**: a bare `ff` separator goes out as one
/// byte. Expectations come shape by shape from measurements of the reference
/// `tmp/seq-320/seq_ref` (`tmp/argv-443/ab-after.log`): `-s<ff>` gives
/// `1\xff2\xff3\xff` (a separator after every number), and `-t<ff>` gives
/// `1\n2\n3\n\xff` (appended only at the end). The old shell printed the three
/// bytes `ef bf bd` for the same shape — the symptom of one lossy fold in
/// `bin_support::args`.
#[test]
fn a_raw_nonutf8_separator_byte_is_written_as_one_byte() {
    let (out, code, err) = run_raw(&[b"-s\xff".to_vec(), b"1".to_vec(), b"3".to_vec()], b"");
    assert_eq!((code, err), (0, Vec::new()));
    assert_eq!(out, b"1\xff2\xff3\xff");
    let (out, code, err) = run_raw(&[b"-t\xff".to_vec(), b"1".to_vec(), b"3".to_vec()], b"");
    assert_eq!((code, err), (0, Vec::new()));
    assert_eq!(out, b"1\n2\n3\n\xff");
    // A value taken from the next token is handled byte-exactly too
    let (out, code, err) = run_raw(
        &[
            b"-s".to_vec(),
            b"\xff".to_vec(),
            b"1".to_vec(),
            b"3".to_vec(),
        ],
        b"",
    );
    assert_eq!((code, err), (0, Vec::new()));
    assert_eq!(out, b"1\xff2\xff3\xff");
}

/// An operand that fails `e_atof()` is echoed by `%s` **as raw bytes**: for
/// `seq $'1\xff3'` the reference prints
/// `seq: invalid floating point argument: 1\xff3` and exits 2. This shape pins
/// two things at once — the numeric grammar is ASCII (that byte necessarily
/// lands in "trailing garbage"), and the error hands back the bytes from the
/// command line, not the replacement character after a lossy conversion.
#[test]
fn a_raw_nonutf8_operand_is_echoed_as_its_own_bytes() {
    let (out, code, err) = run_raw(&[b"1\xff3".to_vec()], b"");
    assert_eq!(code, 2, "实际 stderr {err:?}");
    assert_eq!(out, Vec::new());
    assert_eq!(err, b"seq: invalid floating point argument: 1\xff3\n");
}
