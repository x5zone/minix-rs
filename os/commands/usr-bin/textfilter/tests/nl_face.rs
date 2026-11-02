//! End-to-end face for the `nl` binary: real process, real pipes, checked against
//! `minix3/usr.bin/nl/nl.c` (the option loop `:149-233`, the operand guard
//! `:239-249`, `parse_numbering` `:340-373` and `usage()` `:400-405`) plus the
//! libc `getopt` it drives (`minix3/lib/libc/stdlib/getopt.c:101-104`, `:132-136`).
//!
//! Two exit shapes must be kept apart; that is the whole point of this gate: errors
//! **getopt itself** reports (a letter outside the set, a value-taking option missing
//! its value) are "the libc line + the command's two usage lines"; the command's own
//! `errx` is a single line with no usage after it. The old wording squashed both into
//! one invented line and had no per-letter getopt scan (`-ba`, `-pb t` both fell into
//! the unknown-letter branch).

mod face_support;

use face_support::write_stdin;
use std::process::{Command, Stdio};

const NL: &str = env!("CARGO_BIN_EXE_nl");

/// The two verbatim lines of C `usage()`: the first ends with `\n\t`, the second is indented with a tab character.
const USAGE: &str = concat!(
    "Usage: nl [-p] [-b type] [-d delim] [-f type] [-h type] [-i incr] [-l num]\n",
    "\t[-n format] [-s sep] [-v startnum] [-w width] [file]\n"
);

