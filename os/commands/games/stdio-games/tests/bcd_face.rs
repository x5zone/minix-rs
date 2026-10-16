//! End-to-end face for the `bcd` binary: real process, real pipes, checked against
//! `minix3/games/bcd/bcd.c` (`main` `:129-149`, `printcard` `:153-220`).
//!
//! The card shape comes straight out of `printcard`: fifteen lines per card — a top rule
//! of one blank plus 48 underscores, one text row between `/` and `|`, twelve hole rows
//! between `|` and `|`, and a bottom rule of `|` plus 48 underscores plus `|`. Characters
//! whose `holes[]` entry is zero leave a blank in the text row, the text is upper-cased
//! and cut at 48 columns, and each input character's twelve bits decide the `]` marks
//! against the row filler `"   123456789"`.
//!
//! The framing rule this gate exists to pin is `fgets` on `char cardline[80]`
//! (`:132`, `:142`): a physical line longer than **79** characters arrives as a
//! 79-character chunk and then the remainder, and each chunk becomes its own card.

mod face_support;

use std::process::{Command, Stdio};

const BCD: &str = env!("CARGO_BIN_EXE_bcd");

fn run(args: &[&str], input: &[u8]) -> (String, i32, String) {
    let mut child = Command::new(BCD)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the bcd binary should be runnable");
    face_support::write_stdin(&mut child, input);
    let output = child.wait_with_output().expect("the run should finish");
    (
        String::from_utf8_lossy(&output.stdout).into_owned(),
        output.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

/// One card is fifteen lines; `split` on the trailing newline yields one artifact.
const LINES_PER_CARD: usize = 15;

fn top_rule() -> String {
    format!(" {}", "_".repeat(48))
}

fn bottom_rule() -> String {
    format!("|{}|", "_".repeat(48))
}

#[test]
fn a_card_is_fifteen_lines_framed_by_the_two_rules() {
    let (out, status, err) = run(&["A"], b"");
    assert_eq!(status, 0, "stderr was {err:?}");
    let lines: Vec<&str> = out.split('\n').collect();
    assert_eq!(
        lines.len(),
        LINES_PER_CARD + 1,
        "fifteen lines plus the artifact"
    );
    assert_eq!(lines[0], top_rule());
    assert_eq!(lines[14], bottom_rule());
    // Every column row is `/` or `|` on the left, `|` on the right, 48 columns inside.
    for line in &lines[1..14] {
        assert_eq!(
            line.chars().count(),
            50,
            "48 columns plus the two rails: {line:?}"
        );
        assert!(line.ends_with('|'));
    }
    assert!(lines[1].starts_with('/'), "the text row opens with a slash");
}

#[test]
fn the_text_row_prints_only_characters_that_carry_a_pattern() {
    // `holes[' ']` is zero, so a blank operand produces an all-blank text row while the
    // twelve hole rows still use the row filler (`printcard`, `bcd.c:183-192`).
    let (blank, _, _) = run(&[" "], b"");
    let lines: Vec<&str> = blank.split('\n').collect();
    assert_eq!(lines[1], format!("/{}|", " ".repeat(48)));
    assert_eq!(
        lines[2],
        format!("|{}|", " ".repeat(48)),
        "rows 0..2 fill with blanks"
    );
    // `rowchars` is `"   123456789"`: the first three rows are blank and rows 3 to 11
    // carry the digits 1 to 9, so the fourth printed row is a `1` and the last is a `9`.
    assert_eq!(
        lines[5],
        format!("|{}|", "1".repeat(48)),
        "row index 3 prints `1`"
    );
    assert_eq!(
        lines[13],
        format!("|{}|", "9".repeat(48)),
        "row index 11 prints `9`"
    );
    // A character with a pattern is echoed at its own column.
    let (letter, _, _) = run(&["A"], b"");
    assert!(letter.lines().nth(1).unwrap().starts_with("/A"));
    // Among printable ASCII the space is the only entry with no pattern, so the
    // discriminating case is a control byte: `printcard` swaps it for a blank and never
    // emits the character itself (`bcd.c:186-189` with the table of `:87-120`).
    let (tab, _, _) = run(&["\t"], b"");
    let tab_lines: Vec<&str> = tab.split('\n').collect();
    assert_eq!(tab_lines[1], format!("/{}|", " ".repeat(48)));
    assert!(
        !tab.contains('\t'),
        "the tab itself never reaches the card: {tab:?}"
    );
}

#[test]
fn the_twelve_rows_read_the_pattern_from_the_top_bit_down() {
    // `bit(holes[c], 11 - row)` (`bcd.c:203`): row 0 is the high bit and row 11 the low
    // one. `A` is 0x900 — bits 11 and 8 — so exactly rows 0 and 3 punch, and the other
    // nine rows show their filler from `"   123456789"`.
    let (out, _, _) = run(&["A"], b"");
    let lines: Vec<&str> = out.split('\n').collect();
    assert_eq!(lines[2].chars().nth(1), Some(']'), "row 0 punches");
    assert_eq!(lines[5].chars().nth(1), Some(']'), "row 3 punches");
    assert_eq!(
        lines[3].chars().nth(1),
        Some(' '),
        "row 1 shows its blank filler"
    );
    assert_eq!(
        lines[6].chars().nth(1),
        Some('2'),
        "row 4 shows the filler `2`"
    );
    assert_eq!(
        lines[13].chars().nth(1),
        Some('9'),
        "row 11 shows the filler `9`"
    );
    // Only the two punched rows differ from the filler, so the `]` count is two.
    assert_eq!(out.chars().filter(|&ch| ch == ']').count(), 2);
}

#[test]
fn the_text_is_upper_cased_and_cut_at_forty_eight_columns() {
    // `printcard` truncates at `COLUMNS` and then upper-cases (`:161-169`), so the extra
    // letters of a sixty-letter operand never reach the card and `a` is drawn as `A`.
    let (out, _, _) = run(&["a".repeat(60).as_str()], b"");
    let text = out.lines().nth(1).unwrap();
    assert_eq!(text, format!("/{}|", "A".repeat(48)));
    let (same, _, _) = run(&["A".repeat(60).as_str()], b"");
    assert_eq!(out, same, "case is normalised before the pattern lookup");
}

#[test]
fn a_line_longer_than_seventy_nine_characters_becomes_two_cards() {
    // C reads with `fgets` into `char cardline[80]`, so the first chunk is 79 characters
    // (the terminator owns the last byte) and the rest follows in a second `fgets` — and
    // every chunk is its own card, itself cut at 48 columns (`:132`, `:142-144`).
    let hundred = "A".repeat(100);
    let (out, status, err) = run(&[], format!("{hundred}\n").as_bytes());
    assert_eq!(status, 0, "stderr was {err:?}");
    let lines: Vec<&str> = out.split('\n').collect();
    assert_eq!(
        lines.len(),
        2 * LINES_PER_CARD + 1,
        "one card for the 79-character chunk, one for the remaining 21"
    );
    assert_eq!(
        lines[1],
        format!("/{}|", "A".repeat(48)),
        "first card is full width"
    );
    let leftover = [vec![b'A'; 21], vec![b' '; 27]].concat();
    let expected_second = format!("/{}|", String::from_utf8_lossy(&leftover));
    assert_eq!(
        lines[LINES_PER_CARD + 1],
        expected_second,
        "the second card carries only the leftover characters"
    );
    assert_eq!(
        lines[LINES_PER_CARD],
        top_rule(),
        "the second card starts its own frame"
    );
}

#[test]
fn there_is_no_option_parsing_so_a_leading_dash_is_text() {
    // `main` only tests the argument count (`bcd.c:139`), so `-x` becomes a card and the
    // lone dash of `bcd -` is rendered rather than treated as standard input.
    let (out, status, err) = run(&["-x"], b"");
    assert_eq!(status, 0, "stderr was {err:?}");
    assert!(out.lines().nth(1).unwrap().starts_with("/-X"));
    // A lone `-` is an operand too, so it is punched rather than meaning “read stdin”.
    let (dash, status, _) = run(&["-"], b"");
    assert_eq!(status, 0);
    assert_eq!(
        dash.lines().nth(1).unwrap(),
        format!("/-{}|", " ".repeat(47))
    );
    let (quiet, status, err) = run(&[], b"");
    assert_eq!(
        (quiet.as_str(), status, err.as_str()),
        ("", 0, ""),
        "empty input prints nothing"
    );
}
