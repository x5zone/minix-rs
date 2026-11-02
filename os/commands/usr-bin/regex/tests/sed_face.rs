//! End-to-end face for the `sed` binary: a real process, real pipes and real
//! files, checked against `minix3/usr.bin/sed/main.c` (which operand gets
//! read, in what order, and what a failure costs) and
//! `minix3/usr.bin/sed/process.c` (the per-line print rules).
//!
//! This file exists because the crate's input legs had only one carrier: both
//! standard input and the file operands went through `minix_sys::{open, read}`,
//! which on the host is a shim that hands back nothing. Measured before the
//! fix, `printf 'abc\ndef\n' | sed 's/a/A/'` printed **nothing** and exited 0
//! while the host's own sed printed two lines — so every input claim in
//! `rewrite-notes/18-stage-commands/08-grep-sed.md` was unobservable from a
//! process test. The legs are now `cfg` twins (`read_stdin` / `read_file` in
//! `os/commands/usr-bin/regex/src/bin_support.rs`), which is what these tests
//! pin; the transformation itself keeps its own gates inside the library.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

const SED: &str = env!("CARGO_BIN_EXE_sed");

/// Runs the binary with `input` on standard input, and hands back the three
/// channels as they came out: standard output stays bytes because a filter's
/// output is data, not a message.
fn run(args: &[&str], input: &[u8]) -> (Vec<u8>, i32, Vec<u8>) {
    let mut child = Command::new(SED)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the sed binary should be runnable");
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

/// A scratch operand under the system temp directory, named after the running
/// test process so parallel tests cannot collide.
fn written(tag: &str, bytes: &[u8]) -> PathBuf {
    let mut path = std::env::temp_dir();
    path.push(format!("sed-face-{}-{tag}", std::process::id()));
    fs::write(&path, bytes).expect("writing the scratch operand");
    path
}

fn text(path: &Path) -> String {
    path.to_str()
        .expect("a scratch path the child can take")
        .to_owned()
}

#[test]
fn standard_input_reaches_the_pattern_space() {
    // No operand means the one file C adds is `NULL`, and `mf_fgets` opens it
    // as `stdin` (`main.c:214`, `:361-363`). Each line is substituted and
    // auto-printed (`process.c:270` `new: if (!nflag && !pd) OUT()`).
    let (out, status, err) = run(&["s/a/A/"], b"abc\ndef\n");
    assert_eq!(
        out,
        b"Abc\ndef\n".to_vec(),
        "stderr was {:?}",
        String::from_utf8_lossy(&err)
    );
    assert_eq!(status, 0);
    assert!(err.is_empty());
}

#[test]
fn a_named_operand_is_read_and_agrees_with_the_pipe() {
    // C reads a named operand through the same line loop, so the two shapes
    // must give byte-identical output; only the opening differs.
    let path = written("named", b"abc\ndef\n");
    let (from_file, file_status, file_err) = run(&["s/a/A/", &text(&path)], b"");
    let (from_pipe, pipe_status, _) = run(&["s/a/A/"], b"abc\ndef\n");
    assert_eq!(
        from_file,
        from_pipe,
        "stderr was {:?}",
        String::from_utf8_lossy(&file_err)
    );
    assert_eq!(file_status, pipe_status);
    assert_eq!(file_status, 0);
}

#[test]
fn every_operand_is_transformed_in_order() {
    // C chains the operands into one file list and walks it (`main.c:203-214`),
    // so output is the first file's lines then the second's.
    let first = written("first", b"one\ntwo\n");
    let second = written("second", b"three\n");
    let (out, status, err) = run(&["s/o/0/", &text(&first), &text(&second)], b"");
    assert_eq!(
        out,
        b"0ne\ntw0\nthree\n".to_vec(),
        "stderr was {:?}",
        String::from_utf8_lossy(&err)
    );
    assert_eq!(status, 0);
}

#[test]
fn quiet_prints_only_what_the_p_flag_prints() {
    // `-n` suppresses the auto-print, so only `s///p`'s own `OUT()` inside
    // `substitute()` (`process.c:466-467`) survives: the matching line once,
    // the non-matching line not at all.
    let (out, status, err) = run(&["-n", "s/a/A/p"], b"abc\ndef\n");
    assert_eq!(
        out,
        b"Abc\n".to_vec(),
        "stderr was {:?}",
        String::from_utf8_lossy(&err)
    );
    assert_eq!(status, 0);
}

#[test]
fn an_unopenable_operand_warns_and_flags_the_exit_status() {
    // `main.c:449-452`: `warn("%s", fname)` then `rval = 1` then `continue`, so
    // the run goes on with the remaining operands and `exit(rval)` reports it.
    // The sentence's exact shape (C's `warn` goes through `vwarnc`, which
    // prints `<program name>: <text>: <strerror>` — this shell prints the first
    // two segments with a hardwired name) is registered as `OPEN-WARN-SHAPE`,
    // so this pins only that the operand is named and that the run is neither
    // silent nor fatal.
    let good = written("after-bad", b"abc\n");
    let (out, status, err) = run(&["s/a/A/", "/no-such-operand-here", &text(&good)], b"");
    assert_eq!(
        out,
        b"Abc\n".to_vec(),
        "stderr was {:?}",
        String::from_utf8_lossy(&err)
    );
    assert_eq!(status, 1);
    assert!(
        String::from_utf8_lossy(&err).contains("no-such-operand-here"),
        "stderr was {:?}",
        String::from_utf8_lossy(&err)
    );
}

#[test]
fn a_bare_dash_stays_an_operand_name() {
    // NetBSD sed opens operands through `fopen`, with no `-` special case
    // (unlike `split`), so `-` is looked up as a file in the working directory
    // and never read as standard input: the piped bytes must not appear.
    let (out, status, _) = run(&["s/a/A/", "-"], b"abc\n");
    assert!(out.is_empty(), "standard input must not have been read");
    assert_eq!(status, 1, "the dash operand should have failed to open");
}

#[test]
fn a_chain_of_substitutions_composes_on_one_pattern_space() {
    // C walks the compiled command list per line (`process.c:113` `while (cp !=
    // NULL)`) and each `s` swaps its result into the same pattern space
    // (`process.c:460-463`), so the second command sees the first one's output:
    // `abc` becomes `Abc`, and then `B` replaces the `b` that is still there.
    let (out, status, err) = run(&["s/a/A/;s/b/B/"], b"abc\nxyz\n");
    assert_eq!(
        out,
        b"ABc\nxyz\n".to_vec(),
        "stderr was {:?}",
        String::from_utf8_lossy(&err)
    );
    assert_eq!(status, 0);
}

#[test]
fn a_separator_inside_a_pattern_is_pattern_text() {
    // The delimiter, not the separator, ends an `s`: `compile_subst` reads to
    // the next unescaped delimiter, so `;` inside the pattern or the replacement
    // is ordinary text (`compile.c:480-490`).
    let (out, status, err) = run(&["s/a;b/x/"], b"a;b\nq;w\n");
    assert_eq!(
        out,
        b"x\nq;w\n".to_vec(),
        "stderr was {:?}",
        String::from_utf8_lossy(&err)
    );
    assert_eq!(status, 0);
}

#[test]
fn the_p_flag_prints_again_on_top_of_the_automatic_print() {
    // Without `-n` a line printed by `s///p` is printed **twice**: once inside
    // `substitute()` (`process.c:466-467`, which checks neither `nflag` nor `pd`)
    // and once by the automatic print at `new:` (`process.c:269-270`). A
    // non-matching line gets the automatic print only, and the line the second
    // command did not touch still gets one.
    let (out, status, err) = run(&["s/a/A/p"], b"abc\nxyz\n");
    assert_eq!(
        out,
        b"Abc\nAbc\nxyz\n".to_vec(),
        "stderr was {:?}",
        String::from_utf8_lossy(&err)
    );
    assert_eq!(status, 0);
}

#[test]
fn delete_ends_the_cycle_and_suppresses_the_automatic_print() {
    // `case 'd': pd = 1; goto new;` (`process.c:142-144`) jumps out of the
    // command walk, so nothing after the `d` runs and `new:`'s `!pd` test fails.
    let (out, status, err) = run(&["s/a/A/;d"], b"abc\nxyz\n");
    assert_eq!(
        out,
        Vec::<u8>::new(),
        "stderr was {:?}",
        String::from_utf8_lossy(&err)
    );
    assert_eq!(status, 0);
    // A `d` reached only after `p` still leaves the one print `p` did.
    let (out, status, err) = run(&["p;d"], b"abc\nxyz\n");
    assert_eq!(
        out,
        b"abc\nxyz\n".to_vec(),
        "stderr was {:?}",
        String::from_utf8_lossy(&err)
    );
    assert_eq!(status, 0);
}

#[test]
fn a_print_command_emits_under_quiet_and_a_number_command_counts_across_operands() {
    // `case 'p'` prints whatever stands in the space (`process.c:191-194`), and
    // `case '='` prints the line number with no condition at all
    // (`process.c:264-265`). The counter is set once for the whole run
    // (`process.c:109` `for (linenum = 0; …)`) and C clears it only on the
    // in-place path (`main.c:441-443`), so it keeps counting across operands.
    let (out, status, err) = run(&["-n", "p"], b"abc\nxyz\n");
    assert_eq!(
        out,
        b"abc\nxyz\n".to_vec(),
        "stderr was {:?}",
        String::from_utf8_lossy(&err)
    );
    assert_eq!(status, 0);
    let first = written("number-one", b"one\ntwo\n");
    let second = written("number-two", b"three\n");
    let (out, status, err) = run(&["=", &text(&first), &text(&second)], b"");
    assert_eq!(
        out,
        b"1\none\n2\ntwo\n3\nthree\n".to_vec(),
        "stderr was {:?}",
        String::from_utf8_lossy(&err)
    );
    assert_eq!(status, 0);
}

#[test]
fn separators_and_comments_follow_the_compile_loop() {
    // `semicolon:` eats blanks, then a separator, then goes back for more of the
    // same, and a `#` starts a comment that costs the rest of **its** line
    // (`compile.c:177-184`). A line end separates commands the same way `;`
    // does, because C hands one line at a time to the compiler.
    let (out, status, err) = run(&[" p ; #x;d"], b"abc\n");
    assert_eq!(
        out,
        b"abc\nabc\n".to_vec(),
        "stderr was {:?}",
        String::from_utf8_lossy(&err)
    );
    assert_eq!(status, 0);
    let (out, status, err) = run(&["p\nd"], b"abc\nxyz\n");
    assert_eq!(
        out,
        b"abc\nxyz\n".to_vec(),
        "stderr was {:?}",
        String::from_utf8_lossy(&err)
    );
    assert_eq!(status, 0);
}

#[test]
fn a_number_line_at_the_script_start_switches_to_quiet() {
    // `main.c:286-287` tests the first two bytes of a script string, so `#n` is
    // both a comment and the quiet switch — and because the comment costs the
    // rest of its own line, the command has to sit on the next one. `#n;s/a/A/p`
    // on a single line runs nothing at all.
    let (out, status, err) = run(&["#n\ns/a/A/p"], b"abc\nxyz\n");
    assert_eq!(
        out,
        b"Abc\n".to_vec(),
        "stderr was {:?}",
        String::from_utf8_lossy(&err)
    );
    assert_eq!(status, 0);
    let (out, status, _) = run(&["#n;s/a/A/p"], b"abc\nxyz\n");
    assert_eq!(out, Vec::<u8>::new(), "the whole line is a comment");
    assert_eq!(status, 0);
}

#[test]
fn addresses_and_other_verbs_are_refused_loudly() {
    // This build carries `s`, `d`, `p` and `=` with no addresses; C compiles
    // addresses (`compile.c:193-207`) and the rest of the verb table
    // (`compile.c:102-132`), so those shapes must fail with a message rather than
    // quietly ignoring the part it cannot run.
    for script in ["1d", "$d", "/a/d", "q", "y/ab/BA/", "{d}"] {
        let (out, status, err) = run(&[script], b"abc\n");
        assert!(out.is_empty(), "{script:?} must not have run: {out:?}");
        assert_eq!(
            status,
            1,
            "{script:?} stderr was {:?}",
            String::from_utf8_lossy(&err)
        );
        assert!(
            String::from_utf8_lossy(&err).contains("not modelled"),
            "{script:?} stderr was {:?}",
            String::from_utf8_lossy(&err)
        );
    }
}

#[test]
fn an_empty_pattern_runs_the_previous_expression() {
    // `s//x/` does not compile an empty expression (this engine refuses one
    // outright at `pattern.rs:fn compile`); it names the last one the script
    // recorded — `process.c:668-671` keeps it in `defpreg`. On `aa` the first
    // command turns the leading `a` into `A`, and the reused `a` then rewrites
    // the one that is left.
    let (out, status, err) = run(&["s/a/A/;s//x/"], b"aa\n");
    assert_eq!(
        out,
        b"Ax\n".to_vec(),
        "stderr was {:?}",
        String::from_utf8_lossy(&err)
    );
    assert_eq!(status, 0);
}

#[test]
fn an_empty_pattern_takes_the_most_recent_expression() {
    // The reuse follows the script order: with `s/a/A/;s/b/B/;s//C/` the third
    // command runs `b`, not `a`, so the `b` that the second command left behind
    // is the one rewritten. If it reused the first expression the output would
    // stay `AB b`.
    let (out, status, err) = run(&["s/a/A/;s/b/B/;s//C/"], b"ab b\n");
    assert_eq!(
        out,
        b"AB C\n".to_vec(),
        "stderr was {:?}",
        String::from_utf8_lossy(&err)
    );
    assert_eq!(status, 0);
}

#[test]
fn an_empty_pattern_keeps_the_reused_expression_captures() {
    // The captures come from the **new** match, not from the earlier command:
    // `s/\(a\)/X\1/` leaves `Xa`, and the reused expression matches that `a`
    // again, so `\1` expands to `a` a second time.
    let (out, status, err) = run(&["s/\\(a\\)/X\\1/;s//Y\\1/"], b"a\n");
    assert_eq!(
        out,
        b"XYa\n".to_vec(),
        "stderr was {:?}",
        String::from_utf8_lossy(&err)
    );
    assert_eq!(status, 0);
}

#[test]
fn an_empty_pattern_with_nothing_recorded_stops_with_the_c_sentence() {
    // `errx(1, "first RE may not be empty")` (`process.c:670`) carries no line
    // number or file name, so the printed line is exactly what C writes: program
    // name, the sentence, newline — and it goes to standard error with exit 1.
    let (out, status, err) = run(&["s//B/"], b"aa\n");
    assert_eq!(out, Vec::<u8>::new(), "stdout must stay empty");
    assert_eq!(status, 1);
    assert_eq!(
        String::from_utf8_lossy(&err),
        "sed: first RE may not be empty\n"
    );
}

#[test]
fn a_compile_refusal_carries_the_line_and_the_script_name_as_c_prints_them() {
    // Every compile-time `errx` in `compile.c` opens with `"%lu: %s: "` — the
    // script line and `fname` — and `errx` itself prefixes `<program name>: `
    // (`minix3/lib/libc/gen/verrx.c:57`). These are the sentences, byte for byte:
    // `bad flag` (`compile.c:636`), `extra characters at the end of %c command`
    // naming the verb (`:264`), `invalid command code` (`:320`), `unterminated
    // substitute pattern` (`:331`), the newline-or-backslash delimiter (`:325`),
    // and the occurrence-number overflow (`:601`).
    let cases = [
        ("s/a/b/x", "sed: 1: \"s/a/b/x\": bad flag in substitute command: 'x'\n"),
        (
            "dx",
            "sed: 1: \"dx\": extra characters at the end of d command\n",
        ),
        ("Z", "sed: 1: \"Z\": invalid command code Z\n"),
        ("s/a/b", "sed: 1: \"s/a/b\": unterminated substitute pattern\n"),
        (
            "s\\a/b/",
            "sed: 1: \"s\\a/b/\": substitute pattern can not be delimited by newline or backslash\n",
        ),
        (
            "s/a/b/2147483648",
            "sed: 1: \"s/a/b/2147483648\": overflow in the 'N' substitute flag\n",
        ),
    ];
    for (script, expected) in cases {
        let (out, status, err) = run(&[script], b"");
        assert_eq!(out, Vec::<u8>::new(), "{script:?} must not have run");
        assert_eq!(status, 1, "{script:?} stderr was {err:?}");
        assert_eq!(
            String::from_utf8_lossy(&err),
            expected,
            "{script:?} should print C's own line"
        );
    }
}

#[test]
fn the_refusal_names_the_script_line_the_command_sits_on() {
    // `cu_fgets` raises C's `linenum` once per script line (`main.c:274`, `:292`,
    // `:306`), so a command on the third line is reported as line 3 — and the name
    // slot still carries the **whole** script, newlines included, since that is
    // what `fname` points at.
    let (out, status, err) = run(&["p\np\n d x"], b"");
    assert_eq!(out, Vec::<u8>::new());
    assert_eq!(status, 1);
    assert_eq!(
        String::from_utf8_lossy(&err),
        "sed: 3: \"p\np\n d x\": extra characters at the end of d command\n"
    );
}

#[test]
fn a_script_too_long_for_the_name_slot_is_cut_where_c_cuts_it() {
    // `string_ident` is 30 bytes (`main.c:242`): when `"<script>"` does not fit in
    // 29, `strcpy(string_ident + 24, " ...\"")` leaves the first 24 bytes and the
    // ellipsis (`main.c:261-266`).
    let script = "s/abcdefghijklmnopqrstuvwxyz0123456789/x";
    let (out, status, err) = run(&[script], b"");
    assert_eq!(out, Vec::<u8>::new());
    assert_eq!(status, 1);
    assert_eq!(
        String::from_utf8_lossy(&err),
        "sed: 1: \"s/abcdefghijklmnopqrstu ...\": unterminated substitute pattern\n"
    );
}

#[test]
fn a_blank_between_flags_still_belongs_to_the_flags() {
    // C's `compile_flags` eats blanks at the top of every iteration
    // (`compile.c:576`, labelled EXTENSION there), so `s/a/b/ g` is a global
    // substitution and `s/a/b/ g p` both. Treating the blank as the end of the
    // flags would refuse both shapes.
    let (out, status, err) = run(&["s/a/b/ g"], b"aaa\n");
    assert_eq!(
        out,
        b"bbb\n".to_vec(),
        "stderr was {:?}",
        String::from_utf8_lossy(&err)
    );
    assert_eq!(status, 0);
    let (out, status, _) = run(&["-n", "s/a/b/ p"], b"aab\n");
    // The padded `p` flag is set, and without a `g` only the first match goes.
    assert_eq!(out, b"bab\n".to_vec(), "the padded `p` flag still prints");
    assert_eq!(status, 0);
}
