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

mod face_support;

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
    face_support::write_stdin(&mut child, input);
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
    // A lone `-a` gets C's named refusal (`pr.c:1823-1828`), but the exit code 1
    // does not come from it: after `setup` returns 1, `main` appends the three usage
    // lines (`:142-157`).
    let (out, status, err) = run(&["-a"], b"a\n");
    assert_eq!(out, "");
    assert_eq!(status, 1, "stderr was {err:?}");
    assert_eq!(
        err,
        format!("pr: -a flag requires multiple columns\n{USAGE}")
    );
    // `-m` cannot be combined with `-a` (`pr.c:1829-1832`); the usage follows here too.
    let (_, status, err) = run(&["-m", "-a", "-2"], b"a\n");
    assert_eq!(status, 1);
    assert_eq!(err, format!("pr: -m cannot be used with -a\n{USAGE}"));
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
    assert_eq!(err, format!("pr: invalid value for -s b\n{USAGE}"));
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
        assert_eq!(
            err,
            format!("pr: Number of lines must be 1 or more\n{USAGE}"),
            "{bad}"
        );
    }
    let (_, status, err) = run(&["-l", " 8"], b"a\n");
    assert_eq!(status, 1);
    assert_eq!(
        err,
        format!("pr: Number of lines must be 1 or more\n{USAGE}")
    );
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
fn numeric_option_diagnostics_are_c_named_messages_then_the_usage() {
    // Each numeric option has its own sentence (not just usage), but `main` appends
    // the three usage lines after any setup error (`pr.c:142-157`); `-w`'s sentence
    // really does end with one space (`:1800`).
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
        assert_eq!(err, format!("{message}{USAGE}"), "{flag}");
    }
    // A page too narrow for the requested column count is the other named error
    // (`pr.c:1863-1867`).
    let (_, status, err) = run(&["-100", "-w", "20"], b"a\n");
    assert_eq!(status, 1);
    assert_eq!(
        err,
        format!("pr: page width is too small for 100 columns\n{USAGE}")
    );
}

/// C `usage()` (`pr.c:1616-1627`): three lines, the last two carrying ten leading
/// spaces each, and the whole thing does not rely on `getprogname()` (C hardcodes
/// `pr` in the string). It is written as one literal because Rust's line
/// continuation would eat exactly those ten spaces.
const USAGE: &str = "usage: pr [+page] [-col] [-adFfmprt] [-e[ch][gap]] [-h header]\n          [-i[ch][gap]] [-l line] [-n[ch][width]] [-o offset]\n          [-s[ch]] [-T timefmt] [-w width] [-] [file ...]\n";

/// The same command seam as `run`, but standard error comes back **byte for byte**:
/// `from_utf8_lossy` renders a bare `c3` as the three-byte U+FFFD, so the text form
/// cannot tell "one byte" from "two bytes" among the error shapes.
fn run_bytes(args: &[&str], input: &[u8]) -> (Vec<u8>, i32, Vec<u8>) {
    let mut child = Command::new(PR)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the pr binary should be runnable");
    face_support::write_stdin(&mut child, input);
    let output = child.wait_with_output().expect("the run should finish");
    (
        output.stdout,
        output.status.code().unwrap_or(-1),
        output.stderr,
    )
}

