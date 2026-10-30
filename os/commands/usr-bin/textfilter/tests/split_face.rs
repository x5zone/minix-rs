//! End-to-end face for the `split` binary, checked against
//! `minix3/usr.bin/split/split.c` (option loop `:79-130`, operand section
//! `:133-150`, `usage()` `:355-361`) and the libc `getopt` it drives
//! (`minix3/lib/libc/stdlib/getopt.c:74-78`, `:81-92`, `:95-104`, `:132-136`).
//!
//! The optstring at `split.c:80` is `"0123456789b:l:a:n:"`: `0-9` are six
//! **legal letters** (the obsolete `-<digit>` form) plus four value-taking
//! options, so its exit surface has a whole extra layer compared with
//! `rev`/`colrm`, which take only an empty optstring. The reference binary
//! (`split.c` itself + Minix's `getopt.c` + a program-name shim, linked on the
//! host) measured these shapes one by one:
//! 1. unknown option letter = one libc line + one usage line, exit 1 (`-Z` and
//!    `-Zb` report `Z`; `--x` reports `-`);
//! 2. value-taking option with no value = libc's
//!    `option requires an argument -- <letter>` + one usage line, exit 1;
//! 3. **no permutation**: the option loop ends as soon as a non-option token
//!    arrives, so the `-Z` in `split <file> -Z` is the **output prefix** (the
//!    reference produces `-Zaa` and exits 0), not a bad option;
//! 4. more than two operands prints only usage; the program name in that line
//!    follows `getprogname()` and the exit code is 1;
//! 5. the obsolete form takes the whole token minus the `-` (`split.c:86-90`'s
//!    `argv[optind] + 1`), so both `-12x` and `-3b5` report
//!    `split: <original text>: illegal line count.`;
//! 6. an unopenable input is the three-part `err(1, "%s")` shape, one line,
//!    with no usage after it;
//! 7. an input that opens but cannot be read (named file and stdin share the
//!    same leg) is labelled `read`, and the number in that `strerror` sentence
//!    comes from the platform, not from this shell.
//!
//! This gate did not exist before: `split`'s diagnostic surface lacked libc's
//! two lines, the usage text was this shell's own short form with exit code 2,
//! and options after an operand were permuted back into the option loop.

mod face_support;

use face_support::write_stdin;
use std::fs;
use std::process::{Command, Stdio};

const SPLIT: &str = env!("CARGO_BIN_EXE_split");
/// The line from C `split.c:357-359` (the program name follows
/// `getprogname()`; both this shell and the reference are called split).
const USAGE: &str = "usage: split [-b byte_count] [-l line_count] [-n chunk_count] [-a suffix_length] [file [prefix]]\n";
/// Input used by the reference: 12 lines of 8 bytes each.
const INPUT: &[u8] = b"line001\nline002\nline003\nline004\nline005\nline006\nline007\nline008\nline009\nline010\nline011\nline012\n";