fn run(args: &[&str], input: &[u8]) -> (String, i32, String) {
    let mut child = Command::new(NL)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the nl binary should be runnable");
    write_stdin(&mut child, input);
    let output = child.wait_with_output().expect("the run should finish");
    (
        String::from_utf8_lossy(&output.stdout).into_owned(),
        output.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

/// Same command seam as `run`, but standard error comes back **byte by byte**:
/// `from_utf8_lossy` renders a bare `c3` as a three-byte U+FFFD, and the text form
/// cannot tell a one-byte error from a two-byte one.
fn run_bytes(args: &[&str], input: &[u8]) -> (Vec<u8>, i32, Vec<u8>) {
    let mut child = Command::new(NL)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the nl binary should be runnable");
    write_stdin(&mut child, input);
    let output = child.wait_with_output().expect("the run should finish");
    (
        output.stdout,
        output.status.code().unwrap_or(-1),
        output.stderr,
    )
}

/// The letter position on the unknown-letter line is **one bare byte**. Each expected
/// shape is taken from the reference fixture `tmp/nl-417/nl_ref` (compiled from
/// `minix3/usr.bin/nl/nl.c` + Minix's `getopt.c` + the shim, started through a
/// same-name symlink `nl` so the program-name segment matches this shell; this family
/// had no Minix-blooded nl reference before, so the earlier readings were derived
/// straight from the C source): `-é` and `-pé` both give `nl: unknown option -- ` + a
/// single `c3` + newline followed by those **two** usage lines, exit 1; `-Zé` stops at
/// `Z`; in the counter-case `-dé`, `é` is a **value** (only `p` takes no value in the
/// option string `"pb:d:f:h:i:l:n:s:v:w:"`), used as a regex, exit 0 and no report.
/// This branch's whole loop lives in the thin shell (`src/bin/nl.rs:136`'s
/// `bytes[pos] as char`) with no unit-testable library function, so the reading half and
/// the rendering half are both carried by this one face gate (same shape as `uniq`; the
/// conclusion there was "if the loop is not in the library, do not fabricate a library
/// gate").
#[test]
fn the_libc_line_of_a_multibyte_flag_is_one_byte_for_nl() {
    let want: Vec<u8> = [
        b"nl: unknown option -- ".as_slice(),
        b"\xc3\n",
        USAGE.as_bytes(),
    ]
    .concat();
    for form in ["-\u{e9}", "-p\u{e9}"] {
        let (out, status, err) = run_bytes(&[form], b"a\n");
        assert_eq!(out, Vec::new(), "{form} 的报文只走标准错误");
        assert_eq!(status, 1, "{form} 的 stderr 是 {err:?}");
        assert_eq!(err, want, "{form} 的字母该是一个裸字节 c3");
    }

    let (_out, status, err) = run_bytes(&["-Z\u{e9}"], b"a\n");
    assert_eq!(status, 1);
    assert_eq!(
        err,
        [b"nl: unknown option -- Z\n".as_slice(), USAGE.as_bytes(),].concat(),
        "环停在第一个坏字母上，第二个字节根本不该被读到"
    );

    let (out, status, err) = run_bytes(&["-d\u{e9}"], b"a\n");
    assert_eq!(status, 0, "带值字母把两字节整个当值收走，退码是成功的");
    assert_eq!(err, Vec::new(), "那一形不报案：{err:?}");
    assert_eq!(
        out,
        b"     1\ta\n".to_vec(),
        "分隔符正则用不上就不改行（参照件同形）"
    );
}

/// Unknown letter: libc first prints `nl: unknown option -- <letter>` (`getopt.c:101-104`),
/// then `case '?': default: usage()` (`nl.c:230-233`) — three lines, exit 1.
#[test]
fn an_unknown_letter_prints_the_libc_line_then_the_usage() {
    let (out, status, err) = run(&["-Z"], b"a\n");
    assert_eq!(out, "", "报文只走 stderr");
    assert_eq!(status, 1, "stderr 是 {err:?}");
    assert_eq!(err, format!("nl: unknown option -- Z\n{USAGE}"));
}

/// Value-taking option missing its value: libc's other sentence (`getopt.c:132-136`),
/// likewise followed by usage. The old wording treated this branch as "illegal value"
/// and used the rejected value's own sentence, merging the two branches into one.
#[test]
fn a_value_taking_letter_without_its_value_prints_the_libc_line() {
    for (letter, args) in [('b', &["-b"][..]), ('w', &["-w"][..]), ('n', &["-n"][..])] {
        let (out, status, err) = run(args, b"a\n");
        assert_eq!(out, "");
        assert_eq!(status, 1, "{args:?} 的 stderr 是 {err:?}");
        assert_eq!(
            err,
            format!("nl: option requires an argument -- {letter}\n{USAGE}"),
            "{args:?}"
        );
    }
}

/// The `parse_numbering` sentence carries the **section name** (the `name` fields of
/// `numbering_properties[]` are footer / body / header in order, `nl.c:73-75`) and prints
/// the whole rejected value (`:366-369`). One line only, no usage.
#[test]
fn numbering_type_errors_name_their_own_section() {
    for (flag, name) in [('b', "body"), ('f', "footer"), ('h', "header")] {
        let (out, status, err) = run(&[&format!("-{flag}"), "q"], b"a\n");
        assert_eq!(out, "");
        assert_eq!(status, 1);
        assert_eq!(
            err,
            format!("nl: illegal {name} line numbering type -- q\n"),
            "-{flag} 的句子该带自己的段名"
        );
    }
}

/// getopt's per-letter scan: a value may be attached to the same token (`-ba`), and
/// letters may be clustered (`-pb t`). The old wording matched whole tokens, so both
/// shapes fell into the unknown-letter branch.
#[test]
fn inline_and_clustered_letters_parse_like_getopt() {
    // This gate covers only the diagnostic face: the shape of the numbered output belongs to the library tests in `src/nl.rs`; the binary's stdout is not used as gold here.
    let (out, status, err) = run(&["-ba", "-w", "4"], b"x\n");
    assert_eq!((status, err.as_str()), (0, ""), "附值形该被接受");
    assert!(!out.is_empty(), "被接受就该有编号输出");
    let (out, status, err) = run(&["-pb", "t"], b"x\n");
    assert_eq!((status, err.as_str()), (0, ""), "聚簇形该被接受");
    assert!(!out.is_empty(), "被接受就该有编号输出");
}

/// Each of the three `-w` branches has a different sentence: unparseable is
/// `invalid width value -- <value>`, parsed but ≤ 0 is
/// `width argument must be > 0 -- <value>` (`nl.c:220-228`, the value is printed too).
#[test]
fn width_rejects_unparseable_and_nonpositive_with_different_sentences() {
    let (_, status, err) = run(&["-w", "x"], b"a\n");
    assert_eq!(
        (status, err.as_str()),
        (1, "nl: invalid width value -- x\n")
    );
    let (_, status, err) = run(&["-w", "0"], b"a\n");
    assert_eq!(
        (status, err.as_str()),
        (1, "nl: width argument must be > 0 -- 0\n")
    );
}

/// `-d` takes the first two characters and rejects only the third (`nl.c:157-169`); an
/// empty value leaves the default untouched (neither `if (optarg[n] != '\0')` holds), so
/// `-d ''` is a legal no-op.
#[test]
fn delim_takes_two_characters_and_refuses_a_third() {
    let (_, status, err) = run(&["-d", "abc"], b"a\n");
    assert_eq!(
        (status, err.as_str()),
        (1, "nl: invalid delim argument -- abc\n")
    );
    let (out, status, err) = run(&["-d", ""], b"x\n");
    assert_eq!((status, err.as_str()), (0, ""), "空值该是无操作");
    assert!(!out.is_empty(), "被接受就该有编号输出");
    // 单字节：`optarg[1]` 已是空字节，`delim[1]` 保持默认——裁决要求空、单、
    // 双三种长度分别测试，这是单字节那一枚。
    let (out, status, err) = run(&["-d", "x"], b"x\n");
    assert_eq!((status, err.as_str()), (0, ""), "单字节该被接受：{err}");
    assert!(!out.is_empty(), "被接受就该有编号输出");
}

/// The remaining numeric branches each report their own sentence and all print the rejected value verbatim (`nl.c:176-228`).
#[test]
fn the_number_arguments_quote_the_rejected_value() {
    for (args, sentence) in [
        (vec!["-i", "2x"], "invalid incr argument -- 2x"),
        (vec!["-l", "2x"], "invalid num argument -- 2x"),
        (vec!["-v", "1x"], "invalid startnum value -- 1x"),
        (vec!["-n", "xy"], "illegal format -- xy"),
    ] {
        let (_, status, err) = run(&args, b"a\n");
        assert_eq!(status, 1, "{args:?} 的 stderr 是 {err:?}");
        assert_eq!(err, format!("nl: {sentence}\n"), "{args:?}");
    }
}

/// C's counters are `int` and neither arm has a sign test (`nl.c:176-183` for
/// `-i`, `:208-216` for `-v`), so a negative first number and a negative
/// increment are accepted and the numbering runs backwards. The reference
/// `tmp/nl-417/nl_ref` reads `-v 3 -i -1` as `3/2/1` and `-v -2` as
/// `-2/-1/0` in the default 6-wide right-justified field — this closes
/// `NL-SIGNED-COUNTERS`, the leg that used to answer with an invented
/// "signed numbering counters (-i/-v) not wired".
#[test]
fn signed_counters_run_backwards_end_to_end() {
    let (out, status, err) = run(&["-v", "3", "-i", "-1"], b"a\nb\nc\n");
    assert_eq!(status, 0, "{err}");
    assert_eq!(out, "     3\ta\n     2\tb\n     1\tc\n");
    let (out, status, err) = run(&["-v", "-2"], b"a\nb\nc\n");
    assert_eq!(status, 0, "{err}");
    assert_eq!(out, "    -2\ta\n    -1\tb\n     0\tc\n");
    // The increment may also cross the boundary the counter wraps at; the
    // field is widened so this run never reaches the truncation leg.
    let (out, status, _) = run(&["-v", "2147483647", "-i", "1", "-w", "12"], b"a\nb\n");
    assert_eq!(status, 0);
    assert_eq!(out, "  2147483647\ta\n -2147483648\tb\n");
}

/// The accepted domain is C's `int`: Minix's userland `long` is 32 bits on
/// i386, and C casts the result to `int` anyway (`incr = (int)val`,
/// `startnum = (int)val`), so a value outside `int` takes the same sentence as
/// trailing junk does. **The host-built reference cannot pin this shape** — it
/// links glibc, whose 64-bit `long` accepts `2147483648` and truncates it in
/// the cast, so the judgment here is the C source plus the platform width
/// (recorded in 18 篇 with the reading that shows the divergence). The
/// boundary values inside the domain are accepted, `i32::MIN` included.
#[test]
fn a_value_beyond_the_int_domain_takes_the_rejecting_sentence() {
    for (flag, value, sentence) in [
        ("-i", "2147483648", "invalid incr argument -- 2147483648"),
        ("-v", "2147483648", "invalid startnum value -- 2147483648"),
        ("-v", "-2147483649", "invalid startnum value -- -2147483649"),
    ] {
        let (_, status, err) = run(&[flag, value], b"a\n");
        assert_eq!(status, 1, "{flag} {value} 该退 1");
        assert_eq!(err, format!("nl: {sentence}\n"), "{flag} {value}");
    }
    for (flag, value) in [("-i", "-2147483648"), ("-v", "-2147483648")] {
        let (_, status, err) = run(&[flag, value], b"a\n");
        assert_eq!(status, 0, "{flag} {value} 在 `int` 域内，该被接受：{err}");
    }
}

/// End to end: C's number field is printed as `intbuffer + max(0, consumed -
/// width)` (`nl.c:331-335`), so a counter that outgrows `-w` loses cells from
/// the **left**, and the loss is the same under all three `-n` formats. Every
/// byte below is the reference's reading (`tmp/nl-417/nl_ref`): `-w 1 -v 100`
/// → `0`,`1`; `-w 1 -v -100` → `0`,`9` (the sign is the first cell dropped);
/// `-n ln -w 3 -v 12345` → `345`,`346`; `-w 4 -v -100000` → `0000`,`9999`.
/// Before this leg the shell printed the whole rendering, so all four shapes
/// were one line longer than C's.
#[test]
fn a_number_wider_than_the_field_keeps_its_last_cells() {
    for (args, want) in [
        (vec!["-w", "1", "-v", "100"], "0\ta\n1\tb\n"),
        (vec!["-w", "1", "-v", "-100"], "0\ta\n9\tb\n"),
        (
            vec!["-n", "ln", "-w", "3", "-v", "12345"],
            "345\ta\n346\tb\n",
        ),
        (
            vec!["-n", "rz", "-w", "3", "-v", "12345"],
            "345\ta\n346\tb\n",
        ),
        (vec!["-w", "4", "-v", "-100000"], "0000\ta\n9999\tb\n"),
        (vec!["-w", "3", "-v", "100"], "100\ta\n101\tb\n"),
    ] {
        let (out, status, err) = run(&args, b"a\nb\n");
        assert_eq!(status, 0, "{args:?} 的 stderr 是 {err}");
        assert_eq!(out, want, "{args:?}");
    }
}

/// C has **no** non-zero check here (it only checks `*ep`), so `-i 0` is legal and exits
/// 0; the old wording invented an "increment must be non-zero" sentence, treating a guard
/// that does not exist as real.
#[test]
fn a_zero_increment_is_accepted() {
    let (_, status, err) = run(&["-i", "0"], b"a\n");
    assert_eq!((status, err.as_str()), (0, ""), "stderr 是 {err:?}");
}

/// Two or more operands take the `usage()` branch (`nl.c:246-248`), **without** the libc
/// line — the difference from the unknown-letter case is exactly what this gate pins:
/// both are usage, but the line counts differ.
#[test]
fn more_than_one_operand_is_plain_usage_without_the_libc_line() {
    let (_, status, err) = run(&["f1", "f2"], b"a\n");
    assert_eq!(status, 1);
    assert_eq!(err, USAGE);
}

/// Operand leg: a readable file name has its content put on standard output and
/// numbered. This gate was added because the two earlier operand-related gates pinned
/// only "an unopenable file reports `Cannot open …`" and "more than one operand is
/// usage", and **no gate asserted the numbered output of a readable file name**, so a
/// defect like discarding the bytes from the success side of `read_file` in
/// `src/bin/nl.rs` could pass silently (`nl file` printing nothing and exiting 0). The
/// gold follows the reading of the reference fixture `tmp/nl-417/nl_ref`: each of the two
/// lines gets a `     1\t`/`     2\t` prefix, exit 0 and no report.
/// Standard input is empty here, so content on the output is what proves "the content
/// really came from the file name".
#[test]
fn a_readable_operand_is_numbered_into_stdout() {
    let path = format!("/tmp/nl-face-{}-operand.txt", std::process::id());
    std::fs::write(&path, b"alpha\nbeta\n").expect("the operand file should be writable");
    let (out, status, err) = run(&[path.as_str()], b"");
    let _ = std::fs::remove_file(&path);
    assert_eq!(status, 0, "stderr 是 {err:?}");
    assert_eq!(err, "", "正常编号不该报案");
    assert_eq!(
        out, "     1\talpha\n     2\tbeta\n",
        "档名内容必须进输出：读不到内容就是操作数腿被丢弃"
    );
}

/// A file name whose last line has no newline: the reference fixture `tmp/nl-417/nl_ref`
/// prints `     1\tx` (**no newline added**), and this shell differs from the reference
/// in this shape (`NL-FINAL-LINE-ADDS-NEWLINE` is registered as open, same family as
/// `head`/`tail`), so this gate pins only "the content reaches the output", which is what
/// this change fixed; the final-line newline difference is tracked by the registration.
#[test]
fn an_operand_without_a_trailing_newline_still_reaches_the_output() {
    let path = format!("/tmp/nl-face-{}-noeol.txt", std::process::id());
    std::fs::write(&path, b"x").expect("the operand file should be writable");
    let (out, status, err) = run(&[path.as_str()], b"");
    let _ = std::fs::remove_file(&path);
    assert_eq!(status, 0, "stderr 是 {err:?}");
    assert!(
        out.starts_with("     1\tx"),
        "编号内容要来自档名，实际 {out:?}"
    );
}

/// An unreadable operand goes through `err(3)`: program name, the **backtick-quoted
/// operand**, and the `strerror` sentence, all three present (`nl.c:244-246`'s
/// `err(EXIT_FAILURE, "Cannot open `%s'", argv[0])`), exit 1.
#[test]
fn an_unreadable_operand_names_the_operand_and_the_number_sentence() {
    let (out, status, err) = run(&["/tmp/nl-face-definitely-absent"], b"a\n");
    assert_eq!(out, "");
    assert_eq!(status, 1, "stderr 是 {err:?}");
    assert!(
        err.starts_with("nl: Cannot open `/tmp/nl-face-definitely-absent': "),
        "该是 err(3) 的三段形，实际 {err:?}"
    );
    assert!(
        !err.contains("cannot open input"),
        "自创短语顶替了 strerror 句子：{err:?}"
    );
}

/// The numbering leg adds no newline to a final line without one. C reads with `fgets`,
/// keeping the trailing newline, then prints verbatim with `printf("%s%s", sep, buffer)`
/// (`minix3/usr.bin/nl/nl.c:288`, `:340`), and the reference fixture `tmp/nl-417/nl_ref`
/// measures `     1\talpha\n     2\tbeta\n     3\tgamma` (no trailing newline). The
/// registration `NL-FINAL-LINE-ADDS-NEWLINE` is closed by this; the previous gate
/// deliberately asserted only `starts_with` (adding a newline was a fork at the time), and
/// this one pins it byte for byte.
#[test]
fn an_unterminated_final_line_is_numbered_without_a_newline() {
    let (out, code, err) = run(&[], b"alpha\nbeta\ngamma");
    assert_eq!(
        (out.as_str(), code, err.as_str()),
        ("     1\talpha\n     2\tbeta\n     3\tgamma", 0, "")
    );
}

/// Command seam with a byte-form argv: `&str` cannot carry a bare `ff` (an `OsStr` built
/// from a `str` is already valid UTF-8). Both output streams come back byte by byte — the
/// values of `-d` and `-s` directly decide "which line counts as a page delimiter" and
/// "what is printed between the number and the text", and the text form cannot tell one
/// byte from three folded bytes.
fn run_argv(args: &[&[u8]], input: &[u8]) -> (Vec<u8>, i32, Vec<u8>) {
    use std::os::unix::ffi::OsStrExt as _;
    let mut child = Command::new(NL)
        .args(args.iter().map(|word| std::ffi::OsStr::from_bytes(word)))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the nl binary should be runnable");
    write_stdin(&mut child, input);
    let output = child.wait_with_output().expect("the run should finish");
    (
        output.stdout,
        output.status.code().unwrap_or(-1),
        output.stderr,
    )
}

/// The argument seam on `nl` changes **the criterion, the match and the output** all at
/// once, the same family as `join`'s `-t` but denser:
/// ① `nl.c:158-161`'s `-d` takes the two **bytes** `optarg[0]` and `optarg[1]`, and only
///   the third byte (`:163`) triggers `invalid delim argument -- %s` (`:165`). Under the
///   text seam those two bare bytes each fold into three (`ef bf bd` ×2 ⇒ value length
///   six), flipping acceptance into rejection — negative control L1 (reverting the shell
///   side's argv to the lossy `args()` reading) turns this gate red on the spot; the
///   reading is in `tmp/nl-465/negctl-L1.log`.
/// ② The matching step also consumes bytes: when the input line `\xff\xa1` matches
///   `-d<ff><a1>`, the whole line is a delimiter ⇒ split, not numbered, and the following
///   `q` line comes back as six spaces plus a tab (identical byte for byte on both sides).
/// ③ The `-s` value is printed verbatim between the number and the text by `nl.c:340`'s
///   `printf("%s%s", sep, buffer)`; **it replaces the default tab entirely**, so `1` is
///   followed directly by that one `ff`.
/// ④ Control shape `-s<é>` (`c3 a9`, two bytes of valid UTF-8): both sides print the two
///   bytes verbatim — changing the seam to bytes neither loosens the criterion nor splits
///   a valid sequence into single bytes.
#[test]
fn a_raw_byte_delimiter_matches_the_line_and_a_raw_byte_separator_is_printed() {
    // ①②: two bare bytes as the delimiter: accepted, and actually used to match the line.
    let (out, status, err) = run_argv(&[b"-d\xff\xa1"], b"\xff\xa1\nq\n");
    assert_eq!(
        status, 0,
        "两枚裸字节是合法的分隔符，不该被拒：stderr 是 {err:?}"
    );
    assert_eq!(
        out, b"      \tq\n",
        "分隔符行切段后不编号：六枚空格 + 制表符 + 正文"
    );
    // ③: the `-s` value reaches the output: the number is followed directly by that one `ff`, with nothing else.
    let (out, status, err) = run_argv(&[b"-s\xff"], b"a\nb\n");
    assert_eq!(status, 0, "stderr 是 {err:?}");
    assert_eq!(
        out,
        [
            b"     1".as_slice(),
            &[0xff],
            b"a\n     2".as_slice(),
            &[0xff],
            b"b\n".as_slice(),
        ]
        .concat(),
        "分隔串必须是 argv 原字节，不是折叠出的三枚"
    );
    // ④ control: a valid two-byte sequence goes out verbatim.
    let (out, status, err) = run_argv(&[b"-s\xc3\xa9"], b"a\n");
    assert_eq!(status, 0, "stderr 是 {err:?}");
    assert_eq!(
        out,
        [b"     1".as_slice(), &[0xc3, 0xa9], b"a\n".as_slice()].concat()
    );
    // ⑤ A single bare byte: `optarg[1]` is `\0` ⇒ only `delim[0]` is changed and the second
    //   character keeps the default `:` (a source-code criterion, same family as the library's
    //   `test_delim_one_char_option`). The reference fixture reports `invalid delim argument` in
    //   this shape — that is an artifact of the out-of-bounds read of `optarg[2]`, not the
    //   criterion itself (registered `NL-DELIM-LENGTH-RULE` pending a ruling), so this segment
    //   pins the shape this shell derives from the source and does not align with the fixture.
    let (out, status, err) = run_argv(&[b"-d\xff"], b"\xff:\nq\n");
    assert_eq!(status, 0, "stderr 是 {err:?}");
    assert_eq!(out, b"      \tq\n", "分隔符是 `ff` 加默认的 `:`");
}

/// Seven legs splice the value into their report and the value position is always the raw
/// argv bytes: `invalid delim argument -- %s` (`nl.c:165`), `invalid incr argument -- %s`
/// (`:182`), `invalid num argument -- %s` (`:191`), `illegal format -- %s` (`:203`),
/// `invalid startnum value -- %s` (`:214`), `invalid width value -- %s` (`:223`),
/// `illegal <section name> line numbering type -- %s` (`:393`). All seven are `errx`:**one
/// line only, no usage, exit 1**, so the line count and the value position are each pinned
/// once in this gate. The letter position belongs to the cross-shell table
/// `no_command_aborts_on_a_nonutf8_option_letter` and is not repeated here.
/// Note the first-byte criterion of the three illegal-type branches: C's
/// `parse_numbering` only does `switch (argstr[0])` (`nl.c:362`), so `-b 'a<ff>'` is
/// **accepted** (this gate takes the shape whose first byte is already illegal).
#[test]
fn the_seven_value_echoes_carry_the_argv_bytes() {
    let cases: [(&[&[u8]], &[u8]); 7] = [
        (
            &[b"-dabc\xff", b"2"],
            b"nl: invalid delim argument -- abc\xff\n",
        ),
        (
            &[b"-i", b"5\xff", b"2"],
            b"nl: invalid incr argument -- 5\xff\n",
        ),
        (
            &[b"-l", b"5\xff", b"2"],
            b"nl: invalid num argument -- 5\xff\n",
        ),
        (&[b"-n", b"l\xff", b"2"], b"nl: illegal format -- l\xff\n"),
        (
            &[b"-v", b"5\xff", b"2"],
            b"nl: invalid startnum value -- 5\xff\n",
        ),
        (
            &[b"-w", b"5\xff", b"2"],
            b"nl: invalid width value -- 5\xff\n",
        ),
        (
            &[b"-b", b"\xff", b"2"],
            b"nl: illegal body line numbering type -- \xff\n",
        ),
    ];
    for (argv, want) in cases {
        let (out, status, err) = run_argv(argv, b"a\n");
        assert_eq!(out, Vec::new(), "{argv:?} 的报文只走标准错误");
        assert_eq!(status, 1, "{argv:?} 的 stderr 是 {err:?}");
        assert_eq!(
            err, want,
            "{argv:?} 该是那一行且值位是原字节（少了 usage 那一行）"
        );
    }
    // The same sentence on `-f`/`-h` only swaps the section name (the name field of `numbering_properties[]`).
    for (letter, name) in [("-f", "footer"), ("-h", "header")] {
        let (_out, _status, err) = run_argv(&[letter.as_bytes(), &[0xff][..], b"2"], b"a\n");
        assert_eq!(
            err,
            [
                format!("nl: illegal {name} line numbering type -- ").as_bytes(),
                &[0xff][..],
                b"\n".as_slice(),
            ]
            .concat(),
            "`{letter}` 那一句带的是自己的段名"
        );
    }
    // The shape with a legal first byte and a bare byte in the tail is **accepted** (C looks
    // only at the first character) — this counter-shape is pinned here so "illegal type" is
    // not mistaken for a reaction to any bad tail.
    // No operand here: `nl`'s only operand is a **file name** (not reps, unlike positional
    // arguments such as `jot`'s), and passing a `2` would become the "unopenable file name"
    // sentence and exit first.
    let (out, status, err) = run_argv(&[b"-ba\xff"], b"a\n");
    assert_eq!(status, 0, "stderr 是 {err:?}");
    assert_eq!(out, b"     1\ta\n");
}
