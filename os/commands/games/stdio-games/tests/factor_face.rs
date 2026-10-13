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

use std::io::Write as _;
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
    child
        .stdin
        .as_mut()
        .expect("stdin piped")
        .write_all(input)
        .expect("writing standard input");
    drop(child.stdin.take());
    let output = child.wait_with_output().expect("the run should finish");
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
fn an_unterminated_last_line_is_still_a_value_and_blank_lines_are_skipped() {
    // C's `fgets` returns the final fragment (it is not an empty read), and an empty line
    // is dropped by the leading-blank scan that finds `\n` right away (`:163-166`).
    let (tail, status, err) = run(&[], b"6");
    assert_eq!(status, 0, "stderr was {err:?}");
    assert_eq!(tail, "6: 2 3\n", "the last line needs no newline");
    let (blanks, status, _) = run(&[], b"\n\n");
    assert_eq!(
        (blanks.as_str(), status),
        ("", 0),
        "empty lines produce nothing at all"
    );
    // Mixed: blanks in front and in the middle do not shift the value lines.
    let (mixed, _, _) = run(&[], b"\n6\n\n8\n");
    assert_eq!(mixed, "6: 2 3\n8: 2 2 2\n");
}
