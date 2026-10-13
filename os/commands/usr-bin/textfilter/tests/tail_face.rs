//! End-to-end face for the `tail` binary: real process, real pipes and real
//! files, checked against expectations derived from `minix3/usr.bin/tail/`
//! (`tail.c`, `forward.c`, `reverse.c`).
//!
//! The host's GNU `tail` is used only where the two specifications agree. It is
//! deliberately not compared on: `-b` (GNU has no block mode), `-r` (GNU has no
//! reverse), and the line shapes on input whose last line is unterminated — this
//! shell rebuilds each printed line with its newline (a declared simplification
//! shared with the reverse path), while C and GNU copy bytes.

use std::fs;
use std::io::Write as _;
use std::path::PathBuf;
use std::process::{Command, Stdio};

const TAIL: &str = env!("CARGO_BIN_EXE_tail");

/// Runs `tail` with the given words and standard input.
fn run(args: &[&str], input: &str) -> (String, i32, String) {
    run_with(TAIL, args, input)
}

/// The same run for any program, so the differential leg drives the host `tail`
/// through the identical pipe and comparison.
fn run_with(program: &str, args: &[&str], input: &str) -> (String, i32, String) {
    let mut child = Command::new(program)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap_or_else(|_| panic!("{program} should be runnable"));
    child
        .stdin
        .as_mut()
        .expect("stdin piped")
        .write_all(input.as_bytes())
        .expect("writing standard input");
    drop(child.stdin.take());
    let output = child.wait_with_output().expect("the run should finish");
    (
        String::from_utf8_lossy(&output.stdout).into_owned(),
        output.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

fn scratch(name: &str) -> PathBuf {
    let mut path = std::env::temp_dir();
    path.push(format!("tail-face-{}-{}", std::process::id(), name));
    path
}

/// Twelve newline-terminated lines, so the default ten-line window is visible.
fn twelve() -> String {
    (1..=12).map(|number| format!("{number}\n")).collect()
}

#[test]
fn default_is_the_last_ten_lines() {
    // `tail.c:160-162`: no number option and no `-r` is `RLINES` with `off = 10`.
    let (out, status, err) = run(&[], &twelve());
    assert_eq!(out, "3\n4\n5\n6\n7\n8\n9\n10\n11\n12\n");
    assert_eq!(status, 0, "stderr was {err:?}");
}

#[test]
fn line_byte_and_block_counts_take_the_trailing_window() {
    assert_eq!(run(&["-n", "2"], &twelve()).0, "11\n12\n");
    // Obsolete leading-digit form is folded to `-n` by `obsolete()`.
    assert_eq!(run(&["-2"], &twelve()).0, "11\n12\n");
    assert_eq!(run(&["-c", "3"], "abcdef").0, "def");
    // `-b N` counts 512-byte blocks, so one block of a short file is all of it.
    assert_eq!(run(&["-b", "1"], "abcdef").0, "abcdef");
    assert_eq!(run(&["-c", "100"], "abcdef").0, "abcdef");
}

#[test]
fn forward_offsets_are_one_based_and_skip_nothing_at_one() {
    // `ARG`'s forward arm: `+1` leaves `off` at zero, which prints the whole file.
    assert_eq!(run(&["-n", "+1"], &twelve()).0, twelve());
    assert_eq!(run(&["-n", "+10"], &twelve()).0, "10\n11\n12\n");
    assert_eq!(run(&["-c", "+4"], "abcdef").0, "def");
    assert_eq!(run(&["-b", "+2"], "abcdef").0, "");
    // An offset past the end prints nothing, and that is a successful exit.
    let (out, status, err) = run(&["-n", "+99"], &twelve());
    assert_eq!(out, "");
    assert_eq!(status, 0, "stderr was {err:?}");
}

#[test]
fn zero_counts_select_nothing_and_exit_zero() {
    // C does not reject them: `forward.c` seeks to the end for `RLINES`/`RBYTES`
    // when `off` is zero, and a pipe is simply drained.
    for args in [&["-n", "0"][..], &["-c", "0"][..], &["-b", "0"][..]] {
        let (out, status, err) = run(args, &twelve());
        assert_eq!(out, "", "args {args:?} printed {out:?}");
        assert_eq!(status, 0, "args {args:?} stderr was {err:?}");
    }
}

#[test]
fn reverse_forms_walk_the_input_bottom_up() {
    // GNU `tail` has no `-r`, so these are pinned to C alone (`reverse.c`).
    assert_eq!(run(&["-r"], "a\nb\n").0, "b\na\n");
    assert_eq!(run(&["-r", "-n", "1"], "a\nb\n").0, "b\n");
    // `tail -r -c 3` on the four bytes `abc\n` is `r_reg` with `off = 3`: it clamps
    // the walk to three bytes and starts at the last one, so the newline is the
    // “special last char”, then `b` and `c` are walked backwards and the leftover
    // fragment `bc\n` is written by the closing `if (llen) WR(p, llen)`. The `a`
    // is outside the byte window and never appears.
    assert_eq!(run(&["-r", "-c", "3"], "abc\n").0, "bc\n");
    // Folding `+N` under `-r` yields a backward count (`tail.c:146-149`): the `+3`
    // becomes `RLINES` with `off = 2`, so two lines come back bottom-up, while
    // `+1` leaves `off` at zero and `reverse.c:79-80` returns without printing.
    assert_eq!(run(&["-r", "-n", "+1"], "a\nb\n").0, "");
    assert_eq!(run(&["-r", "-n", "+3"], "a\nb\n").0, "b\na\n");
}

#[test]
fn follow_mode_and_unknown_letters_are_refused_not_leaked() {
    for (args, message) in [
        (&["-f"][..], "not modelled"),
        (&["-x"][..], "usage:"),
        (&["-n", "x"][..], "illegal offset -- x"),
    ] {
        let (out, status, err) = run(args, "a\n");
        assert_eq!(out, "", "args {args:?} printed {out:?}");
        assert_eq!(status, 1, "args {args:?} stderr was {err:?}");
        assert!(err.contains(message), "args {args:?} stderr was {err:?}");
    }
}

#[test]
fn line_window_cap_rejects_the_count_without_a_diagnostic() {
    // A declared divergence from C, pinned here so removing it is a deliberate
    // edit: the line window keeps at most 32 borrowed lines (`window.rs`), while
    // C `forward()` + `RLINES` walks back through `mmap` for any count. The
    // rejection is currently silent — a nonzero exit with no message — which is
    // the second half of what the follow-up round owes.
    let (out, status, err) = run(&["-n", "40"], &twelve());
    assert_eq!(out, "");
    assert_eq!(status, 1);
    assert_eq!(err, "", "the cap rejection is silent today");
    // The same count in the reverse shape is not the window's business (C walks
    // the file with `mmap` there), so it prints every line bottom-up.
    assert_eq!(
        run(&["-r", "-n", "40"], &twelve()).0,
        "12\n11\n10\n9\n8\n7\n6\n5\n4\n3\n2\n1\n"
    );
}

#[test]
fn several_operands_get_a_banner_and_a_blank_line_between() {
    // `tail.c:172-177` prints `"%s==> %s <==\n"` with the separator only after the
    // first file, and this NetBSD `tail` has no `-q`/`-v` to change it.
    let one = scratch("many-a");
    let two = scratch("many-b");
    fs::write(&one, "a1\na2\n").unwrap();
    fs::write(&two, "b1\nb2\n").unwrap();
    let one_text = one.to_str().unwrap().to_owned();
    let two_text = two.to_str().unwrap().to_owned();
    let (out, status, err) = run(&[&one_text, &two_text], "");
    assert_eq!(status, 0, "stderr was {err:?}");
    assert_eq!(
        out,
        format!("==> {one_text} <==\na1\na2\n\n==> {two_text} <==\nb1\nb2\n")
    );

    // A single operand gets no banner at all.
    let (out, _, _) = run(&[&one_text], "");
    assert_eq!(out, "a1\na2\n");

    for path in [&one, &two] {
        let _ = fs::remove_file(path);
    }
}

#[test]
fn an_unopenable_operand_warns_and_the_others_still_print() {
    // `fopen` failure is `ierr(); continue;` — non-fatal, exit status becomes 1,
    // and no banner is printed for the file that could not be opened.
    let good = scratch("partial-a");
    let missing = scratch("partial-missing");
    fs::write(&good, "g1\n").unwrap();
    let good_text = good.to_str().unwrap().to_owned();
    let missing_text = missing.to_str().unwrap().to_owned();
    let (out, status, err) = run(&[&missing_text, &good_text], "");
    assert_eq!(status, 1);
    assert!(err.contains("tail:"), "stderr was {err:?}");
    assert_eq!(out, format!("==> {good_text} <==\ng1\n"));
    let _ = fs::remove_file(&good);
}

#[test]
fn agrees_with_the_host_tail_on_the_shared_forms() {
    // Only the shapes whose NetBSD and GNU semantics match: a line count, a
    // forward line offset, a byte count, and the multi-file banner. Byte counts
    // and the offsets use newline-terminated input so the shared "copy the bytes"
    // behaviour is what is compared.
    let cases: Vec<(Vec<&str>, String)> = vec![
        (vec![], twelve()),
        (vec!["-n", "3"], twelve()),
        (vec!["-n", "+9"], twelve()),
        (vec!["-c", "4"], "abcdef\n".to_string()),
    ];
    let system = if Command::new("tail").arg("--version").output().is_ok() {
        "tail"
    } else {
        eprintln!("no host tail to compare against; skipping the differential leg");
        return;
    };
    for (options, input) in cases {
        let ours = run(&options, &input);
        let theirs = run_with(system, &options, &input);
        assert_eq!(
            ours.0, theirs.0,
            "options {options:?}: ours {:?}, host {:?}",
            ours.0, theirs.0
        );
        assert_eq!(ours.1, theirs.1, "options {options:?}");
    }
}
