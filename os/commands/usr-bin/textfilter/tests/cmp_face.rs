//! End-to-end face for the `cmp` binary: real process, real pipes and real
//! files, checked against the report strings and exit statuses in
//! `minix3/usr.bin/cmp/` (`cmp.c`, `misc.c`, `regular.c`).
//!
//! The host's GNU `cmp` is used only to compare exit statuses and whether a
//! channel stayed empty. Its wording is not compared: GNU prints
//! `"… differ: byte N, line M"` where NetBSD prints `"char"`, and its
//! `-l` columns differ too, so a text match there would measure the two
//! programs rather than this one.

mod face_support;

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

const CMP: &str = env!("CARGO_BIN_EXE_cmp");

/// Runs `cmp` with the given words and standard input.
fn run(args: &[&str], input: &str) -> (String, i32, String) {
    run_with(CMP, args, input)
}

/// The same run for any program, so the differential leg drives the host `cmp`
/// through the identical pipe.
fn run_with(program: &str, args: &[&str], input: &str) -> (String, i32, String) {
    let mut child = Command::new(program)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap_or_else(|_| panic!("{program} should be runnable"));
    face_support::write_stdin(&mut child, input.as_bytes());
    let output = child.wait_with_output().expect("the run should finish");
    (
        String::from_utf8_lossy(&output.stdout).into_owned(),
        output.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

/// A scratch file holding `bytes`, named after `tag` so parallel tests cannot
/// collide. Returns its path.
fn written(tag: &str, bytes: &[u8]) -> PathBuf {
    let mut path = std::env::temp_dir();
    path.push(format!("cmp-face-{}-{tag}", std::process::id()));
    fs::write(&path, bytes).expect("writing the scratch file");
    path
}

fn text(path: &Path) -> String {
    path.to_str().unwrap().to_owned()
}

#[test]
fn identical_files_are_silent_and_zero() {
    let one = written("same-a", b"abc\n");
    let two = written("same-b", b"abc\n");
    let (out, status, err) = run(&[&text(&one), &text(&two)], "");
    assert_eq!(out, "");
    assert_eq!(err, "");
    assert_eq!(status, 0);
}

#[test]
fn first_difference_names_the_char_and_the_line() {
    // `misc.c` `diffmsg`: `"%s %s differ: char %lld, line %lld\n"` on standard
    // output, then `exit(DIFF_EXIT)` — status 1.
    let one = written("diff-a", b"abc\n");
    let two = written("diff-b", b"abd\n");
    let (out, status, err) = run(&[&text(&one), &text(&two)], "");
    assert_eq!(
        out,
        format!("{} {} differ: char 3, line 1\n", text(&one), text(&two))
    );
    assert_eq!(err, "");
    assert_eq!(status, 1);

    // The line counter advances on every newline passed, so a difference in the
    // second line reports `line 2` (`regular.c` bumps `line` after each `'\n'`).
    let one = written("lines-a", b"a1\nb2\n");
    let two = written("lines-b", b"a1\nb3\n");
    let (out, _, _) = run(&[&text(&one), &text(&two)], "");
    assert!(out.ends_with("differ: char 5, line 2\n"), "out was {out:?}");
}

#[test]
fn list_mode_prints_one_line_per_differing_byte_in_octal() {
    // `regular.c`: `printf("%6lld %3o %3o\n", byte, ch, *p2)`, and `-l` keeps
    // going to the end of the shorter stream instead of exiting at the first hit.
    let one = written("list-a", b"abc\nxy\n");
    let two = written("list-b", b"abd\nxz\n");
    let (out, status, err) = run(&["-l", &text(&one), &text(&two)], "");
    assert_eq!(err, "");
    assert_eq!(status, 1, "list mode still reports a difference");
    assert_eq!(
        out,
        format!(
            "{:>6} {:3o} {:3o}\n{:>6} {:3o} {:3o}\n",
            3, b'c', b'd', 6, b'y', b'z'
        )
    );
}

#[test]
fn silent_mode_writes_nothing_but_keeps_the_status() {
    // `diffmsg` guards its print with `if (!sflag)` and still exits `DIFF_EXIT`.
    let one = written("silent-a", b"abc\n");
    let two = written("silent-b", b"abd\n");
    let (out, status, err) = run(&["-s", &text(&one), &text(&two)], "");
    assert_eq!(out, "");
    assert_eq!(err, "");
    assert_eq!(status, 1);
    let same = written("silent-c", b"abc\n");
    assert_eq!(run(&["-s", &text(&one), &text(&same)], "").1, 0);

    // A silent run of two *files* of different sizes reports "different" even
    // when the skipped windows would match: `c_regular` opens with
    // `if (sflag && len1 != len2) exit(1)`, comparing the unadjusted sizes
    // before any skip is applied. The same shapes without `-s` still report by
    // content, and a standard-input operand goes through `c_special`, which has
    // no such test.
    let longer = written("silent-d", b"XXabc\n");
    let four = text(&one);
    let six = text(&longer);
    // Four bytes against six, but the second file loses its two-byte prefix, so
    // the windows are identical: `abc\n` either way.
    assert_eq!(run(&["-s", &four, &six, "0", "2"], "").1, 1);
    let (out, status, _) = run(&[&four, &six, "0", "2"], "");
    assert_eq!(out, "");
    assert_eq!(status, 0);
    // Same content once skipped, but reached through a standard-input operand:
    // `cmp.c` marks that run `special` and calls `c_special`, which has no size
    // test at all, so the answer stays "equal".
    assert_eq!(run(&["-s", "-", &six, "0", "2"], "abc\n").1, 0);
}

#[test]
fn a_shorter_stream_reports_eof_on_the_shorter_name() {
    // `regular.c` ends with `eofmsg(len1 > len2 ? file2 : file1, byte, line)` —
    // the file that ran out is the one named, on standard error, status 1.
    let longer = written("short-a", b"abc\n");
    let shorter = written("short-b", b"ab");
    let (out, status, err) = run(&[&text(&longer), &text(&shorter)], "");
    assert_eq!(out, "");
    assert_eq!(status, 1);
    assert_eq!(err, format!("cmp: EOF on {}\n", text(&shorter)));
}

#[test]
fn skip_operands_take_any_base_and_ignore_a_trailing_tail() {
    // `strtoq(argv[2], &ep, 0)` with only two failure tests: nothing converted,
    // or overflow. `0x2` is two bytes, `02` is octal two, `2x` converts the `2`
    // and stops, and leading blanks are skipped — all five are the same skip.
    let one = written("skip-a", b"abc\n");
    let two = written("skip-b", b"abd\n");
    for skip in ["2", "0x2", "02", "2x", " 2"] {
        let (out, status, err) = run(&[&text(&one), &text(&two), skip, "0"], "");
        assert_eq!(status, 1, "skip {skip:?} stderr was {err:?}");
        // Both streams lose the same two bytes, so the report shifts its offset
        // back by two while keeping the same shape.
        assert!(
            out.contains("differ: char 1, line 1\n"),
            "skip {skip:?} gave {out:?}"
        );
    }
    // Without the skip the same pair differs at byte three.
    assert!(
        run(&[&text(&one), &text(&two)], "")
            .0
            .contains("char 3, line 1\n")
    );
}

#[test]
fn a_skip_past_the_end_is_an_eof_report() {
    // `c_regular`: `if (skip1 > len1) eofmsg(file1, len1 + 1, 0)` — with `line`
    // zero the `-l` form drops the line and keeps `: char <len + 1>`.
    let one = written("past-a", b"abc\n");
    let two = written("past-b", b"abc\n");
    let (out, status, err) = run(&[&text(&one), &text(&two), "9"], "");
    assert_eq!(out, "");
    assert_eq!(status, 1);
    assert_eq!(err, format!("cmp: EOF on {}\n", text(&one)));
    let (out, _, err) = run(&["-l", &text(&one), &text(&two), "9"], "");
    assert_eq!(out, "");
    assert_eq!(err, format!("cmp: EOF on {}: char 5\n", text(&one)));
    // Exactly at the end is not past it: the window is empty and equal.
    assert_eq!(run(&[&text(&one), &text(&two), "4", "4"], "").1, 0);
}

#[test]
fn malformed_invocations_exit_two() {
    // `ERR_EXIT` covers `usage()` and the `lflag && sflag` guard in `cmp.c`; the
    // messages go to standard error while stdout stays empty.
    let one = text(&written("bad-a", b"a"));
    let two = text(&written("bad-b", b"a"));
    let cases: Vec<(Vec<&str>, &str)> = vec![
        (vec!["-x", &one, &two], "usage:"),
        (
            vec!["-l", "-s", &one, &two],
            "only one of -l and -s may be specified",
        ),
        (vec![&one], "usage:"),
        (vec![&one, &two, "zz"], "usage:"),
        (vec![&one, &two, "1", "2", "3"], "usage:"),
    ];
    for (words, message) in cases {
        let (out, status, err) = run(&words, "");
        assert_eq!(out, "", "args {words:?} printed {out:?}");
        assert_eq!(status, 2, "args {words:?} stderr was {err:?}");
        assert!(err.contains(message), "args {words:?} stderr was {err:?}");
    }

    // Both operands are opened before the skip values are parsed (`cmp.c:95-119`
    // then `:121-134`), so an unopenable file is what the user hears about even
    // when a skip value is also malformed — that is the open failure's message,
    // not `usage:`.
    let missing =
        text(&std::env::temp_dir().join(format!("cmp-face-{}-missing", std::process::id())));
    let (out, status, err) = run(&[&missing, &two, "zz"], "");
    assert_eq!(out, "");
    assert_eq!(status, 2, "stderr was {err:?}");
    assert!(err.contains(&missing), "stderr was {err:?}");
    assert!(
        !err.contains("usage:"),
        "the open failure must win over the malformed skip: {err:?}"
    );
}

#[test]
fn a_dash_operand_compares_standard_input_and_is_named_in_the_report() {
    // `cmp.c:95-113`: `-` becomes fd 0 and the label `"stdin"`.
    let two = written("stdin-b", b"abd\n");
    let (out, status, _) = run(&["-", &text(&two)], "abc\n");
    assert_eq!(status, 1);
    assert_eq!(
        out,
        format!("stdin {} differ: char 3, line 1\n", text(&two))
    );
    // Both operands naming standard input is rejected before anything opens.
    let (out, status, err) = run(&["-", "-"], "abc\n");
    assert_eq!(out, "");
    assert_eq!(status, 2);
    assert!(
        err.contains("standard input may only be specified once"),
        "stderr was {err:?}"
    );
}

#[test]
fn agrees_with_the_host_cmp_on_status_and_emptiness() {
    // Exit statuses and the "this channel stayed empty" facts are shared between
    // NetBSD and GNU; the report wording is not, so it is not compared here.
    let cases: Vec<(Vec<&str>, &[u8], &[u8])> = vec![
        (vec![], b"abc\n", b"abc\n"),
        (vec![], b"abc\n", b"abd\n"),
        (vec!["-s"], b"abc\n", b"abd\n"),
        (vec!["-l"], b"abc\n", b"abd\n"),
        (vec![], b"abc\n", b"ab"),
        (vec!["-s"], b"abc\n", b"abc\n"),
    ];
    let system = if Command::new("cmp").arg("--version").output().is_ok() {
        "cmp"
    } else {
        eprintln!("no host cmp to compare against; skipping the differential leg");
        return;
    };
    for (number, (options, left, right)) in cases.into_iter().enumerate() {
        let one = written(&format!("delta-a-{number}"), left);
        let two = written(&format!("delta-b-{number}"), right);
        let words: Vec<String> = options
            .iter()
            .map(|word| (*word).to_owned())
            .chain([text(&one), text(&two)])
            .collect();
        let word_refs: Vec<&str> = words.iter().map(String::as_str).collect();
        let ours = run(&word_refs, "");
        let theirs = run_with(system, &word_refs, "");
        assert_eq!(ours.1, theirs.1, "options {word_refs:?}");
        assert_eq!(
            ours.0.is_empty(),
            theirs.0.is_empty(),
            "options {word_refs:?}: ours {:?}, host {:?}",
            ours.0,
            theirs.0
        );
    }
}
