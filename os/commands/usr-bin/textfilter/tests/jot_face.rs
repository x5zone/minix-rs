//! End-to-end face for the `jot` binary, checked against
//! `minix3/usr/bin/jot/jot.c` (the option loop at `:149-177`, the operand
//! counting `switch (argc)` at `:184-226`, `usage()` at `:313-320`) and the
//! libc `getopt` it drives (`minix3/lib/libc/stdlib/getopt.c:74-78`,
//! `:81-92`, `:95-104`, `:118-141`).
//!
//! The optstring at `jot.c:150` is `"b:cnp:rs:w:"`: three letters with no
//! value (`c`, `n`, `r`) plus four with one (`b`, `p`, `s`, `w`), so there
//! are **four** reachable exits, each with its own sentence:
//! (1) a letter outside the set = one libc line + one usage line, exit 1
//!     (a cluster reports only its first bad letter; `--x` reports `-`);
//! (2) a value-taking letter without its value = libc's
//!     `option requires an argument -- <letter>` + two usage lines, the same
//!     for all four letters (the earlier implementation sent a missing `-p`
//!     value down the `Bad precision value` branch, which differs in both
//!     sentence and line count);
//! (3) a `-p` value failing the `strtol(optarg, &ep, 0)` whole-string check,
//!     or negative = `jot: Bad precision value` on a **single line**, with no
//!     usage;
//! (4) only after the loop ends normally do the operands count: none
//!     (including a lone `--`) is usage with **no** libc line; five or more is
//!     `jot: Too many arguments.  What do you mean by <fifth>?` on a single
//!     line.
//!
//! Two more facts are visible only end to end: `-b` and `-w` share one word
//! slot (the FALLTHROUGH at `:172-176`), and the loop does **not**
//! rearrange — in `jot 3 -Z`, `-Z` is the begin operand, and where `sscanf`
//! does not take it, the code point of its last character becomes the value
//! (`:203-204`), so it prints 90/91/92.
//!
//! This branch has no read/write seam defect: `jot` neither reads standard
//! input nor opens any file, so every value is derived from the command
//! line. The family where `prec` stays `-1` is not tested either (`jot 3`,
//! `-n 3`, `3 - 5`, `-s , 3`): the format specifier `jot.c:349-356` builds
//! there is itself broken and what gets printed depends on each `printf`
//! implementation, which makes it undecidable on this host; it is registered
//! as `JOT-PREC-MINUS-ONE-FORMAT`. The `-r` random stream is a separate
//! divergence: this layer uses an injected LCG rather than libc's
//! `random()`.

use std::process::{Command, Stdio};

const JOT: &str = env!("CARGO_BIN_EXE_jot");
const USAGE: &str = "usage: jot [-cnr] [-b word] [-p precision] [-s string] [-w word] [reps [begin [end [step | seed]]]]\n";

