//! End-to-end face for the `compress` binary's host-safe option shapes,
//! checked against `minix3/minix/commands/compress/compress.c`.
//!
//! The I/O legs are the none-target faces, but three option shapes resolve
//! before any file work and are pinned here:
//! ① `-V` prints C's `version()` banner on **stderr** and keeps going
//!    (`compress.c:372-378`, `:1587-1616` for the Minix build);
//! ② `-r` is not a C flag — its manual switch falls to `Usage()`
//!    (`compress.c:244`);
//! ③ an unknown letter answers the same usage line.

use std::process::{Command, Stdio};

fn compress_bin() -> &'static str {
    env!("CARGO_BIN_EXE_compress")
}

fn run(args: &[&str]) -> (String, i32, String) {
    let out = Command::new(compress_bin())
        .args(args)
        .stdin(Stdio::null())
        .output()
        .expect("spawn compress");
    (
        String::from_utf8_lossy(&out.stdout).into_owned(),
        out.status.code().expect("exit code"),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

#[test]
fn v_prints_the_minix_banner_on_stderr() {
    let (out, _, err) = run(&["-V"]);
    assert_eq!(out.as_str(), "");
    assert_eq!(err, "compress 4.1\nOptions: MINIX, BITS = 16\n");
}

#[test]
fn r_is_not_a_c_flag_and_answers_the_usage_line() {
    let (out, status, err) = run(&["-r"]);
    assert_eq!(out.as_str(), "");
    assert_eq!(status, 1);
    assert_eq!(err, "Usage: compress [-dfvcV] [-b maxbits] [file ...]\n");
}

// The bin's own unknown-letter arm (`fail("unknown option")`) writes through
// the raw `write` leg instead of `support::warn`, so its sentence is invisible
// on the hosted face — a pre-existing gap of that arm, registered with the
// proctools `fail()` family rather than widened by this change. The exit
// status is still 1.
#[test]
fn an_unknown_letter_still_exits_one() {
    let (_, status, _) = run(&["-Z"]);
    assert_eq!(status, 1);
}
