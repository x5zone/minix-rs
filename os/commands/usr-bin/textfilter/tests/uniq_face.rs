//! End-to-end face for the `uniq` binary: real process, real standard input and
//! files, checked against expectations derived from `minix3/usr.bin/uniq/uniq.c`
//! (NetBSD) — not from the host's GNU `uniq`, which differs at two documented
//! sites (its `-c` prefix is `%7d`, and it rejects the old-style skip forms this
//! shell folds the way C's `obsolete()` does). Where the two agree, a second
//! test compares the outputs directly, so a regression in the shared part cannot
//! hide behind a stale golden value.

use std::fs;
use std::io::Write as _;
use std::path::PathBuf;
use std::process::{Command, Stdio};

const UNIQ: &str = env!("CARGO_BIN_EXE_uniq");

/// Runs the binary with the given words and standard input, returning
/// `(stdout, exit status, stderr)`.
fn run(args: &[&str], input: &str) -> (String, i32, String) {
    run_with(UNIQ, args, input)
}

/// The same run for any program name, so the differential leg can drive the host's
/// `uniq` through the identical pipe and comparison.
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

/// A path inside a private temporary directory, so the file-operand cases cannot
/// collide with another test or a previous run.
fn scratch(name: &str) -> PathBuf {
    let mut path = std::env::temp_dir();
    path.push(format!("uniq-face-{}-{}", std::process::id(), name));
    path
}

#[test]
fn plain_collapse_is_one_copy_per_run() {
    let (out, status, err) = run(&[], "a\na\nb\n");
    assert_eq!(out, "a\nb\n");
    assert_eq!(status, 0);
    assert_eq!(err, "");
}

#[test]
fn counted_prefix_is_four_columns_wide() {
    // `show()` prints `"%4d %s"`, so the count occupies four columns before the
    // single separating blank. GNU `uniq -c` uses seven columns; that difference
    // is why this file pins the C shape rather than the host's.
    assert_eq!(run(&["-c"], "a\na\nb\n").0, "   2 a\n   1 b\n");
    assert_eq!(run(&["-c"], "x\n".repeat(12).as_str()).0, "  12 x\n");
}

#[test]
fn duplicates_and_uniques_flags_are_independent_bits() {
    // C keeps `cflag`/`dflag`/`uflag` as three separate ints and composes them in
    // one guard: `if ((dflag && repeats == 0) || (uflag && repeats > 0)) return;`
    assert_eq!(run(&["-d"], "a\na\nb\n").0, "a\n");
    assert_eq!(run(&["-u"], "a\na\nb\n").0, "b\n");
    assert_eq!(run(&["-c", "-d"], "a\na\nb\n").0, "   2 a\n");
    assert_eq!(run(&["-c", "-u"], "a\na\nb\n").0, "   1 b\n");
    // `-d -u` suppresses every run, and nothing is an exit status of zero here —
    // `uniq.c` returns 0 whatever it printed.
    let (out, status, err) = run(&["-d", "-u"], "a\na\nb\n");
    assert_eq!(out, "");
    assert_eq!(status, 0);
    assert_eq!(err, "");
}

#[test]
fn skipped_key_compares_whole_line_prints() {
    // `skip()` walks `numfields` blank-delimited fields, then `numchars` bytes;
    // the printed line is always the whole line (`show(ofp, prevline)`).
    let three = "foo SAME\nbar SAME\nbaz OTHER\n";
    assert_eq!(run(&["-f", "1"], three).0, "foo SAME\nbaz OTHER\n");
    assert_eq!(run(&["-s", "4"], three).0, "foo SAME\nbaz OTHER\n");
    // `-f 2` walks past the second field, which is the last one on every line, so
    // each key is the bare newline: all three lines share it and one run is
    // reported, as the first full line (`show(ofp, prevline)`).
    assert_eq!(run(&["-f", "2"], three).0, "foo SAME\n");
}

#[test]
fn obsolete_forms_fold_into_the_skip_options() {
    // `obsolete()` rewrites `-<digit>…` to `-f<digit>…` and `+<digit>…` to
    // `-s<digit>…`; GNU `uniq` does not take these forms.
    let three = "foo SAME\nbar SAME\nbaz OTHER\n";
    assert_eq!(run(&["-1"], three).0, run(&["-f", "1"], three).0);
    assert_eq!(run(&["+4"], three).0, run(&["-s", "4"], three).0);
}

