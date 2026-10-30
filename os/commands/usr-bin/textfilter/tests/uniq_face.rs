//! End-to-end face for the `uniq` binary: real process, real standard input and
//! files, checked against expectations derived from `minix3/usr.bin/uniq/uniq.c`
//! (NetBSD) — not from the host's GNU `uniq`, which differs at two documented
//! sites (its `-c` prefix is `%7d`, and it rejects the old-style skip forms this
//! shell folds the way C's `obsolete()` does). Where the two agree, a second
//! test compares the outputs directly, so a regression in the shared part cannot
//! hide behind a stale golden value.

mod face_support;

use std::fs;
use std::path::PathBuf;
use std::process::{Command, Stdio};

const UNIQ: &str = env!("CARGO_BIN_EXE_uniq");

/// Runs the binary with the given words and standard input, returning
/// `(stdout, exit status, stderr)`.
fn run(args: &[&str], input: &str) -> (String, i32, String) {
    run_with(UNIQ, args, input)
}

/// Same seam as `run` but both streams stay **bytes**: `from_utf8_lossy` renders a bare
/// `c3` as the three-byte replacement character, so a text form cannot tell "one byte" from
/// "two bytes" apart.
fn run_bytes(args: &[&str], input: &str) -> (Vec<u8>, i32, Vec<u8>) {
    let mut child = Command::new(UNIQ)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the uniq binary should be runnable");
    face_support::write_stdin(&mut child, input.as_bytes());
    let output = child.wait_with_output().expect("the run should finish");
    (
        output.stdout,
        output.status.code().unwrap_or(-1),
        output.stderr,
    )
}

