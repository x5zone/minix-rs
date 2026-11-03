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
    assert_eq!(err, "cal: bad number: x\n");
}

/// The window guard (`cal.c:378-380`).
#[test]
fn a_window_leaving_the_printable_years_is_refused() {
    let (out, status, err) = run(&["-A", "100", "12", "9999"]);
    assert_eq!(out.as_str(), "");
    assert_eq!(status, 1);
    assert_eq!(err, "cal: year should be in 1-9999\n");
}

/// The default reform is Britain's September 1752 switch (`cal.c:165`):
/// 3 through 13 never existed, so the 2nd (Wednesday) is followed by the
/// 14th (Thursday) in the same week row.
#[test]
fn the_default_calendar_skips_the_1752_gap() {
    let (out, code, err) = run(&["9", "1752"]);
    assert_eq!((code, err.as_str()), (0, ""));
    let rows: Vec<&str> = out.lines().collect();
    assert_eq!(
        rows[0], "   September 1752   ",
        "title centred over 20 columns"
    );
    assert_eq!(rows[2], "       1  2 14 15 16");
    assert_eq!(rows[3], "17 18 19 20 21 22 23");
    assert_eq!(rows[4], "24 25 26 27 28 29 30");
    let days: Vec<u32> = rows[2..]
        .iter()
        .flat_map(|row| row.split_whitespace())
        .filter_map(|cell| cell.parse().ok())
        .collect();
    for missing in 3..=13u32 {
        assert!(!days.contains(&missing), "{missing} must be absent: {out}");
    }
    assert!(days.contains(&2) && days.contains(&14), "{out}");
}

/// `-R Italy` moves the switch to October 1582: 5 through 14 vanish and the
/// 15th (Friday) follows the 4th (Thursday).
#[test]
fn a_named_reform_moves_the_gap() {
    let (out, code, err) = run(&["-R", "Italy", "10", "1582"]);
    assert_eq!((code, err.as_str()), (0, ""));
    let rows: Vec<&str> = out.lines().collect();
    assert_eq!(rows[2], "    1  2  3  4 15 16");
    assert_eq!(rows[5], "31");
}

/// `-r` with no operands shows the reform month instead of today.
#[test]
fn r_starts_from_the_reform_month() {
    let (out, code, err) = run(&["-r"]);
    assert_eq!((code, err.as_str()), (0, ""));
    assert!(out.starts_with("   September 1752   \n"), "{out}");
}

/// C's `-R` rejection sentences (`cal.c:733-802`), program name included.
#[test]
fn the_reform_specifier_answers_c_sentences() {
    let cases = [
        (
            ["-R", "1582"],
            "cal: 1582: ambiguous short reform date specification\n",
        ),
        (["-R", "abc"], "cal: reform name abc invalid\n"),
        (["-R", "0"], "cal: 0: illegal year value: use 1-9999\n"),
        (
            ["-R", "1752/13"],
            "cal: 13: illegal month value: use 1-12\n",
        ),
        (
            ["-R", "1752/2/30"],
            "cal: 30: illegal date value: use 1-29\n",
        ),
        (
            ["-R", "1752/9/3x"],
            "cal: date specifier 1752/9/3x invalid\n",
        ),
    ];
    for (args, expected) in cases {
        let (out, status, err) = run(&args);
        assert_eq!(out.as_str(), "", "{args:?}");
        assert_eq!(status, 1, "{args:?}");
        assert_eq!(err, expected, "{args:?}");
    }
}

/// An unknown letter answers C's usage pair (`cal.c:911-917`), and `-R` with
/// no argument answers the getopt sentence before it.
#[test]
fn unknown_letters_and_a_missing_reform_argument_answer_usage() {
    let usage = "usage: cal [-3hjry] [-A after] [-B before] [-C context] [-d day-of-week] \
[-R reform-spec]\n           [[month] year]\n";
    let (_, status, err) = run(&["-Z"]);
    assert_eq!(status, 1);
    assert_eq!(err, usage);
    let (_, status, err) = run(&["-R"]);
    assert_eq!(status, 1);
    assert_eq!(
        err,
        format!("cal: option requires an argument -- R\n{usage}")
    );
}