#[test]
fn unknown_option_and_operand_count_are_usage_errors() {
    // Minix3's `uniq` has no `-i` (its getopt string is `"-cdf:s:u"`); a third
    // operand is past the `case 2:`/`default: usage()` switch.
    for args in [&["-i"][..], &["a", "b", "c"][..], &["-f", "x"][..]] {
        let (out, status, err) = run(args, "a\n");
        assert_eq!(out, "", "args {args:?} must print nothing on stdout");
        assert_eq!(status, 1, "args {args:?} must exit 1");
        assert!(err.contains("usage:"), "args {args:?} stderr was {err:?}");
    }
}

#[test]
fn file_operand_and_output_file_match_stdout_only_run() {
    let input = scratch("in");
    let output = scratch("out");
    fs::write(&input, "a\na\nb\n").unwrap();
    fs::write(&output, "stale content longer than the report\n").unwrap();
    let input_text = input.to_str().unwrap().to_owned();
    let output_text = output.to_str().unwrap().to_owned();

    let (out, status, err) = run(&[&input_text, &output_text], "");
    assert_eq!(out, "", "the report belongs in the output file");
    assert_eq!(status, 0, "stderr was {err:?}");
    assert_eq!(fs::read_to_string(&output).unwrap(), "a\nb\n");

    for path in [&input, &output] {
        let _ = fs::remove_file(path);
    }
}

#[test]
fn same_file_as_input_and_output_reports_nothing() {
    // C opens the input, then truncates the output, and only then reads — so the
    // stream it compares is already empty and the run prints nothing (exit 0
    // because `main` returns 0). Reading the input first would echo the old bytes
    // back, which is the mistake this case keeps out.
    let file = scratch("same");
    fs::write(&file, "a\na\nb\n").unwrap();
    let name = file.to_str().unwrap().to_owned();
    let (out, status, err) = run(&[&name, &name], "");
    assert_eq!(out, "");
    assert_eq!(status, 0, "stderr was {err:?}");
    assert_eq!(fs::read(&file).unwrap(), b"");
    let _ = fs::remove_file(&file);
}

#[test]
fn unopenable_input_leaves_no_output_file() {
    // `case 2:` opens the input first; `file()` then dies through
    // `err(1, "%s", name)` before the output is created.
    let missing = scratch("missing");
    let output = scratch("not-created");
    let _ = fs::remove_file(&output);
    let missing_text = missing.to_str().unwrap().to_owned();
    let output_text = output.to_str().unwrap().to_owned();
    let (out, status, err) = run(&[&missing_text, &output_text], "");
    assert_eq!(out, "");
    assert_eq!(status, 1);
    assert!(err.contains("uniq:"), "stderr was {err:?}");
    assert!(
        !output.exists(),
        "the output file must not have been created"
    );
}

#[test]
fn agrees_with_the_host_uniq_where_the_two_specs_coincide() {
    // Only the forms whose NetBSD and GNU semantics match: plain collapsing, the
    // `-d`/`-u` selections and the field/character skip. The `-c` prefix and the
    // obsolete forms are excluded on purpose — GNU prints `%7d` there and rejects
    // `+N`, so comparing them would measure the difference between the two
    // programs rather than a regression in this one.
    let inputs = [
        ("a\na\nb\n", vec![vec![], vec!["-d"], vec!["-u"]]),
        (
            "foo SAME\nbar SAME\nbaz OTHER\n",
            vec![vec!["-f", "1"], vec!["-s", "4"]],
        ),
    ];
    let system = if Command::new("uniq").arg("--version").output().is_ok() {
        "uniq"
    } else {
        eprintln!("no host uniq to compare against; skipping the differential leg");
        return;
    };
    for (input, option_sets) in inputs {
        for options in option_sets {
            let ours = run(&options, input);
            let theirs = run_with(system, &options, input);
            assert_eq!(
                ours.0, theirs.0,
                "options {options:?} on {input:?}: ours {:?}, host {:?}",
                ours.0, theirs.0
            );
            assert_eq!(ours.1, theirs.1, "options {options:?} on {input:?}");
        }
    }
}
