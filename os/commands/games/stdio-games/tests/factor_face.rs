//! End-to-end face for the `factor` binary: real process, real pipes, checked against
//! `minix3/games/factor/factor.c` (the option scan and both branches `:146-180`, the
//! printing `pr_fact` `:196-253`).
//!
//! The goldens are read off the C: `pr_fact` writes the value, a `:`, then one
//! `" %llu"` per factor and a final newline (`:241-245`, `:252`), so every value is
//! exactly one line. With operands the values come from `argv` (`:172-179`); with none
//! they come one per `fgets` line of standard input (`:156-171`) — that second branch is
//! what this gate exists to prove, because a shell that reads standard input through the
//! kernel transport instead of the process' own descriptor prints nothing at all.

mod face_support;

use std::process::{Command, Stdio};

const FACTOR: &str = env!("CARGO_BIN_EXE_factor");

fn run(args: &[&str], input: &[u8]) -> (String, i32, String) {
    let mut child = Command::new(FACTOR)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the factor binary should be runnable");
    face_support::write_stdin(&mut child, input);
    let output = child.wait_with_output().expect("the run should finish");
    (
        String::from_utf8_lossy(&output.stdout).into_owned(),
        output.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

/// Runs the binary with standard input set to an **unreadable** descriptor: opening a
/// directory succeeds but reading it fails with `EISDIR`, which is exactly the case C
/// separates from end of input (`factor.c:157-161`, `ppt.c:109-111`).
fn run_with_directory_stdin(args: &[&str]) -> (String, i32, String) {
    let dir = std::fs::File::open(std::env::temp_dir())
        .expect("the temporary directory should open read-only");
    let output = Command::new(FACTOR)
        .args(args)
        .stdin(Stdio::from(dir))
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("the run should finish");
    (
        String::from_utf8_lossy(&output.stdout).into_owned(),
        output.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

#[test]
fn operands_print_one_line_each_in_the_c_shape() {
    // `printf(" %" PRIu64, *fact)` per factor after `value:` and one `putchar('\n')`
    // (`pr_fact`, `factor.c:228-252`): a repeated factor repeats, and a value the prime
    // table cannot divide prints itself after one space.
    let (out, status, err) = run(&["6", "10", "7"], b"");
    assert_eq!(status, 0, "stderr was {err:?}");
    assert_eq!(out, "6: 2 3\n10: 2 5\n7: 7\n");
    let (powers, _, _) = run(&["12", "8"], b"");
    assert_eq!(powers, "12: 2 2 3\n8: 2 2 2\n");
    let (prime, _, _) = run(&["9973"], b"");
    assert_eq!(
        prime, "9973: 9973\n",
        "a prime past the table prints itself"
    );
    let (mixed, _, _) = run(&["1000000"], b"");
    assert_eq!(mixed, "1000000: 2 2 2 2 2 2 5 5 5 5 5 5\n");
}

#[test]
fn standard_input_supplies_one_value_per_line_and_matches_the_operand_run() {
    // The no-operand branch is `fgets` per value (`factor.c:156-171`), so the same three
    // values arriving through the pipe must produce the same three lines as operands.
    let (piped, status, err) = run(&[], b"6\n10\n7\n");
    assert_eq!(status, 0, "stderr was {err:?}");
    assert_eq!(piped, "6: 2 3\n10: 2 5\n7: 7\n");
    let (operands, _, _) = run(&["6", "10", "7"], b"");
    assert_eq!(
        piped, operands,
        "the two branches differ only in where the text comes from"
    );
}

#[test]
fn an_unterminated_last_line_is_still_a_value() {
    // C's `fgets` returns the final fragment because it is not an empty read (`:156-161`),
    // and `BN_dec2bn` then finds the string end after the digits (`:119-128`).
    let (tail, status, err) = run(&[], b"6");
    assert_eq!(status, 0, "stderr was {err:?}");
    assert_eq!(tail, "6: 2 3\n", "the last line needs no newline");
    // Blank lines in front of and between values do not shift the value lines.
    let (mixed, _, _) = run(&[], b"\n6\n\n8\n");
    assert_eq!(mixed, "6: 2 3\n8: 2 2 2\n");
}

#[test]
fn a_value_below_two_stops_the_run_with_c_sentence_on_stderr() {
    // `pr_fact` opens with the firewall and leaves through `errx` (`factor.c:218-219`), so
    // what was printed before it stays out and nothing after it is attempted. The status is
    // 1 and the sentence carries the program name, which is what `errx` prefixes.
    let (out, status, err) = run(&["6", "1", "10"], b"");
    assert_eq!(out, "6: 2 3\n", "the run stops at the rejected value");
    assert_eq!(status, 1);
    assert_eq!(err, "factor: numbers <= 1 aren't permitted.\n");
    // Zero is the same answer, on the stdin path too (`:156-171` feeding `:218-219`).
    let (out, status, err) = run(&[], b"0\n");
    assert_eq!(
        (out.as_str(), status, err.as_str()),
        ("", 1, "factor: numbers <= 1 aren't permitted.\n")
    );
}

#[test]
fn leading_blanks_belong_to_the_number_and_a_blank_only_line_is_silent() {
    // C scans `isblank` bytes off the front of a line and continues when the line then ends
    // (`factor.c:163-165`), and `strtoul` skips the same set, so an indented value parses.
    let (out, status, err) = run(&[], b"  6\n   \n8\n");
    assert_eq!(status, 0, "stderr was {err:?}");
    assert_eq!(out, "6: 2 3\n8: 2 2 2\n");
    // A leading plus sign is part of C's number as well (`:119-128`).
    let (plus, _, _) = run(&["+12"], b"");
    assert_eq!(plus, "12: 2 2 3\n");
}

#[test]
fn the_two_minus_sentences_depend_on_which_loop_saw_the_value() {
    // A `-` on a stdin line is the dedicated check after the blank scan (`factor.c:166-168`),
    // while an operand that starts with `-` answers with the `numbers <= 1` sentence — C's
    // own wording slip (`:173-174`). A lone `-` is an operand, not an option.
    let (out, status, err) = run(&[], b"-6\n");
    assert_eq!(
        (out.as_str(), status, err.as_str()),
        ("", 1, "factor: negative numbers aren't permitted.\n")
    );
    let (out, status, err) = run(&["-"], b"");
    assert_eq!(
        (out.as_str(), status, err.as_str()),
        ("", 1, "factor: numbers <= 1 aren't permitted.\n")
    );
}

#[test]
fn trailing_junk_is_reported_as_an_illegal_numeric_format() {
    // `BN_dec2bn` calls the value valid only when the parse stopped at the end or at a
    // newline (`factor.c:119-128`); anything else prints C's sentence with the operand named
    // (`:177`). (C's stdin branch passes a spent `argv[0]` there; this shell names the line,
    // which is declared in the library's own documentation.)
    let (out, status, err) = run(&["12x"], b"");
    assert_eq!(out, "");
    assert_eq!(status, 1);
    assert_eq!(err, "factor: 12x: illegal numeric format.\n");
    let (junk, _, _) = run(&[], b"6\n12x\n");
    assert_eq!(junk, "6: 2 3\n", "the line before the bad one still prints");
}

#[test]
fn a_failed_read_leaves_with_c_stdin_sentence_and_exits_one() {
    // C 在 `fgets` 返回 `NULL` 且 `ferror(stdin)` 时走 `err(1, "stdin")`（`factor.c:157-161`）,
    // 与「只是读到尽头」分两条路：后者 `exit(0)` 且不出声。读一个目录的描述符必定出错
    // （`EISDIR`），于是这句能在进程级看到；尾句同样取自本树的表（21 号 `Is a directory`）。
    let (out, status, err) = run_with_directory_stdin(&[]);
    assert_eq!(out, "");
    assert_eq!(status, 1);
    assert_eq!(err, "factor: stdin: Is a directory\n");
    // 对照：空的管道只是读到尽头，C 与本树都安静退 0。
    let (quiet, quiet_status, quiet_err) = run(&[], b"");
    assert_eq!(
        (quiet.as_str(), quiet_status, quiet_err.as_str()),
        ("", 0, "")
    );
}

#[test]
fn an_overflowed_value_leaves_with_the_range_sentence_not_the_format_one() {
    // `err(1, "%s", str)` (`factor.c:126`) 把 `strtoul` 设的 `ERANGE` 织进句子，而且走在
    // 行末检验之前：同一个带尾巴的超大数答这句而不提格式。句子取自本树的 `sys_errlist`，
    // 因此是 Minix3 的 `Result too large or too small` 而不是 glibc 的那句。
    let (out, status, err) = run(&["18446744073709551616"], b"");
    assert_eq!(out, "");
    assert_eq!(status, 1);
    assert_eq!(
        err,
        "factor: 18446744073709551616: Result too large or too small\n"
    );
    // 溢出优先于「行末只许换行」。
    let (_, _, junked) = run(&["99999999999999999999999x"], b"");
    assert_eq!(
        junked,
        "factor: 99999999999999999999999x: Result too large or too small\n"
    );
    // 标准输入那条腿同理，只是 C 把整行（含换行）当 `%s`；本壳与 illegal 那句同样
    // 先剔行尾换行（已在库文档里声明的分流）。
    let (out, status, stdin_err) = run(&[], b"18446744073709551616\n");
    assert_eq!(out, "");
    assert_eq!(status, 1);
    assert_eq!(
        stdin_err,
        "factor: 18446744073709551616: Result too large or too small\n"
    );
}

#[test]
fn an_option_like_argument_answers_the_usage_line_and_leaves_with_zero() {
    // The optstring is empty (`factor.c:147`), so any `-x` falls into `case '?'` and reaches
    // `usage()`, which prints to standard error and exits **0** (`:266-270`). `--` ends the
    // scan without becoming an operand, and a lone `-` is an operand rather than an option.
    let (out, status, err) = run(&["-x"], b"");
    assert_eq!(
        (out.as_str(), status, err.as_str()),
        ("", 0, "usage: factor [value ...]\n")
    );
    let (after, status, err) = run(&["--", "6"], b"");
    assert_eq!((after.as_str(), status, err.as_str()), ("6: 2 3\n", 0, ""));
}
