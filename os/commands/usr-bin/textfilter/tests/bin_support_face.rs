//! The argument channel's door: when invalid UTF-8 bytes appear in argv, the host leg
//! and the target leg must take the same step.
//!
//! The seam is `os/commands/usr-bin/textfilter/src/bin_support.rs:args` — a pair of
//! `cfg` twins: the target side renders the raw bytes handed over by
//! `minix_rt::crt0::args()` lossily, while the host side used `std::env::args()`,
//! which panics (`SIGABRT`) on invalid bytes. C treats argv as raw bytes and never
//! validates them (`minix3/lib/libc/stdlib/getopt.c:79` reads bytes), so "die on the
//! first invalid byte" is neither C's shape nor the target side's shape.
//!
//! Every message pinned below was taken from measurements: before the change all six
//! affected commands exited with `rc=-6`, afterwards the raw bytes match the
//! reference builds (`tr`, `seq`, `paste`).
//!
//! **Where a difference from C remains, these doors do not hide it**: C prints the
//! raw byte `ff` alone on the line (`tr_ref: unknown option -- \xff`), while the
//! text seam first renders the command line lossily into U+FFFD, and each command
//! then renders it its own way: a byte-wise reader sees `ef` (the first byte of
//! U+FFFD), a `char`-wise reader that renders it sees the two bytes `c3 af`.
//! The argument-seam half turns that bit right command by command — the twenty-two
//! rows of this table (`rev`, `paste`, `seq`, `tr`, `fold`, `col`, `uniq`, `units`,
//! `cksum`, `cmp`, `column`, `comm`, `cut`, `head`, `join`, `jot`, `nl`, `pr`,
//! `sort`, `split`, `tail`, `unexpand`) **all print the raw byte** `ff` by now,
//! matching the reference builds; the shells outside this table that still use the
//! text seam keep returning `ef`.
//! Beyond the option letter there is also an **operand-as-file-path** leg: a path
//! that enters a `&str` interface inside `os/libs` must be folded lossily once.
//! The assertions below therefore pin both "no longer aborts, both legs agree" and
//! **the exact byte sequence at present**: when those legs land, these doors turn red
//! on the spot, and whoever changes them must flip them deliberately rather than
//! letting the door drift silently.

use std::ffi::OsStr;
use std::os::unix::ffi::OsStrExt as _;
use std::process::{Command, Stdio};

const TR: &str = env!("CARGO_BIN_EXE_tr");
const SEQ: &str = env!("CARGO_BIN_EXE_seq");
const PASTE: &str = env!("CARGO_BIN_EXE_paste");
const FOLD: &str = env!("CARGO_BIN_EXE_fold");
const REV: &str = env!("CARGO_BIN_EXE_rev");
const COL: &str = env!("CARGO_BIN_EXE_col");
const UNIQ: &str = env!("CARGO_BIN_EXE_uniq");
const UNITS: &str = env!("CARGO_BIN_EXE_units");
const CKSUM: &str = env!("CARGO_BIN_EXE_cksum");
const CMP: &str = env!("CARGO_BIN_EXE_cmp");
const COLUMN: &str = env!("CARGO_BIN_EXE_column");
const COMM: &str = env!("CARGO_BIN_EXE_comm");
const CUT: &str = env!("CARGO_BIN_EXE_cut");
const HEAD: &str = env!("CARGO_BIN_EXE_head");
const JOIN: &str = env!("CARGO_BIN_EXE_join");
const JOT: &str = env!("CARGO_BIN_EXE_jot");
const NL: &str = env!("CARGO_BIN_EXE_nl");
const PR: &str = env!("CARGO_BIN_EXE_pr");
const SORT: &str = env!("CARGO_BIN_EXE_sort");
const SPLIT: &str = env!("CARGO_BIN_EXE_split");
const TAIL: &str = env!("CARGO_BIN_EXE_tail");
const UNEXPAND: &str = env!("CARGO_BIN_EXE_unexpand");

