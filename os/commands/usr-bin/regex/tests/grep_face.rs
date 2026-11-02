//! End-to-end face for the `grep` binary, covering only the **input seam**
//! that this crate shares with `sed`: which bytes reach the matcher, from
//! where, and what an unopenable operand costs. The selection rules
//! themselves (`-v`, `-c`, `-l`, name prefixes) keep their gates inside
//! `os/commands/usr-bin/regex/src/grep.rs`.
//!
//! Before the seam had two carriers, `minix_sys::open` was the only one and it
//! fails on the host, so every operand took the `file_err` arm and a normal run
//! could not be observed from a process test at all.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

const GREP: &str = env!("CARGO_BIN_EXE_grep");

fn run(args: &[&str], input: &[u8]) -> (Vec<u8>, i32, Vec<u8>) {
    let mut child = Command::new(GREP)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the grep binary should be runnable");
    {
        let mut pipe = child.stdin.take().expect("a standard input pipe");
        use std::io::Write as _;
        pipe.write_all(input)
            .expect("feeding the input should work");
    }
    let output = child.wait_with_output().expect("the run should finish");
    (
        output.stdout,
        output.status.code().unwrap_or(-1),
        output.stderr,
    )
}

fn written(tag: &str, bytes: &[u8]) -> PathBuf {
    let mut path = std::env::temp_dir();
    path.push(format!("grep-face-{}-{tag}", std::process::id()));
    fs::write(&path, bytes).expect("writing the scratch operand");
    path
}

fn text(path: &Path) -> String {
    path.to_str()
        .expect("a scratch path the child can take")
        .to_owned()
}

#[test]
fn a_named_operand_is_read() {
    // One operand prints the matching line with no name prefix, so the only
    // way to see the bytes is that they arrived at all.
    let path = written("named", b"abc\ndef\n");
    let (out, status, err) = run(&["a", &text(&path)], b"");
    assert_eq!(
        out,
        b"abc\n".to_vec(),
        "stderr was {:?}",
        String::from_utf8_lossy(&err)
    );
    assert_eq!(status, 0);
}

#[test]
fn standard_input_and_a_named_operand_agree() {
    // `grep.c` walks the same per-file routine for standard input as for a
    // named operand, so the two shapes must produce the same bytes.
    let path = written("agree", b"abc\ndef\n");
    let (from_file, file_status, file_err) = run(&["a", &text(&path)], b"");
    let (from_pipe, pipe_status, _) = run(&["a"], b"abc\ndef\n");
    assert_eq!(
        from_file,
        from_pipe,
        "stderr was {:?}",
        String::from_utf8_lossy(&file_err)
    );
    assert_eq!(file_status, pipe_status);
}

#[test]
fn several_operands_carry_their_name_prefixes() {
    // The prefix decision reads the **operand count** before anything is
    // opened (`grep.c:493-494`: `hflag` is set only for `argc == 0 || argc == 1`),
    // so with two operands each match is prefixed by its file name — observable
    // only once the operands really open.
    let first = written("first", b"abc\ndef\n");
    let second = written("second", b"xa\nyb\n");
    let (out, status, err) = run(&["a", &text(&first), &text(&second)], b"");
    assert_eq!(
        out,
        format!("{}:abc\n{}:xa\n", text(&first), text(&second)).into_bytes(),
        "stderr was {:?}",
        String::from_utf8_lossy(&err)
    );
    assert_eq!(status, 0);
}

#[test]
fn an_unopenable_operand_only_shows_up_in_the_exit_status() {
    // `procfile` warns and sets `file_err`, the run continues with the rest, and
    // the trouble reaches the exit status as 2
    // (`grep.c:505`: `exit(c ? (file_err ? (qflag ? 0 : 2) : 0) : (file_err ? 2 : 1))`).
    // The sentence is C's three-segment `warn` form, pinned by
    // `an_unopenable_operand_warns_with_the_three_segments_c_prints`.
    let good = written("after-bad", b"abc\n");
    let (out, status, err) = run(&["a", "/no-such-operand-here", &text(&good)], b"");
    assert_eq!(
        out,
        format!("{}:abc\n", text(&good)).into_bytes(),
        "stderr was {:?}",
        String::from_utf8_lossy(&err)
    );
    assert_eq!(status, 2);
    assert!(String::from_utf8_lossy(&err).contains("no-such-operand-here"));
}

