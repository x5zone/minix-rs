//! End-to-end face for the `caesar` binary: real process, real pipes, checked against
//! `minix3/games/caesar/caesar.c` (`main` `:190-218`, `init_rottbl` `:81-97`,
//! `guess_and_rotate` `:141-187`).
//!
//! The C has no option parsing: the argument **count** chooses the path. One argument
//! builds the table from `get_rotation` and streams standard input through it; no argument
//! runs the frequency cracker over the first 2048 bytes, prints those bytes through the
//! winning table and then streams the rest through the same table; two or more arguments
//! are the usage line with status 1. `init_rottbl` starts from the identity table
//! (`:88-89`), so everything that is not a Latin letter is written back unchanged —
//! including bytes above 0x7F, which the identity row also covers.

use std::io::Write as _;
use std::process::{Command, Stdio};

const CAESAR: &str = env!("CARGO_BIN_EXE_caesar");

fn run(args: &[&str], input: &[u8]) -> (Vec<u8>, i32, String) {
    let mut child = Command::new(CAESAR)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the caesar binary should be runnable");
    child
        .stdin
        .as_mut()
        .expect("stdin piped")
        .write_all(input)
        .expect("writing standard input");
    drop(child.stdin.take());
    let output = child.wait_with_output().expect("the run should finish");
    (
        output.stdout,
        output.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

/// Enough English prose that its letter mix is unambiguous for the cracker.
const PLAIN: &str = "the quick brown fox jumps over the lazy dog, and the frequency of \
                     letters in english prose is what this program counts. it keeps the \
                     common vowels frequent and the rare ones scarce, which is enough to \
                     tell the rotation apart from the noise around it. ";

fn shift(text: &str, by: u8) -> Vec<u8> {
    text.bytes()
        .map(|byte| match byte {
            b'a'..=b'z' => (byte - b'a' + by) % 26 + b'a',
            b'A'..=b'Z' => (byte - b'A' + by) % 26 + b'A',
            other => other,
        })
        .collect()
}

#[test]
fn a_rotation_shifts_letters_alone_and_folds_at_the_alphabet() {
    let (out, status, err) = run(&["13"], b"Hello, World! 42\n");
    assert_eq!(status, 0, "stderr was {err:?}");
    assert_eq!(out, b"Uryyb, Jbeyq! 42\n".to_vec());
    // The table is built from the identity, so a rotation of 26 is the identity and 27
    // behaves like 1 (`init_rottbl`, `caesar.c:86-96`).
    let (same, _, _) = run(&["26"], PLAIN.as_bytes());
    assert_eq!(same, PLAIN.as_bytes().to_vec());
    let (one, _, _) = run(&["1"], b"aZ zA!");
    let (twenty_seven, _, _) = run(&["27"], b"aZ zA!");
    assert_eq!(one, b"bA aB!".to_vec());
    assert_eq!(one, twenty_seven, "the rotation folds with `% 26`");
    // Bytes above the ASCII letters, including non-ASCII, keep their own row.
    let (raw, _, _) = run(&["5"], &[0xffu8, b'A', 0x80, b'\n', b'z']);
    assert_eq!(raw, vec![0xff, b'F', 0x80, b'\n', b'e']);
}

#[test]
fn the_argument_free_mode_guesses_the_rotation_and_prints_the_plaintext() {
    // The cracker's answer to text shifted by 5 is 21, because the dot product walks each
    // observed letter back onto its standard index; applying that to the ciphertext
    // restores it (`caesar.c:167-186`).
    for shift_by in [1u8, 5, 13, 19, 25] {
        let cipher = shift(PLAIN, shift_by);
        let (out, status, err) = run(&[], &cipher);
        assert_eq!(status, 0, "stderr was {err:?}");
        assert_eq!(
            out,
            PLAIN.as_bytes().to_vec(),
            "shift {shift_by} must be recovered by the guesser"
        );
    }
}

#[test]
fn only_the_first_two_thousand_forty_eight_bytes_vote_and_all_of_them_rotate() {
    // C fills `inbuf[2048]` and stops there for the analysis, then prints that prefix and
    // streams the remainder through the same table (`:156-186`). A long tail that would
    // vote for a different rotation therefore has to leave the answer alone.
    let mut input = PLAIN.as_bytes().repeat(30);
    input.truncate(2048);
    let tail = shift(PLAIN, 19).repeat(20);
    input.extend_from_slice(&tail);
    let (out, status, err) = run(&[], &input);
    assert_eq!(status, 0, "stderr was {err:?}");
    // The window is ordinary English, so the vote is rotation zero and the whole run comes
    // out as it went in — the long cipher tail changed nothing.
    assert_eq!(
        out, input,
        "the tail beyond the window cannot drag the vote"
    );
    // …which is only informative because that same tail, alone, does get a non-zero vote.
    let (alone, _, _) = run(&[], &tail);
    let expected_tail = PLAIN.as_bytes().repeat(20);
    assert_eq!(
        alone, expected_tail,
        "the tail by itself would be decrypted"
    );
    assert_ne!(alone, tail);
}

#[test]
fn the_rotation_argument_is_parsed_like_strtol_and_rejected_in_two_orders() {
    // `get_rotation` parses with `strtol`, so leading white space and a plus sign are
    // accepted before anything is decided (`caesar.c:131`), and `INT_MAX` survives to be
    // folded by `init_rottbl` (`:86`).
    for arg in [" 3", "+3", "\t3"] {
        let (out, status, err) = run(&[arg], b"abc\n");
        assert_eq!(status, 0, "stderr was {err:?}");
        assert_eq!(out, b"def\n".to_vec(), "argument {arg:?}");
    }
    let (max, status, err) = run(&["2147483647"], b"a");
    assert_eq!(status, 0, "stderr was {err:?}");
    assert_eq!(max, b"x".to_vec(), "2147483647 folds to 23");
    // A minus sign parses and then fails the range test; nothing-or-junk fails the invalid
    // test. Both print C's sentence *including* the strerror tail, which this tree does
    // carry (the Minix3 `sys_errlist` table, batch 233) — the two loops below assert the
    // exact sentences, so nothing here is left to "a phrase we invented".
    // 与 `primes` 的区别值得留在注释里：那两个命令对「一个位也没转出」的落点不同，
    // 不是因为 libc 行为不同，而是因为 caesar 自己合成 errno（`caesar.c:131-135`），
    // 而 primes 只看 `strtoumax` 留下的 errno（真源在无转换时不置号）。
    for arg in ["-1", "2147483648", "99999999999"] {
        let (out, status, err) = run(&[arg], b"abc");
        assert_eq!(out, Vec::new(), "argument {arg:?} must not be rotated");
        assert_eq!(status, 1);
        // `err(3)` 缀的是 Minix3 自己那张表的句子（`errno.h:78` 对 ERANGE 的注与之一致）。
        assert_eq!(
            err,
            format!("caesar: Bad rotation value `{arg}': Result too large or too small\n")
        );
    }
    // `abc` 是 `primes` 那处误读的照妖镜：同一个输入在 caesar 这里是 EINVAL（C 自己合成），
    // 在 primes 那里却是「illegal numeric format」那句（真源不置 errno）——两形都各有门钉住。
    for arg in ["", "5x", "-", "abc"] {
        let (out, status, err) = run(&[arg], b"abc");
        assert_eq!(out, Vec::new(), "argument {arg:?} must not be rotated");
        assert_eq!(status, 1);
        assert_eq!(
            err,
            format!("caesar: Bad rotation value `{arg}': Invalid argument\n")
        );
    }
}

#[test]
fn a_failed_write_reports_the_stdout_leg_with_the_number() {
    // `caesar.c:103-107`: a `putchar` that returns `EOF` leaves at once through
    // `err(EXIT_FAILURE, "<stdout>")` — so this leg is the one that carries the
    // number's sentence, unlike the two stdio-flag checks at the end of `main`.
    // `/dev/full` makes every write fail, which is what the C `putchar` sees.
    let full = std::fs::File::options()
        .write(true)
        .open("/dev/full")
        .expect("/dev/full is the host seam for a failing write");
    let mut child = Command::new(CAESAR)
        .args(["3"])
        .stdin(Stdio::piped())
        .stdout(Stdio::from(full))
        .stderr(Stdio::piped())
        .spawn()
        .expect("the caesar binary should be runnable");
    child
        .stdin
        .as_mut()
        .expect("stdin piped")
        .write_all(b"abc\n")
        .expect("feed the text");
    let output = child.wait_with_output().expect("wait for caesar");
    let err = String::from_utf8_lossy(&output.stderr).into_owned();
    assert_eq!(
        output.status.code(),
        Some(1),
        "`err(EXIT_FAILURE, …)` 退 1：`{err}`"
    );
    let head = "caesar: <stdout>: ";
    assert!(err.starts_with(head), "`{err}` should start with `{head}`");
    let sentence = err[head.len()..].trim_end_matches('\n');
    assert!(
        !sentence.is_empty() && sentence.chars().any(|c| c.is_lowercase()),
        "缀的应是一句 strerror 文本而不是代号：`{sentence}`"
    );
    // 更紧的一层：句子必须来自**那次失败的写**留下的号（宿主 `/dev/full` 回
    // `ENOSPC`，本树表里就是「No space left on device」）。随便填一个号——哪怕同样
    // 是一句话——都会在这里露馅，因为这句话不再说到空间。
    assert!(
        sentence.to_lowercase().contains("space"),
        "句子该说那次写的失手，不是任意一个号：`{sentence}`"
    );
}

#[test]
fn a_failed_read_reports_the_stdin_leg_without_a_number() {
    // `caesar.c:205-208`: after the output, `main` looks at `ferror(stdin)` — a flag,
    // not a number, so C leaves through `errx`: program name plus `<stdin>`, no
    // sentence. A directory as standard input is the host seam for "reading fails"
    // (the same trick `ppt`'s door uses for `EISDIR`).
    let dir = std::fs::File::open("/tmp").expect("/tmp stands in for an unreadable stream");
    let output = Command::new(CAESAR)
        .args(["3"])
        .stdin(Stdio::from(dir))
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("the caesar binary should be runnable");
    let err = String::from_utf8_lossy(&output.stderr).into_owned();
    assert_eq!(
        output.status.code(),
        Some(1),
        "`errx(EXIT_FAILURE, …)` 退 1：`{err}`"
    );
    assert_eq!(
        err, "caesar: <stdin>\n",
        "这一支只该有程序名与短语——号在 stdio 的错标志里已经丢了"
    );
    assert!(
        output.stdout.is_empty(),
        "读不出字节就没有输出可写：{:?}",
        String::from_utf8_lossy(&output.stdout)
    );
}

#[test]
fn more_than_one_argument_answers_the_usage_line_on_stderr() {
    // `main` only counts arguments (`caesar.c:192-203`), so the third case is the usage
    // line — on standard error, with nothing read from standard input.
    let (out, status, err) = run(&["5", "7"], b"abc");
    assert_eq!(out, Vec::new());
    assert_eq!((status, err.as_str()), (1, "usage: caesar [rotation]\n"));
    let (out, status, err) = run(&["5", "7", "9"], b"");
    assert_eq!(out, Vec::new());
    assert_eq!((status, err.as_str()), (1, "usage: caesar [rotation]\n"));
}