fn run(args: &[&str]) -> (String, i32, String) {
    let output = Command::new(JOT)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("the jot binary should be runnable");
    (
        String::from_utf8_lossy(&output.stdout).into_owned(),
        output.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

/// The same command seam as `run`, but handing the bytes back **verbatim**:
/// `from_utf8_lossy` renders a bare `c3` as the three-byte U+FFFD, so the
/// text form cannot tell the two failure shapes "one byte" and "two bytes"
/// apart (the `rev`/`paste` branches have the same issue).
fn run_bytes(args: &[&str]) -> (Vec<u8>, i32, Vec<u8>) {
    let output = Command::new(JOT)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("the jot binary should be runnable");
    (
        output.stdout,
        output.status.code().unwrap_or(-1),
        output.stderr,
    )
}

/// For a letter outside the set, the letter position is a **single raw
/// byte**: the reference measured both `-é` and `-cé` as
/// `jot: unknown option -- ` + `c3` + newline, then usage and exit 1 (`c` is
/// a legal valueless letter in `"b:cnp:rs:w:"`, so the cluster keeps
/// consuming and takes the first byte of `é`; `-Zé` stops at `Z` because
/// `jot.c:151-152` calls `usage()` on `'?'` and the second letter is never
/// reached). `-bé 3` is the counter-case for this family: `b` takes a
/// value, the whole `é` is swallowed as that value, and exit 0 repeats the
/// word on three lines as `reps` — byte handling must not touch the
/// value-taking half. ASCII `-Z` and "a cluster reports only its first bad
/// letter" each already have an owner in this file
/// (`an_unknown_letter_prints_the_libc_line_then_the_usage`,
/// `a_cluster_reports_only_its_first_bad_letter`) and are not repeated
/// here.
#[test]
fn the_libc_line_of_a_multibyte_flag_is_a_single_raw_byte() {
    let multibyte: Vec<u8> = [
        b"jot: unknown option -- ".as_slice(),
        b"\xc3\n",
        USAGE.as_bytes(),
    ]
    .concat();
    for form in ["-\u{e9}", "-c\u{e9}"] {
        let (out, code, err) = run_bytes(&[form, "3"]);
        assert_eq!(out, Vec::new(), "{form} 的报文只走 stderr");
        assert_eq!(code, 1, "{form} 的 stderr 是 {err:?}");
        assert_eq!(err, multibyte, "{form} 的字母该是一个裸字节 c3");
    }

    let (_, code, err) = run_bytes(&["-Z\u{e9}", "3"]);
    assert_eq!(code, 1);
    assert_eq!(
        err,
        [b"jot: unknown option -- Z\n".as_slice(), USAGE.as_bytes(),].concat(),
        "环停在第一个坏字母上，`é` 根本不该被读到"
    );

    let (out, code, err) = run_bytes(&["-b\u{e9}", "3"]);
    // The reference measured `jot_ref -bé 3` as three lines of `c3 a9 0a`
    // (`-b` repeats that word reps times), exit 0, nothing on stderr.
    assert_eq!(
        (out.as_slice(), code),
        ("\u{e9}\n\u{e9}\n\u{e9}\n".as_bytes(), 0),
        "{err:?}"
    );
    assert_eq!(err, Vec::new(), "`-b` 的值不是字母位，多字节字符照原样出去");
}

/// A letter outside the set: the libc line first, `usage()` after, exit 1
/// (`jot.c:151-152`). The earlier implementation was missing the libc line.
#[test]
fn an_unknown_letter_prints_the_libc_line_then_the_usage() {
    let (out, status, err) = run(&["-Z", "3"]);
    assert_eq!((out.as_str(), status), ("", 1));
    assert_eq!(err, format!("jot: unknown option -- Z\n{USAGE}"));
}

/// A cluster reports only its first bad letter (reference: `-cz` →
/// `unknown option -- z`; `c` took effect but the scan does not go on).
#[test]
fn a_cluster_reports_only_its_first_bad_letter() {
    for (args, letter) in [("-cz", "z"), ("-Zb", "Z"), ("-ncrZ", "Z")] {
        let (out, status, err) = run(&[args, "3"]);
        assert_eq!((out.as_str(), status), ("", 1), "stderr 是 {err:?}");
        assert_eq!(err, format!("jot: unknown option -- {letter}\n{USAGE}"));
    }
}

/// `--x` reports the second hyphen as the letter: the optstring contains no
/// `-` (reference: `unknown option -- -`).
#[test]
fn a_dash_followed_by_a_word_is_an_unknown_option_named_by_the_dash() {
    let (out, status, err) = run(&["--x", "3"]);
    assert_eq!((out.as_str(), status), ("", 1));
    assert_eq!(err, format!("jot: unknown option -- -\n{USAGE}"));
}

/// A value-taking letter without its value is uniform for all four: the
/// libc line + usage, exit 1. The earlier implementation printed only usage
/// for `-b`/`-s`/`-w` and `Bad precision value` for `-p`, so the line counts
/// and sentences were both off.
#[test]
fn a_value_letter_without_its_value_prints_the_libc_line_then_the_usage() {
    for letter in ['b', 'p', 's', 'w'] {
        let flag = format!("-{letter}");
        let (out, status, err) = run(&[&flag]);
        assert_eq!(
            (out.as_str(), status),
            ("", 1),
            "{flag} 的 stderr 是 {err:?}"
        );
        assert_eq!(
            err,
            format!("jot: option requires an argument -- {letter}\n{USAGE}"),
            "缺值这一支是两句、程序名跟着 `getprogname()`"
        );
        // A cluster landing at the end of the string takes the same branch —
        // but **no further token may follow**: the `3` in `-cb 3` would be
        // taken as `-b`'s word value, so the missing-value condition would
        // not hold at all (reference measurement: that prints the usage).
        let clustered = format!("-c{letter}");
        let (_, status, err) = run(&[&clustered]);
        assert_eq!(status, 1);
        assert_eq!(
            err,
            format!("jot: option requires an argument -- {letter}\n{USAGE}")
        );
    }
}

/// A bad `-p` value is a single `errx` line (`jot.c:161-165`): the program
/// name is hard-coded in the sentence, **no usage follows**, and the exit
/// code is 1. The test is `strtol`'s whole-string check, so both `09` (octal
/// stops at `0`) and `3x` are rejected.
#[test]
fn a_rejected_precision_is_one_line_without_the_usage() {
    for word in ["abc", "-1", "3x", "09", " ", "-", "99999999999999999999"] {
        let (out, status, err) = run(&["-p", word, "3"]);
        assert_eq!(
            (out.as_str(), status),
            ("", 1),
            "`-p {word}` 的 stderr 是 {err:?}"
        );
        assert_eq!(err, "jot: Bad precision value\n");
    }
}

/// No operand at all means usage, and **no** libc line. In `-b X`, the `X`
/// is eaten by `-b`, so again there is no operand (reference measurement:
/// `jot -b X` prints the usage and exits 1).
#[test]
fn an_empty_operand_list_is_the_usage_without_a_libc_line() {
    for args in [
        vec![],
        vec!["--"],
        vec!["-n"],
        vec!["-c", "-r"],
        vec!["-b", "X"],
    ] {
        let (out, status, err) = run(&args);
        assert_eq!(
            (out.as_str(), status),
            ("", 1),
            "`{args:?}` 的 stderr 是 {err:?}"
        );
        assert_eq!(err, USAGE, "只有 usage 一行");
    }
}

/// Five operands or more is one `errx` sentence naming the **fifth**
/// (`argv[4]`); the four before it are not interpreted at all. Two spaces
/// follow `arguments.` (`jot.c:223-225`).
#[test]
fn the_fifth_operand_is_the_one_named_by_the_too_many_arguments() {
    let (out, status, err) = run(&["3", "1", "5", "2", "9"]);
    assert_eq!((out.as_str(), status), ("", 1));
    assert_eq!(err, "jot: Too many arguments.  What do you mean by 9?\n");

    let (_, status, err) = run(&["1", "2", "3", "4", "5", "6"]);
    assert_eq!(status, 1);
    assert_eq!(
        err, "jot: Too many arguments.  What do you mean by 5?\n",
        "报的是第五个操作数，不是最后一个"
    );

    // A bad first operand never gets a turn before this branch: the count
    // check precedes any interpretation.
    let (_, status, err) = run(&["x", "1", "2", "3", "4"]);
    assert_eq!(status, 1);
    assert_eq!(err, "jot: Too many arguments.  What do you mean by 4?\n");
}

/// Both the `reps` and the `step` rejection sentences are "two spaces + the
/// operand verbatim + no trailing period" (`jot.c:189` and `:215`).
#[test]
fn a_rejected_operand_names_itself_after_two_spaces() {
    let (out, status, err) = run(&["x"]);
    assert_eq!((out.as_str(), status), ("", 1));
    assert_eq!(err, "jot: Bad reps value:  x\n");

    let (_, status, err) = run(&["3", "1", "2", "x"]);
    assert_eq!(status, 1);
    assert_eq!(err, "jot: Bad step value:  x\n");
}

/// The loop does not rearrange: a `-c` after the operands is no longer an
/// option but the fourth operand, i.e. step, and `strtod`'s whole-string
/// check rejects it on the spot (reference: `jot 3 65 69 -c` →
/// `Bad step value:  -c`).
#[test]
fn the_ring_does_not_rearrange_options_after_the_first_operand() {
    let (out, status, err) = run(&["3", "65", "69", "-c"]);
    assert_eq!((out.as_str(), status), ("", 1));
    assert_eq!(err, "jot: Bad step value:  -c\n");

    // `-Z` lands in the begin slot: `sscanf("%lf")` does not take it, so the
    // code point of its last character is used (`jot.c:203-204`); `Z` is 90.
    let (out, status, err) = run(&["3", "-Z"]);
    assert_eq!(
        (out.as_str(), status),
        ("90\n91\n92\n", 0),
        "stderr 是 {err:?}"
    );
    assert_eq!(err, "");
}

/// A bare `-` is an operand, not an alias for stdin, and its slot keeps the
/// default; a `-Z` after `--` is a literal operand. All three shapes go
/// through the default derivation of step/begin/ender, and the FALLTHROUGH
/// at `jot.c:193-219` looks back slot by slot.
#[test]
fn a_bare_dash_keeps_the_default_for_its_slot() {
    // The fourth slot supplies step; the `-` in the third lets ender keep its
    // default: reps=3, begin=65, step=2.
    let (out, status, err) = run(&["3", "65", "-", "2"]);
    assert_eq!(
        (out.as_str(), status),
        ("65\n67\n69\n", 0),
        "stderr 是 {err:?}"
    );
    assert_eq!(err, "");

    // The third and fourth slots keep their defaults, so begin=65 pairs with
    // the default step=1.
    let (out, status, _err) = run(&["3", "65", "-", "-"]);
    assert_eq!((out.as_str(), status), ("65\n66\n67\n", 0));

    let (out, status, _err) = run(&["--", "3", "65"]);
    assert_eq!((out.as_str(), status), ("65\n66\n67\n", 0));
}

/// `-b` only raises `boring`; its value goes into the same word slot as
/// `-w`, so the later write wins, while `boring` stays raised once set
/// (`jot.c:172-176`).
#[test]
fn b_and_w_write_into_one_word_slot() {
    let (out, status, err) = run(&["-bX", "-wY", "3"]);
    assert_eq!(
        (out.as_str(), status),
        ("Y\nY\nY\n", 0),
        "stderr 是 {err:?}"
    );
    assert_eq!(err, "");

    let (out, status, _err) = run(&["-bX", "-c", "3"]);
    assert_eq!(
        (out.as_str(), status),
        ("X\nX\nX\n", 0),
        "`boring` 赢过 `-c`"
    );
}

/// The accepted set for `-p` is defined by `strtol(…, 0)`: base 0 recognizes
/// `0x`, an empty string yields no digits yet still counts as fully
/// consumed, and not one stray trailing character is allowed. This test pins
/// down the earlier `parse::<usize>()` behaviour: it rejected both `0x2` and
/// the empty string, while the reference accepts both.
#[test]
fn the_precision_takes_base_zero_and_requires_a_full_operand() {
    let (out, status, err) = run(&["-p", "0x2", "3"]);
    assert_eq!(
        (out.as_str(), status),
        ("1.00\n2.00\n3.00\n", 0),
        "stderr 是 {err:?}"
    );
    assert_eq!(err, "");

    let (out, status, _err) = run(&["-p", "", "3"]);
    assert_eq!(
        (out.as_str(), status),
        ("1\n2\n3\n", 0),
        "空串是合法的 0，走整数形"
    );

    let (out, status, _err) = run(&["-p", "010", "3"]);
    assert_eq!(
        (out.as_str(), status),
        ("1.00000000\n2.00000000\n3.00000000\n", 0),
        "`010` 是八进 8，即 8 位小数"
    );

    let (out, status, _err) = run(&["-p", "2", "3", "1.5"]);
    assert_eq!((out.as_str(), status), ("1.50\n2.50\n3.50\n", 0));
}

#[test]
fn four_operands_with_a_zero_step_stay_bounded() {
    // Reference measurement: `jot 3 1 100 0` is three lines of `1`, exit 0;
    // the derived switch once misrouted it to the `case ENDER | STEP` branch
    // with reps zeroed, so it never terminated
    for args in [&["3", "1", "100", "0"][..], &["3", "1", "1", "0"][..]] {
        let (out, code, err) = run(args);
        assert_eq!((code, err.as_str()), (0, ""), "{args:?}");
        assert_eq!(out, "1\n1\n1\n", "{args:?}");
    }
    // With a non-zero step, take the smaller of "the given reps" and "the
    // computed count": `jot 3 1 100 5` is three lines, not twenty
    let (out, code, err) = run(&["3", "1", "100", "5"]);
    assert_eq!((code, err), (0, String::new()));
    assert_eq!(out, "1\n6\n11\n");
    // When the computed count is smaller, use it (`jot 2 1 100 5` gives two
    // lines; `jot 20 1 100 5` gives twenty, and only the first two are pinned
    // here)
    let (out, code, _) = run(&["2", "1", "100", "5"]);
    assert_eq!((code, out.as_str()), (0, "1\n6\n"));
    // Default reps with begin/ender/step all given: C computes reps in the
    // same body as `case ENDER | STEP | BEGIN` (`(10 - 1 + 2) / 2` = 5);
    // with this label missing it would fall into `default` and run with the
    // default 100 rows
    let (out, code, _) = run(&["-", "1", "10", "2"]);
    assert_eq!((code, out.as_str()), (0, "1\n3\n5\n7\n9\n"));
    // Six given reps but ender already reached within the step: C still takes
    // the smaller of reps and the computed count, so six lines of `1` print
    // here
    let (out, code, _) = run(&["6", "1", "10", "0"]);
    assert_eq!((code, out.as_str()), (0, "1\n1\n1\n1\n1\n1\n"));
}

#[test]
fn a_negative_or_fractional_seed_is_accepted_like_c() {
    // `jot.c:118` is `srandom((unsigned long) step)`: negative and fractional
    // values are **legal** seeds. The reference `jot -r 3 1 10 -5` exits 0 and
    // prints three integers (an earlier layer used its own `v >= 0.0` check and
    // reported `Bad step value`). The random stream's **values** are a separate
    // divergence (this layer's LCG is not libc's `random()`), so only the exit
    // code, the line count, and "all digits" are pinned here.
    for arg in ["-5", "-2147483648", "-0.5", "0", "5"] {
        let (out, code, err) = run(&["-r", "3", "1", "10", arg]);
        assert_eq!(code, 0, "种子 {arg} 的 stderr={err:?}");
        assert_eq!(err, "", "种子 {arg} 不该有诊断");
        let lines: Vec<&str> = out.trim_end_matches('\n').split('\n').collect();
        assert_eq!(lines.len(), 3, "种子 {arg} 实际输出 {out:?}");
        assert!(
            lines.iter().all(|l| l.bytes().all(|b| b.is_ascii_digit())),
            "种子 {arg} 输出不全是整数：{out:?}"
        );
    }
    // A visible consequence of the truncation: C's `(unsigned long)(-0.5)` is
    // 0, so the `-0.5` and `0` sequences must be byte-identical (the reference
    // measured `9\n4\n8\n` for both)
    let (a, _, _) = run(&["-r", "3", "1", "10", "-0.5"]);
    let (b, _, _) = run(&["-r", "3", "1", "10", "0"]);
    assert_eq!(a, b, "小数种子要先截断，不能当成坏值");
    // `strtod`'s whole-string check is unchanged: a trailing stray character
    // still reports the same sentence and exits 1
    let (out, code, err) = run(&["-r", "3", "1", "10", "5x"]);
    assert_eq!((code, out.as_str()), (1, ""));
    assert_eq!(err, "jot: Bad step value:  5x\n");
}

/// The four registered shapes (`jot 3`, `-n 3`, `3 - 5`, `-s , 3`) all
/// belong to the family where `prec` is never written. What can be pinned at
/// process level for this shape is its form: no decimal point on any line (a
/// float with precision 0, or a rounded integer), and no `%` either (the host
/// glibc rejects `%.-1f` as a bad specifier and prints the literal `%.0-1f`,
/// which is an artifact of the libc the reference links against, not Minix
/// ground truth, so this gate deliberately does not use the reference's shape
/// as its criterion).
#[test]
fn the_unwritten_precision_family_prints_no_decimal_point_and_no_literal() {
    for args in [
        &["3"][..],
        &["-n", "3"][..],
        &["3", "-", "5"][..],
        &["-s", ",", "3"][..],
    ] {
        let (out, status, err) = run(args);
        assert_eq!(status, 0, "{args:?} 的退码：stderr 是 {err:?}");
        assert_eq!(err, "", "{args:?} 不该有诊断");
        assert!(!out.contains('.'), "{args:?} 的输出不该出现小数点：{out:?}");
        assert!(
            !out.contains('%'),
            "{args:?} 的输出不该是打回的说明符：{out:?}"
        );
        assert!(!out.is_empty(), "{args:?} 要有输出");
    }
}

/// The command seam for byte-shaped argv: `&str` cannot deliver a bare `ff`
/// (by the time an `OsStr` comes from a `str` it is already valid UTF-8), and
/// `jot`'s three value-taking letters (`-b`/`-s`/`-w`) take values that **are
/// output content**, so this fixture layer is required. Both output streams
/// are handed back byte by byte: the text form cannot distinguish the two
/// failure shapes "one byte" and "three bytes from folding".
fn run_argv(args: &[&[u8]]) -> (Vec<u8>, i32, Vec<u8>) {
    use std::os::unix::ffi::OsStrExt as _;
    let output = Command::new(JOT)
        .args(args.iter().map(|word| std::ffi::OsStr::from_bytes(word)))
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("the jot binary should be runnable");
    (
        output.stdout,
        output.status.code().unwrap_or(-1),
        output.stderr,
    )
}

/// The argument seam on `jot` changes the **printed content** directly: the
/// `-b`/`-w` word is sent to stdout verbatim by `putdata` (`jot.c:300-311`),
/// and the `-s` string is printed between entries by `fputs(sepstring,
/// stdout)` at `:310`. Below a text seam a bare `ff` is first folded into
/// `ef bf bd`, so every line gains two bytes. The counter-case is `%c`: C goes
/// through `putchar` and hands back **one** byte (the reference measured `c8`
/// for `-p 0 -w '%c' 1 200 1`), while `v as u8 as char` becomes the two bytes
/// `c3 88` through UTF-8 — this branch's `char` channel is out in the byte
/// shape.
#[test]
fn a_raw_byte_in_the_word_or_the_separator_is_printed_as_that_byte() {
    // (1) The `-b` word: both lines are `ff 77 64`.
    let (out, status, err) = run_argv(&[b"-b\xffwd", b"2"]);
    assert_eq!(status, 0, "stderr 是 {err:?}");
    assert_eq!(err, Vec::new(), "成功路一声不出");
    assert_eq!(out, vec![0xff, b'w', b'd', b'\n', 0xff, b'w', b'd', b'\n']);

    // (2) The `-s` string: what sits between entries is the two bytes `ff 78`,
    // not five.
    let (out, status, err) = run_argv(&[b"-s\xffx", b"3", b"1", b"5"]);
    assert_eq!(status, 0, "stderr 是 {err:?}");
    assert_eq!(
        out,
        vec![b'1', 0xff, b'x', b'3', 0xff, b'x', b'5', b'\n'],
        "分隔符必须按 argv 的原字节打出去"
    );

    // (3) In the `-w` word, the literal section before the percent sign goes
    // into the output as well.
    let (out, status, err) = run_argv(&[b"-wa\xff%d", b"2"]);
    assert_eq!(status, 0, "stderr 是 {err:?}");
    assert_eq!(out, vec![b'a', 0xff, b'1', b'\n', b'a', 0xff, b'2', b'\n']);

    // (4) `%%` in the prefix expands to one (the reference measured
    // `%x1|%x3|%x5` for `-p 2 -w '%%x%d' 3 1 5`).
    let (out, status, err) = run(&["-p", "2", "-w%%x%d", "3", "1", "5"]);
    assert_eq!(status, 0, "stderr 是 {err:?}");
    assert_eq!(out, "%x1\n%x3\n%x5\n", "两枚百分号进、一枚百分号出");

    // (5) `%c` hands back exactly one byte: another observable face of
    // "by bytes" on the output side.
    let (out, status, err) = run_argv(&[b"-p", b"0", b"-w%c", b"1", b"200", b"1"]);
    assert_eq!(status, 0, "stderr 是 {err:?}");
    assert_eq!(
        out,
        vec![0xc8, b'\n'],
        "`putchar` 那一枚就是 `c8`，不是 `c3 88`"
    );
}

/// The three legs that splice a value into a sentence all use the argv bytes
/// at the value position: `unknown or invalid format \`%s'` (`jot.c:385`,
/// `:396` — this branch previously had **no value at all** in this shell),
/// `Bad reps value:  %s` (`:215`), and `Too many arguments.  What do you mean
/// by %s?` (`:225`). All three are `errx`: one line, no usage, exit 1.
/// The other family is an observable **criterion** difference: an empty reps
/// is a legal 0 in C (`strtoul` leaves `ep` at the start), and with
/// begin/ender present it lands on `Infinite sequences cannot be bounded`
/// (`:272-275`) rather than the Bad reps sentence; an empty begin is an
/// out-of-bounds read of `argv[i][-1]` in C, and the host reference returns
/// `1 3 5` for `jot 3 '' 5` (that slot keeps its default), whereas this shell
/// used to **panic on the spot**.
#[test]
fn the_operand_and_the_word_echoed_into_a_sentence_are_the_argv_bytes() {
    // (1) Specifier validation failure: what sits between the backquotes is
    // the **whole format word**, including that raw byte.
    let (out, status, err) = run_argv(&[b"-w%\xff", b"2"]);
    assert_eq!(out, Vec::new(), "报文只走标准错误");
    assert_eq!(status, 1);
    assert_eq!(
        err,
        [
            b"jot: unknown or invalid format `%".as_slice(),
            &[0xff],
            b"'\n".as_slice()
        ]
        .concat(),
        "这一句在 C 带值；不带值就少了一整个可观察面"
    );

    // (2) Bad reps: what follows the two spaces is the raw byte.
    let (out, status, err) = run_argv(&[b"x\xff", b"2"]);
    assert_eq!(out, Vec::new());
    assert_eq!(status, 1);
    assert_eq!(
        err,
        [
            b"jot: Bad reps value:  x".as_slice(),
            &[0xff],
            b"\n".as_slice()
        ]
        .concat(),
        "两个空格是 C 的原句，值位是原字节"
    );

    // (3) The fifth operand: the fifth is reported, the first four are not
    // interpreted at all.
    let (out, status, err) = run_argv(&[b"1", b"2", b"3", b"4", b"5\xff"]);
    assert_eq!(out, Vec::new());
    assert_eq!(status, 1);
    assert_eq!(
        err,
        [
            b"jot: Too many arguments.  What do you mean by 5".as_slice(),
            &[0xff],
            b"?\n".as_slice(),
        ]
        .concat()
    );

    // (4) An empty reps is a legal 0: an unbounded sequence with
    // begin/ender lands on the bounded sentence, not on Bad reps.
    let (out, status, err) = run_argv(&[b"", b"1", b"2"]);
    assert_eq!(out, Vec::new(), "无界又不可能界定，一格都不该打");
    assert_eq!(status, 1);
    assert_eq!(err, b"jot: Infinite sequences cannot be bounded\n");

    // (5) A high-byte operand lands through `char` and is therefore negative:
    //     the reference measured `-1 2 5` for `jot 3 <ff> 5` (begin = -1, step
    //     (5-(-1))/2 = 3); reading it as unsigned would print `189 97 5` or
    //     `1 97 189`.
    let (out, status, err) = run_argv(&[b"3", &[0xff][..], b"5"]);
    assert_eq!(status, 0, "stderr 是 {err:?}");
    assert_eq!(out, b"-1\n2\n5\n".to_vec(), "最后一枚字节要经 signed char");

    // (6) Empty begin: that slot keeps its default (the reference measured
    // `1 3 5`); the key point is that it **no longer panics**.
    let (out, status, err) = run_argv(&[b"3", b"", b"5"]);
    assert_eq!(status, 0, "退码该是 0：stderr 是 {err:?}");
    assert_eq!(
        out,
        b"1\n3\n5\n".to_vec(),
        "begin 留缺省 1，步长由 (5-1)/(3-1) 推出来"
    );
}