#[test]
fn an_unescaped_parenthesis_in_a_basic_pattern_is_text() {
    // The basic dialect has no group operator, so `(a)` searches for the four
    // bytes `(a)` and a lone `)` searches for itself
    // (`regcomp.c:628-690`'s `default: ordinary`, and `:570-597`'s body stop is
    // the two-byte pair `\)`). The engine used to refuse `(a)` outright.
    let path = written("paren", b"(a)\nab\nx)\n(a)b\n");
    let name = text(&path);
    let (out, status, err) = run(&["-G", "(a)", &name], b"");
    assert_eq!(
        out,
        b"(a)\n(a)b\n".to_vec(),
        "stderr was {:?}",
        String::from_utf8_lossy(&err)
    );
    assert_eq!(status, 0);
    let (out, status, _) = run(&["-G", "x)", &name], b"");
    assert_eq!(out, b"x)\n".to_vec(), "a lone closer is text as well");
    assert_eq!(status, 0);
    // The escaped pair is the group, and it still groups.
    let (out, status, _) = run(&["-G", "y\\(a\\)b", &name], b"");
    assert_eq!(out, Vec::<u8>::new(), "no line holds `yab`");
    assert_eq!(status, 1);
}

#[test]
fn the_extended_dialect_groups_and_a_stray_closer_is_still_text() {
    // `regcomp.c:417-427` makes `(` open a group whose body ends at the bare `)`,
    // while a `)` with no group open is ordinary text because Minix compiles the
    // library with `-DPOSIX_MISTAKE` (`Makefile.inc:7`, removing the error arm at
    // `regcomp.c:428-438`).
    let path = written("ere-paren", b"abc\na(b)c\nx)\n");
    let name = text(&path);
    let (out, status, err) = run(&["-E", "a(b)c", &name], b"");
    assert_eq!(
        out,
        b"abc\n".to_vec(),
        "stderr was {:?}",
        String::from_utf8_lossy(&err)
    );
    assert_eq!(status, 0);
    let (out, status, _) = run(&["-E", "x)", &name], b"");
    assert_eq!(out, b"x)\n".to_vec(), "a stray closer must not be an error");
    assert_eq!(status, 0);
}

#[test]
fn an_unopenable_operand_warns_with_the_three_segments_c_prints() {
    // `util.c:111` is `warn("%s", fn)`, and `vwarnc` renders three segments —
    // program name, the operand, then the `strerror` sentence
    // (`minix3/lib/libc/gen/vwarnc.c:3-8`). The run still only sets the exit
    // status (`grep.c:505` gives 2 once anything was selected or the trouble
    // stands alone), which the gate above pins.
    let (out, status, err) = run(&["a", "/no-such-operand-here"], b"");
    assert_eq!(out, Vec::<u8>::new());
    assert_eq!(status, 2);
    assert_eq!(
        String::from_utf8_lossy(&err),
        "grep: /no-such-operand-here: No such file or directory\n"
    );
}

#[test]
fn the_program_name_slot_follows_argv0() {
    use std::os::unix::process::CommandExt;
    // C's `getprogname()` is the last component of `argv[0]`, so the name in
    // every diagnostic — this one and the `-i` refusal — moves with the
    // invocation instead of coming from a literal in the source.
    let mut child = std::process::Command::new(GREP)
        .arg0("gg")
        .arg("a")
        .arg("/no-such-operand-here")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the grep binary should be runnable");
    child.stdin.take();
    let output = child.wait_with_output().expect("the run should finish");
    assert_eq!(output.stdout, Vec::<u8>::new());
    assert_eq!(output.status.code(), Some(2));
    assert_eq!(
        String::from_utf8_lossy(&output.stderr),
        "gg: /no-such-operand-here: No such file or directory\n"
    );
}

