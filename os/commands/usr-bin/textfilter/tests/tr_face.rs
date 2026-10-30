//! End-to-end face for the `tr` binary, checked against
//! `minix3/usr.bin/tr/tr.c` (the option loop `:68`, `case '?': default:
//! usage();` `:79-81`, the arity checks `:86-97`, `-ds` requiring string2
//! `:104-106`, `-d` forbidding string2 `:123-125`, single-set squeeze
//! `:139-140`, the translation path requiring two sets `:156-157`,
//! `empty string2` `:187-188`, `usage()` `:276-282`) and the libc `getopt` it
//! drives (`minix3/lib/libc/stdlib/getopt.c:74-78`, `:80-85`, `:86-93`,
//! `:94-95`, `:98-106`).
//!
//! Every expected value below was measured from the reference binary
//! `tmp/tr-322/tr_ref`; the shape-by-shape reconciliation table is in
//! `tmp/tr-322/ab322.log` (SAME=62 / DIFF=3, the three shapes all registered as
//! `TR-SET-GRAMMAR-SENTENCES`). Six shapes were measured:
//! 1. The optstring `"cds"` has **no letter taking a value**, so libc's
//!    "option requires an argument" is unreachable on this side; and `:79-81`
//!    sends `?` and `default` straight into `usage()`, exiting the loop on the
//!    spot ⇒ an unknown letter produces **one line** only of libc text (`-ZQ`
//!    reports only `Z`, `-Q -Z a` only `Q`), unlike `seq`'s per-letter
//!    accumulation (`jot` takes this same shape).
//! 2. `--s a` reports the letter `-`: `getopt.c:79` takes the first byte after
//!    the dash, `:80-85` recognizes as the terminator **exactly** two dashes,
//!    so a non-empty third byte keeps the walk going, and `:98` finds no `-` in
//!    `"cds"` ⇒ `unknown option -- -`.
//! 3. A bare `-` is neither an option nor part of the operand cursor: `:86-93`
//!    leaves `-` among the operands, so `tr - a` maps `-` to `a`, and `tr - -`
//!    copies its input through.
//! 4. The four usage lines hardcode the program name in the string
//!    (`:278-281`), while the libc line's name follows `argv[0]`
//!    (`getprogname()` at `getopt.c:103`) — two lines of different origin in
//!    one command, which
//!    `the_libc_line_follows_argv_zero_but_the_block_is_hardwired` pins.
//! 5. `empty string2` appears only on the translation path and is **not**
//!    followed by usage (`:187-188`); neither `-d ''` nor `-ds a ''` triggers
//!    it.
//! 6. This side has no read/write-operand leg (data comes only from standard
//!    input), so the "thin shell's bare `read` cannot read input on the host"
//!    family stops at `paste`'s eleven sites.
//!
//! The six set-grammar sentences are all wired in this shell (`genclass()`,
//! `genseq()`, `genequiv()` in `str.c`); expected values come from measurements
//! of the reference: `tr_ref '[x*3]' y` and `tr_ref '[ab*2]' y` both print
//! `Sequences only valid in string2`, `tr_ref abc '[ab*2]'` prints `Misplaced
//! sequence asterisk`, `tr_ref abc '[x*1a]'` prints `illegal sequence count`,
//! `tr_ref [:bogus:] y` prints `unknown class bogus`, `tr_ref '[=ab=]' y` prints
//! `Misplaced equivalence equals sign`, and `tr_ref '[=a=b=]' y` prints
//! `Misplaced equivalence right bracket`; all exit 1 and are **not** followed
//! by the usage block. The same round copied `bracket()`'s criteria too
//! (`str.c:176-210`): the "not formed" expressions `[x]`, `[:alpha`, `[x`,
//! `[x]y*` are **literals** in the reference and exit 0.
//! `tr.c`'s OOBCH leg (registered as `TR-EQUIV-OOBCH-SET2`) is now wired as
//! well: when string2 ends in a character class (`[:alpha:]`/`[=c=]`), what
//! repeats after exhaustion is `OOBCH` (256), truncated to `\0` by `putchar`;
//! only when it ends in an ordinary character, a range, or `[x*n]` does the
//! last byte repeat.
//! The `-s` leg has one more rule: C maps and squeezes in **one pass**
//! (`tr.c:240-246`) with the criterion in the `int` domain, so two adjacent
//! OOBCH-NULs squeeze into one, while a NUL already present in the input takes
//! no part in squeezing.