/// Run once in a private scratch directory (`split` writes chunk files into the
/// current directory) and return the exit code, stderr, and the generated file
/// names (sorted).
fn run(args: &[&str], input: &[u8]) -> (i32, String, Vec<String>, String) {
    let dir = scratch();
    let mut child = Command::new(SPLIT)
        .args(args)
        .current_dir(&dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the split binary should be runnable");
    write_stdin(&mut child, input);
    let output = child.wait_with_output().expect("the run should finish");
    let mut files: Vec<String> = fs::read_dir(&dir)
        .expect("the scratch dir should be readable")
        .filter_map(|entry| {
            entry
                .ok()
                .map(|e| e.file_name().to_string_lossy().into_owned())
        })
        .collect();
    files.sort();
    let first = files
        .first()
        .map(|name| {
            String::from_utf8_lossy(
                &fs::read(dir.join(name)).expect("the chunk file should be readable"),
            )
            .into_owned()
        })
        .unwrap_or_default();
    let err = String::from_utf8_lossy(&output.stderr).into_owned();
    let status = output.status.code().unwrap_or(-1);
    fs::remove_dir_all(&dir).expect("clean the scratch dir");
    (status, err, files, first)
}

/// A fresh directory per call: cargo runs tests in parallel threads, and a
/// pid-named temporary directory would let them see each other's chunk files
/// (this gate's assertions are exactly "which files are in the directory").
static SEQ: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

fn scratch() -> std::path::PathBuf {
    let seq = SEQ.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    let dir = std::env::temp_dir().join(format!("split-face-{}-{}", std::process::id(), seq));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("scratch dir");
    dir
}

/// Same scratch-directory scheme as `run`, but stderr comes back **byte for
/// byte**: `from_utf8_lossy` renders a bare `c3` as a three-byte U+FFFD, and
/// the text form cannot tell a one-byte error from a two-byte one.
fn run_bytes(args: &[&str], input: &[u8]) -> (i32, Vec<u8>, Vec<String>) {
    let dir = scratch();
    let mut child = Command::new(SPLIT)
        .args(args)
        .current_dir(&dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the split binary should be runnable");
    write_stdin(&mut child, input);
    let output = child.wait_with_output().expect("the run should finish");
    let mut files: Vec<String> = fs::read_dir(&dir)
        .expect("the scratch dir should be readable")
        .filter_map(|entry| {
            entry
                .ok()
                .map(|e| e.file_name().to_string_lossy().into_owned())
        })
        .collect();
    files.sort();
    let status = output.status.code().unwrap_or(-1);
    fs::remove_dir_all(&dir).expect("clean the scratch dir");
    (status, output.stderr, files)
}

/// The letter position in the unknown-option line is **one raw byte**. The
/// expected values were measured against the reference binary (started through
/// a `split` symlink so the program-name segment matches this shell and the
/// comparison is byte for byte; an initial `-Z` probe confirmed the source:
/// `unknown option -- Z` is Minix's `getopt.c`, not glibc's quoted
/// `invalid option -- 'Z'`): `-é` gives `split: unknown option -- ` + a single
/// `303` + a newline followed by the usage line, exit **1**; `-Zé` stops at
/// `Z`; conversely, in `-lé` the `é` is a **value**, not a letter, and takes
/// the command's own `split: é: illegal line count.` (`split.c:96-99`'s
/// `strtoull` tail test) as **a single line with no usage after it**. The
/// ASCII forms (`-Z`, `-Zb`, `--x`, the four missing values) already have their
/// owners in this file and are not duplicated here.
#[test]
fn the_libc_line_of_a_multibyte_flag_is_one_byte_for_split() {
    let want: Vec<u8> = [
        b"split: unknown option -- ".as_slice(),
        b"\xc3\n",
        USAGE.as_bytes(),
    ]
    .concat();
    let (status, err, files) = run_bytes(&["-\u{e9}"], INPUT);
    assert_eq!(status, 1, "stderr 是 {err:?}");
    assert!(files.is_empty(), "报错了就不该写分片文件：{files:?}");
    assert_eq!(err, want, "字母该是一个裸字节 c3");

    let (status, err, _files) = run_bytes(&["-Z\u{e9}"], INPUT);
    assert_eq!(status, 1);
    assert_eq!(
        err,
        [b"split: unknown option -- Z\n".as_slice(), USAGE.as_bytes(),].concat(),
        "环停在第一个坏字母上，第二个字节根本不该被读到"
    );

    let (status, err, _files) = run_bytes(&["-l\u{e9}"], INPUT);
    assert_eq!(status, 1);
    assert_eq!(
        err,
        [
            b"split: ".as_slice(),
            "\u{e9}".as_bytes(),
            b": illegal line count.\n".as_slice(),
        ]
        .concat(),
        "带值字母把两字节整个当值收走，句子是命令自己的那一句、不接 usage"
    );
}

/// Unknown option letter: libc's line first, `usage()` after, exit 1
/// (`split.c:128-129`'s `default:`). The old form omitted libc's line.
#[test]
fn an_unknown_letter_prints_the_libc_line_then_the_usage() {
    let (status, err, files, _) = run(&["-Z"], INPUT);
    assert_eq!(status, 1, "stderr 是 {err:?}");
    assert!(files.is_empty(), "报错了就不该写分片文件：{files:?}");
    assert_eq!(err, format!("split: unknown option -- Z\n{USAGE}"));
}

/// A cluster reports only its **first byte** (reference: `-Zb` →
/// `unknown option -- Z`).
#[test]
fn a_cluster_reports_only_its_first_letter() {
    let (status, err, files, _) = run(&["-Zb"], INPUT);
    assert_eq!(status, 1);
    assert!(files.is_empty());
    assert_eq!(err, format!("split: unknown option -- Z\n{USAGE}"));
}

/// `--x` reports the second hyphen as the letter: the optstring has no `-`, so
/// it is an unknown character too (reference: `unknown option -- -`). A pure
/// `--` takes the other branch.
#[test]
fn a_dash_followed_by_a_word_is_an_unknown_option_named_by_the_dash() {
    let (status, err, _, _) = run(&["--x"], INPUT);
    assert_eq!(status, 1);
    assert_eq!(err, format!("split: unknown option -- -\n{USAGE}"));
}

/// A value-taking option without its value produces libc's **other** sentence
/// (`getopt.c:132-136`), then usage: all four value-taking letters were
/// measured one by one (the reference reads the same). The old form printed
/// only usage on all four branches.
#[test]
fn a_value_taking_letter_without_its_value_prints_the_libc_line() {
    for letter in ['l', 'b', 'a', 'n'] {
        let flag = format!("-{letter}");
        let (status, err, files, _) = run(&[flag.as_str()], INPUT);
        assert_eq!(status, 1, "`-{letter}` 该退 1");
        assert!(files.is_empty(), "`-{letter}` 缺值不该写文件：{files:?}");
        assert_eq!(
            err,
            format!("split: option requires an argument -- {letter}\n{USAGE}"),
            "`-{letter}` 缺值该是 libc 那一行加 usage"
        );
    }
}

/// The `usage()` line has C's four-option form, the program name follows
/// `getprogname()`, and the exit code is **1** (`split.c:355-361`). The old
/// form got both wrong: the sentence was this shell's short `[-l lines]` form
/// and the exit code was 2. This form is reached through `-l 3 -l 4` (C's
/// `if (numlines != 0) usage()`).
#[test]
fn the_usage_line_names_all_four_value_options_and_exits_one() {
    let (status, err, files, _) = run(&["-l", "3", "-l", "4"], INPUT);
    assert_eq!(status, 1, "C 的 usage 是 exit(1)，旧写法写的是 2");
    assert!(files.is_empty());
    assert_eq!(err, USAGE, "usage 行该逐字等于 C 的那一行");
}

/// **No permutation** (the other half of this gate's criteria): as soon as a
/// non-option token arrives, `getopt.c:74-78` ends the option loop then and
/// there, and every following token lands in the operand section — the first
/// is the input file, the second is the **output prefix**. Measured against the
/// reference: `split <file> -Z` produces `-Zaa` and exits 0; that `-Z` is never
/// treated as an option.
#[test]
fn an_option_looking_token_after_an_operand_is_the_output_prefix() {
    let dir = scratch();
    let file = dir.join("in");
    fs::write(&file, INPUT).expect("write operand");
    let mut child = Command::new(SPLIT)
        .arg(file.to_str().unwrap())
        .arg("-Z")
        .current_dir(&dir)
        .stdin(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the split binary should be runnable");
    write_stdin(&mut child, b"");
    let output = child.wait_with_output().expect("the run should finish");
    let err = String::from_utf8_lossy(&output.stderr).into_owned();
    let files: Vec<String> = fs::read_dir(&dir)
        .expect("read scratch")
        .filter_map(|entry| {
            entry
                .ok()
                .map(|e| e.file_name().to_string_lossy().into_owned())
        })
        .filter(|name| name != "in")
        .collect();
    assert_eq!(output.status.code().unwrap_or(-1), 0, "stderr 是 {err:?}");
    assert_eq!(files, vec!["-Zaa".to_string()], "`-Z` 该被当输出前缀");
    let _ = fs::remove_dir_all(dir);
}

/// More than two operands prints only usage (`split.c:148-149`'s
/// `if (*argv != NULL) usage()`), with no libc line.
#[test]
fn three_operands_print_only_the_usage() {
    let dir = scratch();
    let file = dir.join("in");
    fs::write(&file, INPUT).expect("write operand");
    let mut child = Command::new(SPLIT)
        .arg(file.to_str().unwrap())
        .arg("pp")
        .arg("qq")
        .current_dir(&dir)
        .stdin(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the split binary should be runnable");
    write_stdin(&mut child, b"");
    let output = child.wait_with_output().expect("the run should finish");
    let err = String::from_utf8_lossy(&output.stderr).into_owned();
    assert_eq!(output.status.code().unwrap_or(-1), 1);
    assert_eq!(err, USAGE);
    let _ = fs::remove_dir_all(dir);
}

/// When "an unopenable input" and "a third operand" are both present, the
/// **file name** is reported, not usage: C runs `open(*argv)` first
/// (`split.c:135-139`; failing that it does `err(1, "%s", *argv)`) and only
/// then `if (*argv != NULL) usage()` (`:148-149`). The reference behaves this
/// way in all four forms (the default leg and `-l`/`-b`/`-n`); a bare `-` is
/// not opened (the `:136` special case), so that form has no open failure to
/// report and falls straight into usage.
#[test]
fn an_unopenable_input_outranks_the_third_operand() {
    let missing = std::env::temp_dir().join(format!("split427-absent-{}", std::process::id()));
    let name = missing.to_str().unwrap();
    let expected = format!("split: {name}: No such file or directory\n");
    for argv in [
        vec![name, "pp", "qq"],
        vec!["-l", "1", name, "pp", "qq"],
        vec!["-b", "1", name, "pp", "qq"],
        vec!["-n", "2", name, "pp", "qq"],
    ] {
        let (status, err, files, _) = run(&argv, b"");
        assert_eq!(
            (status, err.as_str()),
            (1, expected.as_str()),
            "形 {argv:?}：先开档，开不成就用档名说话"
        );
        assert!(files.is_empty(), "报错的那一形不该落下分片：{files:?}");
    }
    let (status, err, files, _) = run(&["-", "pp", "qq"], b"");
    assert_eq!(
        (status, err.as_str()),
        (1, USAGE),
        "裸 `-` 不开档，于是轮到 usage"
    );
    assert!(files.is_empty());
}

/// An unopenable input is `err(1, "%s", *argv)` (`split.c:137-138`): program
/// name, the operand's original text, and the `strerror` sentence — all three
/// parts, **one line**, no usage after it, exit 1. The old form lacked the third
/// part. The whole third part is pinned: comparing only the prefix would let a
/// change that swaps the platform number for a hardcoded one still print a
/// non-empty strerror without turning the gate red (the reference measured
/// `No such file or directory`).
#[test]
fn an_unopenable_input_prints_the_err_shape_in_three_parts() {
    let missing = std::env::temp_dir().join(format!("split311-absent-{}", std::process::id()));
    let (status, err, files, _) = run(&[missing.to_str().unwrap()], INPUT);
    assert_eq!(status, 1);
    assert!(files.is_empty(), "输入打不开就不该写分片文件：{files:?}");
    assert_eq!(
        err,
        format!(
            "split: {}: No such file or directory\n",
            missing.to_str().unwrap()
        ),
        "三段之外不许多一个字，也不许把 `strerror` 的句子换成别的号"
    );
    assert!(!err.contains("usage:"), "输入腿的 err 不接 usage：{err:?}");
}

/// A bare `-` is the first operand and means standard input (`split.c:136`
/// special-cases it), and the prefix defaults to `x`; `--` is not itself an
/// operand (`getopt.c:81-85` does `++optind` first). Reference: `-- -Z` tries to
/// open a file named `-Z` and reports `split: -Z: No such file or directory`.
#[test]
fn the_solitary_dash_reads_stdin_and_the_marker_is_not_an_operand() {
    let (status, err, files, content) = run(&["-"], INPUT);
    assert_eq!((status, err.as_str()), (0, ""));
    assert_eq!(files, vec!["xaa".to_string()], "裸 `-` 该读标准输入");
    assert_eq!(content, String::from_utf8_lossy(INPUT));

    let (status, err, files, _) = run(&["--"], INPUT);
    assert_eq!((status, err.as_str()), (0, ""), "`--` 自身不该被当操作数");
    assert_eq!(files, vec!["xaa".to_string()]);

    let missing = std::env::temp_dir().join(format!("split311-dashZ-{}", std::process::id()));
    let (status, err, files, _) = run(&["--", missing.to_str().unwrap()], INPUT);
    assert_eq!(status, 1);
    assert!(files.is_empty());
    assert!(
        err.starts_with(&format!("split: {}: ", missing.to_str().unwrap())),
        "`--` 之后的 token 该按输入档处理：{err:?}"
    );
}

/// The obsolete `-<digits>` form takes the whole token minus the `-` (C starts
/// at `argv[optind] + 1`): `-3` cuts 12 lines into four chunks, and `-12x`,
/// `-3b5`, and `-0` all report the original text as an illegal line count
/// (`split.c:86-96`). The old form recognised only an all-digit tail and
/// treated a tail with other characters as an unknown option letter.
#[test]
fn the_obsolete_digit_form_takes_the_whole_token_as_the_count() {
    let (status, err, files, content) = run(&["-3"], INPUT);
    assert_eq!((status, err.as_str()), (0, ""));
    assert_eq!(
        files,
        vec![
            "xaa".to_string(),
            "xab".to_string(),
            "xac".to_string(),
            "xad".to_string()
        ],
        "12 行按 3 行切该是四片"
    );
    assert_eq!(content, "line001\nline002\nline003\n");

    for (word, reported) in [("-0", "0"), ("-12x", "12x"), ("-3b5", "3b5")] {
        let (status, err, files, _) = run(&[word], INPUT);
        assert_eq!(status, 1, "`{word}` 该被拒：stderr 是 {err:?}");
        assert!(files.is_empty());
        assert_eq!(
            err,
            format!("split: {reported}: illegal line count.\n"),
            "`{word}` 该把整段原文报出去"
        );
    }
}

/// Values may be attached to the letter or given as a separate token, and both
/// forms really take effect (`-l3` and `-l 3` both mean three lines per chunk;
/// `-a 3` changes the suffix width to three letters, producing `xaaa`).
#[test]
fn option_values_take_the_attached_and_the_separate_form() {
    for args in [&["-l3"][..], &["-l", "3"][..]] {
        let (status, err, files, _) = run(args, INPUT);
        assert_eq!((status, err.as_str()), (0, ""), "args 是 {args:?}");
        assert_eq!(files.len(), 4, "三行一片该切四片：{files:?}");
    }
    let (status, err, files, _) = run(&["-a", "3"], INPUT);
    assert_eq!((status, err.as_str()), (0, ""));
    assert_eq!(
        files,
        vec!["xaaa".to_string()],
        "`-a 3` 该把后缀宽度改成三支"
    );
}

/// Output leg: the default is 1000 lines per chunk, so 12 lines produce a
/// single `xaa` whose content is byte-identical to the input.
#[test]
fn the_split_reaches_the_chunk_files_end_to_end() {
    let (status, err, files, content) = run(&[], INPUT);
    assert_eq!((status, err.as_str()), (0, ""), "stderr 是 {err:?}");
    assert_eq!(files, vec!["xaa".to_string()]);
    assert_eq!(
        content,
        String::from_utf8_lossy(INPUT),
        "一片装得下全部输入"
    );

    let (status, _, files, _) = run(&["-l", "5"], INPUT);
    assert_eq!(status, 0);
    assert_eq!(
        files,
        vec!["xaa".to_string(), "xab".to_string(), "xac".to_string()],
        "十二行按五行切是三片（5 + 5 + 2）"
    );

    // The second operand is the output prefix: `split <file> pp` produces
    // `ppaa`.. (the reference has the same shape, compared byte for byte).
    // Note that `pp` in `split -l 3 pp` is the **first** operand, meaning "read
    // the file named pp", which cannot be opened; the reference gives the same
    // sentence for that shape, so the input file name must come first here.
    let input = std::env::temp_dir().join(format!("split311-in-{}", std::process::id()));
    fs::write(&input, INPUT).expect("write the input file");
    let (status, err, files, _) = run(&["-l", "3", input.to_str().unwrap(), "pp"], b"");
    assert_eq!((status, err.as_str()), (0, ""), "stderr 是 {err:?}");
    assert_eq!(
        files,
        vec![
            "ppaa".to_string(),
            "ppab".to_string(),
            "ppac".to_string(),
            "ppad".to_string()
        ],
        "第二操作数该当输出前缀"
    );
    let _ = fs::remove_file(&input);
}

/// **Raw-byte** reading for line mode. The `run` helper above turns the first
/// chunk into a lossy string, folding invalid bytes into U+FFFD and hiding
/// exactly what these tests pin, so this leg returns each chunk's bytes
/// directly (sorted by file name).
fn run_chunk_bytes(args: &[&str], input: &[u8]) -> (i32, Vec<(String, Vec<u8>)>) {
    let dir = scratch();
    let mut child = Command::new(SPLIT)
        .args(args)
        .current_dir(&dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the split binary should be runnable");
    write_stdin(&mut child, input);
    let output = child.wait_with_output().expect("the run should finish");
    let mut chunks: Vec<(String, Vec<u8>)> = fs::read_dir(&dir)
        .expect("the scratch dir should be readable")
        .filter_map(|entry| {
            let entry = entry.ok()?;
            let name = entry.file_name().to_string_lossy().into_owned();
            let data = fs::read(entry.path()).expect("the chunk file should be readable");
            Some((name, data))
        })
        .collect();
    chunks.sort();
    fs::remove_dir_all(&dir).expect("clean the scratch dir");
    (output.status.code().unwrap_or(-1), chunks)
}

/// Line mode used to validate the whole file as UTF-8 and fold an entirely
/// undecodable file into an empty string: that produced no chunks at all and
/// still exited 0 (silent data loss, P0). C's line mode counts newlines in the
/// read buffer (`split.c:252`'s `read` and the newline scan at `:262-267`) and
/// never validates encoding. The expected values come from an `od -An -c` read
/// of the reference run with `-l 1`: three chunks, with the invalid bytes left
/// in the chunks as they were.
#[test]
fn line_mode_keeps_undecodable_bytes() {
    let (status, chunks) = run_chunk_bytes(&["-l", "1"], b"aa\xffbb\ncc\xfe\ndd\n");
    assert_eq!(status, 0);
    assert_eq!(
        chunks,
        vec![
            ("xaa".to_string(), b"aa\xffbb\n".to_vec()),
            ("xab".to_string(), b"cc\xfe\n".to_vec()),
            ("xac".to_string(), b"dd\n".to_vec()),
        ],
        "不可解码的字节必须留在它原来那一片里"
    );
}

/// A last line without a newline gets none added: C moves a whole line of
/// `Ce - Cs + 1` bytes only when it sees `\n`, and writes the remainder at the
/// buffer tail as it is (`split.c:276-280`). The old form split into
/// unterminated lines and then added `\n` to each line on write, so a round trip
/// lost "did the last line have a newline". The reference gives `xaa`=`a\n` and
/// `xab`=`b` for the same shape (`split -l 1` on a file without a final
/// newline).
#[test]
fn line_mode_does_not_invent_a_final_newline() {
    let (status, chunks) = run_chunk_bytes(&["-l", "1"], b"a\nb");
    assert_eq!(status, 0);
    assert_eq!(
        chunks,
        vec![
            ("xaa".to_string(), b"a\n".to_vec()),
            ("xab".to_string(), b"b".to_vec()),
        ],
        "不带换行的末行就不能带出换行"
    );
}

/// A blank line is a line of its own whose content is that single newline (a
/// three-shape comparison of `zaa`/`zab`/`zac`). Both sides already agree on
/// this shape; it stays here so that "the terminator travels with the line" has
/// a control case without invalid bytes, rather than pinning two things in one
/// gate.
#[test]
fn line_mode_makes_a_blank_line_its_own_chunk() {
    let (status, chunks) = run_chunk_bytes(&["-l", "1"], b"a\n\nb\n");
    assert_eq!(status, 0);
    assert_eq!(
        chunks,
        vec![
            ("xaa".to_string(), b"a\n".to_vec()),
            ("xab".to_string(), b"\n".to_vec()),
            ("xac".to_string(), b"b\n".to_vec()),
        ]
    );
}

/// When a chunk holds several lines, each line keeps its own terminator, and
/// the last line's shape carries through with the chunk: `split -l 2` on
/// `a\nb\nc` (last line unterminated) gives `xaa`=`a\nb\n` and `xab`=`c`.
#[test]
fn line_mode_keeps_each_line_terminator_inside_a_multi_line_chunk() {
    let (status, chunks) = run_chunk_bytes(&["-l", "2"], b"a\nb\nc");
    assert_eq!(status, 0);
    assert_eq!(
        chunks,
        vec![
            ("xaa".to_string(), b"a\nb\n".to_vec()),
            ("xab".to_string(), b"c".to_vec()),
        ]
    );
}

/// Control leg: the same invalid input through `-b 6` is byte-identical before
/// and after the change (the reference gives `xaa`=`aa 377 bb \n`,
/// `xab`=`cc 376 \n d d`, `xac`=`\n`). It shows that this change lands only on
/// line mode; both `-b` legs already read by byte.
#[test]
fn byte_mode_is_untouched_by_the_line_rewrite() {
    let (status, chunks) = run_chunk_bytes(&["-b", "6"], b"aa\xffbb\ncc\xfe\ndd\n");
    assert_eq!(status, 0);
    assert_eq!(
        chunks,
        vec![
            ("xaa".to_string(), b"aa\xffbb\n".to_vec()),
            ("xab".to_string(), b"cc\xfe\ndd".to_vec()),
            ("xac".to_string(), b"\n".to_vec()),
        ]
    );
}

/// A chunk file that cannot be created is `err(1, "%s", fname)`
/// (`split.c:332-333`): it reports the **path** and the `strerror` sentence, not
/// the word `write` — the two arms' labels part ways here, which is also what
/// gives the write-leg number a way to be pinned.
#[test]
fn a_chunk_that_cannot_be_created_names_the_path_not_write() {
    let prefix = format!("{}/no-such-dir/x", scratch().display());
    let (status, err, files, _out) = run(&["-", &prefix], INPUT);
    assert_eq!(
        status, 1,
        "开不出分片文件必须退 1，实得 {status}，文件 {files:?}"
    );
    // The chunk name is "prefix + suffix", and the one reported is the file
    // name that really cannot be created (`xaa`).
    let head = format!("split: {prefix}aa: ");
    assert!(
        err.starts_with(&head),
        "该以 `split: <路径>: ` 开头，实际 {err:?}"
    );
    assert!(
        !err.contains(": write: "),
        "开档臂不许借用写臂的标签：{err:?}"
    );
    let tail = err[head.len()..].trim_end();
    assert!(
        !tail.is_empty() && !tail.contains('\n'),
        "缀的应是一句 strerror 且只有这一行：{err:?}"
    );
    // The appended sentence is the strerror for the platform's number: the
    // failure here is ENOENT (the directory does not exist), and if the open leg
    // flattened the number to `EIO` or anything else this sentence would change
    // on the spot — the gate pins "is the number still there", not the sentence
    // itself (the host reference links glibc and the Minix table uses the same
    // wording; see the verification boundary at the top of this file).
    assert!(
        tail.contains("No such file"),
        "该缀 ENOENT 那一句，实得 {err:?}"
    );
    assert!(!err.contains("usage:"), "这一腿不接 usage：{err:?}");
}

/// A directory as the input operand: it opens but cannot be read. C does
/// `err(1, "read")` at the `read` step
/// (`minix3/usr.bin/split/split.c:185-190`, `:252-257`), with the label being
/// the word `read`; only a `fopen` failure reports the operand name. When the
/// two arms are conflated, a path is printed here. The reference reads the same:
/// `split_ref: read: Is a directory`, exit 1, no chunk files at all.
#[test]
fn a_directory_operand_blames_read_not_the_operand() {
    let dir = scratch();
    let name = dir.to_str().expect("utf-8 scratch path");
    let (status, err, files, _out) = run(&[name, "out"], b"");
    assert_eq!(status, 1, "读不动输入必须退 1，实得 {status}");
    assert!(
        files.is_empty(),
        "一枚也读不出的输入不该留下分片：{files:?}"
    );
    assert!(
        err.starts_with("split: read: "),
        "该报 `read` 那一臂，实得 {err:?}"
    );
    assert!(!err.contains(name), "读腿不许把操作数名字当成标签：{err:?}");
    assert_eq!(err.matches('\n').count(), 1, "一句、不接 usage：{err:?}");
    assert!(!err.contains("usage:"), "{err:?}");
}

/// Standard input itself cannot be read (a directory's fd is handed in as
/// stdin): C's `ifd` defaults to 0, so the failure lands on the same
/// `err(1, "read")` sentence (`minix3/usr.bin/split/split.c:185-190`,
/// `:252-257`), with the **label still `read`, not `stdin`** — this shape is
/// pinned separately from the previous one because, if the stdin leg ever
/// regressed to a read form that cannot obtain the error number, it would only
/// exit 0 silently. The reference reads the same: `split_ref: read: Is a
/// directory`, exit 1, no chunk files at all.
#[test]
fn an_unreadable_stdin_blames_the_read_arm() {
    // The handed-in directory lives in a separate scratch directory: `split`'s
    // chunks land in the current directory, and if the two shared a place the
    // "which files are in the directory" assertion would count the gate's own
    // scaffolding as chunks.
    let dir = scratch();
    let holder = scratch();
    let stream = holder.join("not-a-stream");
    fs::create_dir(&stream).expect("create the directory to hand over as stdin");
    let handle = fs::File::open(&stream).expect("a directory opens read-only here");
    let output = Command::new(SPLIT)
        .current_dir(&dir)
        .stdin(Stdio::from(handle))
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the split binary should be runnable")
        .wait_with_output()
        .expect("the run should finish");
    let err = String::from_utf8_lossy(&output.stderr).into_owned();
    let files: Vec<String> = fs::read_dir(&dir)
        .expect("the scratch dir should be readable")
        .filter_map(|entry| {
            entry
                .ok()
                .map(|e| e.file_name().to_string_lossy().into_owned())
        })
        .collect();
    let _ = fs::remove_dir_all(&dir);
    let _ = fs::remove_dir_all(&holder);
    assert_eq!(output.status.code(), Some(1), "stderr 是 {err:?}");
    assert_eq!(String::from_utf8_lossy(&output.stdout), "");
    assert_eq!(
        err, "split: read: Is a directory\n",
        "一句三段形、标签是 `read`，实得 {err:?}"
    );
    assert!(files.is_empty(), "读不动的输入不该留下分片：{files:?}");
}

/// Both the `-n` chunk-count bound and the number in that message come from
/// the **size the kernel reports**, not from the bytes actually received
/// (`minix3/usr.bin/split/split.c:293-300`: `fstat(ifd, &sb)`, then
/// `chunks > st_size` gives
/// `errx(1, "can't split into more than %d files", (int)sb.st_size)`). On a
/// pipe `st_size` is 0, so "split into 3" can only report an error — the
/// reference reads the same: `split: can't split into more than 0 files`,
/// exit 1, no chunk files at all. This shell used to take the 96 bytes already
/// received as the size, then exit 0 after cutting three chunks.
#[test]
fn a_piped_input_refuses_the_chunk_count_it_cannot_count() {
    let (status, err, files, _) = run(&["-n", "3"], INPUT);
    assert_eq!(
        err, "split: can't split into more than 0 files\n",
        "管道问不到长度，该报 0 而不是报切完，实得 {err:?}"
    );
    assert_eq!(status, 1, "退码该是 1，实得 {status}");
    assert!(files.is_empty(), "报错了就不该有分片：{files:?}");
}

/// The same branch is unaffected for a named file: its size is available
/// (96 bytes), so `-n 3` still produces three chunks and exits 0. This is the
/// control for the previous test — without it, "always report an error" would
/// pass the whole gate set.
#[test]
fn a_named_operand_still_splits_the_chunks_it_can_count() {
    let dir = scratch();
    fs::write(dir.join("in.txt"), INPUT).expect("write the 96-byte operand");
    let output = Command::new(SPLIT)
        .args(["-n", "3", "in.txt"])
        .current_dir(&dir)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the split binary should be runnable")
        .wait_with_output()
        .expect("the run should finish");
    let chunks: Vec<(String, usize)> = fs::read_dir(&dir)
        .expect("the scratch dir should be readable")
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.file_name() != "in.txt")
        .map(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            let size = fs::metadata(entry.path())
                .map(|meta| meta.len())
                .unwrap_or(0);
            (name, size as usize)
        })
        .collect();
    let err = String::from_utf8_lossy(&output.stderr).into_owned();
    let _ = fs::remove_dir_all(&dir);
    assert_eq!(err, "", "命名片那一形不该有报文：{err:?}");
    assert_eq!(output.status.code(), Some(0));
    let mut names: Vec<String> = chunks.iter().map(|(name, _)| name.clone()).collect();
    names.sort();
    assert_eq!(
        names,
        vec!["xaa".to_string(), "xab".to_string(), "xac".to_string()],
        "96 字节切 3 片该是三枚"
    );
    assert!(
        chunks.iter().all(|(_, size)| *size == 32),
        "每片各 32 字节（`split1(96/3, 3)`），实得 {chunks:?}"
    );
}

/// The number in that message comes from the size, not from the chunk count:
/// asking a 96-byte file for 200 chunks reports 96.
#[test]
fn the_chunk_count_complaint_quotes_the_size_not_the_count() {
    let dir = scratch();
    fs::write(dir.join("in.txt"), INPUT).expect("write the 96-byte operand");
    let output = Command::new(SPLIT)
        .args(["-n", "200", "in.txt"])
        .current_dir(&dir)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the split binary should be runnable")
        .wait_with_output()
        .expect("the run should finish");
    let files: Vec<String> = fs::read_dir(&dir)
        .expect("the scratch dir should be readable")
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.file_name() != "in.txt")
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect();
    let err = String::from_utf8_lossy(&output.stderr).into_owned();
    let _ = fs::remove_dir_all(&dir);
    assert_eq!(
        err, "split: can't split into more than 96 files\n",
        "报的该是输入的字节数 96，实得 {err:?}"
    );
    assert_eq!(output.status.code(), Some(1));
    assert!(files.is_empty(), "报错了就不该有分片：{files:?}");
}

/// A read failure on the `-b` leg has the same shape as in line mode:
/// `split1`'s `case -1:` does `err(1, "read")` on the spot
/// (`minix3/usr.bin/split/split.c:188-190`), exit 1. This shell used to exit 0
/// directly at the end of the `-b` branch, dropping that number on the floor;
/// all three legs (line mode, `-n`, `-b`) share one finish, and this test pins
/// exactly "there must be no exception".
#[test]
fn byte_mode_blames_an_unreadable_operand_and_exits_one() {
    let holder = scratch();
    let unreadable = holder.join("not-a-stream");
    fs::create_dir(&unreadable).expect("create the directory to hand over as an operand");
    let name = unreadable.to_str().expect("utf-8 path");
    let (status, err, files, _out) = run(&["-b", "4", name, "out"], b"");
    let _ = fs::remove_dir_all(&holder);
    assert_eq!(status, 1, "`-b` 读不动输入也必须退 1，实得 {status}");
    assert!(
        err.starts_with("split: read: "),
        "标签该是 `read`，实得 {err:?}"
    );
    assert!(
        files.is_empty(),
        "一枚也读不出的输入不该留下分片：{files:?}"
    );
}

/// The same shape through the stdin leg (a directory's fd handed in as stdin):
/// C's `ifd` defaults to 0, so the failure lands on the same `err(1, "read")`
/// sentence, and no chunk file is produced.
#[test]
fn byte_mode_blames_an_unreadable_stdin_and_exits_one() {
    let dir = scratch();
    let holder = scratch();
    let stream = holder.join("not-a-stream");
    fs::create_dir(&stream).expect("create the directory to hand over as stdin");
    let handle = fs::File::open(&stream).expect("a directory opens read-only here");
    let output = Command::new(SPLIT)
        .args(["-b", "4"])
        .current_dir(&dir)
        .stdin(Stdio::from(handle))
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the split binary should be runnable")
        .wait_with_output()
        .expect("the run should finish");
    let err = String::from_utf8_lossy(&output.stderr).into_owned();
    let files: Vec<String> = fs::read_dir(&dir)
        .expect("the scratch dir should be readable")
        .filter_map(|entry| {
            entry
                .ok()
                .map(|e| e.file_name().to_string_lossy().into_owned())
        })
        .collect();
    let _ = fs::remove_dir_all(&dir);
    let _ = fs::remove_dir_all(&holder);
    assert_eq!(output.status.code(), Some(1), "stderr 是 {err:?}");
    assert_eq!(
        err, "split: read: Is a directory\n",
        "一句三段形，实得 {err:?}"
    );
    assert!(files.is_empty(), "读不动的标准输入不该留下分片：{files:?}");
}

/// C's line mode writes the bytes left in the read buffer as they are:
/// `split2` in `minix3/usr.bin/split/split.c` runs
/// `if (Cs < Ce) bigwrite(ofd, Cs, Ce - Cs)` after the newline scan and
/// **adds no terminator to the last line**. This shell's line leg used to run a
/// lossy decode and then rejoin by `\n`, which added a newline to a final line
/// that had none — one extra byte is a visible behavior divergence too. The
/// expected value comes from a live run: the last chunk is `cc\ndd` with no
/// trailing newline.
#[test]
fn line_mode_does_not_invent_a_final_line_terminator() {
    let (status, chunks) = run_chunk_bytes(&["-l", "2"], b"aa\nbb\ncc\ndd");
    assert_eq!(status, 0);
    assert_eq!(
        chunks,
        vec![
            ("xaa".to_string(), b"aa\nbb\n".to_vec()),
            ("xab".to_string(), b"cc\ndd".to_vec()),
        ],
        "末行没有终止符时，分片里不该出现被补上的那个换行"
    );
}

/// A line spanning two reads: C reads one `MAXBSIZE` per `read` (`split.c:252`;
/// `compat_defs.h:1151` writes it as 64 KiB), and only the line count and "is
/// the current chunk still open" survive across reads, so a line crossing a
/// read boundary is written in two pieces into the **same** chunk (`:276-280`).
/// After line mode became streaming this is the easiest shape to get wrong:
/// building a stitching buffer for the spanning line, or starting a new chunk
/// per piece, diverges as soon as a line exceeds 64 KiB.
#[test]
fn a_line_longer_than_the_read_block_stays_in_one_chunk() {
    let mut input: Vec<u8> = vec![b'k'; 70 * 1024];
    input.push(b'\n');
    input.extend_from_slice(b"tail\n");
    let (status, chunks) = run_chunk_bytes(&["-l", "1"], &input);
    assert_eq!(status, 0);
    assert_eq!(chunks.len(), 2, "两行按一行一片是两枚：{chunks:?}");
    assert_eq!(
        chunks[0].1,
        input[..70 * 1024 + 1],
        "长行必须完整地留在第一枚分片里，不许被读边界劈成两枚"
    );
    assert_eq!(chunks[1].1, b"tail\n".to_vec());
}

/// The cut lands exactly on a read boundary: 128 KiB of 1 KiB lines, 64 lines
/// per chunk, puts the cut at 64 KiB and 128 KiB — exactly the shape C's
/// `if (len) newfile(); else file_open = 0;` (`split.c:271-274`) governs. An
/// extra third, empty tail means "the cut fills the end of the piece" was
/// written as "open the next chunk immediately".
#[test]
fn a_cut_exactly_at_the_read_block_makes_two_chunks_not_three() {
    let mut input: Vec<u8> = Vec::with_capacity(128 * 1024);
    for _ in 0..128 {
        input.extend_from_slice(&[b'z'; 1023]);
        input.push(b'\n');
    }
    let (status, chunks) = run_chunk_bytes(&["-l", "64"], &input);
    assert_eq!(status, 0);
    assert_eq!(
        chunks
            .iter()
            .map(|(name, _)| name.as_str())
            .collect::<Vec<&str>>(),
        vec!["xaa", "xab"],
        "128 KiB 的 1 KiB 行按 64 行切只有两枚，不该有空尾分片"
    );
    assert!(
        chunks.iter().all(|(_, data)| data.len() == 64 * 1024),
        "两枚各装 64 行 = 64 KiB"
    );
}

/// Same scratch-directory scheme as `run_bytes`, but **argv is handed in as raw
/// bytes** — invalid bytes cannot go into `&str`, so this leg cannot go through
/// any fixture that takes `str` parameters (`OsStr::from_bytes` exists only on
/// the host side). Returns the exit code, stderr byte for byte, and the file
/// names generated in the directory (as bytes, sorted).
fn run_raw(args: &[&[u8]], input: &[u8]) -> (i32, Vec<u8>, Vec<Vec<u8>>) {
    use std::ffi::OsStr;
    use std::os::unix::ffi::OsStrExt as _;
    let dir = scratch();
    let mut child = Command::new(SPLIT)
        .args(args.iter().map(|a| OsStr::from_bytes(a)))
        .current_dir(&dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the split binary should be runnable");
    write_stdin(&mut child, input);
    let output = child.wait_with_output().expect("the run should finish");
    let mut names: Vec<Vec<u8>> = fs::read_dir(&dir)
        .expect("the scratch dir should be readable")
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.file_name().as_bytes().to_vec())
        .collect();
    names.sort();
    let status = output.status.code().unwrap_or(-1);
    fs::remove_dir_all(&dir).expect("clean the scratch dir");
    (status, output.stderr, names)
}

/// The byte criterion for the argument seam: a bare `0xff` on the command line
/// enters the **message**, and the byte in the message must still be that `ff`.
/// Measured against the reference, shape by shape (before the change, the same
/// shape returned `ef` or `ef bf bd`):
/// 1. the letter position is one line `split: unknown option -- ` + `ff`,
///    followed by usage, exit 1 (`getopt.c:79`'s `optopt = *place++` consumes
///    one byte at a time, and `:103`'s `%c` writes it as is);
/// 2. the value positions of the four `illegal X count.` sentences are
///    `optarg`'s raw bytes (the `%s` family at `split.c:112`), on one line with
///    no usage after it;
/// 3. the reported **chunk name** and **input file name** are raw bytes too
///    (`:143`'s `*argv++` and `:137-138`'s `err(1, "%s", *argv)`).
/// The last shape pins the open leg that **still converts**: the file name goes
/// through the `&str` entry point in `libs` (registered as
/// `ARGV-BYTE-PATH-IN-LIBS`), so what lands on disk is `q` + `ef bf bd` + `aa`
/// while the reference lands `q` + `ff` + `aa`. Pinning this byte string makes
/// the gate turn red the moment that leg lands.
#[test]
fn the_diagnostic_and_the_chunk_name_carry_the_argv_bytes() {
    let unknown = [
        b"split: unknown option -- \xff\n".as_slice(),
        USAGE.as_bytes(),
    ]
    .concat();
    let (status, err, names) = run_raw(&[b"-\xff"], INPUT);
    assert_eq!(status, 1);
    assert_eq!(err, unknown);
    assert!(names.is_empty(), "报错了就不该写分片文件：{names:?}");

    // When a high byte comes first in a cluster, it is still the one reported
    // (the loop stops at the first bad letter).
    let (status, err, _names) = run_raw(&[b"-\xffZ"], INPUT);
    assert_eq!(status, 1);
    assert_eq!(err, unknown, "聚簇的字母位取短横后的第一枚字节");

    // Each of the four sentences returns the value position's raw byte; the
    // sentence is the command's own and has no usage after it.
    for (flag, sentence) in [
        (b"-l\xff".as_slice(), b"illegal line count.".as_slice()),
        (b"-b\xff".as_slice(), b"illegal byte count.".as_slice()),
        (b"-a\xff".as_slice(), b"illegal suffix length.".as_slice()),
        (
            b"-n\xff".as_slice(),
            b"illegal number of chunks.".as_slice(),
        ),
    ] {
        let (status, err, names) = run_raw(&[flag], INPUT);
        assert_eq!(status, 1, "{flag:?} 的退码");
        assert_eq!(
            err,
            [b"split: \xff: ".as_slice(), sentence, b"\n".as_slice()].concat(),
            "{flag:?} 的值位该是原字节 ff，不是 ef bf bd 三枚"
        );
        assert!(names.is_empty(), "{flag:?} 报错后不写分片：{names:?}");
    }

    // The obsolete `-<digits>` form: `ff` is not a digit, so that byte is
    // reported in the letter position (the reference has the same shape).
    let (status, err, _names) = run_raw(&[b"-\xff00"], INPUT);
    assert_eq!(status, 1);
    assert_eq!(err, unknown, "ff 走的是字母位而不是数字 kludge");

    // The prefix enters the chunk name: the reported file name is `p` + `ff` +
    // `/aa` (the open failure obtains the name before the message).
    let (status, err, _names) = run_raw(&[b"-l", b"1", b"-", b"p\xff/"], INPUT);
    assert_eq!(status, 1);
    assert_eq!(
        err,
        [
            b"split: p\xff/aa: ".as_slice(),
            b"No such file or directory\n".as_slice()
        ]
        .concat(),
        "报文里的分片名带着那枚原字节"
    );

    // Unopenable input: the original-text position of `err(1, "%s", *argv)` is
    // raw bytes too.
    let (status, err, _names) = run_raw(&[b"no\xff/such"], INPUT);
    assert_eq!(status, 1);
    assert_eq!(
        err,
        [
            b"split: no\xff/such: ".as_slice(),
            b"No such file or directory\n".as_slice()
        ]
        .concat()
    );

    // The registered exception: the open leg still goes through `&str`, so the
    // file name **on disk** is the three folded bytes (the reference lands
    // `q` + ff + `aa`). The input has 12 lines and `-l 1` means twelve chunks.
    let (status, _err, names) = run_raw(&[b"-l", b"1", b"-", b"q\xff"], INPUT);
    assert_eq!(status, 0);
    let want: Vec<Vec<u8>> = (0..12usize)
        .map(|index| {
            [
                b"q\xef\xbf\xbd".as_slice(),
                &[b'a' + (index / 26) as u8, b'a' + (index % 26) as u8][..],
            ]
            .concat()
        })
        .collect();
    assert_eq!(names, want, "这一形要等 ARGV-BYTE-PATH-IN-LIBS 落地才翻正");
}