#[test]
fn an_interval_repeats_the_atom_it_follows_in_both_dialects() {
    // `{m,n}` in the extended spelling, `\{m,n\}` in the basic one
    // (`regcomp.c:512-529`, `:705-721`). The two dialects agree because both go
    // through the same repetition code; the shapes below also match the host's
    // own grep byte for byte.
    let path = written("interval", b"aab\nab\nb\n[xx]\n");
    let name = text(&path);
    for (flag, script, expected) in [
        ("-E", "a{2}b", "aab\n"),
        ("-G", "a\\{2\\}b", "aab\n"),
        ("-E", "a{1,2}b", "aab\nab\n"),
        ("-G", "a\\{1,2\\}b", "aab\nab\n"),
        ("-E", "a{2,}b", "aab\n"),
        ("-G", "a\\{2,\\}b", "aab\n"),
        // A zero count drops the atom, leaving the rest of the pattern.
        ("-E", "a{0}b", "aab\nab\nb\n"),
    ] {
        let (out, status, err) = run(&[flag, script, &name], b"");
        assert_eq!(
            out,
            expected.as_bytes().to_vec(),
            "{flag} {script:?} stderr was {:?}",
            String::from_utf8_lossy(&err)
        );
        assert_eq!(status, 0, "{flag} {script:?} should have matched");
    }
}

#[test]
fn a_malformed_interval_is_refused_and_a_brace_that_starts_none_is_text() {
    // Reversed bounds die in C's parser too (`REQUIRE(count <= count2,
    // REG_BADBR)`), while a brace that cannot start an interval is ordinary text
    // (`regcomp.c:473`) and keeps matching literally.
    let path = written("interval-shape", b"a{x}\na{2,1}b\nzz\n");
    let name = text(&path);
    let (out, status, err) = run(&["-E", "a{2,1}b", &name], b"");
    assert_eq!(
        out,
        Vec::<u8>::new(),
        "a diagnostic must not ride on stdout"
    );
    assert_eq!(
        String::from_utf8_lossy(&err),
        "grep: bad pattern or option\n",
        "C's grep puts this line on standard error through `errx` (`grep.c:484`)"
    );
    assert_eq!(status, 2, "a refused pattern must not exit 0");
    let (out, status, _) = run(&["-E", "a{x}", &name], b"");
    assert_eq!(out, b"a{x}\n".to_vec(), "a non-digit brace stays text");
    assert_eq!(status, 0);
    // A group interval needs a copy of the subexpression's code with the same
    // capture slots, which this compiler does not have; it refuses rather than
    // quietly matching `ab{2}`.
    let (out, status, err) = run(&["-E", "(ab){2}", &name], b"");
    assert_eq!(
        out,
        Vec::<u8>::new(),
        "the unsupported shape adds no output"
    );
    assert_eq!(
        String::from_utf8_lossy(&err),
        "grep: bad pattern or option\n",
        "refusing out loud is the point; silently matching `ab{{2}}` would not be"
    );
    assert_eq!(status, 2);
}

#[test]
fn every_diagnostic_reaches_standard_error_and_leaves_standard_output_clean() {
    // C's grep has no diagnostic that prints to standard output: `usage()` is
    // `fprintf(stderr, ...)` (`grep.c:111-118`), the pattern failure is
    // `errx(2, "%s", re_error)` (`:484`), and an unreadable operand goes through
    // `warn` (`util.c:111`) — `errx`/`warn` both prefix `<program name>: ` on
    // standard error (`verrx.c:57`, `vwarnc.c:3`). A selected match is the only
    // thing that may reach standard output, which is why these runs must leave it
    // empty: a diagnostic mixed into the data stream is indistinguishable from a
    // matching line for the next pipe stage.
    let path = written("streams", b"abc\n");
    let name = text(&path);
    let cases = [
        (
            vec!["-i", "a", &name],
            "grep: -i waits for engine case-folding (see 08 sec 4.4)\n",
        ),
        (
            vec!["-E", "a{2,1}b", &name],
            "grep: bad pattern or option\n",
        ),
        (vec!["-Z", "a", &name], "grep: bad pattern or option\n"),
    ];
    for (args, expected) in cases {
        let (out, status, err) = run(&args, b"");
        assert_eq!(out, Vec::<u8>::new(), "{args:?} wrote to standard output");
        assert_eq!(
            String::from_utf8_lossy(&err),
            expected,
            "{args:?} stderr differed"
        );
        assert_eq!(status, 2, "{args:?} exit status");
    }
    // The three-segment operand warning shares the same rule, tested above.
    let (out, _, _) = run(&["a", "/no-such-operand-here"], b"");
    assert_eq!(out, Vec::<u8>::new());
}
