//! End-to-end face for the `morse` binary: real process, real pipes, checked against
//! `minix3/games/morse/morse.c` (the option loop `:130-144`, the encode loop
//! `:186-196`, `morse()` `:231-251` and `show()` `:253-261`).
//!
//! The goldens are read off the C, not off what this shell happens to emit. Three
//! shapes carry it: `show()` prints one **line per input byte** (a leading space before
//! every `dit`/`daw`, or the symbol run itself under `-s`), each argument closes with an
//! extra `show("")` line, and one `show("...-.-")` ends the whole message — so the
//! trailing `SK` line belongs to the run, never to an argument.

use std::io::Write as _;
use std::process::{Command, Stdio};

const MORSE: &str = env!("CARGO_BIN_EXE_morse");

fn run(args: &[&str], input: &[u8]) -> (String, i32, String) {
    let mut child = Command::new(MORSE)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the morse binary should be runnable");
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

/// `show("...-.-")`, spelled out: the mark that closes every encode run.
const SK: &str = " dit dit dit daw dit daw\n";

#[test]
fn the_default_form_spells_one_line_per_byte_and_closes_the_run_with_sk() {
    // `morse SOS` walks three bytes through `morse()`/`show()`, then `show("")` for the
    // argument, then the mark once (`morse.c:186-196`).
    let (out, status, err) = run(&["SOS"], b"");
    assert_eq!(status, 0, "stderr was {err:?}");
    assert_eq!(
        out,
        " dit dit dit\n daw daw daw\n dit dit dit\n\n".to_string() + SK,
        "three code lines, the argument's blank line, then the mark"
    );
    // Case makes no difference: C indexes `alph` with `isupper` folded away (`:236`).
    let (lower, _, _) = run(&["sos"], b"");
    assert_eq!(lower, out);
}

#[test]
fn the_short_form_prints_the_symbol_run_and_even_a_blank_line_keeps_its_space() {
    // `-s` replaces the words with the run itself, `printf(" %s", s)` (`morse.c:257`),
    // so the argument's closing line is a single space rather than an empty line — the
    // one place where the two forms differ beyond the alphabet.
    let (out, status, err) = run(&["-s", "SOS"], b"");
    assert_eq!(status, 0, "stderr was {err:?}");
    assert_eq!(out, " ...\n ---\n ...\n \n ...-.-\n");
    // Without the flag the same run spells symbols and its closing line is empty.
    let (spelled, _, _) = run(&["SOS"], b"");
    assert_eq!(spelled.lines().nth(3), Some(""));
    assert_eq!(out.lines().nth(3), Some(" "));
}

#[test]
fn standard_input_is_consumed_byte_by_byte_and_separators_earn_their_own_line() {
    // No operands means `while ((ch = getchar()) != EOF) morse(ch)` (`morse.c:193-194`):
    // no line grouping at all, so the newline is just another `isspace` byte rendered as
    // the empty code, and a run with no trailing newline still ends with its own blank.
    let (out, status, err) = run(&[], b"a\nb");
    assert_eq!(status, 0, "stderr was {err:?}");
    // `a` is `.-` and `b` is `-...` (`morse.c:66-67`), the newline in between is a
    // separator line of its own, and the run closes with the mark.
    assert_eq!(out, " dit daw\n\n daw dit dit dit\n".to_string() + SK);
    // The same bytes through the short form: `.-`, a space line, `-...`.
    let (short, _, _) = run(&["-s"], b"a\nb");
    assert_eq!(short, " .-\n \n -...\n ...-.-\n");
    // Vertical tab is `isspace` in the C locale, so it too earns a blank line — the one
    // separator `u8::is_ascii_whitespace` would have dropped.
    let (vt, _, _) = run(&[], b"\x0b");
    assert_eq!(vt, "\n".to_string() + SK);
}

#[test]
fn bytes_the_tables_do_not_list_contribute_no_line_at_all() {
    // C's `else` arm scans `other[]` and, on a miss, simply never calls `show()`
    // (`morse.c:241-250`) — the byte is silent, not an error and not an empty line.
    let (out, status, err) = run(&[], b"!");
    assert_eq!(status, 0, "stderr was {err:?}");
    assert_eq!(out, SK, "only the mark survives an unlisted byte");
    // The same for a byte above the ASCII tables, and a run of them stays silent.
    let (at, _, _) = run(&[], b"@");
    assert_eq!(at, SK);
    let (high, _, _) = run(&[], b"\xff\x80");
    assert_eq!(high, SK);
}

#[test]
fn punctuation_and_digits_come_straight_from_the_c_tables() {
    // `other[]` (`morse.c:97-111`) gives the period `. - . - . -`, and the `digit[]`
    // table (`:53-64`) gives five dots for `5`.
    let (out, _, _) = run(&[], b".");
    assert_eq!(out, " dit daw dit daw dit daw\n".to_string() + SK);
    let (five, _, _) = run(&[], b"5");
    assert_eq!(five, " dit dit dit dit dit\n".to_string() + SK);
    // A hyphen is punctuation even though `-` is also the dash symbol itself.
    let (dash, _, _) = run(&[], b"-");
    assert_eq!(dash, " daw dit dit dit dit daw\n".to_string() + SK);
}

#[test]
fn each_argument_is_closed_and_the_whole_run_is_closed_once() {
    // The `show("")` sits inside the argument loop (`morse.c:191`) while the mark sits
    // after it (`:195`), so two operands yield two blank lines and one mark.
    let (out, _, _) = run(&["sos", "a"], b"");
    assert_eq!(
        out,
        " dit dit dit\n daw daw daw\n dit dit dit\n\n dit daw\n\n".to_string() + SK
    );
    // An empty operand still goes through the loop: nothing to encode, one closing line.
    let (empty, _, _) = run(&[""], b"");
    assert_eq!(empty, "\n".to_string() + SK);
}

#[test]
fn a_lone_dash_is_an_operand_and_double_dash_ends_the_option_scan() {
    // BSD `getopt` stops at the first non-option, and `-` counts as one (`morse.c:130`),
    // so it is encoded as the hyphen rather than answered with the usage line.
    let (out, status, err) = run(&["-"], b"");
    assert_eq!(status, 0, "stderr was {err:?}");
    assert_eq!(out, " daw dit dit dit dit daw\n\n".to_string() + SK);
    // `--` is consumed by getopt and never reaches the operands.
    let (after, _, _) = run(&["--", "-"], b"");
    assert_eq!(after, " daw dit dit dit dit daw\n\n".to_string() + SK);
}

#[test]
fn an_unknown_letter_answers_the_usage_line_on_stderr_with_status_one() {
    // C's `case '?'`/`default:` writes to `stderr` and exits 1 (`morse.c:138-141`),
    // printing nothing to standard output even if a valid letter came first in the cluster.
    let (out, status, err) = run(&["-x"], b"");
    assert_eq!(status, 1);
    assert_eq!(out, "");
    assert_eq!(err, "usage: morse [-ds] [string ...]\n");
    // Inside a cluster the earlier letters do not soften the answer.
    let (out, status, err) = run(&["-dsx"], b"");
    assert_eq!(
        (out.as_str(), status, err.as_str()),
        ("", 1, "usage: morse [-ds] [string ...]\n")
    );
    // A code-shaped operand is not a free pass either: `getopt` sees the leading dash and
    // answers for the unknown letter after it (`morse.c:130-141`).
    let (out, status, err) = run(&["-d", "-..."], b"");
    assert_eq!(
        (out.as_str(), status, err.as_str()),
        ("", 1, "usage: morse [-ds] [string ...]\n")
    );
}

#[test]
fn the_short_flag_is_inert_when_the_direction_flag_is_set() {
    // `decode()` never reads `sflag` (`morse.c:201-228`), so `-s` beside `-d` changes no
    // byte of the output. Groups separated by a single space also decode the same in C,
    // because its `is_blank` flag is cleared by every decoded group (`:170-182`).
    let (plain, status, err) = run(&["-d"], b"... --- ...\n");
    assert_eq!(status, 0, "stderr was {err:?}");
    assert_eq!(plain, "SOS\n");
    let (short, _, _) = run(&["-sd"], b"... --- ...\n");
    assert_eq!(plain, short);
    // An operand is decoded as one whole string, exactly as C's `decode(*argv)` does
    // (`:146-150`): the group is looked up verbatim, so a name that is not in the tables
    // decodes to nothing here and to `x` in C — the shape difference is registered.
    let (operand, _, _) = run(&["-d", "....-"], b"");
    assert_eq!(operand, "4\n", "digit table index four");
}