mod face_support;

use face_support::write_stdin;
use std::os::unix::process::CommandExt;
use std::process::{Command, Stdio};

const TR: &str = env!("CARGO_BIN_EXE_tr");
/// `tr.c:276-282`: the whole four-line block, the last three lines aligned with
/// seven spaces, the program name hardcoded as `tr`.
const USAGE: &str = concat!(
    "usage: tr [-cs] string1 string2\n",
    "       tr [-c] -d string1\n",
    "       tr [-c] -s string1\n",
    "       tr [-c] -ds string1 string2\n"
);

fn run(args: &[&str], input: &[u8]) -> (String, i32, String) {
    let mut child = Command::new(TR)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the tr binary should be runnable");
    write_stdin(&mut child, input);
    let output = child.wait_with_output().expect("the run should finish");
    (
        String::from_utf8_lossy(&output.stdout).into_owned(),
        output.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

/// A fixture that compares stderr byte by byte: this family must pin "the
/// letter slot is one bare byte", and `String::from_utf8_lossy` reads both a
/// bare `c3` and `c3 83` as the replacement character.
fn run_bytes(args: &[&str], input: &[u8]) -> (Vec<u8>, i32, Vec<u8>) {
    let mut child = Command::new(TR)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the tr binary should be runnable");
    write_stdin(&mut child, input);
    let output = child.wait_with_output().expect("the run should finish");
    (
        output.stdout,
        output.status.code().unwrap_or(-1),
        output.stderr,
    )
}

/// The letter slot of the unknown-letter line is **one bare byte**: the
/// reference measured both `tr -é a b` and `tr -cé a b` printing
/// `tr: unknown option -- ` + `c3` + newline, then the four-line usage block,
/// status 1 (the `default: usage()` at `tr.c:79-81`). `c` is a valid letter, so
/// the cluster keeps walking to the first byte of `é`. The ASCII shapes `-Z`
/// and `-dZ` are pinned at the same time: a change of rendering must not alter
/// them, nor squeeze out the usage block.
#[test]
fn an_off_letter_of_a_multibyte_flag_prints_one_raw_byte() {
    let want: Vec<u8> = [
        b"tr: unknown option -- ".as_slice(),
        b"\xc3\n",
        USAGE.as_bytes(),
    ]
    .concat();
    for form in ["-\u{e9}", "-c\u{e9}"] {
        let (out, code, err) = run_bytes(&[form, "a", "b"], b"");
        assert_eq!(out, Vec::new(), "{form} 的报文只走 stderr");
        assert_eq!(code, 1, "{form} 的 stderr 是 {err:?}");
        assert_eq!(err, want, "{form} 的字母该是一个裸字节 c3");
    }

    let (_out, code, err) = run_bytes(&["-Z", "a", "b"], b"");
    assert_eq!(code, 1);
    assert_eq!(
        err,
        [b"tr: unknown option -- Z\n".as_slice(), USAGE.as_bytes(),].concat(),
        "ASCII 那一形两个入口一字不差"
    );
}

#[test]
fn usage_block_is_four_lines_on_stderr() {
    // For every shape where the flags alone have no operands, the reference
    // exits with just this block: rc=1, empty stdout, stderr exactly four lines
    for args in [
        &[] as &[&str],
        &["-"],
        &["a"],
        &["-c"],
        &["-d"],
        &["-s"],
        &["-ds"],
        &["-cd"],
        &["-cs"],
        &["-dc"],
    ] {
        let (out, code, err) = run(args, b"abc\n");
        assert_eq!(code, 1, "{args:?}");
        assert_eq!(out, "", "{args:?} 诊断不许走标准输出");
        assert_eq!(err, USAGE, "{args:?}");
    }
}

#[test]
fn operand_count_and_combination_all_say_usage_alone() {
    // The arity checks `:86-97` and the two combination checks `:104-106`,
    // `:123-125` all sit **outside** the loop, so these shapes get only usage,
    // without the libc line
    for args in [
        &["a", "b", "c"][..],      // three operands
        &["a", "b", "c", "d"][..], // four
        &["--", "a"][..],          // after `--` is consumed only one remains
        &["a", "-", "b"][..],      // the loop stops at `a`, so `-` is the second operand ⇒ three
        &["a", "b", "-c"][..],     // no reordering: `-c` is an operand too
        &["-d", "a", "b"][..],     // `-d` with one operand too many (`:123-125`)
        &["-dc", "a", "b"][..],    // same, cluster order irrelevant
        &["-cd", "a", "b"][..],
        &["-ds", "a"][..],      // `-ds` with one operand too few (`:104-106`)
        &["-s", "-d", "a"][..], // a cross-token -s and -d combine into `-ds`, still one short
        &["-c", "a"][..],       // without -d/-s a single operand is not enough
    ] {
        let (out, code, err) = run(args, b"aabbcc\n");
        assert_eq!(code, 1, "{args:?}");
        assert_eq!(out, "", "{args:?}");
        assert_eq!(err, USAGE, "{args:?}");
    }
}

#[test]
fn off_letter_gets_one_libc_line_then_the_block() {
    // `:79-81` exits inside the loop ⇒ reports do not accumulate: `-ZQ` reports
    // only Z, `-Q -Z a` only Q
    for (args, letter) in [
        (vec!["-Z"], "Z"),
        (vec!["-h"], "h"),
        (vec!["-0"], "0"),
        (vec!["-a"], "a"),
        (vec!["-ZQ"], "Z"),
        (vec!["-sZ", "a"], "Z"),
        (vec!["-Q", "-Z", "a"], "Q"),
    ] {
        let (out, code, err) = run(&args, b"abc\n");
        assert_eq!((code, out.as_str()), (1, ""), "{args:?}");
        assert_eq!(
            err,
            format!("tr: unknown option -- {letter}\n{USAGE}"),
            "{args:?} 一行 libc 必须在整块 usage 之前"
        );
    }
    // `--s a` reports the dash itself: the third byte `s` keeps it from being
    // read as the terminator
    let (out, code, err) = run(&["--s", "a"], b"aabbcc\n");
    assert_eq!((code, out.as_str()), (1, ""));
    assert_eq!(err, format!("tr: unknown option -- -\n{USAGE}"));
}

#[test]
fn the_libc_line_follows_argv_zero_but_the_block_is_hardwired() {
    // `getopt.c:103` uses `getprogname()`, while `tr.c:278-281` hardcodes `tr`
    // in the string. Reference measurements: argv[0]="trsweep" reports
    // `trsweep:`, argv[0]="/some/dir/tr" reports `tr:` after the basename, and
    // the four lines stay identical in both cases
    for argv0 in ["trsweep", "/some/dir/tr"] {
        let mut child = Command::new(TR)
            .arg0(argv0)
            .arg("-Z")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("the tr binary should be runnable");
        write_stdin(&mut child, b"abc\n");
        let output = child.wait_with_output().expect("the run should finish");
        assert_eq!(output.status.code(), Some(1), "{argv0:?}");
        assert_eq!(output.stdout, b"", "{argv0:?}");
        let err = String::from_utf8_lossy(&output.stderr).into_owned();
        let name = argv0.rsplit('/').next().unwrap_or(argv0);
        assert_eq!(
            err,
            format!("{name}: unknown option -- Z\n{USAGE}"),
            "{argv0:?} 实际 stderr={err:?}"
        );
    }
}

#[test]
fn flag_cluster_is_per_letter_and_order_free() {
    // The same flag set lands identically whether clustered or split, in any
    // order
    for args in [
        vec!["-c", "-s", "a", "b"],
        vec!["-sc", "a", "b"],
        vec!["-cs", "a", "b"],
    ] {
        let (out, code, err) = run(&args, b"aabbcc\n");
        assert_eq!((code, err), (0, String::new()), "{args:?}");
        // The squeeze set is string2; the complemented string1 maps everything
        // else to `b`, so the newline is squeezed out too: the reference gives
        // `aab` with no trailing newline
        assert_eq!(out, "aab", "{args:?}");
    }
    // `-s -d a b` and `-ds a b` are the same shape; `-ds ab b` deletes a and b,
    // then squeezes by string2
    for (args, want) in [
        (vec!["-s", "-d", "a", "b"], "bcc\n"),
        (vec!["-ds", "a", "b"], "bcc\n"),
        (vec!["-ds", "ab", "b"], "cc\n"),
    ] {
        let (out, code, err) = run(&args, b"aabbcc\n");
        assert_eq!((code, err), (0, String::new()), "{args:?}");
        assert_eq!(out, want, "{args:?}");
    }
}

#[test]
fn delete_squeeze_and_complement_shapes_are_the_measured_ones() {
    for (args, input, want) in [
        (vec!["-d", "a"], &b"aabbcc\n"[..], "bbcc\n"),
        (vec!["-d", "ab"], &b"aabbcc\n"[..], "cc\n"),
        (vec!["-cd", "a"], &b"aabbcc\n"[..], "aa"),
        (vec!["-s", "a"], &b"aabbcc\n"[..], "abbcc\n"),
        (vec!["-s", "ab"], &b"aabbcc\n"[..], "abcc\n"),
        (vec!["-s", "a", "b"], &b"aabbcc\n"[..], "bcc\n"),
        (vec!["-cs", "a"], &b"aabbcc\n"[..], "aabc\n"),
        (vec!["-c", "a", "b"], &b"aabbcc\n"[..], "aabbbbb"),
    ] {
        let (out, code, err) = run(&args, input);
        assert_eq!((code, err), (0, String::new()), "{args:?}");
        assert_eq!(out, want, "{args:?}");
    }
}

#[test]
fn empty_string2_sentence_has_no_usage_block() {
    // `:187-188`'s `errx`: one line, status 1, **no** usage after it
    let (out, code, err) = run(&["a", ""], b"abc\n");
    assert_eq!((code, out), (1, String::new()));
    assert_eq!(err, "tr: empty string2\n");
    // The same criterion is absent from the delete and squeeze paths: empty
    // sets are legal
    for (args, want) in [
        (vec!["-d", ""], "aabbcc\n"),
        (vec!["-ds", "a", ""], "bbcc\n"),
    ] {
        let (out, code, err) = run(&args, b"aabbcc\n");
        assert_eq!((code, err), (0, String::new()), "{args:?}");
        assert_eq!(out, want, "{args:?}");
    }
}

#[test]
fn the_ring_does_not_reorder_operands() {
    // A bare `-` stays among the operands: `tr - a` maps `-` to `a`, `tr - -`
    // copies its input through
    let (out, code, err) = run(&["-", "a"], b"a-b\n");
    assert_eq!((code, err), (0, String::new()));
    assert_eq!(out, "aab\n");
    let (out, code, err) = run(&["-", "-"], b"a-b\n");
    assert_eq!((code, err), (0, String::new()));
    assert_eq!(out, "a-b\n");
    // `--` consumes itself: `tr -- a b` has the same shape as `tr a b`
    let (out, code, err) = run(&["--", "a", "b"], b"aabbcc\n");
    assert_eq!((code, err), (0, String::new()));
    assert_eq!(out, "bbbbcc\n");
}

#[test]
fn translation_path_is_unchanged_by_the_shell_rewrite() {
    for (args, input, want) in [
        (vec!["a", "b"], &b"aabbcc\n"[..], "bbbbcc\n"),
        (vec!["ab", "xy"], &b"aabbcc\n"[..], "xxyycc\n"),
        (vec!["a-z", "A-Z"], &b"abc\n"[..], "ABC\n"),
        (vec!["[:lower:]", "[:upper:]"], &b"abc\n"[..], "ABC\n"),
        (vec!["a", "[b*3]"], &b"abc\n"[..], "bbc\n"),
        (vec!["a-z", "z"], &b"abc\n"[..], "zzz\n"),
        (vec!["-d", "[:alpha:]"], &b"aabbcc12\n"[..], "12\n"),
        (vec!["a", "b"], &b"a-b\n"[..], "b-b\n"),
    ] {
        let (out, code, err) = run(&args, input);
        assert_eq!((code, err), (0, String::new()), "{args:?}");
        assert_eq!(out, want, "{args:?}");
    }
    // Empty input is success.
    let (out, code, err) = run(&["a", "b"], b"");
    assert_eq!((out, code, err), (String::new(), 0, String::new()));
}

#[test]
fn an_empty_string1_is_the_identity_map() {
    // The identity prefill at `tr.c:178-181` plus the immediate break on an
    // empty set at `:214-215` ⇒ nothing is ever mapped; the reference exits 0
    // and copies its input through for all four shapes (the pre-work reading is
    // in `tmp/tr-323/measure_before.log`)
    for args in [
        vec!["", "x"],
        vec!["", "b"],
        vec!["", "A-Z"],
        vec!["-s", "", "x"],
    ] {
        let (out, code, err) = run(&args, b"aabbcc\n");
        assert_eq!((code, err), (0, String::new()), "{args:?}");
        assert_eq!(out, "aabbcc\n", "{args:?}");
    }
    // The complement path is unaffected: complementing an empty string1 yields
    // **all bytes**, all mapped to string2's last byte, so all seven input bytes
    // become `x`, including the trailing newline
    let (out, code, err) = run(&["-c", "", "x"], b"aabbcc\n");
    assert_eq!((code, err), (0, String::new()));
    assert_eq!(out, "xxxxxxx");
    // An empty string2 still gets that `errx` line (`:187-188`); the identity
    // branch does not swallow it
    let (out, code, err) = run(&["", ""], b"aabbcc\n");
    assert_eq!((code, out), (1, String::new()));
    assert_eq!(err, "tr: empty string2\n");
    // Three operands including an empty string still fail the arity check
    // (`:86-97`); the identity branch has no say
    let (_, code, err) = run(&["", "x", "extra"], b"aabbcc\n");
    assert_eq!(code, 1);
    assert_eq!(err, USAGE);
}

#[test]
fn the_four_set_grammar_sentences_reach_stderr_verbatim() {
    // Reference measurements: one sentence, status 1, **no** usage block after
    // it (`str.c` calls errx(1, ...) on the spot everywhere)
    for (set1, set2, want) in [
        ("[x*3]", "y", "tr: Sequences only valid in string2\n"),
        ("[ab*2]", "y", "tr: Sequences only valid in string2\n"),
        ("abc", "[ab*2]", "tr: Misplaced sequence asterisk\n"),
        ("abc", "[x*1a]", "tr: illegal sequence count\n"),
        ("[:bogus:]", "y", "tr: unknown class bogus\n"),
    ] {
        let (out, code, err) = run(&[set1, set2], b"abc\n");
        assert_eq!(
            (code, out.as_str(), err.as_str()),
            (1, "", want),
            "[{set1}] [{set2}]"
        );
    }
}

#[test]
fn the_two_equivalence_sentences_reach_stderr_verbatim() {
    // Reference measurements (`tmp/tr-341/measure341.log`): these two sentences
    // are one line each, status 1, **no** usage after them, and both string
    // positions are judged the same way — `bracket()` checks the form and
    // `genequiv()` checks the inside, neither distinguishing the position.
    // Before this test, this shell quietly collapsed `[=ab=]` to the single
    // member `a` and exited 0: a false report of "handled" costs more than no
    // report.
    for (set, want) in [
        ("[=ab=]", "tr: Misplaced equivalence equals sign\n"),
        ("[=abc=]", "tr: Misplaced equivalence equals sign\n"),
        ("[==]", "tr: Misplaced equivalence equals sign\n"),
        (r"[=\=]", "tr: Misplaced equivalence equals sign\n"),
        ("[=a=b=]", "tr: Misplaced equivalence right bracket\n"),
    ] {
        for (a, b) in [(set, "y"), ("y", set)] {
            let (out, code, err) = run(&[a, b], b"abc\n");
            assert_eq!(
                (code, out.as_str(), err.as_str()),
                (1, "", want),
                "[{a}] [{b}]"
            );
        }
    }
    // A well-formed one does not regress: `[=c=]` is still the **single** member
    // `c`, status 0 (the reference measured `abx`, because the replaced input
    // byte is `c`, not `a`)
    let (out, code, err) = run(&["[=c=]", "x"], b"abc\n");
    assert_eq!((code, out.as_str(), err.as_str()), (0, "abx\n", ""));
}

#[test]
fn an_unshaped_bracket_is_a_literal_not_a_complaint() {
    // A false return from `bracket()` falls through to a literal `[`
    // (`str.c:124-132`); the reference exits 0 for all these shapes
    let (out, code, err) = run(&["[x]", "y"], b"abc\n");
    assert_eq!((code, out.as_str(), err.as_str()), (0, "abc\n", ""));
    let (out, code, err) = run(&["abc", "[x]"], b"abc\n");
    assert_eq!((code, out.as_str(), err.as_str()), (0, "[x]\n", ""));
    // `[:alpha` is not formed, so `[`, `:`, `a`, `l`, `p`, `h`, `a` are all
    // literals: the set contains `a`, so `a` becomes `y` — the reference
    // measured `ybc\n` for the same shape (see tmp/tr-335/ab335.log)
    for (set, want) in [("[x", "abc\n"), ("[x]y*", "abc\n"), ("[:alpha", "ybc\n")] {
        let (out, code, err) = run(&[set, "y"], b"abc\n");
        assert_eq!((code, out.as_str(), err.as_str()), (0, want, ""), "{set:?}");
    }
}

#[test]
fn an_open_or_zero_repeat_replaces_every_member() {
    // Both `[x*]` and `[x*0]` are INFINITE; the reference has nothing to do
    // with empty sets (an empty string2 gets its own sentence)
    for set in ["[x*]", "[x*0]"] {
        let (out, code, err) = run(&["abc", set], b"abc\n");
        assert_eq!(
            (code, out.as_str(), err.as_str()),
            (0, "xxx\n", ""),
            "{set:?}"
        );
    }
}

#[test]
fn a_reversed_range_is_three_literals_and_exits_zero() {
    // The reference exits 0 for `tr_ref z-a y`: with the range reversed, z, -,
    // and a each enter the set, so `a` becomes `y`
    let (out, code, err) = run(&["z-a", "y"], b"abc\n");
    assert_eq!((code, out.as_str(), err.as_str()), (0, "ybc\n", ""));
}

/// What C pads with after string2 is exhausted depends on "does the expression
/// end in a character class", not on "repeat the last byte when short": both
/// name classes and equivalence classes set `lastch` to `OOBCH`
/// (`minix3/usr.bin/tr/str.c:158-166` against `:110-122`), `tr.c:220` stores
/// 256 in the map, and `putchar` truncates it to `\0`. Expected values are taken
/// verbatim from the reference (`tmp/tr-322/tr_ref`; the same round measured all
/// eight shapes in `tmp/tr-390/ab.log`).
#[test]
fn an_exhausted_character_class_pads_with_nul_bytes() {
    let (out, status, err) = run(&["abc", "[=c=]"], b"abc\n");
    assert_eq!(
        out, "c\0\0\n",
        "等价类收尾：b、c 两格填 NUL，不是重复末字节 `ccc`"
    );
    assert_eq!((status, err.as_str()), (0, ""));

    let (out, status, _) = run(&["abcdefghijklmnop", "[:digit:]"], b"abcdefghijklmnop\n");
    assert_eq!(out, "0123456789\0\0\0\0\0\0\n", "名字类同理");
    assert_eq!(status, 0);

    // If ordinary characters follow the class, the rule returns to "repeat the
    // last byte" — the other side of the same criterion.
    let (out, _, _) = run(&["abcdefghijklmnop", "[:digit:]x"], b"abcdefghijklmnop\n");
    assert_eq!(out, "0123456789xxxxxx\n");
    let (out, _, _) = run(&["abc", "xy"], b"abc\n");
    assert_eq!(out, "xyy\n", "普通式子仍重复末字节");
}

/// This layer used to reserve eight SET slots with every member taking one, so
/// nine literals (`tr 'abcdefghi' 'x'`) became a status-1 exit **with no
/// message**. C has no such limit: literals, ranges, and escapes are all just
/// one assignment to `lastch`, and only `[:class:]`/`[=c=]` enter the SET state.
/// The reference returns `xxx` and exits 0 for the same shape.
#[test]
fn a_long_literal_set_is_translated_instead_of_vanishing_silently() {
    let (out, status, err) = run(&["abcdefghi", "x"], b"abc\n");
    assert_eq!((out.as_str(), status, err.as_str()), ("xxx\n", 0, ""));

    // 26 range members are the everyday shape of `tr a-z A-Z`; nine literals
    // should not be treated more specially.
    let (out, status, err) = run(&["abcdefghijklmnopqrstuvwxyz", "[=q=]"], b"abcz\n");
    assert_eq!(
        out, "q\0\0\0\n",
        "长 set1 配单成员集合类：除第一格外全部填 NUL，stderr 是 {err:?}"
    );
    assert_eq!((status, err.as_str()), (0, ""));

    let (out, status, _) = run(&["-d", "abcdefghij"], b"abcj\n");
    assert_eq!(
        (out.as_str(), status),
        ("\n", 0),
        "-d 那条腿同样按字节表走，不该有格数上限"
    );
}

/// `-s` shares the pass with translation (`tr.c:240-246`), and the squeeze
/// criterion lives in the `int` domain: two adjacent OOBCHs squeeze into one,
/// while a NUL already in the input maps to itself and is never recorded in the
/// "handed back" table, so it does not squeeze. All expected values come from
/// the reference (including the `ab\0c` shape — the first version of this test
/// wrote `c\0\0` while the reference measures `c\0\0\0`).
#[test]
fn the_squeeze_leg_collapses_padded_nuls_but_not_real_ones() {
    for (input, expected) in [
        (&b"abc\n"[..], "c\0\n"),
        (&b"abbc\n"[..], "c\0\n"),
        (&b"ab\0c\n"[..], "c\0\0\0\n"),
        (&b"a\0b\0c\n"[..], "c\0\0\0\0\n"),
        // `lastch` starts out as OOBCH (`tr.c:240`), so the leading OOBCH is
        // squeezed too: the whole pass leaves only that newline (the reference
        // measured `tr -s ab [=c=]` ← `b\n`).
        (&b"b\n"[..], "\n"),
        (&b"ba\n"[..], "c\n"),
    ] {
        let (out, status, err) = run(&["-s", "abc", "[=c=]"], input);
        assert_eq!(out, expected, "{input:?}");
        assert_eq!((status, err.as_str()), (0, ""), "{input:?}");
    }
}

/// The same process seam as `run`, but with **raw bytes** as arguments: a `&str`
/// cannot hand over a bare `ff`, so only this way can the byte semantics of set
/// expressions and class names be measured. Standard input is fed per the
/// fixture convention; a failed write is not counted as a defect of the code
/// under test.
fn run_raw(args: &[Vec<u8>], input: &[u8]) -> (Vec<u8>, i32, Vec<u8>) {
    use std::ffi::OsString;
    use std::os::unix::ffi::OsStringExt as _;
    let mut child = Command::new(TR)
        .args(args.iter().map(|word| OsString::from_vec(word.clone())))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the tr binary should be runnable");
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

/// Set expressions stay byte-exact: `-d a<ff>b` deletes that **one** `ff`. The
/// expectation comes from the reference `tmp/tr-322/tr_ref` (measured in
/// `tmp/argv-444/ab-after.log`): the input `a\xffbq` comes back as `q`; the
/// lossy form would count the expression as five bytes `a ef bf bd b`, leaving
/// `ff` in the output.
#[test]
fn a_raw_nonutf8_set_member_deletes_that_byte() {
    let (out, code, err) = run_raw(&[b"-d".to_vec(), b"a\xffb".to_vec()], b"a\xffbq\n");
    assert_eq!((code, err), (0, Vec::new()));
    assert_eq!(out, b"q\n");
    // Same for the reverse set: `ff` is a member of `-c a<ff>`'s domain, so it
    // passes through unchanged while `z` becomes `y`
    let (out, code, err) = run_raw(
        &[b"-c".to_vec(), b"a\xff".to_vec(), b"y".to_vec()],
        b"a\xffz\n",
    );
    assert_eq!((code, err), (0, Vec::new()));
    assert_eq!(out, b"a\xffyy");
}

/// An unrecognized class name is echoed **as raw bytes** (`str.c:253`'s `%.*s`
/// carries a length): for `tr $'[:\xfffoo:]' x` the reference prints
/// `tr: unknown class \xfffoo` and exits 1.
#[test]
fn an_unknown_class_name_with_a_raw_byte_is_echoed_by_that_byte() {
    let (out, code, err) = run_raw(&[b"[:\xfffoo:]".to_vec(), b"x".to_vec()], b"a\n");
    assert_eq!(code, 1, "实际 stdout {out:?}");
    assert_eq!(out, Vec::new());
    assert_eq!(err, b"tr: unknown class \xfffoo\n");
}
