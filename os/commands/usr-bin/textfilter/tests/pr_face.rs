//! End-to-end face for the `pr` binary: real process, real pipes and real files,
//! checked against `minix3/usr.bin/pr/pr.c` (option loop `:1655-1810`, defaults
//! and the column arithmetic `:1816-1878`, the three printers `onecol`/`vertcol`/
//! `horzcol`, and the diagnostics `usage()`/`prhead`/`prtail`).
//!
//! The goldens are derived from the C's own formulas, written out in each comment,
//! not from what this shell happens to emit: a column page is `rows` lines wide of
//! `colwd = (pgwd + 1 - clcnt) / clcnt` cells each followed by one separating space
//! (`pr.c:1860-1861`), a page with headers carries `lines - 10` body rows plus the
//! five-line header and five-line trailer (`pr.h:60-61`, `pr.c:1875-1878`), and a
//! headerless page is not padded at all (`prtail`'s `nohead` arm, `pr.c:1526-1538`).

use std::io::Write as _;
use std::process::{Command, Stdio};

const PR: &str = env!("CARGO_BIN_EXE_pr");

fn run(args: &[&str], input: &[u8]) -> (String, i32, String) {
    let mut child = Command::new(PR)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the pr binary should be runnable");
    child
        .stdin
        .as_mut()
        .expect("stdin piped")
        .write_all(input)
        .expect("writing standard input");
    drop(child.stdin.take());
    let output = child.wait_with_output().expect("the run should finish");
    (
        String::from_utf8_lossy(&output.stdout).into_owned(),
        output.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

fn joined(lines: &[&str]) -> String {
    let mut text = String::new();
    for line in lines {
        text.push_str(line);
        text.push('\n');
    }
    text
}

fn numbered(from: usize, to: usize) -> Vec<u8> {
    (from..=to)
        .map(|n| format!("{n}\n").into_bytes())
        .collect::<Vec<Vec<u8>>>()
        .concat()
}

#[test]
fn multi_column_output_lays_lines_down_each_column() {
    // `pgwd = 20`, `clcnt = 2` ⇒ `colwd = (20 + 1 - 2) / 2 = 9`, so each cell
    // occupies ten columns (nine data plus the separating space) and the last column
    // is not padded (`vertcol`'s inner loop, `pr.c:441-486`). With `-l 8 -t` the
    // page is eight rows of two columns, i.e. sixteen input lines per page, filled
    // down column 1 first.
    let (out, status, err) = run(&["-2", "-l", "8", "-w", "20", "-t"], &numbered(1, 16));
    assert_eq!(status, 0, "stderr was {err:?}");
    // The row leaves `vertcol` as `1` + nine blanks + `9`, and `otln` folds those
    // nine blanks into one tab (column 1 reaches the stop at column 8) plus two left
    // over, so the terminal sees `1<TAB>  9`.
    let expected: Vec<String> = (1..=8).map(|row| format!("{row}\t  {}", row + 8)).collect();
    assert_eq!(
        out,
        joined(&expected.iter().map(String::as_str).collect::<Vec<_>>())
    );
    // A page that is short by at least a full column is repacked into balanced
    // columns instead of the staircase the plain down-fill would give
    // (`pr.c:505-586`): ten lines over two columns become rows of five.
    let (balanced, _, _) = run(&["-2", "-l", "8", "-w", "20", "-t"], &numbered(1, 10));
    assert!(
        balanced.starts_with("1\t  6\n"),
        "balanced last page should pair 1 with 6, got {balanced:?}"
    );
}

#[test]
fn across_flag_fills_row_major_and_requires_columns() {
    // `-a` selects `horzcol` (`pr.c:151-152`), which walks the columns inside one
    // line, so input lines 1..4 land as `1 2` then `3 4`.
    let (out, status, err) = run(&["-a", "-2", "-l", "8", "-w", "20", "-t"], &numbered(1, 4));
    assert_eq!(status, 0, "stderr was {err:?}");
    assert_eq!(out, "1\t  2\n3\t  4\n");
    // `-a` alone is C's named refusal, not the usage line, and the exit is 1
    // (`pr.c:1823-1828`).
    let (out, status, err) = run(&["-a"], b"a\n");
    assert_eq!(out, "");
    assert_eq!(status, 1, "stderr was {err:?}");
    assert_eq!(err, "pr: -a flag requires multiple columns\n");
    // `-m` cannot be combined with `-a` (`pr.c:1829-1832`).
    let (_, status, err) = run(&["-m", "-a", "-2"], b"a\n");
    assert_eq!(status, 1);
    assert_eq!(err, "pr: -m cannot be used with -a\n");
}

#[test]
fn separator_character_replaces_the_column_padding() {
    // `-s` sets `sflag`, and `vertcol` writes `schar` instead of padding to the cell
    // width (`pr.c:478-480`); the default `schar` is a tab (`pr.h:69 SCHAR`), and
    // without `-w` the page width jumps to 512 (`pr.c:1834-1838`).
    let (out, status, err) = run(&["-2", "-s", "-l", "8", "-w", "20", "-t"], &numbered(1, 16));
    assert_eq!(status, 0, "stderr was {err:?}");
    let expected: Vec<String> = (1..=8).map(|row| format!("{row}\t{}", row + 8)).collect();
    assert_eq!(
        out,
        joined(&expected.iter().map(String::as_str).collect::<Vec<_>>())
    );
    // `-s` takes one optional glued character, and a second one is C's named error
    // (`pr.c:1779-1789`).
    assert_eq!(
        run(
            &["-2", "-s:", "-l", "8", "-w", "20", "-t"],
            &numbered(1, 16)
        )
        .2,
        ""
    );
    let (_, status, err) = run(&["-sab"], b"a\n");
    assert_eq!(status, 1);
    assert_eq!(err, "pr: invalid value for -s b\n");
}

#[test]
fn a_headered_page_carries_five_lines_each_side_and_pads_its_body() {
    // With headers on, `setup` reserves ten of the page's lines
    // (`pr.c:1877`) and `prtail` pads the remaining rows plus the five trailer lines
    // (`pr.c:1563-1567`). `-l 20` therefore holds ten body rows, so three input
    // lines give 5 header + 10 body + 5 trailer = 20 lines.
    let (out, status, err) = run(&["-h", "T", "-l", "20"], b"a\nb\nc\n");
    assert_eq!(status, 0, "stderr was {err:?}");
    let lines: Vec<&str> = out.trim_end_matches('\n').split('\n').collect();
    assert_eq!(out.matches('\n').count(), 20, "last lines were {lines:?}");
    assert_eq!(lines[..5], ["", "", "T Page 1", "", ""], "header block");
    assert_eq!(lines[5..8], ["a", "b", "c"], "body starts after the header");
    assert!(
        lines[8..].iter().all(|line| line.is_empty()),
        "padded rest of page"
    );
}

#[test]
fn a_short_page_length_loses_the_header_instead_of_failing() {
    // `pr.c:1875-1876`: a page that cannot carry both header and trailer just loses
    // its header. So `-l 4` is usable and emits four body lines per page with no
    // header, no trailer and no padding — the input comes back unchanged.
    let (out, status, err) = run(&["-l", "4"], &numbered(1, 6));
    assert_eq!(status, 0, "stderr was {err:?}");
    assert_eq!(out, "1\n2\n3\n4\n5\n6\n");
    // `-l`'s lower bound in C is 1, and the value must start with a digit: `+8` and
    // a leading space are refused with C's own message and status 1
    // (`pr.c:1733-1739`).
    for bad in ["-l+8", "-l0"] {
        let (_, status, err) = run(&[bad], b"a\n");
        assert_eq!(status, 1, "{bad} should be refused");
        assert_eq!(err, "pr: Number of lines must be 1 or more\n", "{bad}");
    }
    let (_, status, err) = run(&["-l", " 8"], b"a\n");
    assert_eq!(status, 1);
    assert_eq!(err, "pr: Number of lines must be 1 or more\n");
}

#[test]
fn empty_input_produces_no_output_at_all() {
    // The header is printed from inside the line loop after a first line exists
    // (`pr.c:252-262`) and the trailer is guarded by `if (linecnt && ...)`
    // (`:294`), so nothing is emitted for empty input — single column or columns,
    // with or without headers — and the exit status stays success.
    for flags in [
        vec![],
        vec!["-t"],
        vec!["-2", "-w", "20"],
        vec!["-2", "-t", "-w", "20"],
    ] {
        let (out, status, err) = run(&flags, b"");
        assert_eq!(out, "", "{flags:?} printed {out:?}");
        assert_eq!(status, 0, "{flags:?} stderr was {err:?}");
        assert_eq!(err, "");
    }
}

#[test]
fn start_page_counts_the_same_pages_the_output_shows() {
    // `inskip` skips `(page - 1) * lines` input lines before the loop
    // (`pr.c:248-252`), and with `-t` a page is the full `-l` height.
    let (out, status, err) = run(&["-t", "-l", "4", "+2"], &numbered(1, 10));
    assert_eq!(status, 0, "stderr was {err:?}");
    assert_eq!(out, "5\n6\n7\n8\n9\n10\n");
    // A start page past the end prints nothing at all.
    let (out, status, err) = run(&["-t", "-l", "4", "+9"], &numbered(1, 10));
    assert_eq!(out, "", "stderr was {err:?}");
    assert_eq!(status, 0);
}

#[test]
fn numeric_option_diagnostics_are_c_named_messages_with_status_one() {
    // Each numeric option carries its own sentence, and none of them prints the
    // usage line (`pr.c:1655-1810`); `-w`'s message really does end in a space.
    let cases = [
        ("+0", "pr: +page number must be 1 or more\n"),
        ("-0", "pr: -columns must be 1 or more\n"),
        ("-o0", "pr: -o offset must be 1 or more\n"),
        ("-w0", "pr: -w width must be 1 or more \n"),
        ("-n0", "pr: -n width must be 1 or more\n"),
        ("-nxy", "pr: invalid value for -n y\n"),
    ];
    for (flag, message) in cases {
        let (out, status, err) = run(&[flag], b"a\n");
        assert_eq!(out, "", "{flag} printed {out:?}");
        assert_eq!(status, 1, "{flag} stderr was {err:?}");
        assert_eq!(err, message, "{flag}");
    }
    // A page too narrow for the requested column count is the other named error
    // (`pr.c:1863-1867`).
    let (_, status, err) = run(&["-100", "-w", "20"], b"a\n");
    assert_eq!(status, 1);
    assert_eq!(err, "pr: page width is too small for 100 columns\n");
}

#[test]
fn letters_outside_the_c_optstring_print_the_three_line_usage() {
    // `egetopt` answers `?` for anything off `"#adFfmrte?h:i?l:n?o:ps?T:w:"`,
    // `setup` returns 1 and `main` prints C's three-line usage (`:1616-1627`) before
    // exiting 1.
    let (out, status, err) = run(&["-x"], b"a\n");
    assert_eq!(out, "");
    assert_eq!(status, 1, "stderr was {err:?}");
    // Written as one literal: a Rust line continuation would eat the ten leading
    // spaces C's second and third lines carry.
    assert_eq!(
        err,
        "usage: pr [+page] [-col] [-adFfmprt] [-e[ch][gap]] [-h header]\n          [-i[ch][gap]] [-l line] [-n[ch][width]] [-o offset]\n          [-s[ch]] [-T timefmt] [-w width] [-] [file ...]\n"
    );
    // Letters C's optstring carries but this deciding half does not implement are
    // reported rather than ignored: `-p` pauses per page, which needs a terminal leg.
    let (_, status, err) = run(&["-p"], b"a\n");
    assert_eq!(status, 1);
    assert_eq!(err, "pr: option form not modelled yet\n");
}

#[test]
fn e_expands_input_tabs_before_any_layout_sees_the_line() {
    // `-e` reaches `inln` (`pr.c:1032-1068`), so the expansion happens while the line
    // is still being read and every later width — clipping, padding, numbering — sees
    // the expanded text. Without `-e`, and without the implied mode below, a tab in
    // the input is just another byte.
    assert_eq!(
        run(&["-e", "-t", "-l", "8"], b"a\tb\n").0,
        format!("a{}b\n", " ".repeat(7))
    );
    assert_eq!(run(&["-t", "-l", "8"], b"a\tb\n").0, "a\tb\n");
    // The pitch and the character are both the option's to choose.
    assert_eq!(
        run(&["-e4", "-t", "-l", "8"], b"a\tb\n").0,
        format!("a{}b\n", " ".repeat(3))
    );
    assert_eq!(
        run(&["-e^", "-t", "-l", "8"], b"a^tb\n").0,
        format!("a{}tb\n", " ".repeat(7))
    );
}

#[test]
fn i_contracts_body_blanks_and_leaves_the_header_alone() {
    // `otln` is the body line's writer (`pr.c:1137-1240`); the header block goes out
    // through `prhead`'s `printf` (`:1499-1513`) and the trailer through `prtail`'s
    // `putchar` loop (`:1524-1573`), neither of which contracts.
    let (out, status, err) = run(&["-i", "-t"], b"        x\n");
    assert_eq!(status, 0, "stderr was {err:?}");
    assert_eq!(out, "\tx\n");
    let (out, _, _) = run(&["-i", "-h", "A        B", "-l", "20"], b"        x\n");
    let lines: Vec<&str> = out.split('\n').collect();
    assert_eq!(
        lines[2], "A        B Page 1",
        "header keeps its blanks: {lines:?}"
    );
    assert_eq!(lines[5], "\tx", "body is contracted");
}

#[test]
fn multi_column_implies_both_transformations_and_a_tab_survives_the_round_trip() {
    // `-2` turns both legs on (`pr.c:1840-1848`), and they compose rather than cancel
    // by luck: `a<tab>b` expands to `a` + seven blanks + `b`, which is exactly the
    // nine-column data width of `-w 20` (`colwd = (20 + 1 - 2) / 2 = 9`), gets one
    // separating blank before the next column, and then contracts back — seven blanks
    // starting at column 1 reach the stop at column 8, so they come out as one tab,
    // while the single separating blank is below the `ips - ops > 1` threshold and
    // stays a blank.
    let (out, status, err) = run(&["-2", "-t", "-w", "20", "-l", "8"], b"a\tb\n");
    assert_eq!(status, 0, "stderr was {err:?}");
    assert_eq!(out, "a\tb \n");
    // The cell is clipped to `colwd` before any of that: a line of sixteen blanks plus
    // `x` loses the `x` to the nine-column clip, leaving ten blanks in the row (nine
    // cell plus the separating pad), and `otln` folds those into one tab and two.
    let (out, _, _) = run(
        &["-2", "-t", "-w", "20", "-l", "8"],
        format!("{}x\n", " ".repeat(16)).as_bytes(),
    );
    assert_eq!(out, "\t  \n");
}
