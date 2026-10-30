//! End-to-end face for the `random` binary's operand step, checked against
//! `minix3/games/random/random.c` (operand-count check `:89-104`,
//! `err(1, "%s", *argv)` `:96-97`, `errx(1, "denominator is not valid.")`
//! `:98-99`, `usage()` `:143-147`).
//!
//! All expected values were measured from the reference binary
//! `tmp/random-342/random_ref`; the per-shape readings are in
//! `tmp/random-343/ab343-ref.log`. Four shapes came out of the measurements:
//! ① Zero operands is **not** a missing argument: C's `case 0` fixes the
//!    denominator at 2 and filters as usual, exiting 0.
//! ② A single rejected operand reports `random: denominator is not valid.` —
//!    the prefix follows `argv[0]`, and the sentence carries its trailing period
//!    verbatim and **no operand** (`errx` has no `%s` slot).
//! ③ An operand that hits the numeric range reports the three-part shape
//!    `random: <operand>: <sentence>` (`err`), a different sentence from ②.
//! ④ The `usage()` line carries no program-name prefix: `random` is a literal
//!    hardwired into the string (`:144`) and stays put under a renamed
//!    invocation, while the prefixes of ② and ③ follow it —
//!    `a_renamed_invocation_moves_the_err_prefixes_only` pins exactly that
//!    difference.
//!
//! The denominator operand is read as C's `double` (the decimal leg of
//! `strtod`): `1e3`, `1e-3`, `.5`, `-2`, `inf` and `nan` are all accepted;
//! `18e308` (overflow) and `1e-400` (underflow to 0) both set
//! `errno == ERANGE` and take ③. Still open are the two shapes hexadecimal
//! float `0x1p3` and the `nan(0x...)` payload. The stdout leg is already
//! settled per C: on failure it reports `random: stdout: <sentence>` and exits
//! 2 (`random.c:131-133`), gated by
//! `a_stdout_that_stops_taking_bytes_is_blamed_by_name_and_exits_two`.

use std::fs::File;
use std::io::Write as _;
use std::os::unix::process::CommandExt;
use std::process::{Child, Command, Stdio};

fn random_bin() -> &'static str {
    env!("CARGO_BIN_EXE_random")
}

/// Feed stdin. **Only `BrokenPipe` is tolerable**: several shapes of `random`
/// exit before touching input (`-Z` falls into `usage()` and exits 1, `-e`
/// returns before reading stdin at `random.c:109-111`), so which side gets
/// scheduled first decides whether this write sees EPIPE — that is harness
/// timing, not a defect of the code under test. Other write errors still fail
/// on the spot. textfilter settled the same rule on its side
/// (`tests/face_support/mod.rs::write_stdin`); this is this crate's copy under
/// the same criterion (the two crates share no test-support module).
fn feed_stdin(child: &mut Child, input: &[u8]) {
    let mut stdin = child.stdin.take().expect("stdin pipe");
    if let Err(err) = stdin.write_all(input) {
        assert_eq!(
            err.kind(),
            std::io::ErrorKind::BrokenPipe,
            "写标准输入只该因为「子进程已退出」而失败，实际是 {err}"
        );
    }
}