/// The option-letter position of the outside-the-set line is **one bare byte**, and
/// the mechanism-level evidence here is not libc: `pr` links its own `egetopt.c`
/// (`eoptopt = (int)*place++` at `:110` takes one byte at a time, `%c` at `:148`
/// writes it out unchanged), and the wording is its own `illegal option -- <letter>`
/// (not the `unknown option` of the rest of the family). Expected shapes come from
/// the reference build (`pr.c` + `egetopt.c` + shims, including the NetBSD four-step
/// `raise_default_signal` semantics, started through a same-name `pr` symlink so the
/// program-name segment matches this shell): `-é` and `-dé` both print
/// `pr: illegal option -- ` + a single `c3` + newline, then those **three** usage
/// lines (the second and third carrying ten leading spaces each) and exit 1; `-Zé`
/// stops at `Z`; conversely, in `-wé` the `é` is a **value** (`w` takes one), so the
/// command's own space-terminated `pr: -w width must be 1 or more ` follows, then the
/// three usage lines. The ASCII shapes already have their owner in this file and are
/// not repeated here.
#[test]
fn the_libc_line_of_a_multibyte_flag_is_one_byte_for_pr() {
    let want: Vec<u8> = [
        b"pr: illegal option -- ".as_slice(),
        b"\xc3\n",
        USAGE.as_bytes(),
    ]
    .concat();
    for form in ["-\u{e9}", "-d\u{e9}"] {
        let (out, status, err) = run_bytes(&[form], b"a\n");
        assert_eq!(out, Vec::new(), "{form} 的报文只走标准错误");
        assert_eq!(status, 1, "{form} 的 stderr 是 {err:?}");
        assert_eq!(err, want, "{form} 的字母该是一个裸字节 c3");
    }

    let (_out, status, err) = run_bytes(&["-Z\u{e9}"], b"a\n");
    assert_eq!(status, 1);
    assert_eq!(
        err,
        [b"pr: illegal option -- Z\n".as_slice(), USAGE.as_bytes()].concat(),
        "环停在第一个坏字母上，第二个字节根本不该被读到"
    );

    let (_out, status, err) = run_bytes(&["-w\u{e9}"], b"a\n");
    assert_eq!(status, 1);
    assert_eq!(
        err,
        [
            b"pr: -w width must be 1 or more \n".as_slice(),
            USAGE.as_bytes(),
        ]
        .concat(),
        "带值字母把两字节整个当值收走，句子是命令自己的那一句（结尾那个空格也是 C 的原文）"
    );
}

