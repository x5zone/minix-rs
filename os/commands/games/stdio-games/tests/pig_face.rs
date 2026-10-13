//! End-to-end face for the `pig` binary: real process, real pipes, checked against
//! `minix3/games/pig/pig.c` (the stream in `main` `:62-92` and the rotation in `pigout`
//! `:94-130`).
//!
//! Every golden below is a byte-for-byte trace of C, not of what this shell happens to
//! print. The four shapes that carry it: a word is a run of `isalpha` bytes and any other
//! byte both flushes it and passes through (`:78-89`); a word pending at end of input is
//! **never** printed (`:76-92`); the `way` test uses `aeiouAEIOU` while the cluster loop
//! stops at `aeiouyAEIOUY` from the first position (`:103`, `:114`); and a moved cluster
//! keeps its own interior case because only `buf[0]` is lower-cased and only the new first
//! character is upper-cased (`:111-112`, `:124-125`).

use std::io::Write as _;
use std::process::{Command, Stdio};

const PIG: &str = env!("CARGO_BIN_EXE_pig");

fn run(args: &[&str], input: &[u8]) -> (String, i32, String) {
    let mut child = Command::new(PIG)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the pig binary should be runnable");
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
fn a_word_is_a_run_of_letters_and_every_other_byte_passes_through() {
    // Digits and punctuation are not letters, so they flush the pending word and are written
    // unchanged (`pig.c:78-89`). `abc` opens with a vowel, so it only gains `way`.
    let (out, status, err) = run(&[], b"abc123\n");
    assert_eq!(status, 0, "stderr was {err:?}");
    assert_eq!(out, "abcway123\n");
    let (mixed, _, _) = run(&[], b"hello_world; goodbye!");
    assert_eq!(mixed, "ellohay_orldway; oodbyegay!");
}

#[test]
fn a_pending_word_at_end_of_input_is_never_printed() {
    // The loop condition is `getchar() != EOF` and nothing flushes after it
    // (`pig.c:76-92`), so a last word that never met a separator disappears, while the same
    // word followed by a space comes out whole.
    let (out, status, err) = run(&[], b"hello");
    assert_eq!(status, 0, "stderr was {err:?}");
    assert_eq!(out, "", "the unflushed word is dropped");
    let (flushed, _, _) = run(&[], b"hello ");
    assert_eq!(flushed, "ellohay ");
    // One trailing newline is enough to flush the last word of a line.
    let (line, _, _) = run(&[], b"hello\n");
    assert_eq!(line, "ellohay\n");
}

#[test]
fn the_two_vowel_sets_decide_which_words_move_at_all() {
    // `yellow` never rotates because the cluster loop tests the first position against
    // `aeiouyAEIOUY` (`:114`), while `yam` behaves the same way; `quay` rotates as `ayquay`
    // because the `qu` unit moves together (`:117-119`).
    let (out, _, _) = run(&[], b"yellow yam quay Pig\n");
    assert_eq!(out, "yelloway yamay ayquay Igpay\n");
    // A word that opens with a real vowel keeps itself and gains `way` (`:103`).
    let (vowel, _, _) = run(&[], b"apple eat\n");
    assert_eq!(vowel, "appleway eatway\n");
}

#[test]
fn case_travels_with_the_letters_rather_than_being_normalized() {
    // `Pig` loses its capital only in the sense that the capital moves to the new first
    // character (`:124-125`), and an interior capital in the cluster survives the move
    // (`:111-112` lower-cases `buf[0]` alone). An all-capital word keeps every letter and
    // grows a capital suffix.
    let (out, _, _) = run(&[], b"Pig String sTring STRING APPLE\n");
    assert_eq!(out, "Igpay Ingstray ingsTray INGSTRAY APPLEWAY\n");
}

#[test]
fn one_thousand_twenty_four_letters_fit_and_the_next_one_is_fatal() {
    // `buf[1024]` is checked before each store, so a run of 1024 letters still rotates while
    // the 1025th trips `errx(1, "ate too much!")` (`pig.c:78-79`).
    let words = "a".repeat(1024);
    let (out, status, err) = run(&[], format!("{words}\n").as_bytes());
    assert_eq!(status, 0, "stderr was {err:?}");
    assert_eq!(
        out,
        format!("{words}way\n"),
        "a vowel-first run keeps itself"
    );
    let too_many = "a".repeat(1025);
    let (out, status, err) = run(&[], too_many.as_bytes());
    assert_eq!(out, "", "the pending word is dropped on the fatal path");
    assert_eq!(status, 1);
    assert_eq!(err, "pig: ate too much!\n");
}

#[test]
fn operands_are_counted_and_ignored_while_a_bad_option_answers_usage() {
    // The optstring is empty (`pig.c:66`), so `--` is consumed and a lone `-` is an operand;
    // operands never reach the rotation because `main` only reads standard input afterwards
    // (`:76`). Any other `-x` falls into `case '?'` and `usage()` exits 1 (`:68-70`, `:131`).
    let (out, status, err) = run(&["foo"], b"hi\n");
    assert_eq!(status, 0, "stderr was {err:?}");
    assert_eq!(
        out, "ihay\n",
        "the operand is ignored, stdin is still consumed"
    );
    let (out, status, _) = run(&["-"], b"hi\n");
    assert_eq!((out.as_str(), status), ("ihay\n", 0));
    let (out, status, _) = run(&["--", "hi"], b"hi\n");
    assert_eq!((out.as_str(), status), ("ihay\n", 0));
    let (out, status, err) = run(&["-x"], b"");
    assert_eq!(
        (out.as_str(), status, err.as_str()),
        ("", 1, "usage: pig\n")
    );
}
