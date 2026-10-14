//! End-to-end face for the `ppt` binary: real process, real pipes, checked against
//! `minix3/games/ppt/ppt.c` (`main` `:66-127`, `putppt` `:130-145`, `getppt` `:147-170`).
//!
//! The goldens are the C's own shapes. Encoding frames the tape with `EDGE`
//! (`"___________"`, `:52`) via `puts`, so the edge arrives with its own newline, and each
//! byte becomes one line: `|`, bits 7 down to 0 with `o` for a hole and a blank for none,
//! a `.` emitted **before** bit 2's column, then `|` and a newline (`:130-145`). Decoding
//! walks `fgets` lines, skips a leading line with no feed hole as preamble, and stops at
//! the first such line once the tape has synced, owing a newline unless the last byte
//! already was one (`:98-113`). `-h` is not help: it shares the `case '?'` arm and leaves
//! with the usage line on standard error and status 1 (`:58-63`, `:80-83`).

use std::io::Write as _;
use std::process::{Command, Stdio};

const PPT: &str = env!("CARGO_BIN_EXE_ppt");

fn run(args: &[&str], input: &[u8]) -> (String, i32, String) {
    let mut child = Command::new(PPT)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the ppt binary should be runnable");
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

const EDGE: &str = "___________";
/// `putppt('A')`: 0x41 punches bit 6 and bit 0, the feed hole sits before bit 2.
const PUNCHED_A: &str = "| o   .  o|";

#[test]
fn a_failed_read_leaves_with_c_fgets_sentence_and_exits_one() {
    // C 的解码路在读到尽头时补一个换行后安静离开，读**出错**才走 `err(1, "fgets")`
    // （`ppt.c:109-111`，判据是 `!feof(stdin)`）。两句在进程级分开：目录描述符读出 `EISDIR`
    // 得 `ppt: fgets: Is a directory` 并退 1，且不补那个换行。
    let (out, status, err) = run_with_directory_stdin(&["-d"]);
    assert_eq!(out, "");
    assert_eq!(status, 1);
    assert_eq!(err, "ppt: fgets: Is a directory\n");
    // 对照：空输入只是读到尽头，退 0 且不出声。
    let (_, clean_status, clean_err) = run(&["-d"], b"");
    assert_eq!((clean_status, clean_err.as_str()), (0, ""));
}

/// Runs the binary with standard input set to an **unreadable** descriptor: opening a
/// directory succeeds but reading it fails with `EISDIR`, which is exactly the case C
/// separates from end of input (`factor.c:157-161`, `ppt.c:109-111`).
fn run_with_directory_stdin(args: &[&str]) -> (String, i32, String) {
    let dir = std::fs::File::open(std::env::temp_dir())
        .expect("the temporary directory should open read-only");
    let output = Command::new(PPT)
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
fn the_encoder_frames_the_tape_and_punches_one_line_per_byte() {
    // `puts(EDGE)`, one line per byte, `puts(EDGE)` again (`ppt.c:106-119`).
    let (out, status, err) = run(&["A"], b"");
    assert_eq!(status, 0, "stderr was {err:?}");
    assert_eq!(out, format!("{EDGE}\n{PUNCHED_A}\n{EDGE}\n"));
    // Standard input is consumed a byte at a time with no line grouping (`:116-118`),
    // so the trailing newline of the input is itself punched.
    let (piped, _, _) = run(&[], b"A\n");
    assert_eq!(
        piped.lines().next(),
        Some(EDGE),
        "the tape still opens with the edge rule: {piped:?}"
    );
    assert_eq!(piped.lines().count(), 4, "edge, A, the newline byte, edge");
    assert!(piped.ends_with(&format!("{EDGE}\n")[..]));
}

#[test]
fn a_punched_blank_separates_operands_and_none_follows_the_last() {
    // `if ((*(argv))) putppt(' ')` (`ppt.c:112-114`) tests the *next* operand, so two
    // operands get one blank line between them and the tape ends without a trailing blank.
    let (two, _, _) = run(&["A", "A"], b"");
    assert_eq!(
        two,
        format!("{EDGE}\n{PUNCHED_A}\n|  o  .   |\n{PUNCHED_A}\n{EDGE}\n"),
        "0x20 punches bit 5 only"
    );
    let (one, _, _) = run(&["A"], b"");
    assert_eq!(
        one.lines().count(),
        3,
        "no separating blank for one operand"
    );
}

#[test]
fn the_decoder_reads_back_what_the_encoder_punched_and_stops_at_lost_sync() {
    let tape = run(&["A"], b"").0;
    // The edge lines carry no feed hole: they are preamble before the first good line, and
    // the trailing one ends the run with the newline it owes (`ppt.c:98-113`).
    let (out, status, err) = run(&["-d"], tape.as_bytes());
    assert_eq!(status, 0, "stderr was {err:?}");
    assert_eq!(out, "A\n");
    // A broken line in the middle ends everything after it: `start` is already set, so C
    // prints the owed newline and exits at once (`:100-105`).
    let mut broken = String::new();
    broken.push_str(tape.strip_suffix(&format!("{EDGE}\n")[..]).unwrap());
    broken.push_str("no feed hole here\n");
    broken.push_str(&run(&["A"], b"").0);
    let (out, status, _) = run(&["-d"], broken.as_bytes());
    assert_eq!(
        (out.as_str(), status),
        ("A\n", 0),
        "nothing after the break prints"
    );
    // Bytes that already end in a newline owe nothing more (`neednl = (c != '\n')`).
    let newline_tape = run(&[], b"\n").0;
    let (out, _, _) = run(&["-d"], newline_tape.as_bytes());
    assert_eq!(out, "\n", "the decoded newline is the whole output");
}

#[test]
fn decode_refuses_operands_and_h_is_not_help() {
    // `if (argc > 0) usage();` on the decode path (`ppt.c:95-96`), and `-h` shares the
    // error arm (`:80-83`); both leave status 1 with the line on standard error.
    let (out, status, err) = run(&["-d", "A"], b"");
    assert_eq!(
        (out.as_str(), status, err.as_str()),
        ("", 1, "usage: ppt [-d] [string ...]\n")
    );
    let (out, status, err) = run(&["-h"], b"");
    assert_eq!(
        (out.as_str(), status, err.as_str()),
        ("", 1, "usage: ppt [-d] [string ...]\n")
    );
    let (out, status, err) = run(&["-x"], b"");
    assert_eq!(
        (out.as_str(), status, err.as_str()),
        ("", 1, "usage: ppt [-d] [string ...]\n")
    );
}

#[test]
fn double_dash_ends_the_options_and_a_lone_dash_is_an_operand() {
    // BSD `getopt` consumes `--` and treats `-` as a non-option (`ppt.c:74-89`), so both
    // forms reach the puncher as text rather than as flags.
    let (after, status, err) = run(&["--", "-d"], b"");
    assert_eq!(status, 0, "stderr was {err:?}");
    assert_eq!(
        after.lines().next(),
        Some(EDGE),
        "punched, not decoded: {after:?}"
    );
    assert_eq!(after.lines().count(), 4, "edge, '-', 'd', edge");
    let (dash, status, err) = run(&["-"], b"");
    assert_eq!(status, 0, "stderr was {err:?}");
    assert_eq!(
        dash,
        format!("{EDGE}\n|  o o.o o|\n{EDGE}\n"),
        "0x2d punches bits 5, 3, 2 and 0"
    );
}