/// A bare `0xFF`: it is not the prefix of any UTF-8 sequence, and it cannot occur in
/// a valid argv.
const FF: u8 = 0xFF;

/// Spawn a real child process on the host side; argv may contain raw bytes. Invalid
/// bytes cannot enter a `&str`, so no shared fixture that takes `str` arguments is
/// used here; standard input is fed the way the fixtures do it, and a failed write
/// (the parent gets `BrokenPipe` when the command does not read stdin) is not counted
/// as a defect in the code under test.
fn run(exe: &str, args: &[&[u8]], input: &[u8]) -> (i32, Vec<u8>, Vec<u8>) {
    let mut child = Command::new(exe)
        .args(args.iter().map(|a| OsStr::from_bytes(a)))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("起子进程");
    {
        use std::io::Write as _;
        if let Some(mut stdin) = child.stdin.take() {
            let _ = stdin.write_all(input);
            // The handle closes here, so the child sees EOF right after.
        }
    }
    let output = child.wait_with_output().expect("等子进程");
    let code = output
        .status
        .code()
        .expect("退出码该在——被信号杀掉就是双生面又分叉了");
    (code, output.stdout, output.stderr)
}

/// None of the twenty-two thin shells should take the signal path on an invalid byte
/// any more, and the first line is still the getopt message. The option-letter column
/// is this seam's gauge: a shell on `bin_support::args_bytes` returns the **raw byte**
/// `ff`, while a shell still on the text seam returns `ef`, the first byte of the
/// folded U+FFFD.
#[test]
fn no_command_aborts_on_a_nonutf8_option_letter() {
    let flag = [b"-".as_slice(), &[FF][..]].concat();
    let cases: [(&str, &str, i32, &[u8]); 22] = [
        // tr, seq, paste, rev, fold, col and uniq all go through
        // `bin_support::args_bytes`, so the option letter is the raw byte `ff`.
        // The seq entry pins one more layer: its ring accumulates, so a single bare
        // byte should report one line only
        // (`the_accumulating_ring_names_the_raw_byte_once`).
        ("tr", TR, 1, b"tr: unknown option -- \xff\n"),
        ("seq", SEQ, 1, b"seq: unknown option -- \xff\n"),
        ("paste", PASTE, 1, b"paste: unknown option -- \xff\n"),
        ("fold", FOLD, 1, b"fold: unknown option -- \xff\n"),
        ("rev", REV, 1, b"rev: unknown option -- \xff\n"),
        ("col", COL, 1, b"col: unknown option -- \xff\n"),
        // The leftover `ef` is gone now that the argv byte seam is in place:
        // `from_utf8_lossy` folded a bare `ff` into U+FFFD at the argument seam, so
        // the option letter looked like `ef`; with `args_bytes` the ring receives the
        // bare `ff` itself, matching C.
        ("uniq", UNIQ, 1, b"uniq: unknown option -- \xff\n"),
        ("units", UNITS, 3, b"units: unknown option -- \xff\n"),
        // cksum: C's `usage()` exits with `exit(1)` (`cksum.c:549`), unlike `units`'
        // `myexit(3)`, so this row's exit code is still 1.
        ("cksum", CKSUM, 1, b"cksum: unknown option -- \xff\n"),
        // cmp: C's `usage()` exits with `ERR_EXIT` = **2** (`cmp.c:157`), unlike
        // cksum's `exit(1)`, so this row pins 2 rather than copying the row above.
        ("cmp", CMP, 2, b"cmp: unknown option -- \xff\n"),
        // column: its `usage()` exits 1 (`exit(1)` at `column.c:302-307`).
        ("column", COLUMN, 1, b"column: unknown option -- \xff\n"),
        // comm: `usage()` at `comm.c:195-199` also exits 1, but its program name is
        // **hardcoded in the string** (no `getprogname()`); this row pins the option
        // letter of the libc line — the two lines have different origins, see
        // `comm_face`'s `comm_reports_the_raw_option_byte_before_the_hardcoded_usage_line`.
        ("comm", COMM, 1, b"comm: unknown option -- \xff\n"),
        // cut: its usage is three lines and exits 1 (`cut.c:290-296`), and those
        // three lines have no program-name placeholder at all — this row pins only
        // the option letter of the libc line.
        ("cut", CUT, 1, b"cut: unknown option -- \xff\n"),
        // head: its usage is a single line carrying the program name
        // (`head.c:199-203` passes `getprogname()` as `%s`, unlike the hardcoded
        // command names in `cut`/`comm`), and exits 1.
        ("head", HEAD, 1, b"head: unknown option -- \xff\n"),
        // join: its usage is **two lines** and exits 1 (`join.c:630-637`), while the
        // option letter was already taken byte-wise (`char::from(b[1])` in
        // `src/join.rs`) and rendered byte-wise (`line_bytes`) — this row fixes the
        // argv end.
        ("join", JOIN, 1, b"join: unknown option -- \xff\n"),
        // jot: its `usage()` exits 1 (`jot.c:314-319`, ending in `exit(1)`), while the
        // option letter (`char::from(bytes[pos])` in `src/jot.rs`) and the rendering
        // (`line_bytes`) were already byte-wise — this row fixes the argv end, i.e.
        // `support::args()` becomes `args_bytes()`.
        ("jot", JOT, 1, b"jot: unknown option -- \xff\n"),
        // nl: its `usage()` is **two lines** (the first ends in `\n\t`) and exits 1
        // (`nl.c:400-405`); this command's option ring lives entirely in the thin
        // shell, so the option letter and the seven echoes are pinned by `nl_face`;
        // this row pins only "no abort + the option letter is the raw byte".
        ("nl", NL, 1, b"nl: unknown option -- \xff\n"),
        // pr: this row uses **different words from the rest of the family** — `pr`
        // does not use libc's `getopt` but links its own
        // `minix3/usr.bin/pr/egetopt.c`, whose `:148` prints `illegal option -- %c`
        // (libc prints `unknown option` at `getopt.c:103`). The same bare byte yields a
        // different sentence under the two parsers, so flattening it under a "family
        // rule" would get the wording wrong; this row pins this command's own line (the
        // option letter is still the byte `ff`), while the three usage lines and the
        // ten spaces are carried by `pr_face`'s
        // `the_four_invalid_value_echoes_and_the_operand_message_carry_the_argv_bytes`
        // and `letters_outside_the_c_optstring_print_the_egetopt_line_then_the_usage`.
        ("pr", PR, 1, b"pr: illegal option -- \xff\n"),
        // sort: the wording is back on libc's side — `sort.c:156` calls
        // `getopt(argc, argv, SORT_OPTS)` (`unknown option`, `getopt.c:101-104`),
        // which contrasts with the `illegal option` of `pr`'s own `egetopt`; two
        // wordings coexist in one table, concrete evidence that a "family rule" would
        // get the sentence wrong. Its usage is **one line** with a run of thirteen
        // spaces inside (`sort.c:399-412`), and it exits **2**, not 1.
        ("sort", SORT, 2, b"sort: unknown option -- \xff\n"),
        // split: `split.c:80` also uses libc's
        // `getopt(argc, argv, "0123456789b:l:a:n:")`, so the sentence is again
        // `unknown option` (contrast the `illegal option` of `pr`). Its usage is
        // **one line** and exits **1** (`exit(1)` at `split.c:355-361`); the three
        // legs beyond the option letter (the value positions of the four
        // `illegal X count.` sentences, the reported chunk name and the input file
        // name) are carried by `split_face`'s
        // `the_diagnostic_and_the_chunk_name_carry_the_argv_bytes`.
        ("split", SPLIT, 1, b"split: unknown option -- \xff\n"),
        // tail: `tail.c:108`'s `getopt(argc, argv, "Fb:c:fn:r")` is libc's, so the
        // sentence is again `unknown option`. Its usage is **one line**, capitalized
        // `Usage:`, carrying the program name (`tail.c:299-305`), and exits 1. The
        // four legs beyond the option letter (the value positions of the three
        // `illegal offset -- %s` sentences, the whole token carried by `obsolete()`'s
        // `illegal option -- %s`, the rewritten token, and the file name in the
        // diagnostic line) are carried by `tail_face`'s
        // `the_offset_echo_the_token_echo_and_the_file_name_carry_the_argv_bytes`.
        ("tail", TAIL, 1, b"tail: unknown option -- \xff\n"),
        // unexpand: `unexpand.c:86`'s `getopt(argc, argv, "at:")` is libc's, so the
        // sentence is again `unknown option`; its usage is one line, capitalized
        // `Usage:`, and exits 1 (`unexpand.c:69-73`). The six tabstop echoes beyond
        // the option letter and the file-name position of the operand's `Cannot open`
        // message are carried by `unexpand_face`'s
        // `the_tabstop_echoes_and_the_operand_name_carry_the_argv_bytes`.
        (
            "unexpand",
            UNEXPAND,
            1,
            b"unexpand: unknown option -- \xff\n",
        ),
    ];
    for (name, exe, want_code, want_first) in cases {
        let (code, _out, err) = run(exe, &[&flag], b"aabbcc\n");
        assert_eq!(code, want_code, "{name} 在非法字节上的退码");
        assert!(
            err.starts_with(want_first),
            "{name} 的 libc 那句该是 {want_first:?}，实际 {err:?}"
        );
    }
}

