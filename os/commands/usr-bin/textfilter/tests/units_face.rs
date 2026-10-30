//! End-to-end face for the `units` binary, checked against
//! `minix3/usr.bin/units/units.c` (option loop `:817-846`, operand check `:849-851`,
//! `readunits()`'s open branch `:142-181`, `usage()` `:793-807`, the interactive
//! prompt loop `:876-893`) and the libc `getopt` it drives
//! (`minix3/lib/libc/stdlib/getopt.c:74-78`, `:81-85`, `:87-92`, `:95-104`,
//! `:112-141`).
//!
//! The optstring at `units.c:817` is `"lLvqf:"`: `l`/`L`/`q`/`v` take no value; only
//! `f` does. The reference build `tmp/units-319/units_ref` (`units.c`'s real source +
//! Minix's `getopt.c` + program name and `strlcpy`/`strlcat` shims, linked live on the
//! host) measured these shapes one by one:
//! (1) off-charset letter = one libc line + **nine-line** usage, exit 3; a cluster
//!     `-qZ` reports `Z`;
//! (2) a missing value exists only for `-f`, with the same sentence and exit code as
//!     (1);
//! (3) `-v`'s two version lines go to **stderr**, and it exits inside the loop —
//!     `-vZ` has no off-charset line, `-Zv` has no version lines;
//! (4) an operand count other than 0/2/3, and "`-l` with operands", produce **only
//!     the usage block**, with no libc line;
//! (5) this check runs **before** `readunits`, so `-f <bad-file> m` reports usage
//!     rather than an open failure;
//! (6) the two open-failure branches use different sentences: `-f` is `err`'s
//!     three-part shape (with `strerror`), the default file
//!     `/usr/share/misc/units.lib` is `errx`'s two-part shape (without), both exit 1;
//! (7) `--` is consumed, a bare `-` is an operand, and `-f` takes the next token
//!     regardless of what it looks like (`-f - m sec` uses `-` as the file name);
//! (8) the loop does not rearrange: in `units -f <file> m sec -l` the `-l` is the
//!     third operand, hence a want-side unit name, and the message is
//!     `units: Unknown unit '-l'`;
//! (9) the interactive face is `do { prompt; fgets; } while (unrecognized)`: an
//!     unrecognized line **re-asks the same slot** (the reference prints
//!     `You have: You have: You want: ` for `bogus\nm\nsec\n`), and
//!     `fgets(buf, 80, …)` splits a 100-character line into 79+21, each reporting its
//!     own `Unknown unit`.
//!
//! No such face existed before this one: `units`'s interactive face read stdin with a
//! bare `minix_sys::read`, which on the host reads not a single byte (the tenth
//! read-seam defect after the ninth of the same family), while `usage()` had only two
//! lines, `-v` printed to stdout, and the default file name was `/usr/lib/units` —
//! this face pinned all four for the first time.
//!
//! The decisive half (conversion values, `-l`/`-L` expansions, the **partial answer
//! block** on a bad unit) is outside this face's range and is registered in
//! `rewrite-notes/18-stage-commands/todo.md` under batch 319.

mod face_support;

use face_support::write_stdin;
use std::fs;
use std::path::PathBuf;
use std::process::{Command, Stdio};

const UNITS: &str = env!("CARGO_BIN_EXE_units");
/// The nine lines of the two `fprintf`s at `units.c:795-806` combined; `units` is a hard-coded literal.
const USAGE: &str = concat!(
    "\nunits [-Llqv] [-f filename] [[count] from-unit to-unit]\n",
    "\n    -f specify units file\n",
    "    -L list units in standardized base units\n",
    "    -l list units\n",
    "    -q suppress prompting (quiet)\n",
    "    -v print version number\n",
);
/// The two version lines at `units.c:836-839`, also on stderr.
const VERSION: &str = concat!(
    "\n  units version 1.0  Copyright (c) 1993 by Adrian Mariano\n",
    "                    This program may be freely distributed\n",
);
/// The tiny dictionary shared by the reference and this face (same content as `tmp/units-319/tiny.lib`).
const TINY: &str = concat!(
    "/ tiny test database for batch 319 A/B\n",
    "/ primitive units\n",
    "m\t!a!\n",
    "sec\t!c!\n",
    "/ derived\n",
    "km\t1000*m\n",
    "cm\t.01*m\n",
    "/ composite\n",
    "min\t60*sec\n",
);