/// Run one leg with the given `argv[0]`: feed `input` on stdin and collect the
/// three streams. Text is collected lossily — the messages themselves are
/// ASCII, but stdout may carry arbitrary bytes.
fn run_with_argv0(arg0: &str, args: &[&str], input: &[u8]) -> (String, i32, String) {
    let mut child = Command::new(random_bin())
        .arg0(arg0)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn random");
    feed_stdin(&mut child, input);
    let out = child.wait_with_output().expect("wait random");
    (
        String::from_utf8_lossy(&out.stdout).into_owned(),
        out.status.code().expect("exit code"),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

fn run(args: &[&str], input: &[u8]) -> (String, i32, String) {
    run_with_argv0("random", args, input)
}

/// Same as `run_with_argv0`, but all three streams come back as **raw bytes**.
/// A per-byte letter position and a lossy-converted code point are
/// indistinguishable in text form: `c3` and `c3 83` are both "one character"
/// after `from_utf8_lossy`, so a gate asserting on `String` cannot pin the
/// extra byte (the same lesson as textfilter).
fn run_bytes(args: &[&str], input: &[u8]) -> (Vec<u8>, i32, Vec<u8>) {
    let mut child = Command::new(random_bin())
        .arg0("random")
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn random");
    feed_stdin(&mut child, input);
    let out = child.wait_with_output().expect("wait random");
    (
        out.stdout,
        out.status.code().expect("exit code"),
        out.stderr,
    )
}

const INPUT: &[u8] = b"aa\nbb\ncc\ndd\nee\nff\n";
const INPUT_TEXT: &str = "aa\nbb\ncc\ndd\nee\nff\n";

/// `count` lines `line-0`...`line-(count-1)`, used to tell "it really draws"
/// apart from "it drew nothing".
fn many_lines(count: usize) -> String {
    let mut text = String::new();
    for index in 0..count {
        text.push_str(&format!("line-{index}\n"));
    }
    text
}

/// Output must be a subset of the input lines: the filter writes whole lines
/// out and never invents new ones.
fn assert_subset_of_input(stdout: &str, input: &str, shape: &str) {
    let pool: Vec<&str> = input.lines().collect();
    for line in stdout.lines() {
        assert!(pool.contains(&line), "{shape}: 输出行 {line:?} 不在输入里");
    }
}

#[test]
fn no_operand_filters_with_denominator_two_instead_of_complaining() {
    // C's `case 0` (`random.c:90-92`): with zero operands the denominator is 2
    // and stdin is read as usual. The reference binary measured exit 0, a
    // subset of the input lines printed, and nothing on stderr.
    // The six-line input only checks "output is a subset of the input"; it does
    // **not** check the exit code: this filter body also has an invention that
    // says nothing selected and exits 1 when no line is drawn (`random.c:127-137`
    // has no such shape; that is the filter body's own account), and with six
    // lines the probability of "nothing drawn" is 2^-6, too weak to pin the exit
    // code. The exit code is left to the 200-line shape below, where "nothing
    // drawn" has probability 2^-200.
    let six = run(&[], INPUT);
    assert_subset_of_input(&six.0, INPUT_TEXT, "零枚操作数");
    // Drawing 200 lines and hitting none has probability 2^-200: non-empty
    // output is proof that the filter is really running.
    let text = many_lines(200);
    let (out, code, err) = run(&[], text.as_bytes());
    assert_eq!((code, err.as_str()), (0, ""), "stdout={out:?}");
    assert!(
        !out.is_empty(),
        "零枚操作数时应当掷中若干行，实测一行都没有"
    );
    assert_subset_of_input(&out, &text, "零枚 200 行");
}

#[test]
fn one_operand_still_feeds_the_filter() {
    // Denominator 1 ⇒ every line is drawn (`draw` is always 0), so this shape
    // is deterministic: output and input are **byte-identical**. An earlier
    // attempt only dared check "non-empty and a subset of the input lines",
    // because the body then kept only the last drawn line (it printed just
    // `ff\n`); now that the filter body is wired into C's streaming shape, the
    // equality can be pinned here.
    let (out, code, err) = run(&["1"], INPUT);
    assert_eq!((out.as_str(), code, err.as_str()), (INPUT_TEXT, 0, ""));
    // Denominator 6 lies between "all drawn" and "none drawn": only exit 0, no
    // error, and output as a subset of the input lines are required.
    let (out, code, err) = run(&["6"], INPUT);
    assert_eq!((code, err.as_str()), (0, ""), "stdout={out:?}");
    assert_subset_of_input(&out, INPUT_TEXT, "分母 6");
}

#[test]
fn a_rejected_operand_says_the_errx_sentence_without_naming_itself() {
    // Measured from the reference binary: `0`, `abc` and `-` all give
    // `random_ref: denominator is not valid.`, exit 1, empty stdout. The prefix
    // here is `random` (i.e. `argv[0]`), and the sentence carries no operand.
    for word in ["0", "", "abc", "-", "6x"] {
        let (out, code, err) = run(&[word], INPUT);
        assert_eq!(
            (out.as_str(), code, err.as_str()),
            ("", 1, "random: denominator is not valid.\n"),
            "{word:?}"
        );
    }
    // The scope of ② is drawn by two C criteria: `e5` (not one character
    // consumed), `1e` (exponent slot empty, `ep` stops at `e`), `infi`
    // (`inity` fails at the first cell), `3 ` (the tail is not consumed) and
    // `0e99999` (all zeros do not set ERANGE, so it falls back to "not a valid
    // denominator" rather than ③).
    for word in ["e5", "1e", "infi", "3 ", "0e99999"] {
        let (out, code, err) = run(&[word], INPUT);
        assert_eq!(
            (out.as_str(), code, err.as_str()),
            ("", 1, "random: denominator is not valid.\n"),
            "{word:?}"
        );
    }
}

#[test]
fn an_overflowed_operand_uses_the_three_part_err_shape() {
    // `err(1, "%s", *argv)` (`:96-97`) has one more slot than ②: the operand
    // sits in the middle, and the tail is the sentence for the errno. The
    // sentence comes from `SYS_ERRLIST` in `minix-types`
    // (`os/libs/minix-types/src/types/errno.rs:426`, entry 34,
    // `Result too large or too small`); the reference binary on the host uses
    // glibc's table (`Numerical result out of range`), which is a carrier
    // difference. `errno.rs:383` records this pair: overflow (`strtod.c`'s
    // `ovfl`) and underflow of a non-zero digit string to 0 (`undfl`) merge into
    // the same `range_err`, and both shapes report ③. `4294967296` is not in
    // this column: it is exactly 2^32 in `double`, and the reference binary
    // measures exit 0 (`tmp/random-343/ab343-ref.log`) — this gate used to pin
    // the implementation's u32 overflow, not the reference behavior.
    for word in ["18e308", "1e-400"] {
        let (out, code, err) = run(&[word], INPUT);
        let want = format!("random: {word}: Result too large or too small\n");
        assert_eq!(
            (out.as_str(), code, err.as_str()),
            ("", 1, want.as_str()),
            "{word:?}"
        );
        // Only one line: the three-part shape is not followed by the usage block
        // and no second sentence is appended.
        assert_eq!(
            err.lines().filter(|line| !line.is_empty()).count(),
            1,
            "三段形应当只有一行：{err:?}"
        );
    }
}

#[test]
fn two_or_more_operands_print_the_hardwired_usage_line() {
    // The reference binary measures `usage: random [-er] [denominator]\n`,
    // exit 1, empty stdout — note that this line has **no** program-name prefix
    // and spells out `-er`.
    for args in [&["3", "4"][..], &["3", ""][..], &["3", "4", "5"][..]] {
        let (out, code, err) = run(args, INPUT);
        assert_eq!(
            (out.as_str(), code, err.as_str()),
            ("", 1, "usage: random [-er] [denominator]\n"),
            "{args:?}"
        );
    }
    // The count is about the count, not the content: one empty string still
    // takes ② and never falls through to usage.
    let (_, code, err) = run(&[""], INPUT);
    assert_eq!(
        (code, err.as_str()),
        (1, "random: denominator is not valid.\n")
    );
}

#[test]
fn a_renamed_invocation_moves_the_err_prefixes_only() {
    // Two prefix origins within the same command: `errx`/`err` use
    // `getprogname()` (i.e. `argv[0]`), while `usage()` hardwires `random` into
    // the string (`:144`). In the reference binary the rejected shape indeed
    // carries `random_ref:`.
    let (_, code, err) = run_with_argv0("renamed-random", &["abc"], INPUT);
    assert_eq!(
        (code, err.as_str()),
        (1, "renamed-random: denominator is not valid.\n")
    );
    // Swap the overflow operand for `18e308`: `4294967296` is exactly 2^32 in
    // `double` and is accepted without an error.
    let (_, code, err) = run_with_argv0("renamed-random", &["18e308"], INPUT);
    assert_eq!(
        (code, err.as_str()),
        (1, "renamed-random: 18e308: Result too large or too small\n")
    );
    let (_, code, err) = run_with_argv0("renamed-random", &["3", "4"], INPUT);
    assert_eq!(
        (code, err.as_str()),
        (1, "usage: random [-er] [denominator]\n")
    );
}
#[test]
fn selected_lines_come_out_in_input_order() {
    // C is a single streaming pass (`random.c:126-137`): it `putchar`s each
    // selected line, so output is necessarily an **order-preserving
    // subsequence** of the input lines. "Keep only the last drawn line" is
    // exposed immediately by this input.
    let text = many_lines(200);
    let (out, code, err) = run(&[], text.as_bytes());
    assert_eq!((code, err.as_str()), (0, ""), "stdout={out:?}");
    let pool: Vec<&str> = text.lines().collect();
    let printed: Vec<&str> = out.lines().collect();
    assert!(!printed.is_empty(), "200 行、分母 2 却一行都没掷中");
    let mut cursor = 0usize;
    for line in printed {
        let found = pool[cursor..]
            .iter()
            .position(|candidate| *candidate == line)
            .unwrap_or_else(|| {
                panic!("输出行 {line:?} 不在游标 {cursor} 之后：顺序被打乱或造了新行")
            });
        cursor += found + 1;
    }
}

#[test]
fn an_empty_selection_is_silent_and_exits_zero() {
    // C has no notion of "no line drawn": after the loop it just `exit(0)`s
    // (`random.c:114-137`). Denominator 4 billion with one input line gives a
    // draw probability of 2.5e-10, so this shape deterministically yields empty
    // output. The thin shell once invented a `random: nothing selected`
    // sentence and exited 1; the reference binary has no such shape.
    let (out, code, err) = run(&["4000000000"], b"aa\n");
    assert_eq!((out.as_str(), code, err.as_str()), ("", 0, ""));
}

#[test]
fn an_unreadable_stdin_blames_the_stdin_word_and_exits_two() {
    // The `ferror(stdin)` path is `err(2, "stdin")` (`random.c:135-136`): the
    // three-part shape, with the word `stdin` in the slot instead of a file
    // name, and exit code 2. Measured with a directory passed as stdin, the
    // reference binary gives `random_ref: stdin: Is a directory`, exit 2 (last
    // line of `tmp/random-345/ab345-ref.log`). This tree takes the sentence
    // from entry 21 of the `minix-types` table (`errno.rs:413`), the same
    // wording as glibc.
    let dir = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("stdin-is-a-dir");
    std::fs::create_dir_all(&dir).expect("create dir");
    let handle = std::fs::File::open(&dir).expect("open a directory read-only");
    let child = Command::new(random_bin())
        .arg("3")
        .stdin(Stdio::from(handle))
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn random");
    let bytes = child.wait_with_output().expect("wait random");
    let stdout = String::from_utf8_lossy(&bytes.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&bytes.stderr).into_owned();
    assert_eq!(
        (
            stdout.as_str(),
            bytes.status.code().expect("exit code"),
            stderr.as_str()
        ),
        ("", 2, "random: stdin: Is a directory\n")
    );
}
#[test]
fn an_unknown_letter_gets_the_libc_line_then_the_usage_line() {
    // The two sentences differ in order and prefix: the first line uses
    // `getprogname()`, the second line's `random` is hardwired into the string.
    // The wording comes from Minix's own
    // `minix3/lib/libc/stdlib/getopt.c:101-104` (`unknown option -- %c`, letter
    // unquoted). This repo's early reference binary `tmp/random-342/random_ref`
    // linked the host glibc `getopt`, whose sentence is
    // `invalid option -- 'Z'` and cannot carry this ground truth; the reference
    // binary `tmp/random-421/random` was relinked against Minix's `getopt.c`
    // (started through a same-name symlink so the program name matches this
    // shell), and it measures all three shapes `-Z`/`-eZ`/`-Ze` as this
    // sentence + the usage line, exit 1.
    for args in [&["-Z"][..], &["-eZ"][..], &["-Ze"][..]] {
        let (out, code, err) = run(args, INPUT);
        assert_eq!(
            (out.as_str(), code, err.as_str()),
            (
                "",
                1,
                "random: unknown option -- Z\nusage: random [-er] [denominator]\n"
            ),
            "{args:?}"
        );
    }
}

/// The letter slot is the **first byte** after the dash: the reference binary
/// `tmp/random-421/random` measures both `-é` (`c3 a9`) and `-eé` as
/// `random: unknown option -- ` + `303` (=`c3`) + newline + the usage line,
/// exit 1 (`getopt.c:79`'s `optopt = *place++` advances one byte at a time,
/// and `:103` writes it out raw with `%c`). This branch's loop belongs to the
/// "stop at the first bad letter" family (`random.c:80-83`'s
/// `default: case '?': usage();` sits in the loop and exits on the spot), so
/// unlike `seq` there is only **one** libc line here.
#[test]
fn the_libc_line_of_a_multibyte_flag_is_one_byte_for_random() {
    let want: Vec<u8> = [
        b"random: unknown option -- ".as_slice(),
        b"\xc3\n",
        b"usage: random [-er] [denominator]\n",
    ]
    .concat();
    for form in ["-\u{e9}", "-e\u{e9}"] {
        let (out, code, err) = run_bytes(&[form], INPUT);
        assert_eq!(out, Vec::new(), "{form} 的报文只走 stderr");
        assert_eq!(code, 1, "{form} 的 stderr 是 {err:?}");
        assert_eq!(err, want, "{form} 的字母该是一个裸字节 c3");
    }

    // The U+0080 cell is the criterion for this rendering rule: its UTF-8
    // encoding is two bytes `c2 80`, and C prints the first one, `c2`; packing
    // by characters would emit both `c2 80` (one byte too many).
    let (out, code, err) = run_bytes(&["-\u{80}"], INPUT);
    assert_eq!(out, Vec::new());
    assert_eq!(code, 1);
    assert_eq!(
        err,
        [
            b"random: unknown option -- ".as_slice(),
            b"\xc2\n",
            b"usage: random [-er] [denominator]\n",
        ]
        .concat()
    );

    // The ASCII shape must not be affected by the rendering change; a cluster
    // with a valid letter first and a bad letter after still stops at the first
    // bad one.
    for (form, letter) in [("-x", b"x"), ("-Z\u{e9}", b"Z")] {
        let (_out, code, err) = run_bytes(&[form], INPUT);
        assert_eq!(code, 1, "{form}");
        assert_eq!(
            err,
            [
                b"random: unknown option -- ".as_slice(),
                letter.as_slice(),
                b"\nusage: random [-er] [denominator]\n".as_slice(),
            ]
            .concat(),
            "{form}"
        );
    }
}

#[test]
fn dash_dash_ends_the_ring_and_the_rest_are_operands() {
    // `-- 1`: the loop ends at `--`, and the following operand is the
    // denominator ⇒ every line is drawn, output byte-identical to input.
    let (out, code, err) = run(&["--", "1"], INPUT);
    assert_eq!((out.as_str(), code, err.as_str()), (INPUT_TEXT, 0, ""));
    // Bare `--` equals zero operands: denominator 2, filtering as usual; this
    // shape **allows** empty output (not drawing is not an error), so only the
    // exit code and stderr are pinned, not the random quantities.
    let (_, code, err) = run(&["--"], INPUT);
    assert_eq!((code, err.as_str()), (0, ""));
    // A bare `-` is neither a terminator nor a flag: it is simply the rejected
    // operand (the same shape appears in the earlier gate).
    let (_, code, err) = run(&["-e", "-"], INPUT);
    assert_eq!(
        (code, err.as_str()),
        (1, "random: denominator is not valid.\n")
    );
}

#[test]
fn the_e_flag_returns_a_draw_and_reads_nothing() {
    // `random.c:109-111`: this branch returns before the `getchar()` loop, so
    // stdout is necessarily empty and stderr necessarily empty, and the exit
    // code falls in the closed interval [0, denom] of
    // `(denom * random()) / RANDOM_MAX`.
    let (out, code, err) = run(&["-e", "1"], INPUT);
    assert_eq!((out.as_str(), code, err.as_str()), ("", 0, ""));
    for _ in 0..12 {
        let (out, code, err) = run(&["-e", "4"], INPUT);
        assert!(
            (out.is_empty()) && err.is_empty(),
            "out={out:?} err={err:?}"
        );
        assert!(
            (0..=4).contains(&code),
            "`-e 4` 打出退码 {code}，越出闭区间 [0, 4]"
        );
    }
    // `-r` only disables buffering: the write seam already flushes line by line,
    // so it has the same shape as without `-r` (all six lines drawn).
    let (out, code, err) = run(&["-r", "1"], INPUT);
    assert_eq!((out.as_str(), code, err.as_str()), (INPUT_TEXT, 0, ""));
}

#[test]
fn a_scientific_denominator_is_accepted_as_a_double() {
    // `1e3` takes C's exponent branch (`strtod.c`'s
    // `if (c == 'e' || c == 'E')`): once accepted, filtering proceeds as usual.
    // With a 1/1000 draw rate six input lines may well draw none, so what is
    // pinned is the pair "does not exit 1, does not report an error".
    let (out, code, err) = run(&["1e3"], INPUT);
    assert_eq!((code, err.as_str()), (0, ""), "stdout={out:?}");
    assert_subset_of_input(&out, INPUT_TEXT, "分母 1e3");
}

#[test]
fn a_small_fraction_denominator_prints_every_line() {
    // `1e-3`: `(int)(1e-3 * random() / RANDOM_MAX)` gives a quotient of at most
    // about 4.7e-13, which truncates toward zero to 0, so every line is drawn —
    // output byte-identical to input. A denominator below 1 is a legal
    // "pass everything" in C.
    let text = many_lines(60);
    let (out, code, err) = run(&["1e-3"], text.as_bytes());
    assert_eq!((code, err.as_str()), (0, ""), "stdout={out:?}");
    assert_eq!(out, text);
}

#[test]
fn a_negative_denominator_is_accepted_and_prints_every_line() {
    // A negative number is not an "invalid denominator": C never says `denom`
    // must be positive, only that it must not be 0 and that the tail must be
    // fully consumed. The quotient of `-1e-3` lands in [-4.7e-13, 0], truncating
    // toward zero to 0 → every line is drawn.
    let text = many_lines(60);
    let (out, code, err) = run(&["--", "-1e-3"], text.as_bytes());
    assert_eq!((code, err.as_str()), (0, ""), "stdout={out:?}");
    assert_eq!(out, text);
}

#[test]
fn a_big_integer_denominator_is_not_out_of_range() {
    // `2^32` is exactly representable in `double`, and `strtod` does not set
    // ERANGE: this branch used to judge it as overflow on the u32 surface,
    // reporting the reference's exit-0 shape as the three-part shape.
    let (out, code, err) = run(&["4294967296"], INPUT);
    assert_eq!((code, err.as_str()), (0, ""), "stdout={out:?}");
    assert_subset_of_input(&out, INPUT_TEXT, "分母 2^32");
}

#[test]
fn infinity_and_nan_are_accepted_and_never_select_a_line() {
    // `inf`/`nan` are accepted via `INFNAN_CHECK` (enabled at
    // `gdtoaimp.h:191` and `:643-645`). C's criterion is "truncate to integer
    // and compare against 0": for a non-finite value that conversion is
    // undefined behavior in C, and the x86-64 the reference ran on yields
    // `0x80000000`, which is not 0 → no line is drawn, but the exit code is
    // still 0 and no error is reported.
    let text = many_lines(40);
    for word in ["inf", "nan"] {
        let (out, code, err) = run(&[word], text.as_bytes());
        assert_eq!((out.as_str(), code, err.as_str()), ("", 0, ""), "{word:?}");
    }
}

#[test]
fn leading_whitespace_is_skipped_and_trailing_is_not() {
    // C's leading `switch (*s)` eats whitespace but the trailing whitespace is
    // not eaten: `" 3"` is accepted, while `"3 "` falls to ② because
    // `*ep != '\0'`. The two shapes differ by one space and take two different
    // exits.
    let (out, code, err) = run(&[" 3"], INPUT);
    assert_eq!((code, err.as_str()), (0, ""), "stdout={out:?}");
    assert_subset_of_input(&out, INPUT_TEXT, "前导空白");
    let (out, code, err) = run(&["3 "], INPUT);
    assert_eq!(
        (out.as_str(), code, err.as_str()),
        ("", 1, "random: denominator is not valid.\n")
    );
}

#[test]
fn a_hex_denominator_is_accepted_without_a_binary_exponent() {
    // `0x10` is 16: the `pcheck:` `switch (*s)` in `gethex` has only the two
    // `case`s `p`/`P` and no `default` rejection branch
    // (`minix3/lib/libc/gdtoa/gethex.c:110-112`), so omitting the exponent costs
    // nothing.
    let (out, code, err) = run(&["0x10"], INPUT);
    assert_eq!((code, err.as_str()), (0, ""), "stdout={out:?}");
    assert_subset_of_input(&out, INPUT_TEXT, "分母 0x10");
}

#[test]
fn a_hex_denominator_with_a_binary_exponent_is_accepted() {
    // `0x1p3` is 8: the binary exponent's weight is a power of two, and
    // `gethex.c:111-137` reads the exponent as decimal digits and then folds it
    // into a power of two.
    let (out, code, err) = run(&["0x1p3"], INPUT);
    assert_eq!((code, err.as_str()), (0, ""), "stdout={out:?}");
    assert_subset_of_input(&out, INPUT_TEXT, "分母 0x1p3");
}

#[test]
fn a_hex_overflow_is_accepted_rather_than_reported_out_of_range() {
    // Diverges from the decimal `18e308`: the hexadecimal branch of `strtod.c`
    // distinguishes only `NoNumber`/`Zero` in its `switch`, and everything else
    // goes through `ULtod` and straight to `goto ret`, never entering
    // `range_err`, so `errno` is untouched — the denominator is an accepted
    // infinity, no line is drawn, exit 0, no error. The reference binary is
    // compiled against the host glibc, which sets ERANGE for this shape and
    // reports `Numerical result out of range`; that is a carrier difference, not
    // behavior to copy.
    let text = many_lines(30);
    let (out, code, err) = run(&["0x1p99999999"], text.as_bytes());
    assert_eq!((out.as_str(), code, err.as_str()), ("", 0, ""));
}

#[test]
fn a_hex_operand_without_any_digit_is_not_valid() {
    // `0x` and `0xg` contain no hexadecimal digit at all: `gethex` returns
    // `STRTOG_Zero` with `*sp = s0 - 1`, so `denom == 0` blocks it and ② is
    // reported rather than ③.
    for word in ["0x", "0xg"] {
        let (out, code, err) = run(&[word], INPUT);
        assert_eq!(
            (out.as_str(), code, err.as_str()),
            ("", 1, "random: denominator is not valid.\n"),
            "{word:?}"
        );
    }
}

#[test]
fn a_nan_payload_is_consumed_and_never_selects_a_line() {
    // `nan(0x1f)`: the block at `strtod.c:314` (`#ifndef No_Hex_NaN`, a macro
    // this repository never defines) hands the payload to `hexnan`, which eats
    // the whole string when it hits the closing parenthesis — so `*ep == '\0'`,
    // `denom == 0` is always false, and it is accepted. The payload's bit
    // pattern is invisible to `random`: every NaN draws nothing and exits 0.
    let text = many_lines(25);
    for word in ["nan(0x1f)", "nan()", "nan(xy)"] {
        let (out, code, err) = run(&[word], text.as_bytes());
        assert_eq!((out.as_str(), code, err.as_str()), ("", 0, ""), "{word:?}");
    }
}

#[test]
fn an_unclosed_nan_payload_is_rejected_at_the_open_parenthesis() {
    // Without a closing parenthesis `hexnan` leaves the cursor alone (the
    // `return STRTOG_NaN` at `gdtoa/hexnan.c:146-147` does not write `*sp`), so
    // `ep` stays at the opening parenthesis → `*ep != '\0'` → "not a valid
    // denominator" is reported, not the three-part shape.
    for word in ["nan(0x1f", "nan("] {
        let (out, code, err) = run(&[word], INPUT);
        assert_eq!(
            (out.as_str(), code, err.as_str()),
            ("", 1, "random: denominator is not valid.\n"),
            "{word:?}"
        );
    }
}

#[test]
fn a_stdout_that_stops_taking_bytes_is_blamed_by_name_and_exits_two() {
    // C checks `ferror(stdout)` after each written line and, on failure, takes
    // `err(2, "stdout")` (`random.c:131-133`): the three-part shape carries the
    // word `stdout` in the slot instead of a file name (stdout is this branch's
    // only output leg), and the exit code is 2. The host injects this shape via
    // `/dev/full` — writing it always answers "no space left on device", and the
    // sentence is rendered from the errno table. Denominator 1:
    // `denom * draw / RANDOM_MAX` is always below 1, so every line is drawn and
    // the first line hits it. The expected reading was taken from a live run
    // (`od -c`; see rewrite-notes/18-stage-commands/todo.md).
    let mut child = Command::new(random_bin())
        .arg0("random")
        .args(["1"])
        .stdin(Stdio::piped())
        .stdout(Stdio::from(
            File::create("/dev/full").expect("open /dev/full"),
        ))
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn random against /dev/full");
    feed_stdin(&mut child, INPUT);
    let out = child.wait_with_output().expect("wait random");
    let err = String::from_utf8_lossy(&out.stderr).into_owned();
    assert_eq!(
        (out.status.code(), err.as_str()),
        (Some(2), "random: stdout: No space left on device\n"),
        "{err}"
    );
}