#[test]
fn letters_outside_the_c_optstring_print_the_egetopt_line_then_the_usage() {
    // For a letter outside `"#adFfmrte?h:i?l:n?o:ps?T:w:"`, `egetopt` prints its own
    // line first (`egetopt.c:144-151`), then returns `'?'`; `setup` returns 1, and
    // `main` appends the three usage lines (`pr.c:1616-1627`) before exiting 1. Both
    // sentences are therefore at least two lines each.
    let (out, status, err) = run(&["-x"], b"a\n");
    assert_eq!(out, "");
    assert_eq!(status, 1, "stderr was {err:?}");
    assert_eq!(err, format!("pr: illegal option -- x\n{USAGE}"));
    // The missing-value sentence: word for word identical to libc's `getopt`
    // (`egetopt.c:196-205`), while the outside-the-set sentence differs (libc says
    // `unknown option`) — two versions of the same fact.
    let (_, status, err) = run(&["-w"], b"a\n");
    assert_eq!(status, 1);
    assert_eq!(
        err,
        format!("pr: option requires an argument -- w\n{USAGE}")
    );
    // Letters C's optstring carries but this deciding half does not implement are
    // reported rather than ignored: `-p` pauses per page, which needs a terminal leg.
    // That sentence belongs to this shell (C accepts `-p`), so it does not imitate C's shape either.
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

#[test]
fn an_unreadable_operand_gets_the_comma_form_and_does_not_stop_the_run() {
    // C's open-failure leg (`pr.c:1364-1370`) is
    // `fprintf(errf, "pr: Cannot open %s, %s\n", argv[…], strerror(errno))`: capital
    // C, a comma between the name and the sentence (not the colon form of
    // `err(3)`/`warn(3)`), then `continue`; the exit code is raised to 1 by `errcnt`
    // (`:157-159`) and **no usage follows** — this is not a setup error.
    let missing = format!("pr-face-{}-missing", std::process::id());
    let (out, status, err) = run(&[&missing], b"a\n");
    assert_eq!(out, "");
    assert_eq!(status, 1, "stderr was {err:?}");
    assert_eq!(
        err,
        format!("pr: Cannot open {missing}, No such file or directory\n")
    );
    // Later operands still run (C does `continue`, not a fatal stop), one sentence
    // per file. `-t` is added to avoid the header (pr gives every page five header
    // and five trailer lines by default; this test exercises the open leg, not
    // pagination).
    let (out, status, err) = run(&["-t", &missing, "-"], b"a\n");
    assert_eq!(out, "a\n", "第二个操作数该照常输出，stderr 是 {err:?}");
    assert_eq!(status, 1);
    assert_eq!(
        err,
        format!("pr: Cannot open {missing}, No such file or directory\n")
    );
}

/// `-r` is C's `++nodiag` (`pr.c:1775-1777`), consulted at exactly one place:
/// the open-failure leg raises `errcnt` and only then skips the sentence
/// (`pr.c:1364-1369`). So the flag removes the line, never the failure — the
/// exit status stays 1, the operand is still skipped, and a later operand still
/// prints. Before this leg the shell answered `-r` with
/// `pr: option form not modelled yet` and exit 1 without reading anything.
#[test]
fn dash_r_silences_the_open_diagnostic_but_not_the_failure() {
    let missing = format!("pr-face-{}-missing", std::process::id());
    let (out, status, err) = run(&["-r", &missing], b"a\n");
    assert_eq!(
        (out, err),
        (String::new(), String::new()),
        "退 1 之外不该有任何输出"
    );
    assert_eq!(status, 1, "nodiag 只压句子，不压失败");

    // Same with a readable operand behind it: the body arrives, the sentence does
    // not, and the run still ends in failure (`errcnt`).
    let (out, status, err) = run(&["-t", "-r", &missing, "-"], b"a\n");
    assert_eq!(out, "a\n");
    assert_eq!(err, "");
    assert_eq!(status, 1);

    // Two unreadable operands under `-r`: still silent, still one failure.
    let other = format!("pr-face-{}-missing2", std::process::id());
    let (out, status, err) = run(&["-r", &missing, &other], b"");
    assert_eq!((out, err), (String::new(), String::new()));
    assert_eq!(status, 1);

    // Without `-r` the sentence is back, unchanged (the comma form of the test above).
    let (_, status, err) = run(&[&missing], b"a\n");
    assert_eq!(status, 1);
    assert_eq!(
        err,
        format!("pr: Cannot open {missing}, No such file or directory\n")
    );
}

#[test]
fn form_feed_ends_every_header_page_and_no_header_cancels_it() {
    // `prtail`'s form-feed arm writes one `\f` and returns (`pr.c:1555-1561`), so a
    // page under `-F` shows its header, its real body lines and that byte — the blank
    // padding rows and the five trailer lines it would otherwise write are both gone.
    let (out, status, err) = run(&["-F", "-l", "20", "-h", "T"], b"a\nb\nc\n");
    assert_eq!(status, 0, "stderr was {err:?}");
    assert_eq!(
        out, "\n\nT Page 1\n\n\na\nb\nc\n\u{c}",
        "two blanks, title, two blanks, then the three body lines"
    );
    assert_eq!(out.matches('\u{c}').count(), 1);
    // The multi-column page ends through the same `prtail` call (`pr.c:604-608`), so
    // three lines in two columns are two rows under the header and nothing after them
    // but the form feed: 5 header + 2 body lines instead of 5 + 10 + 5.
    let (cols, _, _) = run(
        &["-F", "-l", "20", "-h", "T", "-2", "-w", "20"],
        b"1\n2\n3\n",
    );
    assert_eq!(cols.matches('\u{c}').count(), 1);
    assert!(cols.ends_with('\u{c}'), "-F rides the last line: {cols:?}");
    assert_eq!(cols.trim_end_matches('\u{c}').lines().count(), 7);
    let (wide, _, _) = run(&["-l", "20", "-h", "T", "-2", "-w", "20"], b"1\n2\n3\n");
    assert_eq!(wide.matches('\u{c}').count(), 0);
    assert_eq!(
        wide.lines().count(),
        20,
        "padded body plus the five trailer lines"
    );
    // `-t` makes `prtail` return inside its `nohead` arm before the form-feed arm
    // (`pr.c:1526-1537`), so the byte is gone as well: `-F` changes nothing at all.
    let (out, _, _) = run(&["-F", "-t", "-l", "2"], &numbered(1, 5));
    assert_eq!(out, "1\n2\n3\n4\n5\n");
    assert_eq!(out.matches('\u{c}').count(), 0);
    // The same cancellation reaches the multi-column path, because `vertcol` ends each
    // page through `prtail` too: with headers off, `-F` is indistinguishable from a
    // plain run byte for byte.
    let (fed, _, _) = run(&["-F", "-t", "-l", "20", "-2", "-w", "20"], b"1\n2\n3\n");
    let (bare, _, _) = run(&["-t", "-l", "20", "-2", "-w", "20"], b"1\n2\n3\n");
    assert_eq!(fed, bare, "no form feed without headers, even in columns");
    assert_eq!(fed.matches('\u{c}').count(), 0);
    // `-f` falls through to `-F` in C's switch (`pr.c:1693-1700`); its other half, the
    // per-page pause, only fires when stdout is a terminal, so the piped output is
    // identical byte for byte.
    let (f_out, _, _) = run(&["-f", "-l", "20", "-h", "T"], b"a\nb\nc\n");
    assert_eq!(f_out, "\n\nT Page 1\n\n\na\nb\nc\n\u{c}");
}

/// The gate's own contract, pinned here rather than elsewhere: when the command
/// exits **before reading standard input**, writing the input is not a failure.
///
/// The diagnostic gates above all use this shape (the setup errors at
/// `pr.c:142-157` come before any read). Whether the writer gets `BrokenPipe`
/// depends on which side is scheduled first — with only a few input bytes the parent
/// usually finishes writing first, so the same case is flaky under load. Here the
/// test **waits for the child to exit before writing**, deterministically creating
/// the "pipe read end already closed" state: the write is quietly ignored
/// (`face_support::write_stdin` tolerates only `BrokenPipe`, any other write error
/// still fails on the spot).
///
/// Why poll with `try_wait` rather than `wait`: `Child::wait` drops the
/// `child.stdin` handle first (std does that so a child cannot hang forever waiting
/// for input), and then there is nothing to write to. The polling has a patience cap
/// and reports the timeout explicitly, leaving no unbounded spin.
#[test]
fn writing_stdin_to_an_already_exited_child_is_not_a_gate_failure() {
    let mut child = Command::new(PR)
        .args(["+0"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the pr binary should be runnable");
    let mut exited = child.try_wait().expect("polling the child should work");
    for _ in 0..2000 {
        if exited.is_some() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
        exited = child.try_wait().expect("polling the child should work");
    }
    let status = exited.expect("pr exits on the setup error without reading stdin");
    assert_eq!(status.code().unwrap_or(-1), 1, "这一形走的是 usage 那一支");
    face_support::write_stdin(&mut child, b"a\n");
}

/// The line legs are end-to-end byte-wise: the record `fgetln` hands over is never
/// decoded, so a non-UTF-8 byte still makes a line, still takes part in tab
/// expansion, and still comes out through the header layout of a custom page title.
/// Expectations are byte-for-byte from the reference build (started through a
/// same-name symlink); fixtures always write literal bytes, because using `é`'s UTF-8
/// escape as a fixture would double-encode and go falsely green.
#[test]
fn a_nonutf8_line_keeps_its_bytes_through_the_line_legs() {
    let raw = b"a\xffb\tp\nq\xff\n";
    // Default single column plus `-t` (no header or trailer): this leg is a plain copy.
    let (out, status, err) = run_bytes(&["-t"], raw);
    assert_eq!(status, 0, "stderr was {err:?}");
    assert_eq!(
        out, raw,
        "the pass-through leg must not touch a single byte"
    );

    // `-e` expands input tabs on an eight-column pitch: five blanks are inserted
    // before the three characters after `\t`, and the `ff` byte takes part in no
    // processing beyond counting.
    let (expanded, status, err) = run_bytes(&["-t", "-e"], raw);
    assert_eq!(status, 0, "stderr was {err:?}");
    assert_eq!(expanded, b"a\xffb     p\nq\xff\n");

    // `-h` replaces the header text; lines mixing multibyte and bare bytes still come
    // out byte for byte.
    let mixed = b"caf\xc3\xa9 z\n\xff q\n";
    let (headed, status, err) = run_bytes(&["-t", "-h", "T"], mixed);
    assert_eq!(status, 0, "stderr was {err:?}");
    assert_eq!(headed, mixed);
}

/// The command seam with byte-shaped argv: a `&str` cannot deliver a bare `ff` (an
/// `OsStr` built from a `str` is already valid UTF-8). Both streams come back byte
/// for byte — three of `pr`'s values are **output content** (the `-s` column
/// separator, the `-n` line-number suffix, the `-h` header title), one is an
/// **input-side comparison operand** (`-e`), and one an **output-side comparison
/// operand** (`-i`).
fn run_argv(args: &[&[u8]], input: &[u8]) -> (Vec<u8>, i32, Vec<u8>) {
    use std::os::unix::ffi::OsStrExt as _;
    let mut child = Command::new(PR)
        .args(args.iter().map(|word| std::ffi::OsStr::from_bytes(word)))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the pr binary should be runnable");
    face_support::write_stdin(&mut child, input);
    let output = child.wait_with_output().expect("the run should finish");
    (
        output.stdout,
        output.status.code().unwrap_or(-1),
        output.stderr,
    )
}

/// On `pr` the argument seam changes **output content and comparison operands**, not
/// message rendering. The reference build matched these five shapes byte for byte.
/// ① `-s`'s value is printed between columns by `vertcol`/`horzcol` (`pr.c:487`,
///   `:745`); between two columns it is that one `ff` and **no other character**
///   (with `-s` present no blank padding is inserted between columns).
/// ② `-n`'s value is `nmchar` (`pr.c:95`), placed after the line number by `addnum`
///   (`:215`) ⇒ the `ff` follows the number directly.
/// ③ `-h`'s value is printed unchanged on the header line by `prhead`'s
///   `printf(HDFMT, buf+offst, fname, pagcnt)` (`pr.h:59`, `pr.c:1511`) — this shell
///   does not render the time field, so the line is `title + " Page 1"` and the title
///   position still holds the raw argv bytes.
/// ④⑤ The `-e` and `-i` legs differ **in signedness**: `inln` compares
///   `int ch = getc()` (0..=255) against `int chk = (int)inchar`, where `inchar` is a
///   signed `char` (`pr.c:87`, `:1038`, `:1049`) ⇒ a high byte is promoted to a
///   negative number and **never matches any input byte**; `otln` compares
///   `*buf == ochar` (`:1165`), both `char`, so the same high byte does match on the
///   output side. The default tab character (0x09) is unaffected, and this shape
///   doubles as the control for "nothing was loosened".
#[test]
fn a_raw_byte_in_a_value_that_is_printed_reaches_the_output_as_that_byte() {
    // ① The separator byte between two columns.
    let (out, status, err) = run_argv(&[b"-t", b"-2", b"-s\xff", b"-w", b"20"], b"a\nb\nc\nd\n");
    assert_eq!(status, 0, "stderr 是 {err:?}");
    assert_eq!(
        out,
        [b"a\xff".as_slice(), b"c\nb\xff", b"d\n"].concat(),
        "列间只有 argv 那一枚字节，不补空白"
    );
    // ② The line-number suffix byte.
    let (out, status, err) = run_argv(&[b"-t", b"-n\xff2"], b"a\nb\n");
    assert_eq!(status, 0, "stderr 是 {err:?}");
    assert_eq!(out, b" 1\xffa\n 2\xffb\n");
    // ③ The header title bytes (this shell does not render the time field, so that
    // line is title + " Page 1").
    let (out, status, err) = run_argv(&[b"-l", b"20", b"-h\xff\xfe"], b"a\n");
    assert_eq!(status, 0, "stderr 是 {err:?}");
    assert_eq!(
        out.split(|&b| b == b'\n').nth(2).unwrap(),
        b"\xff\xfe Page 1"
    );
    // ④ A high bit in -e's character ⇒ the whole expansion leg becomes a no-op
    // (chk is negative in C).
    let (out, status, err) = run_argv(&[b"-t", b"-e\xff4"], b"x\xffy\n");
    assert_eq!(status, 0, "stderr 是 {err:?}");
    assert_eq!(out, b"x\xffy\n", "高位字符配不上任何输入字节，字节原样通过");
    // ④ Control: with the default tab in the character position, expansion still happens.
    let (out, _status, _err) = run_argv(&[b"-t", b"-e4"], b"x\ty\n");
    assert_eq!(out, b"x   y\n");
    // ⑤ The same high byte still matches on the -i side (both sides are char in C).
    let (out, status, err) = run_argv(&[b"-t", b"-i\xff4"], b"a\xff    b\n");
    assert_eq!(status, 0, "stderr 是 {err:?}");
    assert_eq!(out, b"a\xff\xffb\n", "模拟 + 收缩都照走");
}

/// The four `invalid value for -X %s` sentences (`pr.c:1726-1729` for `-e`,
/// `:1757-1760` for `-n`, `:1785-1788` for `-s`, and `:1710-1728` for the `-i` leg)
/// and the `Cannot open %s` sentence (`pr.c:1364-1370`) all echo raw argv bytes.
/// Their **shape** is pinned as well: the command's own sentence is followed by
/// `usage()`'s three lines (`main` appends them only when `setup` returns nonzero,
/// `pr.c:142-157`), and the exit code is 1. The `-s<é>` shape also pins "take only the
/// first byte as the value and echo the rest as raw bytes": in `c3 a9`, `c3` becomes
/// the separator and the echo is the single `a9` — under the text seam both would be
/// folded into three bytes and neither could be recognized.
#[test]
fn the_four_invalid_value_echoes_and_the_operand_message_carry_the_argv_bytes() {
    // Those three lines reuse the file's existing `USAGE` constant — it is written as
    // one literal precisely because line continuation would eat the ten leading
    // spaces of the last two lines.
    let usage = USAGE.as_bytes();
    for (letter, want) in [
        ('e', b"pr: invalid value for -e \xff\n".as_slice()),
        ('i', b"pr: invalid value for -i \xff\n".as_slice()),
        ('n', b"pr: invalid value for -n \xff\n".as_slice()),
        ('s', b"pr: invalid value for -s \xff\n".as_slice()),
    ] {
        // All four optstrings use `?` (an optional value ⇒ **only the part attached
        // to the same word is consumed**; a separate word becomes an operand), so the
        // bare byte must be appended right after the letter to form one token.
        let mut token = format!("-{letter}x").into_bytes();
        token.push(0xff);
        let (out, status, err) = run_argv(&[token.as_slice()], b"a\n");
        assert_eq!(out, Vec::new(), "报文只走标准错误");
        assert_eq!(status, 1, "{letter} 那一句该退 1");
        assert_eq!(
            err,
            [&*want.to_vec(), usage].concat(),
            "-{letter} 那一行带的是剩下的那一枚字节，且后面接三行 usage"
        );
    }
    // A valid two-byte sequence: the first byte becomes the value, the trailing byte
    // goes into the echo.
    let (out, status, err) = run_argv(&[b"-s\xc3\xa9"], b"a\n");
    assert_eq!(out, Vec::new());
    assert_eq!(status, 1);
    assert_eq!(
        err.split(|&b| b == b'\n').next().unwrap(),
        b"pr: invalid value for -s \xa9"
    );
    // An unopenable file name: C's sentence is
    // `fprintf(errf, "pr: Cannot open %s, %s\\n", argv[…], strerror)`, and the name
    // position holds raw bytes (a regression item, not a divergence).
    let (out, status, err) = run_argv(&[b"/tmp/nosuch-pr-\xff"], b"a\n");
    assert_eq!(out, Vec::new());
    assert_eq!(status, 1);
    assert_eq!(
        err.split(|&b| b == b'\n').next().unwrap(),
        b"pr: Cannot open /tmp/nosuch-pr-\xff, No such file or directory"
    );
}
