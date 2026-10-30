//! End-to-end face for the `unexpand` binary, checked against
//! `minix3/usr.bin/unexpand/unexpand.c` (option ring `:86-120`, tabstop echo
//! `:124-125`, operand loop `:127-135`, `tabify` `:141-215`, `usage()`
//! `:69-73`) and the libc `getopt` it drives
//! (`minix3/lib/libc/stdlib/getopt.c:74-78`, `:81-92`, `:95-104`, `:132-136`).
//!
//! The optstring at `unexpand.c:86` is `"at:"`: one value-less letter plus one
//! value-taking letter, two mutually exclusive arms.
//! The reference binary (`unexpand.c` ground truth + Minix's `getopt.c` + a
//! program-name and `fgetln`/`erealloc` shim, linked on the host) measures
//! these shapes form by form:
//! (1) unknown letter = one libc line + one usage line, exit 1 (`-Z`, `-Za`
//!     reports `Z`, `--x` reports `-`);
//! (2) a missing `-t` value = libc's `option requires an argument -- t` plus
//!     one usage line, exit 1;
//! (3) the tabstop table is **printed to standard error** after the option
//!     ring (the `fprintf(stderr, "%lu %zu\n", i, tabstops[i])` loop at
//!     `unexpand.c:124-125`) — a debug line left in the ground truth, but it is
//!     part of the external behavior;
//! (4) tabstop numbers go through `strtoul(..., 0)`: `010` is 8 and `0x10` is
//!     16; a bad tabstop has two different sentences (`Invalid tabstop` and
//!     `Bad tabstop spec …, must be greater than the previous …`);
//! (5) `-a` and `-t` conflicting prints the usage line only (first letter
//!     **capitalized**, program name from `getprogname()`, with `[file ...]`),
//!     exit 1;
//! (6) no permutation: a non-option token ends the option ring on arrival, so
//!     the `8` in `unexpand -t 4 8` is the **second file name** rather than a
//!     continuation of the tabstop table (reference: first `0 4`, then
//!     `Cannot open `8'`);
//! (7) an unopenable file name is the three-part
//!     `err(EXIT_FAILURE, "Cannot open `%s'")` shape; a bare `-` is not a
//!     stdin alias — it is handed to `freopen` and reports an error;
//! (8) a last line without a newline gets **no** newline added (C's `putchar`
//!     writes only the bytes present in the input).
//!
//! None of this exists in front of this gate: `unexpand`'s read seam calls
//! bare `open`/`read` directly, so on the host no input can be read at all (the
//! sixth case in this family), which is why its output used to be covered only
//! by library tests.

mod face_support;

use face_support::write_stdin;
use std::fs;
use std::process::{Command, Stdio};

const UNEXPAND: &str = env!("CARGO_BIN_EXE_unexpand");
/// The line at C `unexpand.c:70-71`: first letter capitalized, program name
/// from `getprogname()`, with `[file ...]`.
const USAGE: &str = "Usage: unexpand [-a] [-t tabstop] [file ...]\n";
/// The input the reference binary used (the same bytes as its lead fixture):
/// one line of eight spaces, one line of four.
const INPUT: &[u8] = b"        x\n    y\n";

