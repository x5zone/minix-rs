//! End-to-end face for the `unvis` binary: real process, real pipes and real
//! files, checked against `minix3/usr.bin/unvis/unvis.c` and the decoder in
//! `minix3/lib/libc/gen/unvis.c`.
//!
//! There is no differential leg: a Linux host has no `unvis`, and the byte-exact
//! expectations below are what the C source says. Output is compared as bytes
//! because the decoder works on bytes — that is the point of the second test.

use std::fs;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

const UNVIS: &str = env!("CARGO_BIN_EXE_unvis");

fn run(args: &[&str], input: &[u8]) -> (Vec<u8>, i32, String) {
    let mut child = Command::new(UNVIS)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the unvis binary should be runnable");
    child
        .stdin
        .as_mut()
        .expect("stdin piped")
        .write_all(input)
        .expect("writing standard input");
    drop(child.stdin.take());
    let output = child.wait_with_output().expect("the run should finish");
    (
        output.stdout,
        output.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

fn written(tag: &str, bytes: &[u8]) -> PathBuf {
    let mut path = std::env::temp_dir();
    path.push(format!("unvis-face-{}-{tag}", std::process::id()));
    fs::write(&path, bytes).expect("writing the scratch file");
    path
}

fn text(path: &Path) -> String {
    path.to_str().unwrap().to_owned()
}

#[test]
fn default_decodes_the_vis_grammar() {
    // `\101` is octal, `\M^?` is the meta-and-caret pair for 0xFF, `\^I` is a tab:
    // the three forms the encoder's default mode produces (`lib/libc/gen/unvis.c`
    // states S_START, S_OCTAL2, S_META and S_CTRL).
    let (out, status, err) = run(&[], b"\\101\\M^?\\^I");
    assert_eq!(out, vec![0x41, 0xFF, 0x09], "stderr was {err:?}");
    assert_eq!(status, 0);
    assert_eq!(err, "");
    // The hidden forms emit nothing at all: `\` + newline is `vis -f`'s break and
    // `\$` is `vis -l`'s marker (`unvis.c:307-315`).
    assert_eq!(run(&[], b"a\\\nb\\$").0, b"ab".to_vec());
    // A doubled backslash is one backslash.
    assert_eq!(run(&[], b"\\\\").0, b"\\".to_vec());
    // `\x` is hexadecimal and needs no flag at all (`lib/libc/gen/unvis.c:305-307`),
    // and `xtod` folds the case, so both spellings of A land on 0x41.
    assert_eq!(run(&[], b"\\x41\\x4b").0, vec![0x41, 0x4B]);
}

#[test]
fn bytes_above_the_eighth_bit_survive_untouched() {
    // The machine runs on bytes, not on text: an input that is not valid UTF-8 is
    // still decoded, and a bare 0xFF passes through the ground state unchanged
    // (`unvis.c:226-246`, `usr.bin/unvis/unvis.c:113-116`).
    let (out, status, err) = run(&[], b"a\xffb");
    assert_eq!(out, vec![b'a', 0xFF, b'b'], "stderr was {err:?}");
    assert_eq!(status, 0);
    assert_eq!(err, "");
    // Same through a file operand, so this is not a stdin-only accident.
    let one = written("high-byte", b"x\xff\x00y");
    assert_eq!(run(&[&text(&one)], b"").0, vec![b'x', 0xFF, 0x00, b'y']);
    let _ = fs::remove_file(one);
}

#[test]
fn percent_and_equals_are_literal_until_the_mode_asks_for_them() {
    // The two introducers are gated by flags in the ground state
    // (`lib/libc/gen/unvis.c:232-243`), so the default mode reads them as data.
    assert_eq!(run(&[], b"%41").0, b"%41".to_vec());
    assert_eq!(run(&[], b"=41").0, b"=41".to_vec());
    // `-h` (VIS_HTTP1808) folds either hex case; `-m` (VIS_MIMESTYLE) requires
    // uppercase, because the C's `XTOD` has no case fold and the digit test rejects
    // lowercase outright (`unvis.c:79-80,404,413`).
    assert_eq!(run(&["-h"], b"%41%4b").0, vec![0x41, 0x4B]);
    assert_eq!(run(&["-m"], b"=4A=41").0, vec![0x4A, 0x41]);
    // A `%` or `=` with nothing decodable after it is not dropped silently: C emits
    // the pending zero and reprocesses the byte (`UNVIS_VALIDPUSH`, `:371-386`),
    // which is why `a%z` reads as `a`, NUL, `z`.
    assert_eq!(run(&["-h"], b"a%z").0, vec![b'a', 0x00, b'z']);
}

#[test]
fn noescape_takes_the_backslash_away_as_an_introducer() {
    // `-e` is `VIS_NOESCAPE` (`usr.bin/unvis/unvis.c:61-62`), and the guard on it
    // sits in the ground state only (`lib/libc/gen/unvis.c:228`): every byte is
    // data, so an encoded stream round-trips through nothing.
    assert_eq!(run(&["-e"], b"\\101").0, b"\\101".to_vec());
    assert_eq!(run(&["-e"], b"\\M^?").0, b"\\M^?".to_vec());
    // Without it the same input decodes.
    assert_eq!(run(&[], b"\\101").0, b"A".to_vec());
}

#[test]
fn mime_soft_line_break_disappears() {
    // An `=` before a line end is quoted-printable's continuation: `S_MIME1` moves
    // to `S_EATCRNL` without emitting, and a run of line ends after it is eaten too
    // (`lib/libc/gen/unvis.c:400-433`).
    assert_eq!(run(&["-m"], b"ab=\ncd").0, b"abcd".to_vec());
    assert_eq!(run(&["-m"], b"ab=\r\n\ncd").0, b"abcd".to_vec());
    // A non-line-end byte after the `=` is not a hex digit either, and the byte that
    // broke the sequence is consumed with it: `unvis(1)` warns, zeroes the state and
    // reads the *next* byte, so the `x` never reaches the output.
    let (out, status, err) = run(&["-m"], b"ab=x");
    assert_eq!(out, b"ab".to_vec());
    assert_eq!(status, 0, "stderr was {err:?}");
    assert_eq!(err, "unvis: <stdin>: offset: 4: can't decode\n");
    // The same digits in a legitimate uppercase hex pair do decode.
    assert_eq!(run(&["-m"], b"ab=41").0, b"abA".to_vec());
}

#[test]
fn an_undecodable_byte_warns_with_its_offset_and_the_run_continues() {
    // `UNVIS_SYNBAD` is a warning, not a fatal error: C prints
    // `<name>: offset: N: can't decode`, resets the state and carries on
    // (`usr.bin/unvis/unvis.c:126-128`). `\Mb` is bad because `S_META` accepts only
    // `-` or `^`; the offending byte is consumed, not reprocessed, so `b` is lost
    // with it and only `a` survives.
    let (out, status, err) = run(&[], b"a\\Mb");
    assert_eq!(out, b"a".to_vec());
    assert_eq!(status, 0, "stderr was {err:?}");
    assert_eq!(err, "unvis: <stdin>: offset: 4: can't decode\n");
    // Through a file operand the name is the operand's own (`:91-99`).
    let one = written("bad-sequence", b"\\Mz");
    let (out, status, err) = run(&[&text(&one)], b"");
    assert_eq!(out, Vec::<u8>::new());
    assert_eq!(status, 0, "stderr was {err:?}");
    assert!(
        err.contains(": offset: 3: can't decode"),
        "stderr was {err:?}"
    );
    assert!(err.contains("bad-sequence"), "stderr was {err:?}");
    let _ = fs::remove_file(one);
}

#[test]
fn an_unopenable_operand_warns_without_changing_the_exit_status() {
    // `unvis.c:91-104` warns and moves on, and the program returns `EXIT_SUCCESS`
    // whatever happens — the opposite of `vis`, which sets `rval = 1`.
    let one = written("operand-a", b"\\101");
    let missing = format!("unvis-face-{}-missing", std::process::id());
    let (out, status, err) = run(&[&text(&one), &missing], b"");
    assert_eq!(out, b"A".to_vec(), "stderr was {err:?}");
    assert_eq!(
        status, 0,
        "a missing operand must not fail the run: {err:?}"
    );
    assert!(err.contains(&missing), "stderr was {err:?}");
    // Operands share one output stream, in argument order.
    let two = written("operand-b", b"\\M^?");
    assert_eq!(run(&[&text(&two), &text(&one)], b"").0, vec![0xFF, 0x41]);
    for path in [&one, &two] {
        let _ = fs::remove_file(path);
    }
}

#[test]
fn flags_cluster_and_the_mix_is_refused_by_name() {
    // `getopt("eHhm")` clusters letters, and `--` ends option scanning so that what
    // follows is an operand (`usr.bin/unvis/unvis.c:57-79`).
    assert_eq!(
        run(&["-em"], b"\\101=41").0,
        run(&["-e", "-m"], b"\\101=41").0,
        "-em must equal -e -m"
    );
    // ... and what they agree on is the backslash left literal by `-e` while the
    // `=41` still decodes under `-m`.
    assert_eq!(
        run(&["-em"], b"\\101=41").0,
        vec![0x5C, b'1', b'0', b'1', 0x41]
    );
    // The `--` case above is an operand that cannot be opened, and that only warns:
    // `unvis` returns success whatever the operands do (`:91-104`).
    let (out, status, err) = run(&["--", "-h"], b"");
    assert_eq!(out, Vec::<u8>::new());
    assert_eq!(status, 0, "stderr was {err:?}");
    assert!(err.contains("unvis: -h"), "stderr was {err:?}");
    // `-m` cannot be mixed with `-h` and/or `-H` (`:80-89`), the check sits after the
    // whole option loop, and it outranks an unreadable operand.
    for flags in [
        vec!["-m", "-h"],
        vec!["-hm"],
        vec!["-m", "-h", "missing-operand"],
    ] {
        let (out, status, err) = run(&flags, b"a");
        assert_eq!(out, Vec::<u8>::new(), "{flags:?} printed {out:?}");
        assert_eq!(status, 1, "{flags:?} stderr was {err:?}");
        assert!(
            err.contains("Can't mix -m with -h and/or -H"),
            "{flags:?} stderr was {err:?}"
        );
        assert!(
            !err.contains("missing-operand"),
            "{flags:?} should not have reached the operands: {err:?}"
        );
    }
    // `-e` is not part of that test, so it may ride along.
    let (out, status, err) = run(&["-e", "-m", "-h"], b"a");
    assert_eq!(status, 1, "stderr was {err:?}");
    assert_eq!(out, Vec::<u8>::new());
}

#[test]
fn letters_outside_the_c_optstring_are_refused_with_usage() {
    // C's getopt string is `"eHhm"`; anything else lands on `default:` and prints
    // the usage line (`usr.bin/unvis/unvis.c:73-79`), exit `EXIT_FAILURE`.
    for flag in ["-x", "-1", "-v"] {
        let (out, status, err) = run(&[flag], b"a");
        assert_eq!(out, Vec::<u8>::new(), "flag {flag} printed {out:?}");
        assert_eq!(status, 1, "flag {flag} stderr was {err:?}");
        assert!(
            err.contains("Usage: unvis [-e] [-Hh | -m] [file ...]"),
            "flag {flag} stderr was {err:?}"
        );
    }
    // `-H` is a known letter whose leg this decoder has not grown: the hundred-name
    // entity table of `lib/libc/gen/unvis.c:85-196`. It is refused, not ignored.
    let (out, status, err) = run(&["-H"], b"a");
    assert_eq!(out, Vec::<u8>::new());
    assert_eq!(status, 1);
    assert!(err.contains("-H is not modelled"), "stderr was {err:?}");
}
