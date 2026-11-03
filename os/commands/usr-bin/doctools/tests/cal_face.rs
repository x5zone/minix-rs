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