fn run(args: &[&str], input: &[u8]) -> (String, i32, String) {
    let mut child = Command::new(UNITS)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the units binary should be runnable");
    write_stdin(&mut child, input);
    let output = child.wait_with_output().expect("the run should finish");
    (
        String::from_utf8_lossy(&output.stdout).into_owned(),
        output.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

/// Write the tiny dictionary to a temp file and return its path (the reference runs the same content).
fn database(tag: &str) -> (PathBuf, String) {
    let dir = scratch(tag);
    let path = dir.join("tiny.lib");
    fs::write(&path, TINY).expect("write tiny.lib");
    let name = path.to_str().expect("utf-8 scratch path").to_string();
    (dir, name)
}

fn scratch(tag: &str) -> PathBuf {
    static SEQ: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let n = SEQ.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    let dir = std::env::temp_dir().join(format!("units-face-{}-{tag}-{n}", std::process::id()));
    fs::create_dir_all(&dir).expect("scratch dir");
    dir
}

#[test]
fn off_letter_gets_libc_line_then_the_nine_line_usage_block() {
    let (out, code, err) = run(&["-Z"], b"");
    assert_eq!(code, 3, "参照件 `-Z` 退 3，实际 stdout={out:?}");
    assert_eq!(err, format!("units: unknown option -- Z\n{USAGE}"));
    assert_eq!(out, "");
}

#[test]
fn cluster_reports_the_off_letter_after_the_valid_ones() {
    // Both `-qZ` and `-Zq` report `Z`: in the former `q` is set first and then the
    // off-charset letter is hit, in the latter `Z` comes first. Both exit in
    // `usage()`, so the difference is invisible externally — but the letter must not
    // be the first one.
    for args in [&["-qZ"][..], &["-Zq"][..]] {
        let (out, code, err) = run(args, b"");
        assert_eq!(code, 3, "{args:?} 该退 3，实际 stdout={out:?}");
        assert_eq!(
            err,
            format!("units: unknown option -- Z\n{USAGE}"),
            "{args:?}"
        );
        assert_eq!(out, "");
    }
}

#[test]
fn missing_value_is_reachable_only_through_f() {
    let (out, code, err) = run(&["-f"], b"");
    assert_eq!(code, 3);
    assert_eq!(
        err,
        format!("units: option requires an argument -- f\n{USAGE}")
    );
    assert_eq!(out, "");
    // `l`/`q`/`v` take no value, so "the following token is swallowed as a value"
    // cannot happen here: `-l m sec` takes the "-l with operands" branch, while `-q`
    // goes all the way to reading the default file.
    let (_, code, err) = run(&["-q"], b"");
    assert_eq!(code, 1, "-q 之后没选项了，该去读词典");
    assert_eq!(
        err,
        "units: Can't find units file '/usr/share/misc/units.lib'\n"
    );
}

#[test]
fn attached_value_is_never_reported_missing() {
    // `-fZ`'s value is attached, so libc does not report a missing value; this shape
    // falls into one of the three open-failure branches.
    let (out, code, err) = run(&["-fZ"], b"");
    assert_eq!(code, 1);
    assert_eq!(
        err,
        "units: Unable to open units file 'Z': No such file or directory\n"
    );
    assert_eq!(out, "");
}

#[test]
fn version_lines_go_to_stderr_and_win_over_later_letters() {
    // The old form printed these two lines to stdout (`units.c:836` is
    // `fprintf(stderr, …)`).
    let (out, code, err) = run(&["-v"], b"");
    assert_eq!(code, 3);
    assert_eq!(out, "", "版本两行在标准错误上，标准输出该是空的");
    assert_eq!(err, format!("{VERSION}{USAGE}"));
    // `-v` exits inside the loop: the `Z` later in the same token is never scanned.
    let (_, code, err) = run(&["-vZ"], b"");
    assert_eq!(code, 3);
    assert_eq!(
        err,
        format!("{VERSION}{USAGE}"),
        "-v 之后不该再有集外字母那一行"
    );
    // Conversely `-Zv` has only the off-charset line plus usage; the version lines
    // never print.
    let (_, code, err) = run(&["-Zv"], b"");
    assert_eq!(code, 3);
    assert_eq!(err, format!("units: unknown option -- Z\n{USAGE}"));
    // `-f <file> -v` is the same: `-v` precedes the operand-count check, so even after
    // reading a bad value the version wins.
    let (_dir, db) = database("version-after-f");
    let (_, code, err) = run(&["-f", &db, "-v"], b"");
    assert_eq!(code, 3);
    assert_eq!(err, format!("{VERSION}{USAGE}"));
}

#[test]
fn the_operand_count_check_runs_before_the_database_read() {
    let (_dir, db) = database("count-before-read");
    // One operand: C calls usage at `units.c:849-851`, without even opening the file
    // name.
    let (out, code, err) = run(&["-f", &db, "m"], b"");
    assert_eq!(code, 3);
    assert_eq!(err, USAGE, "旧写法把这形拖到读库之后，坏档时会先印开档失败");
    assert_eq!(out, "");
    // Four operands take the same branch.
    let (_, code, err) = run(&["-f", &db, "m", "sec", "km", "cm"], b"");
    assert_eq!(code, 3);
    assert_eq!(err, USAGE);
    // Bad file + a legal operand count: only then does the open-failure sentence come
    // into play.
    let (_, code, err) = run(&["-f", "/definitely-not-here", "m", "sec"], b"");
    assert_eq!(code, 1);
    assert_eq!(
        err,
        "units: Unable to open units file '/definitely-not-here': No such file or directory\n"
    );
}

#[test]
fn default_database_failure_is_the_errx_two_part_shape() {
    // Without `-f`, C looks for `_PATH_UNITSLIB` (`minix3/usr.bin/units/pathnames.h`);
    // if it is missing the path goes through `errx`: the sentence names the file but
    // **does not** include `strerror`, unlike the three-part shape of the `-f` branch.
    for args in [&["--"][..], &["-q"][..], &["m", "-l", "sec"][..]] {
        let (out, code, err) = run(args, b"");
        assert_eq!(code, 1, "{args:?}");
        assert_eq!(
            err, "units: Can't find units file '/usr/share/misc/units.lib'\n",
            "{args:?}"
        );
        assert_eq!(out, "", "{args:?} 那句在标准错误上");
    }
}

#[test]
fn double_dash_is_consumed_and_bare_dash_is_an_operand() {
    // `--` ends the loop at `getopt.c:81-85` and is itself consumed, so `-- -Z` leaves
    // only one operand: the wrong-count branch, with only the usage block and no libc
    // off-charset line.
    let (_, code, err) = run(&["--", "-Z"], b"");
    assert_eq!(code, 3);
    assert_eq!(err, USAGE);
    // A bare `-` ends the loop without being consumed (`:87-92`), so it is an operand;
    // `-f` takes the next token regardless, so `-f - m sec` uses `-` as the file name.
    let (_, code, err) = run(&["-f", "-", "m", "sec"], b"");
    assert_eq!(code, 1);
    assert_eq!(
        err,
        "units: Unable to open units file '-': No such file or directory\n"
    );
}

#[test]
fn list_mode_rejects_operands_and_lists_the_tiny_database() {
    let (_dir, db) = database("list");
    let (_, code, err) = run(&["-l", "-f", &db, "m", "sec"], b"");
    assert_eq!(code, 3);
    assert_eq!(
        err, USAGE,
        "`list && argc != 0` 与个数不对同走 usage 一支，没有 libc 行"
    );
    // Only without operands does it enter list mode; this segment is where the
    // decisive half overlaps the external shape, and the reference measured exactly
    // this tiny dictionary.
    let (out, code, err) = run(&["-l", "-f", &db], b"");
    assert_eq!(code, 0, "列表里有坏定义才退 1，这份词典没有");
    assert_eq!(err, "");
    assert_eq!(
        out,
        "/ Primitive units\nm\t!a!\nsec\t!c!\n/ Prefixes\n/ Other units\n\
         km\t1000*m\ncm\t.01*m\nmin\t60*sec\n"
    );
}

#[test]
fn ring_does_not_rearrange_the_tail() {
    // The loop ends at the first non-option token (`getopt.c:74-78`), so an option
    // written after the operands is **not an option**. This shape has four operands
    // (`m`, `sec`, `-l`, the file name) and takes the wrong-count branch: only the
    // usage block, exit 3. If anyone rearranged the arguments, `-l` would be consumed
    // as an option and only two operands would remain, changing the exit code to 0.
    let (_dir, db) = database("rearrange");
    let (out, code, err) = run(&["m", "sec", "-l", &db], b"");
    assert_eq!(
        code, 3,
        "旧写法会把 `-l` 与 `-f` 捞回环内，于是这形变成合法的两个操作数"
    );
    assert_eq!(err, USAGE);
    assert_eq!(out, "");
    // With three operands that path treats the tail as unit names: in
    // `units -f <file> m sec -l`, `-l` is a want-side name and the message is C's
    // `Unknown unit`, **not** a listing (the old form exited 1 here without any
    // sentence).
    let (out, code, err) = run(&["-f", &db, "m", "sec", "-l"], b"");
    assert_eq!(code, 0, "C 忽略换算失败照样退 0");
    assert!(
        !out.contains("/ Primitive units"),
        "这一形不该进列表模式，stdout={out:?}"
    );
    assert!(
        err.starts_with("units: Unknown unit '") && err.ends_with("'\n"),
        "参照件报的是 `units: Unknown unit '-l'`；本壳的名字段少了前导短横，\
         那一处失真登记为 UNITS-LEADING-DASH-NAME"
    );
}

#[test]
fn unknown_operand_name_gets_the_c_warning_and_exit_zero() {
    // Reference: `units -f <file> bogus m` prints `units: Unknown unit 'bogus'`
    // (stderr, two-part shape) and still produces an answer block, exit 0. The answer
    // block is the decisive half's "partial display", registered as
    // UNITS-PARTIAL-UNIT-ON-ERROR; this face pins only the sentence and the exit code.
    let (_dir, db) = database("unknown-name");
    let (_, code, err) = run(&["-f", &db, "bogus", "m"], b"");
    assert_eq!(code, 0);
    assert_eq!(err, "units: Unknown unit 'bogus'\n");
    // When both sides are unknown, one line each is printed in have→want order (the
    // `tmp/units-319/measure4.log` shape of the reference: `Unknown unit 'bogus'` then
    // `Unknown unit 'nope'`).
    let (_, code, err) = run(&["-f", &db, "bogus", "nope"], b"");
    assert_eq!(code, 0);
    assert_eq!(
        err, "units: Unknown unit 'bogus'\nunits: Unknown unit 'nope'\n",
        "两句的顺序照 C 的先 have 后 want"
    );
}

#[test]
fn interactive_pair_reads_stdin_and_prompts_in_order() {
    // This is the face for the tenth read-seam defect of the family: the old form read
    // stdin with a bare `minix_sys::read`, which on the host reads not one byte, so the
    // interactive face degenerated to "print prompt → immediate EOF".
    let (_dir, db) = database("interactive");
    let (out, code, err) = run(&["-f", &db], b"km\ncm\n");
    assert_eq!(code, 0);
    assert_eq!(err, "");
    assert_eq!(
        out,
        "5 units, 0 prefixes\n\nYou have: You want: \t* 100000\n\t/ 1e-05\nYou have: \n"
    );
}

#[test]
fn unknown_have_line_reprompts_the_same_slot() {
    // C's `do { prompt; fgets; } while (addunit || completereduce)`: a bad line
    // re-asks **the same slot**. The old form treated two reads as a have/want pair,
    // so `bogus` consumed the want slot.
    let (_dir, db) = database("reprompt");
    let (out, code, err) = run(&["-f", &db], b"bogus\nm\nsec\n");
    assert_eq!(code, 0);
    assert_eq!(err, "units: Unknown unit 'bogus'\n");
    assert!(
        out.starts_with("5 units, 0 prefixes\n\nYou have: You have: You want: "),
        "坏的一行该重问 have，实际 stdout={out:?}"
    );
    assert!(
        out.ends_with("You have: \n"),
        "成对之后还要再问一次 have，实际 {out:?}"
    );
}

#[test]
fn eof_mid_pair_prints_a_newline_and_exits_zero() {
    let (_dir, db) = database("eof-mid");
    // have was read and the want slot hits EOF: when **not `-q`**, C emits a newline
    // then exits 0 without an answer block.
    let (out, code, err) = run(&["-f", &db], b"m\n");
    assert_eq!(code, 0);
    assert_eq!(err, "");
    assert_eq!(out, "5 units, 0 prefixes\n\nYou have: You want: \n");
    // EOF right away: only the single have prompt plus the newline.
    let (out, code, _) = run(&["-f", &db], b"");
    assert_eq!(code, 0);
    assert_eq!(out, "5 units, 0 prefixes\n\nYou have: \n");
}

#[test]
fn quiet_suppresses_prompts_and_the_counts_line() {
    let (_dir, db) = database("quiet");
    // Under `-q` neither the prompts nor the counts print, nor the EOF newline — all
    // three channels empty, exit 0.
    let (out, code, err) = run(&["-q", "-f", &db], b"");
    assert_eq!(code, 0);
    assert_eq!(out, "", "-q 下提示、counts 与 EOF 换行都不打");
    assert_eq!(err, "");
    let (out, code, err) = run(&["-q", "-f", &db], b"km\ncm\n");
    assert_eq!(code, 0);
    assert_eq!(err, "");
    assert_eq!(out, "\t* 100000\n\t/ 1e-05\n");
}

#[test]
fn long_input_line_truncates_at_seventy_nine_bytes() {
    // C uses `fgets(havestr, 80, stdin)`: a line is at most 79 bytes, and the rest
    // (including the newline) goes to the next read. For 100 `x`s the reference
    // reports segments of 79 and 21, then treats `km`/`cm` as the second and third
    // reads.
    let (_dir, db) = database("long-line");
    let input = format!("{}\nkm\ncm\n", "x".repeat(100));
    let (out, code, err) = run(&["-f", &db], input.as_bytes());
    assert_eq!(code, 0);
    assert_eq!(
        err,
        format!(
            "units: Unknown unit '{}'\nunits: Unknown unit '{}'\n",
            "x".repeat(79),
            "x".repeat(21)
        )
    );
    assert!(
        out.starts_with("5 units, 0 prefixes\n\nYou have: You have: You have: You want: "),
        "长行切两段各重问一次 have，实际 stdout={out:?}"
    );
}

#[test]
fn blank_line_is_a_valid_dimensionless_pair() {
    // A blank line is not "skipped": C hands it to `addunit`, yielding the
    // dimensionless 1, so the answer is `* 1` `/ 1`.
    let (_dir, db) = database("blank");
    let (out, code, err) = run(&["-f", &db], b"\n\n");
    assert_eq!(code, 0);
    assert_eq!(err, "");
    assert_eq!(
        out,
        "5 units, 0 prefixes\n\nYou have: You want: \t* 1\n\t/ 1\nYou have: \n"
    );
}

#[test]
fn colon_in_the_optstring_is_a_marker_not_a_selectable_letter() {
    // Measured against the reference (`tmp/units-319/measure5.log`): `units -:` and
    // `units -l:` both print `units: unknown option -- :` followed by usage, exit 3;
    // `units -f:` uses the colon itself as the file name.
    for args in [&["-:"][..], &["-l:"][..]] {
        let (_, code, err) = run(args, b"");
        assert_eq!(code, 3, "{args:?}");
        assert_eq!(
            err,
            format!("units: unknown option -- :\n{USAGE}"),
            "{args:?}"
        );
    }
    let (_, code, err) = run(&["-f:"], b"");
    assert_eq!(code, 1);
    assert_eq!(
        err,
        "units: Unable to open units file ':': No such file or directory\n"
    );
}

#[test]
fn listexpand_flag_expands_definitions_in_the_listing() {
    // `-L` is a second letter, not a case variant of `-l`: besides entering list mode
    // it sets `listexpand`, so definition lines are reduced to decimal factors (the
    // reference shows `km\t1000 m`), whereas `-l` passes `km\t1000*m` through
    // unchanged.
    let (_dir, db) = database("listexpand");
    let (out, code, err) = run(&["-L", "-f", &db], b"");
    assert_eq!(code, 0);
    assert_eq!(err, "");
    assert_eq!(
        out,
        "/ Primitive units\nm\t!a!\nsec\t!c!\n/ Prefixes\n/ Other units\n\
km\t1000 m\ncm\t0.01 m\nmin\t60 sec\n"
    );
}

#[test]
fn off_letter_outside_the_charset_is_reported_by_its_first_byte() {
    // For `-é` the reference reports **the first byte** (libc's `%c` prints that one
    // byte): `tmp/units-319/units -é` and `-qé` both print
    // `units: unknown option -- ` + `c3` + newline + the nine-line usage, exit 3.
    // This shell's cursor already walks bytes (so no `column -té`-style panic past a
    // character boundary); the only thing that used to differ was rendering: printing
    // by `char` gives the Latin-1 code point U+00C3, i.e. the two bytes `c3 83` —
    // registered as `UNITS-NONASCII-OPTLETTER` and closed in batch 399 by switching to
    // `GetoptReport::line_bytes`, so the assertions are now byte-wise.
    // Text fixtures no longer suffice here: `String::from_utf8_lossy` renders both a
    // raw `c3` and `c3 83` as the replacement character, so this compares raw stderr
    // bytes.
    let want: Vec<u8> = [
        b"units: unknown option -- ".as_slice(),
        b"\xc3\n",
        USAGE.as_bytes(),
    ]
    .concat();
    for form in ["-\u{e9}", "-q\u{e9}"] {
        let out = Command::new(UNITS)
            .args([form])
            .stdin(Stdio::null())
            .stderr(Stdio::piped())
            .output()
            .expect("units 该能起");
        assert_eq!(out.stdout, Vec::new(), "{form} 的报文只走 stderr");
        assert_eq!(out.status.code(), Some(3), "{form} 走 usage 那支退 3");
        assert_eq!(out.stderr, want, "{form} 的字母该是一个裸字节 c3");
    }

    // For the ASCII shape the two entry points match exactly
    let (_, code, err) = run(&["-Z"], b"");
    assert_eq!(code, 3);
    assert_eq!(err, format!("units: unknown option -- Z\n{USAGE}"));
}

/// Byte-wise fixture leg: `run` returns both output streams as lossy `String`s, and
/// text cannot distinguish a raw `ff` from the folded `ef bf bd` — which is exactly
/// what the two faces in this file compare (same as `col_face`'s `run_bytes`).
fn run_bytes(args: &[&str], input: &[u8]) -> (Vec<u8>, Vec<u8>, Option<i32>) {
    let mut child = Command::new(UNITS)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the units binary should be runnable");
    write_stdin(&mut child, input);
    let output = child.wait_with_output().expect("the run should finish");
    (output.stdout, output.stderr, output.status.code())
}

#[test]
fn a_raw_byte_in_a_dictionary_name_reaches_the_listing() {
    // Dictionary-line names and definitions enter the table as bytes (C's
    // `fgets(line, 79, unitfile)` never decodes, and `readunits` splits bytes with
    // `strspn`/`strcspn`). The old form first decoded the **whole** file as UTF-8 on
    // the shell side and treated failure as an empty string — a single raw byte could
    // make every unit disappear; registered as `UNITS-NONUTF8-DROP` and closed by
    // switching the unit-name path to the byte form.
    let dir = scratch("raw-name-listing");
    let path = dir.join("raw.lib");
    fs::write(
        &path,
        [
            b"m\t!a!\n".as_slice(),
            b"caf\xc3\xa9\t2 m\n".as_slice(),
            b"weird\xff\t3 m\n".as_slice(),
        ]
        .concat(),
    )
    .expect("write raw.lib");
    let name = path.to_str().expect("utf-8 scratch path");
    let (out, err, code) = run_bytes(&["-l", "-f", name], b"");
    assert_eq!(code, Some(0), "两份词典行都在表里，退码不该是错误");
    assert_eq!(err, Vec::new(), "-l 的句子走 stdout 且本形没有句子");
    assert_eq!(
        out,
        [
            b"/ Primitive units\nm\t!a!\n/ Prefixes\n/ Other units\n".as_slice(),
            b"caf\xc3\xa9\t2 m\n".as_slice(),
            b"weird\xff\t3 m\n".as_slice(),
        ]
        .concat(),
        "清单里的名字该是词典里那串原字节"
    );
}

/// Byte-form fixture for the argv shape: a raw `ff` cannot be written as `&str`, so
/// arguments are passed as bytes (`OsStr::from_bytes`); this compares the exact byte
/// string the child process received.
fn run_argv(args: &[&[u8]], input: &[u8]) -> (Vec<u8>, Vec<u8>, Option<i32>) {
    use std::os::unix::ffi::OsStrExt as _;
    let mut child = Command::new(UNITS)
        .args(args.iter().map(|a| std::ffi::OsStr::from_bytes(a)))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the units binary should be runnable");
    write_stdin(&mut child, input);
    let output = child.wait_with_output().expect("the run should finish");
    (output.stdout, output.stderr, output.status.code())
}

#[test]
fn an_operand_with_a_raw_byte_is_warned_about_in_raw_bytes() {
    // An operand is a unit-name expression; C hands `argv` to `addunit` unchanged at
    // `units.c:855-875`, and `mywarnx("Unknown unit '%s'", *product)` echoes it byte
    // by byte when unknown. The argument-seam half previously folded it to U+FFFD
    // (three bytes) inside `support::args()`, leaving this shell three bytes off from
    // the reference.
    let (_dir, db) = database("raw-operand");
    let operand = [b"weird".as_slice(), b"\xff"].concat();
    let args: Vec<&[u8]> = vec![
        b"-f".as_slice(),
        db.as_bytes(),
        operand.as_slice(),
        b"m".as_slice(),
    ];
    let (_, err, code) = run_argv(&args, b"");
    assert_eq!(code, Some(0), "C 不看推导返回值，坏名也退 0");
    assert_eq!(
        err,
        [
            b"units: Unknown unit 'weird".as_slice(),
            b"\xff'\n".as_slice(),
        ]
        .concat(),
        "回显位该是 argv 里那一枚裸 ff"
    );
}

#[test]
fn a_raw_byte_in_a_typed_name_comes_back_in_the_warning() {
    // Bytes from the interactive line enter the unit-name path unchanged: both
    // `addunit`'s `strtok` and `lookupunit`'s `strcmp` work on bytes, and
    // `mywarnx("Unknown unit '%s'")` (`units.c:554`) returns the name via `%s`. The
    // old form folded once with `from_utf8_lossy` on the shell side, so a raw `c3`
    // became three replacement bytes and a raw `ff` became `ef bf bd` — a whole
    // string's worth of difference from the reference.
    let (_dir, db) = database("raw-name-warning");
    let (out, err, code) = run_bytes(
        &["-q", "-f", &db],
        &[b"caf\xc3\xa9\n".as_slice(), b"weird\xff\n".as_slice()].concat(),
    );
    assert_eq!(code, Some(0), "EOF 退 0（`units.c:876-893`）");
    assert_eq!(out, Vec::new(), "-q 不打提示也不出答案块");
    assert_eq!(
        err,
        [
            b"units: Unknown unit 'caf\xc3\xa9'\n".as_slice(),
            b"units: Unknown unit 'weird\xff'\n".as_slice(),
        ]
        .concat(),
        "回显位该逐字节交出用户打的那串字节"
    );
}