fn run(args: &[&str], input: &[u8]) -> (String, i32, String) {
    let mut child = Command::new(UNEXPAND)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the unexpand binary should be runnable");
    write_stdin(&mut child, input);
    let output = child.wait_with_output().expect("the run should finish");
    (
        String::from_utf8_lossy(&output.stdout).into_owned(),
        output.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

/// The same command seam as `run`, but standard error comes back **byte by
/// byte**: `from_utf8_lossy` renders a raw `c3` as the three-byte U+FFFD, and
/// the text form cannot tell "one byte" from "two bytes" in these mistakes.
fn run_bytes(args: &[&str], input: &[u8]) -> (Vec<u8>, i32, Vec<u8>) {
    let mut child = Command::new(UNEXPAND)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the unexpand binary should be runnable");
    write_stdin(&mut child, input);
    let output = child.wait_with_output().expect("the run should finish");
    (
        output.stdout,
        output.status.code().unwrap_or(-1),
        output.stderr,
    )
}

/// In the unknown-letter line, the letter slot is **one raw byte**. The
/// expected value per form comes from the reference binary (probing `-Z` first
/// to confirm the owner: it returns `unknown option -- Z`, which is Minix's
/// `getopt.c`, not glibc's quoted `invalid option -- 'Z'`): `-é` and `-aé` both
/// print `unexpand: unknown option -- ` + a single `303` + newline followed by
/// that usage line, exit **1**; `-Zé` stops on `Z` (the ring does not pass the
/// first bad letter); conversely, in `-té` the `é` is a **value** rather than a
/// letter and goes through the command's own
/// `unexpand: Invalid tabstop `303 251'` **on a single line, with no usage**
/// (`unexpand.c:101-103`).
/// The ASCII forms (`-Z`, `-Za`, `--x`, a missing `-t` value) already have
/// owners elsewhere in this file and are not carried twice.
#[test]
fn the_libc_line_of_a_multibyte_flag_is_one_byte_for_unexpand() {
    let want: Vec<u8> = [
        b"unexpand: unknown option -- ".as_slice(),
        b"\xc3\n",
        USAGE.as_bytes(),
    ]
    .concat();
    for form in ["-\u{e9}", "-a\u{e9}"] {
        let (out, status, err) = run_bytes(&[form], INPUT);
        assert_eq!(out, Vec::new(), "{form} 的报文只走标准错误");
        assert_eq!(status, 1, "{form} 的 stderr 是 {err:?}");
        assert_eq!(err, want, "{form} 的字母该是一个裸字节 c3");
    }

    let (_out, status, err) = run_bytes(&["-Z\u{e9}"], INPUT);
    assert_eq!(status, 1);
    assert_eq!(
        err,
        [
            b"unexpand: unknown option -- Z\n".as_slice(),
            USAGE.as_bytes(),
        ]
        .concat(),
        "环停在第一个坏字母上，第二个字节根本不该被读到"
    );

    let (_out, status, err) = run_bytes(&["-t\u{e9}"], INPUT);
    assert_eq!(status, 1);
    assert_eq!(
        err,
        b"unexpand: Invalid tabstop `\xc3\xa9'\n".to_vec(),
        "带值字母把两字节整个当值收走，句子是命令自己的那一句、不接 usage"
    );
}

/// Unknown letter: the libc line first, `usage()` after, exit 1
/// (`unexpand.c:116-118`'s `default:`).
#[test]
fn an_unknown_letter_prints_the_libc_line_then_the_usage() {
    let (out, status, err) = run(&["-Z"], INPUT);
    assert_eq!(out, "");
    assert_eq!(status, 1, "stderr 是 {err:?}");
    assert_eq!(err, format!("unexpand: unknown option -- Z\n{USAGE}"));
}

/// A cluster reports only the first character (reference: `-Za` →
/// `unknown option -- Z`).
#[test]
fn a_cluster_reports_only_its_first_letter() {
    let (out, status, err) = run(&["-Za"], INPUT);
    assert_eq!((out.as_str(), status), ("", 1));
    assert_eq!(err, format!("unexpand: unknown option -- Z\n{USAGE}"));
}

/// `--x` reports the second hyphen: the optstring has no `-` (reference:
/// `unknown option -- -`).
#[test]
fn a_dash_followed_by_a_word_is_an_unknown_option_named_by_the_dash() {
    let (_, status, err) = run(&["--x"], INPUT);
    assert_eq!(status, 1);
    assert_eq!(err, format!("unexpand: unknown option -- -\n{USAGE}"));
}

/// No value after `-t` is libc's **other** sentence, not usage's own line.
#[test]
fn a_missing_tabstop_value_prints_the_other_libc_line() {
    let (out, status, err) = run(&["-t"], INPUT);
    assert_eq!((out.as_str(), status), ("", 1));
    assert_eq!(
        err,
        format!("unexpand: option requires an argument -- t\n{USAGE}")
    );
}

/// The tabstop table goes to stderr after the option ring and before reading
/// input (`unexpand.c:124-125`). This shape is easy to dismiss as "noise in the
/// ground truth", but the reference binary does print it, and the exit code is
/// still 0.
#[test]
fn the_stop_list_is_echoed_to_stderr_before_any_output() {
    let (out, status, err) = run(&["-t", "4"], INPUT);
    // With a tabstop at 4, the four leading spaces also cross a stop, so the
    // second line folds to a tab as well (same shape in the reference).
    assert_eq!(
        (out.as_str(), status),
        ("\tx\n\ty\n", 0),
        "stderr 是 {err:?}"
    );
    assert_eq!(err, "0 4\n", "C 那一行是「下标 空格 档位」，不带程序名");

    let (out, status, err) = run(&["-t", "4,8"], INPUT);
    assert_eq!((out.as_str(), status), ("\t\tx\n\ty\n", 0));
    assert_eq!(err, "0 4\n1 8\n");

    // With no tabstop at all the loop never runs, so stderr is empty
    // (reference: `-t \"\"`).
    let (out, status, err) = run(&["-t", ""], INPUT);
    assert_eq!(
        (out.as_str(), status),
        ("\tx\n    y\n", 0),
        "空表等于默认档"
    );
    assert_eq!(err, "");
}

/// The two bad-tabstop sentences differ word for word and must not merge with
/// usage into one line.
#[test]
fn the_two_stop_sentences_are_distinct_and_carry_the_offending_piece() {
    let (out, status, err) = run(&["-t", "x"], INPUT);
    assert_eq!((out.as_str(), status), ("", 1));
    assert_eq!(err, "unexpand: Invalid tabstop `x'\n");

    let (_, status, err) = run(&["-t", "3x"], INPUT);
    assert_eq!(status, 1);
    assert_eq!(err, "unexpand: Invalid tabstop `3x'\n");

    let (_, status, err) = run(&["-t", "8,4"], INPUT);
    assert_eq!(status, 1);
    assert_eq!(
        err, "unexpand: Bad tabstop spec `4', must be greater than the previous `8'\n",
        "递减那支报的是后写的档位与前一支的值"
    );
    assert!(!err.contains("Usage"), "这两句都不接 usage：{err:?}");
}

/// Tabstop numbers are base 0: `010` is the octal 8 and `0x10` is the hex 16
/// (witnessed by the reference binary's echo).
#[test]
fn the_stop_numbers_are_base_zero() {
    let (out, status, err) = run(&["-t", "010"], INPUT);
    assert_eq!(
        (out.as_str(), status),
        ("\tx\n    y\n", 0),
        "stderr 是 {err:?}"
    );
    assert_eq!(err, "0 8\n", "前导零走八进制");

    let (out, status, err) = run(&["-t", "0x10"], INPUT);
    assert_eq!((out.as_str(), status), ("        x\n    y\n", 0));
    assert_eq!(
        err, "0 16\n",
        "0x 走十六进制，16 列比这行还宽，所以什么都不折"
    );
}

/// `-a` and `-t` are mutually exclusive: either conflict prints the usage line
/// alone (no libc line), exit 1.
#[test]
fn the_a_and_t_conflict_prints_only_the_usage() {
    for args in [
        &["-a", "-t", "4"][..],
        &["-t", "4", "-a"][..],
        &["-at4"][..],
    ] {
        let (out, status, err) = run(args, INPUT);
        assert_eq!((out.as_str(), status), ("", 1), "args 是 {args:?}");
        assert_eq!(err, USAGE, "冲突那一支只印 usage：{args:?}");
    }
}

/// No permutation: a non-option token ends the option ring. The `8` in
/// `unexpand -t 4 8` is the **second file name**; the reference binary first
/// echoes `0 4` to stderr, then tries to open that nonexistent file and reports
/// the three-part shape.
#[test]
fn a_non_option_token_ends_the_ring() {
    let missing = std::env::temp_dir().join(format!("ue312-eight-{}", std::process::id()));
    let (out, status, err) = run(&["-t", "4", missing.to_str().unwrap()], INPUT);
    assert_eq!(out, "");
    assert_eq!(status, 1, "stderr 是 {err:?}");
    assert!(
        err.starts_with("0 4\nunexpand: Cannot open `"),
        "档位回显之后该是第二支档名的三段形：{err:?}"
    );

    let dir = scratch();
    let file = dir.join("in");
    fs::write(&file, INPUT).expect("write operand");
    // `unexpand <file> -a`: `-a` is taken as the second file name; the
    // reference reports `Cannot open `-a'`.
    let (_, status, err) = run(&[file.to_str().unwrap(), "-a"], b"");
    assert_eq!(status, 1);
    assert!(
        err.starts_with("unexpand: -a: ") || err.contains("Cannot open `-a'"),
        "操作数之后的 `-a` 该被当档名处理：{err:?}"
    );
    let _ = fs::remove_dir_all(dir);
}

/// An unopenable file name is the three-part
/// `err(EXIT_FAILURE, "Cannot open `%s'")` shape: program name, the backquoted
/// file name, the `strerror` sentence, one line, no usage, exit 1. A bare `-`
/// goes here too — `unexpand` makes no stdin exception for it (C hands it
/// straight to `freopen`).
#[test]
fn an_unopenable_operand_prints_the_err_shape_in_three_parts() {
    let missing = std::env::temp_dir().join(format!("ue312-absent-{}", std::process::id()));
    let (out, status, err) = run(&[missing.to_str().unwrap()], INPUT);
    assert_eq!(out, "");
    assert_eq!(status, 1, "stderr 是 {err:?}");
    let head = format!("unexpand: Cannot open `{}': ", missing.to_str().unwrap());
    assert!(err.starts_with(&head), "该以 `{head}` 开头，实际 {err:?}");
    let tail = err[head.len()..].trim_end();
    assert!(
        !tail.is_empty() && !tail.contains('\n'),
        "缀的应是一句 strerror：{err:?}"
    );
    assert!(!err.contains("Usage"), "这一支不接 usage：{err:?}");

    let (_, status, err) = run(&["-"], INPUT);
    assert_eq!(status, 1, "stderr 是 {err:?}");
    assert_eq!(
        err, "unexpand: Cannot open `-': No such file or directory\n",
        "裸 `-` 不是 stdin 的别名"
    );
}

/// The output leg (the other half this gate exists for): previously `unexpand`
/// used bare `open`/`read`, so no input could be read on the host and the
/// conversion was never tested at process level. The golden values are taken
/// form by form from the reference binary's readings.
#[test]
fn the_tabify_reaches_the_output_end_to_end() {
    // Default eight-column stop: eight spaces fold into one tab, four spaces
    // stay as they are.
    let (out, status, err) = run(&[], INPUT);
    assert_eq!(
        (out.as_str(), err.as_str(), status),
        ("\tx\n    y\n", "", 0)
    );
    // `--` ends the option ring and is not an operand itself.
    let (out, status, _) = run(&["--"], INPUT);
    assert_eq!((out.as_str(), status), ("\tx\n    y\n", 0));
    // Without `-a`, whitespace inside a line is untouched; only `-a` folds
    // mid-line whitespace runs.
    let (out, status, _) = run(&["-a"], b"   a   b   c\n");
    assert_eq!(
        (out.as_str(), status),
        ("   a   b   c\n", 0),
        "八个以内不跨档"
    );
    let (out, _, _) = run(&["-a"], b"ab        c\n");
    assert_eq!(out, "ab\t  c\n");
    // A last line without a newline gets none added (reference: the input
    // `   ab` comes out as one line still without a newline).
    let (out, status, err) = run(&[], b"   ab");
    assert_eq!((out.as_str(), err.as_str(), status), ("   ab", "", 0));
    // Multiple file names are processed in order and reset the line cursor
    // (reference: two files produce one combined output).
    let dir = scratch();
    let first = dir.join("a");
    let second = dir.join("b");
    fs::write(&first, b"aaa\n").expect("write first");
    fs::write(&second, INPUT).expect("write second");
    let (out, status, err) = run(&[first.to_str().unwrap(), second.to_str().unwrap()], b"");
    assert_eq!(
        (out.as_str(), err.as_str(), status),
        ("aaa\n\tx\n    y\n", "", 0),
        "两支档名该依次转换"
    );
    let _ = fs::remove_dir_all(dir);
}

fn scratch() -> std::path::PathBuf {
    static SEQ: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let seq = SEQ.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    let dir = std::env::temp_dir().join(format!("unexpand-face-{}-{}", std::process::id(), seq));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("scratch dir");
    dir
}

/// Measured against the reference binary: tabstop `0` is not a "bad tabstop" —
/// C's `strtoul` accepts it, `limit` and both `tabstops[n] - 1` sites wrap as
/// `size_t`, so the whole line comes back untouched with exit 0, while the
/// tabstop table still goes to standard error per `unexpand.c:124-125`. This
/// shell used to crash here (three unsigned subtractions in `src/unexpand.rs`
/// trapped before they could wrap).
#[test]
fn a_zero_tabstop_reaches_the_line_unchanged() {
    let (out, status, err) = run(&["-t", "0"], b"a\tb\tc\n");
    assert_eq!(
        (out.as_str(), status, err.as_str()),
        ("a\tb\tc\n", 0, "0 0\n")
    );
    let (out, status, err) = run(&["-t", "0,8"], b"    abc\n");
    assert_eq!(
        (out.as_str(), status, err.as_str()),
        ("    abc\n", 0, "0 0\n1 8\n")
    );
    // Input without a trailing newline comes back as C does (`fgetln` gives
    // only the bytes present in the input).
    let (out, status, err) = run_bytes(&["-t", "0"], b"x\t y");
    assert_eq!(
        (out, status, err),
        (b"x\t y"[..].to_vec(), 0, b"0 0\n".to_vec())
    );
}

/// Measured against the reference binary: a tab that cannot reach the next
/// stop comes back as it was; when the last line is all whitespace and the
/// input has no newline, the reference hands back **zero bytes** (C's loop
/// reaches the end of the buffer and returns directly).
/// The half that has a newline (`b"    \n"` against `\t\n`) belongs to a known
/// gap — the blank-line-end flush needs a terminator — and is not pinned here.
#[test]
fn a_tab_past_the_last_stop_and_a_blank_final_line_reach_stdout() {
    let (out, status, err) = run(&["-t", "8"], b"  \tx\n");
    assert_eq!(
        (out.as_str(), status, err.as_str()),
        ("  \tx\n", 0, "0 8\n")
    );
    let (out, status, err) = run(&["-t", "8"], b"        \ty\n");
    assert_eq!(
        (out.as_str(), status, err.as_str()),
        ("\t\ty\n", 0, "0 8\n")
    );
    let (out, status, err) = run(&["-t", "4,8"], b"    \ty\n");
    assert_eq!(
        (out.as_str(), status, err.as_str()),
        ("\t\ty\n", 0, "0 4\n1 8\n")
    );
    let (out, status, err) = run_bytes(&["-t", "8"], b"        ");
    assert_eq!((out, status, err), (Vec::new(), 0, b"0 8\n".to_vec()));
    let (out, status, err) = run_bytes(&[], b"   ");
    assert_eq!((out, status, err), (Vec::new(), 0, Vec::new()));
    let (out, status, err) = run_bytes(&["-t", "8"], b"\t");
    assert_eq!((out, status, err), (b"\t".to_vec(), 0, b"0 8\n".to_vec()));
}

/// The same command seam as `run_bytes`, but **argv carries raw bytes**: a raw
/// `0xff` cannot enter a `&str`, so this leg cannot go through any fixture that
/// takes `str` arguments. It runs in a temporary directory holding a real file
/// whose **name itself carries a raw byte** plus an ordinary `in.txt`, so that
/// both "the operand reaches `freopen`" and "the operand reaches the message"
/// can be put on the table.
fn run_raw(args: &[&[u8]], input: &[u8]) -> (Vec<u8>, i32, Vec<u8>) {
    use std::ffi::OsStr;
    use std::os::unix::ffi::OsStrExt as _;
    static SEQ: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let seq = SEQ.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    let dir = std::env::temp_dir().join(format!("unexpand-raw-{}-{}", std::process::id(), seq));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("scratch dir");
    std::fs::write(dir.join("in.txt"), b"    a\n").expect("in.txt");
    std::fs::write(dir.join(OsStr::from_bytes(b"a\xff.txt")), b"    x\n").expect("raw name file");
    let mut child = Command::new(UNEXPAND)
        .args(args.iter().map(|a| OsStr::from_bytes(a)))
        .current_dir(&dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the unexpand binary should be runnable");
    face_support::write_stdin(&mut child, input);
    let output = child.wait_with_output().expect("the run should finish");
    let read = (
        output.stdout,
        output.status.code().unwrap_or(-1),
        output.stderr,
    );
    let _ = std::fs::remove_dir_all(&dir);
    read
}

/// The byte criterion of the argument seam (eighteen forms measured on three
/// sides: the shell before the change matched the reference byte for byte in
/// 9/18, after the change in 17/18). The eight forms that flipped are the
/// letter slot, the six tabstop echoes (attached value, word splitting,
/// numeric tail, mixed table, space-separated, comma-separated), and the
/// file-name slot of the operand's `Cannot open \`%s'` sentence.
/// C's two echo slots both print the word `strsep(&optarg, ", \t")` cut out
/// (`unexpand.c:96-112`), while `err(EXIT_FAILURE, "Cannot open \`%s'",
/// argv[0])` (`:129-130`) prints the operand itself. The last thing pinned is
/// the leg that is **still on the books**: for a real file whose name carries a
/// raw byte, the reference binary can open it (`freopen` gets argv's raw bytes)
/// while this shell cannot — the file name goes through `os/libs`'s `&str`
/// interface, so this test pins the current shape: the name in the message is
/// raw bytes, but the outcome is still "no such file".
#[test]
fn the_tabstop_echoes_and_the_operand_name_carry_the_argv_bytes() {
    let (out, status, err) = run_raw(&[b"-\xff"], b"  a\n");
    assert_eq!(out, Vec::new(), "字母位那一行不走标准输出");
    assert_eq!(status, 1);
    assert_eq!(
        err,
        [
            b"unexpand: unknown option -- \xff\n".as_slice(),
            USAGE.as_bytes()
        ]
        .concat(),
        "字母该是那一枚字节本身，不是折过的第一字节 ef"
    );

    // Six echo forms: the same raw ff prints as itself wherever it lands in
    // the value.
    for (args, want) in [
        (
            vec![b"-t\xff".as_slice()],
            b"unexpand: Invalid tabstop `\xff'\n".to_vec(),
        ),
        (
            vec![b"-t".as_slice(), b"\xff".as_slice()],
            b"unexpand: Invalid tabstop `\xff'\n".to_vec(),
        ),
        (
            vec![b"-t4\xff".as_slice()],
            b"unexpand: Invalid tabstop `4\xff'\n".to_vec(),
        ),
        (
            vec![b"-t8,4\xff".as_slice()],
            b"unexpand: Invalid tabstop `4\xff'\n".to_vec(),
        ),
        (
            vec![b"-t8 \xff".as_slice()],
            b"unexpand: Invalid tabstop `\xff'\n".to_vec(),
        ),
        (
            vec![b"-t,\xff".as_slice()],
            b"unexpand: Invalid tabstop `\xff'\n".to_vec(),
        ),
    ] {
        let (_out, status, err) = run_raw(&args, b"  a\n");
        assert_eq!(status, 1, "{args:?} 的退码");
        assert_eq!(err, want, "{args:?} 的回声位该是原字节");
        assert!(_out.is_empty(), "{args:?} 报错时不写标准输出");
    }

    // Unopenable file name: the name inside the `err()` sentence is raw
    // bytes, in the three-part shape (program name, backquoted word, sentence).
    let (_out, status, err) = run_raw(&[b"no\xff/such"], b"  a\n");
    assert_eq!(status, 1);
    assert_eq!(
        err,
        [
            b"unexpand: Cannot open `no\xff/such': ".as_slice(),
            b"No such file or directory\n".as_slice()
        ]
        .concat()
    );

    // The registered shape: a **real** file whose name carries a raw byte
    // cannot be opened by this shell, while the reference binary opens it.
    let (out, status, err) = run_raw(&[b"-t", b"4", b"a\xff.txt"], b"");
    assert_eq!(
        status, 1,
        "参照件这里退 0 并打出 `\tx`；本壳要等那条 &str 口才翻正"
    );
    assert_eq!(
        out,
        Vec::new(),
        "开不了就没有任何输出（C 那一形交回的是 `\tx`）"
    );
    assert_eq!(
        err,
        [
            b"0 4\n".as_slice(),
            b"unexpand: Cannot open `a\xff.txt': ".as_slice(),
            b"No such file or directory\n".as_slice()
        ]
        .concat(),
        "档位表那行调试输出在前（`unexpand.c:124-125`），报文里的档名仍是原字节"
    );
}