/// The letter position of the libc line is **one raw byte**: the reference build
/// (`uniq.c` + Minix's `getopt.c` + the progname/`util.h` shim set,
/// started through the `uniq` symlink so both binaries carry the same `getprogname()`) prints
/// `uniq: unknown option -- ` + `c3` + newline for `-\u{e9}`, `-c\u{e9}` and `-u\u{e9}`
/// (`c`/`d`/`u` are C's valueless letters in `"-cdf:s:u"`, so clusters keep scanning into the
/// first byte of `\u{e9}`), then the single usage line, exit 1. `-Z\u{e9}` stops at `Z`.
/// ASCII refusals are already pinned by the neighbouring gate, so this one carries only the
/// byte-width fact and the cluster-stop shape.
#[test]
fn a_multibyte_flag_letter_is_emitted_as_a_single_byte_by_uniq() {
    const USAGE: &str = "usage: uniq [-cdu] [-f fields] [-s chars] [input_file [output_file]]\n";
    let want: Vec<u8> = [
        b"uniq: unknown option -- ".as_slice(),
        b"\xc3\n",
        USAGE.as_bytes(),
    ]
    .concat();
    for form in ["-\u{e9}", "-c\u{e9}", "-u\u{e9}"] {
        let (out, status, err) = run_bytes(&[form], "a\n");
        assert_eq!(out, Vec::new(), "{form} 的报文只走标准错误");
        assert_eq!(status, 1, "{form} 的 stderr 是 {err:?}");
        assert_eq!(err, want, "{form} 的字母该是一个裸字节 c3");
    }

    let (_out, status, err) = run_bytes(&["-Z\u{e9}"], "a\n");
    assert_eq!(status, 1);
    assert_eq!(
        err,
        [b"uniq: unknown option -- Z\n".as_slice(), USAGE.as_bytes(),].concat(),
        "环停在第一个坏字母上，第二个字节根本不该被读到"
    );
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
    face_support::write_stdin(&mut child, input.as_bytes());
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
fn only_the_operand_count_takes_a_bare_usage() {
    // A third operand goes through `uniq.c:124`'s `default: usage()`: the option
    // ring has long finished, so there is **no** libc line in front of it.
    const USAGE: &str = "usage: uniq [-cdu] [-f fields] [-s chars] [input_file [output_file]]\n";
    for args in [&["a", "b", "c"][..], &["-c", "a", "b", "c"][..]] {
        let (out, status, err) = run(args, "a\n");
        assert_eq!(out, "", "args {args:?} must print nothing on stdout");
        assert_eq!(status, 1, "args {args:?} must exit 1");
        assert_eq!(err, USAGE, "args {args:?}");
    }
}

#[test]
fn unknown_option_and_missing_value_take_two_lines() {
    // Minix3's `uniq` has no `-i` letter (the option string is `"-cdf:s:u"`). Two lines:
    // the `getopt.c:101-104` line (the letter is unquoted, unlike this host's glibc) plus
    // usage.
    const USAGE: &str = "usage: uniq [-cdu] [-f fields] [-s chars] [input_file [output_file]]\n";
    let (out, status, err) = run(&["-i"], "a\n");
    assert_eq!(out, "");
    assert_eq!(status, 1);
    assert_eq!(err, format!("uniq: unknown option -- i\n{USAGE}"));
    // A missing value behaves the same (`getopt.c:132-136`): `-f` and `-s` each report
    // their own letter.
    for letter in ['f', 's'] {
        let word = format!("-{letter}");
        let (out, status, err) = run(&[&word], "a\n");
        assert_eq!(out, "", "{word} 不该打任何东西");
        assert_eq!(status, 1);
        assert_eq!(
            err,
            format!("uniq: option requires an argument -- {letter}\n{USAGE}"),
            "`-{letter}` 缺值那一支"
        );
    }
    // The valueless flag `-c` is itself a legal option and must not go to usage (the
    // dividing line from the two branches above).
    assert_eq!(
        run(&["-c"], "a\na\n"),
        (String::from("   2 a\n"), 0, String::new())
    );
}

#[test]
fn the_two_skip_values_get_their_own_sentences_with_the_bad_value_echoed() {
    // C here is `errx(1, "illegal field skip value: %s", optarg)` and its `-s`
    // sibling (`uniq.c:90-98`): their own sentences, carrying the bad value,
    // **without usage**.
    for (args, message) in [
        (&["-f", "x"][..], "uniq: illegal field skip value: x\n"),
        (&["-s", "x"][..], "uniq: illegal character skip value: x\n"),
        (&["-f-1"][..], "uniq: illegal field skip value: -1\n"),
        (
            &["-s", "5x"][..],
            "uniq: illegal character skip value: 5x\n",
        ),
    ] {
        let (out, status, err) = run(args, "a\n");
        assert_eq!(out, "", "{args:?} 不该打任何东西");
        assert_eq!(status, 1, "{args:?} 的 stderr 是 {err:?}");
        assert_eq!(err, message, "{args:?}");
    }
    // `-f -1` differs: that `-1` is first folded by `obsolete()` into `-f1`, so what
    // comes back is `-f1` (the host reference reads the same).
    let (out, status, err) = run(&["-f", "-1"], "a\n");
    assert_eq!(out, "");
    assert_eq!(status, 1);
    assert_eq!(err, "uniq: illegal field skip value: -f1\n");
}

#[test]
fn an_empty_or_padded_skip_value_is_a_count_not_an_error() {
    // C's check has only two arms: a converted negative number, or a leftover in `*p`
    // (`uniq.c:91`, `:97`). When `strtol` converts not a single digit the value is 0
    // and `*p` stays at the start of the digits, so `-f ''` is a legal zero (skip 0
    // fields, equivalent to omitting `-f`), and leading blanks are skipped.
    let three = "foo SAME\nbar SAME\nbaz OTHER\n";
    let plain = run(&["-f", "0"], three).0;
    assert_eq!(plain, run(&["-f", ""], three).0, "空值就是零");
    assert_eq!(plain, run(&["-f", " 0"], three).0, "前导空白跳掉");
    assert_eq!(run(&["-f+0"], three).0, plain, "附值形可带一个正号");
    assert_ne!(plain, run(&["-f", "1"], three).0, "零与一真的不一样");
    // A separate `+0` never reaches `strtol`: C's `obsolete()` first folds it into
    // `-s0`, so the value `-f` receives is `-s0`, which converts no digits and so
    // reports errx (the host reference reads the same:
    // `uniq_ref: illegal field skip value: -s0`).
    let (out, status, err) = run(&["-f", "+0"], three);
    assert_eq!(out, "");
    assert_eq!(status, 1);
    assert_eq!(err, "uniq: illegal field skip value: -s0\n");
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
    // The three components of `err(1, "%s", name)` (program name, operand,
    // `strerror` sentence), and it is fatal (exit 1).
    assert_eq!(
        err,
        format!("uniq: {missing_text}: No such file or directory\n")
    );
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

/// Run a leg with standard output connected to an already-open sink (`/dev/full` is the
/// one that matters: writing it always answers `ENOSPC`).
fn run_into_sink(args: &[&str], input: &str, sink_path: &str) -> (i32, String) {
    let sink = fs::OpenOptions::new()
        .write(true)
        .open(sink_path)
        .expect("/dev/full carries a failing write");
    let mut child = Command::new(UNIQ)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::from(sink))
        .stderr(Stdio::piped())
        .spawn()
        .expect("the uniq binary should be runnable");
    face_support::write_stdin(&mut child, input.as_bytes());
    let output = child.wait_with_output().expect("the run should finish");
    (
        output.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

/// Write leg, shape one (standard output takes no bytes): C's output leg never checks
/// for failure — `file(argv[1], "w")` only calls `err(1, "%s", name)` when **opening**
/// fails (`minix3/usr.bin/uniq/uniq.c:216-221`), and after that the return value of
/// `fprintf` is never examined; the whole file has not a single `ferror`/`fflush`. The
/// reference reports not a word and **exits 0** on this shape. An earlier shell silently
/// exited 1 through `support::emit`: staying silent is right, the exit code was not.
#[test]
fn a_stdout_that_stops_taking_bytes_stays_silent_and_exits_zero() {
    let (status, err) = run_into_sink(&[], "a\na\nb\n", "/dev/full");
    assert_eq!(
        (status, err.as_str()),
        (0, ""),
        "C 查不到写失手，本壳也不该因此改退码或添句子"
    );
}

/// Write leg, shape two (the output operand takes no bytes): on the host
/// `File::create("/dev/full")` succeeds at this layer (truncating a character device is
/// a no-op) and failure lands only in the write and the flush — exactly the two places
/// C does not look at. An earlier shell printed a two-part `uniq: <path>` and exited 1:
/// the sentence is not C's sentence (C's two-part form appears only in `errx`), and the
/// exit code is not C's either.
#[test]
fn an_output_operand_that_cannot_be_written_stays_silent_and_exits_zero() {
    let input = scratch("sink-in");
    fs::write(&input, "a\na\nb\n").unwrap();
    let name = input.to_str().unwrap().to_owned();
    let (status, err) = run_into_sink(&[&name, "/dev/full"], "", "/dev/full");
    assert_eq!(
        (status, err.as_str()),
        (0, ""),
        "写失手在 C 那侧不可观察，本壳两形都要一致地静默退 0"
    );
    let _ = fs::remove_file(&input);
}

/// `uniq` does not add a newline to a final line that lacks one, and it prints the first
/// line of a run — that line's termination state travels with it. C's `uniq` uses
/// `fgetln` to get the line including its newline, strips the newline for comparison, and
/// hands it back as is for printing (`minix3/usr.bin/uniq/uniq.c`); the reference build
/// measured `x\nx\ny` (no trailing newline) returning `x\ny`: the two `x` lines merge
/// into one run and the first is printed with its newline, while `y` has none and ends
/// as is.
#[test]
fn uniq_does_not_supply_a_newline_the_input_lacked() {
    let (out, code, err) = run(&[], "x\nx\ny");
    assert_eq!((out.as_str(), code, err.as_str()), ("x\ny", 0, ""));
}