/// An invalid byte inside an **operand** is ordinary data: the reference build's
/// `tr a\xffb c` exits 0 and prints `cccccc`, and this layer matches. When an operand
/// is taken as data, the text seam and the byte seam produce the same bytes, so this
/// test pins "does not blow up", not the option letter — the latter belongs to the
/// table above.
#[test]
fn a_raw_byte_inside_an_operand_is_ordinary_data() {
    let set1 = [b"a".as_slice(), &[FF][..], b"b"].concat();
    let (code, out, err) = run(TR, &[&set1, b"c"], b"aabbcc\n");
    assert_eq!(code, 0, "实际 stderr {err:?}");
    assert_eq!(out, b"cccccc\n");
    assert!(err.is_empty(), "实际 {err:?}");
}

/// When `-d` **consumes the invalid byte as a value** that is not an error at all:
/// the reference build's `paste -d\xff` exits 0 with no output (the zero-operand
/// rule: it neither reads standard input nor reports an error).
#[test]
fn a_raw_byte_consumed_as_an_option_value_is_not_an_error() {
    let dflag = [b"-d".as_slice(), &[FF][..]].concat();
    let (code, out, err) = run(PASTE, &[&dflag], b"aabbcc\n");
    assert_eq!(code, 0, "实际 stderr {err:?}");
    assert!(out.is_empty(), "实际 {out:?}");
    assert!(err.is_empty(), "实际 {err:?}");
}

/// The accumulating option ring (`seq.c:101-113` accumulates `errflg` instead of
/// stopping at the first bad letter) on the **text seam** reports the three lossily
/// produced bytes as three letters on three lines; with `seq` on
/// `bin_support::args_bytes` the bare `ff` on the command line is a single letter, so
/// the message matches the reference build: **one line, the option letter being the
/// raw byte**, followed by one usage line. This pins the shape after the change so
/// that switching the seam back to `String` cannot quietly bring back three lines.
#[test]
fn the_accumulating_ring_names_the_raw_byte_once() {
    let flag = [b"-".as_slice(), &[FF][..]].concat();
    let usage = b"usage: seq [-w] [-f format] [-s string] [-t string] [first [incr]] last\n";
    let (code, _out, err) = run(SEQ, &[&flag], b"aabbcc\n");
    assert_eq!(code, 1);
    assert_eq!(
        err,
        [b"seq: unknown option -- \xff\n".as_slice(), usage].concat()
    );
}
