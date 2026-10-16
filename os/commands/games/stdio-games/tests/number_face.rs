//! End-to-end face for the `number` binary: real process, real pipes, checked against
//! `minix3/games/number/number.c` (`main` `:89-123`, `convert` `:125-192`, `unit` `:194-227`,
//! `number` `:229-265`, `pfract` `:267-283`).
//!
//! The goldens were derived by transcribing C's printing calls — including its habit of
//! ending every scale group with a period of its own (`unit` at `number.c:207-208`, `:216-217`,
//! `:222-223`, so `1234` comes out as two lines), and the fraction name is built from the
//! fraction's **digit count**, not its value (`pfract`, `:267-283`). A C reference binary built
//! from that source with the host compiler was run for every refusal shape below.
//!
//! The three entry-face shapes are the ones a reader is most likely to smooth away: an
//! operand-less run reads `fgets` lines of at most 255 characters and a line without its
//! newline is fatal (`:108-111`); every item after the first is announced by `"...\n"`
//! (`:112-113`, `:118-119`); and `-l` swaps the group periods for spaces while the
//! fraction name keeps its period (`:190-191`).

mod face_support;

use std::os::unix::fs::PermissionsExt as _;
use std::path::Path;
use std::process::{Command, Stdio};

const NUMBER: &str = env!("CARGO_BIN_EXE_number");
const USAGE: &str = "usage: number [# ...]\n";

fn run(args: &[&str], input: &[u8]) -> (String, i32, String) {
    run_at(Path::new(NUMBER), args, input)
}

