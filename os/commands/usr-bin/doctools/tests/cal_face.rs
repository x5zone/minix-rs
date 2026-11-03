//! End-to-end face for the `cal` binary's `-h` arm, checked against
//! `minix3/usr.bin/cal/cal.c`.
//!
//! C's `-h` calls `init_hilite` (`cal.c:252-254`), which bumps the highlight
//! counter and looks up the terminal's attribute strings through
//! `setupterm`. With no capable TERM (or a non-tty stdout) the escape pair
//! stays unset, and `ascii_day` renders plain — so the piped output is
//! byte-identical with the plain form (`cal.c:891-908`, `:517`). This tree
//! has no terminfo facility, which is that same no-escape shape for every
//! harness run; the identity below pins it.

use std::process::{Command, Stdio};

fn cal_bin() -> &'static str {
    env!("CARGO_BIN_EXE_cal")
}

fn run(args: &[&str]) -> (String, i32, String) {
    let out = Command::new(cal_bin())
        .args(args)
        .stdin(Stdio::null())
        .output()
        .expect("spawn cal");
    (
        String::from_utf8_lossy(&out.stdout).into_owned(),
        out.status.code().expect("exit code"),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

#[test]
fn h_leaves_the_piped_output_byte_identical() {
    let (plain, code, err) = run(&[]);
    assert_eq!(code, 0);
    assert_eq!(err.as_str(), "");
    let (hilite, code, err) = run(&["-h"]);
    assert_eq!(
        (hilite.as_str(), code, err.as_str()),
        (plain.as_str(), 0, "")
    );
}

#[test]
fn a_second_h_uses_the_reverse_mode_and_stays_identical_when_piped() {
    // C's `init_hilite` flips to `enter_reverse_mode` when the counter passes
    // one (`cal.c:905-908`); without the terminfo facility neither escape
    // exists, so the piped form is still the plain form.
    let (plain, _, _) = run(&[]);
    let (out, code, err) = run(&["-h", "-h"]);
    assert_eq!((out.as_str(), code, err.as_str()), (plain.as_str(), 0, ""));
}

#[test]
fn a_year_operand_with_h_is_also_identical() {
    // The highlight rides every month face; 1999 is fixed and safe to diff.
    let (plain, _, _) = run(&["1999"]);
    let (out, code, err) = run(&["-h", "1999"]);
    assert_eq!((out.as_str(), code, err.as_str()), (plain.as_str(), 0, ""));
}

/// `-C 1` is `-3`'s window expressed through the context flags
/// (`cal.c:243` sets `after = before = getnum(...)`; `-3` sets both to one,
/// `cal.c:267-268`) — the two faces must be byte-identical.
#[test]
fn c_one_equals_the_triple_window() {
    let (triple, code, err) = run(&["-3", "3", "2024"]);
    assert_eq!((code, err.as_str()), (0, ""));
    let (ctx, code, err) = run(&["-C", "1", "3", "2024"]);
    assert_eq!((code, err.as_str()), (0, ""));
    assert_eq!(ctx, triple);
}

/// A window that crosses a year boundary prints both year banners.
#[test]
fn a_cross_year_window_prints_year_banners() {
    let (out, code, err) = run(&["-A", "1", "12", "2024"]);
    assert_eq!((code, err.as_str()), (0, ""));
    assert!(out.contains("2024") && out.contains("2025"), "{out}");
}

/// A bad window value takes C's `getnum` sentence (`cal.c:869-889`).
#[test]
fn a_bad_window_value_reports_the_number_sentence() {
    let (out, status, err) = run(&["-A", "x", "3", "2024"]);
    assert_eq!(out.as_str(), "");
    assert_eq!(status, 1);
    assert_eq!(err, "bad number: x\n");
}

/// The window guard (`cal.c:378-380`).
#[test]
fn a_window_leaving_the_printable_years_is_refused() {
    let (out, status, err) = run(&["-A", "100", "12", "9999"]);
    assert_eq!(out.as_str(), "");
    assert_eq!(status, 1);
    assert_eq!(err, "year should be in 1-9999\n");
}