fn run_at(program: &Path, args: &[&str], input: &[u8]) -> (String, i32, String) {
    let mut child = Command::new(program)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the number binary should be runnable");
    face_support::write_stdin(&mut child, input);
    let output = child.wait_with_output().expect("the run should finish");
    (
        String::from_utf8_lossy(&output.stdout).into_owned(),
        output.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

#[test]
fn every_scale_group_ends_with_a_period_of_its_own() {
    let (out, status, err) = run(&["1234"], b"");
    assert_eq!(status, 0, "stderr was {err:?}");
    assert_eq!(out, "one thousand.\ntwo hundred thirty-four.\n");
    let (small, _, _) = run(&["0"], b"");
    assert_eq!(small, "zero.\n");
    let (zeros, _, _) = run(&["007"], b"");
    assert_eq!(zeros, "seven.\n", "a leading zero group speaks nothing");
    let (big, _, _) = run(&["123456789"], b"");
    assert_eq!(
        big,
        "one hundred twenty-three million.\nfour hundred fifty-six thousand.\n\
         seven hundred eighty-nine.\n"
    );
}

#[test]
fn the_fraction_is_named_by_how_many_digits_it_has() {
    let (out, _, _) = run(&["1.25"], b"");
    assert_eq!(out, "one.\nand\ntwenty-five.\nhundredths.\n");
    let (zero_fraction, _, _) = run(&["1.0"], b"");
    assert_eq!(
        zero_fraction, "one.\n",
        "an all-zero fraction says nothing more"
    );
    let (leading_zero, _, _) = run(&["0.05"], b"");
    assert_eq!(leading_zero, "five.\nhundredths.\n");
    // `-l` puts the whole reading on one line but keeps `pfract`'s own period, and the line
    // still ends with the newline C adds after it (`number.c:190-191`).
    let (linear, _, _) = run(&["-l", "1.25"], b"");
    assert_eq!(linear, "one and twenty-five hundredths.\n\n");
    let (linear_big, _, _) = run(&["-l", "1234"], b"");
    assert_eq!(linear_big, "one thousand two hundred thirty-four\n");
}

#[test]
fn an_operand_without_a_newline_is_a_line_too_long() {
    // `fgets` keeps the newline it read, and its absence means the line was cut short —
    // C calls this fatal rather than treating the fragment as a line (`:108-111`).
    let (out, status, err) = run(&[], b"5");
    assert_eq!(out, "");
    assert_eq!((status, err.as_str()), (1, "number: line too long.\n"));
    // The same bytes with the newline are fine, and a second line is announced by "...".
    let (ok, status, err) = run(&[], b"5\n");
    assert_eq!(status, 0, "stderr was {err:?}");
    assert_eq!(ok, "five.\n");
    // A 255-digit line is exactly what C's `fgets` cannot finish: the terminator owns the
    // 256th byte of `line[256]`, so the first chunk has no newline and the refusal comes
    // before the digit-count ceiling is ever consulted.
    let long_digits = "1".repeat(255);
    let (out, status, err) = run(&[], format!("{long_digits}\n").as_bytes());
    assert_eq!(out, "");
    assert_eq!((status, err.as_str()), (1, "number: line too long.\n"));
    let (two, _, _) = run(&[], b"7\n9\n");
    assert_eq!(two, "seven.\n...\nnine.\n");
    let (operands, _, _) = run(&["7", "9"], b"");
    assert_eq!(operands, two, "operands share the separator rule");
}

#[test]
fn the_refusals_print_exactly_what_c_would_still_hold() {
    // C's scan writes a terminator at **one** byte only — the `.` it turns into `\0`
    // (`number.c:149`) — so `errx(1, "illegal number: %s", line)` (`:156`) prints everything
    // from the (possibly advanced) start to that `.` when there is one, and the whole rest of
    // the buffer when there is not. The C reference binary answers `1e5` with `1e5`.
    let (out, status, err) = run(&["1e5"], b"");
    assert_eq!(out, "");
    assert_eq!((status, err.as_str()), (1, "number: illegal number: 1e5\n"));
    let (out, status, err) = run(&["1.2", "3"], b"");
    assert_eq!(status, 0, "stderr was {err:?}");
    let _ = &err;
    assert_eq!(out, "one.\nand\ntwo.\ntenths.\n...\nthree.\n");
    // The second `.` is the refusal, and the first one has already ended C's string, so the
    // report is still just `1`.
    let (out, status, err) = run(&["--", "1..2"], b"");
    assert_eq!(out, "");
    assert_eq!((status, err.as_str()), (1, "number: illegal number: 1\n"));
    // A `-` that is not leading is refused, and the bytes after it are still in `line`.
    let (out, status, err) = run(&["12-34"], b"");
    assert_eq!(out, "");
    assert_eq!(
        (status, err.as_str()),
        (1, "number: illegal number: 12-34\n")
    );
    // 65 digits is the ceiling, on either side of the point (`:162-164`).
    let long = "9".repeat(70);
    let (out, status, err) = run(&[long.as_str()], b"");
    assert_eq!(out, "");
    assert_eq!(
        (status, err.as_str()),
        (1, "number: number too large, max 65 digits.\n")
    );
}

#[test]
fn a_refusal_on_the_read_path_carries_the_lines_own_newline() {
    // On the `fgets` path C's `line` keeps its newline (`:108-109`), and `errx` appends one of
    // its own (`verrx.c:62`). So a refused stdin line prints its text *and* a blank line — a
    // real observable, not a formatting accident, and the reason the operand and stdin paths
    // of the same refusal differ by that one byte.
    let (out, status, err) = run(&[], b"12a\n");
    assert_eq!(out, "");
    assert_eq!(
        (status, err.as_str()),
        (1, "number: illegal number: 12a\n\n")
    );
    // Interior blanks are not a cut point either: the tab survives inside the report.
    let (_, _, err) = run(&[], b"12\t34\n");
    assert_eq!(err, "number: illegal number: 12\t34\n\n");
    // With a `.` in front, that byte ends the text — and the newline ends with it, so this
    // shape stays a single line.
    let (_, _, err) = run(&[], b"1.2 3\n");
    assert_eq!(err, "number: illegal number: 1\n");
    // Leading blanks are consumed one by one, and the report starts after them (`:135-140`).
    let (_, _, err) = run(&[], b"  7 x\n");
    assert_eq!(err, "number: illegal number: 7 x\n\n");
}

#[test]
fn the_three_refusals_follow_argv_zero_while_the_usage_does_not() {
    // `errx` prints `getprogname()` (`verrx.c:59`), but `usage()` spells the word `number`
    // into its literal (`number.c:288`). A renamed copy therefore moves three lines and not
    // the fourth.
    let copied = std::env::temp_dir().join(format!("number-face-copy-{}", std::process::id()));
    std::fs::copy(NUMBER, &copied).expect("copy the binary");
    std::fs::set_permissions(&copied, std::fs::Permissions::from_mode(0o755)).expect("make it run");
    let name = copied.file_name().unwrap().to_str().unwrap().to_string();

    let (_, status, err) = run_at(&copied, &["1e5"], b"");
    assert_eq!(
        (status, err.as_str()),
        (1, format!("{name}: illegal number: 1e5\n").as_str())
    );

    let (_, _, err) = run_at(&copied, &[], b"5");
    assert_eq!(err, format!("{name}: line too long.\n"));

    let huge = "9".repeat(70);
    let (_, _, err) = run_at(&copied, &[huge.as_str()], b"");
    assert_eq!(err, format!("{name}: number too large, max 65 digits.\n"));

    let (_, status, err) = run_at(&copied, &["-x"], b"");
    assert_eq!((status, err.as_str()), (1, USAGE));

    let _ = std::fs::remove_file(&copied);
}

#[test]
fn the_option_face_has_one_letter_and_no_getopt_magic() {
    // `getopt(argc, argv, "l")` (`:95-103`): the only letter is `l`, everything else answers
    // the usage line, `--` ends the scan so a leading minus can be a number, and `-l` twice
    // is just the same flag.
    let (out, status, err) = run(&["-x"], b"");
    assert_eq!(out, "");
    assert_eq!((status, err.as_str()), (1, USAGE));
    let (minus, status, err) = run(&["-42"], b"");
    assert_eq!(minus, "");
    assert_eq!((status, err.as_str()), (1, USAGE));
    let (signed, status, _) = run(&["--", "-42"], b"");
    assert_eq!(status, 0);
    assert_eq!(signed, "minus\nforty-two.\n");
    let (twice, _, _) = run(&["-l", "-l", "6"], b"");
    assert_eq!(twice, "six\n");
}
