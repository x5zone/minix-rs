//! The argument-channel face: when argv carries **invalid UTF-8 bytes**, the host
//! leg must take the same step as the target leg.
//!
//! The seam is `os/commands/usr-bin/textfilter/src/bin_support.rs:args` — a pair of
//! `cfg` twins: the target side renders the raw bytes handed over by
//! `minix_rt::crt0::args()` lossily, while the host side used to call
//! `std::env::args()`, which panics (`SIGABRT`) on an invalid byte. C treats argv as
//! raw bytes and never validates it (`minix3/lib/libc/stdlib/getopt.c:79` takes a
//! byte), so "die on the first invalid byte" is neither C's shape nor the target
//! side's shape.
//!
//! Every message pinned below comes from a direct reading:
//! `tmp/bin-support-329/measure.log` (before the fix: all six binaries `rc=-6`) and
//! `tmp/bin-support-329/measure_after.log` (after); the reference readings are
//! `tmp/tr-322/tr_ref`, `tmp/seq-320/seq_ref`, `tmp/paste-321/paste_ref`.
//!
//! **Where this still differs from C, these faces do not hide it**: C prints the raw
//! byte `ff` in one line (`tr_ref: unknown option -- \xff`), while the text seam
//! first folds the command line into U+FFFD and each shell then renders according to
//! its own reader: a byte-wise reader gets `ef` (the first byte of U+FFFD), and a
//! `char`-wise reader that re-renders gets the two bytes `c3 af`. The argument-seam
//! work turned this slot right shell by shell — this table's rows (`rev`, `paste`,
//! `seq`, `tr`, `fold`, `col`, `uniq`, `units`, `cksum`, `cmp`, `column`, `comm`,
//! `cut`, `head`, `join`, `jot`, `nl`, `pr`, `sort`, `split`, `tail`, `unexpand`,
//! `unifdef`, `vis`) **are all the raw byte** `ff` now, the same shape as the
//! reference; shells outside the table that still use the text seam still hand back
//! `ef`.
//! Besides the letter slot there is a **file-path operand** leg: once a path enters
//! the `&str` entry in `os/libs`, it must be folded (registered under
//! `ARGV-BYTE-PATH-IN-LIBS`). So the assertions below pin both "no more abort, the
//! two legs agree" and **the byte string as it is today**: when those legs land,
//! these tests will turn red on purpose, to be corrected deliberately by whoever
//! makes the change rather than letting the face drift silently.

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
const UNIFDEF: &str = env!("CARGO_BIN_EXE_unifdef");
const VIS: &str = env!("CARGO_BIN_EXE_vis");
const WC: &str = env!("CARGO_BIN_EXE_wc");
const EXPAND: &str = env!("CARGO_BIN_EXE_expand");
const DIFF: &str = env!("CARGO_BIN_EXE_diff");
const HEXDUMP: &str = env!("CARGO_BIN_EXE_hexdump");
const UNVIS: &str = env!("CARGO_BIN_EXE_unvis");
const ASA: &str = env!("CARGO_BIN_EXE_asa");
const TSORT: &str = env!("CARGO_BIN_EXE_tsort");
const LOOK: &str = env!("CARGO_BIN_EXE_look");
const LAM: &str = env!("CARGO_BIN_EXE_lam");
const PATCH: &str = env!("CARGO_BIN_EXE_patch");

/// A bare `0xFF`: neither a prefix of any UTF-8 sequence nor possible inside a valid
/// argv.
const FF: u8 = 0xFF;

/// Starts a real child process on the host side, with argv allowed to be raw bytes.
/// Invalid bytes cannot enter a `&str`, so this does not borrow any shared fixture
/// that takes `str` arguments; standard input is fed the way the fixtures do, and a
/// failed write (the parent gets `BrokenPipe` when the command does not read stdin)
/// is not counted as a defect in the code under test.
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
            // The handle closes here, so the child sees EOF right away.
        }
    }
    let output = child.wait_with_output().expect("等子进程");
    let code = output
        .status
        .code()
        .expect("退出码该在——被信号杀掉就是双生面又分叉了");
    (code, output.stdout, output.stderr)
}

/// None of the shells in the table may take the signal path on an invalid byte any
/// more, and the first line is still the getopt message. The letter column is this
/// seam's gauge: a shell on `bin_support::args_bytes` hands back the **raw byte**
/// `ff`, while a shell still on the text seam hands back `ef`, the first byte of the
/// folded U+FFFD.
#[test]
fn no_command_aborts_on_a_nonutf8_option_letter() {
    let flag = [b"-".as_slice(), &[FF][..]].concat();
    let cases: [(&str, &str, i32, &[u8]); 24] = [
        // tr, seq, paste, rev, fold, col and uniq all go through
        // `bin_support::args_bytes`, so the letter slot is the raw byte `ff`.
        // The seq row pins one more layer: its ring accumulates, and one raw byte
        // should produce exactly one line
        // (`the_accumulating_ring_names_the_raw_byte_once`).
        ("tr", TR, 1, b"tr: unknown option -- \xff\n"),
        ("seq", SEQ, 1, b"seq: unknown option -- \xff\n"),
        ("paste", PASTE, 1, b"paste: unknown option -- \xff\n"),
        ("fold", FOLD, 1, b"fold: unknown option -- \xff\n"),
        ("rev", REV, 1, b"rev: unknown option -- \xff\n"),
        ("col", COL, 1, b"col: unknown option -- \xff\n"),
        // The `ef` residue in units was turned right by its argv byte seam:
        // `from_utf8_lossy` folded a bare `ff` into U+FFFD on the argument seam, so
        // the letter slot looked like `ef`; with `args_bytes` the ring receives that
        // `ff` itself, the same shape as C.
        ("uniq", UNIQ, 1, b"uniq: unknown option -- \xff\n"),
        ("units", UNITS, 3, b"units: unknown option -- \xff\n"),
        // cksum: C's `usage()` is `exit(1)` (`cksum.c:549`), unlike `units`'
        // `myexit(3)`, so this row's exit status stays 1.
        ("cksum", CKSUM, 1, b"cksum: unknown option -- \xff\n"),
        // cmp: C's `usage()` exits with `ERR_EXIT` = **2** (`cmp.c:157`), unlike
        // cksum's `exit(1)`, so this row pins 2 rather than copying the row above.
        ("cmp", CMP, 2, b"cmp: unknown option -- \xff\n"),
        // column: its `usage()` exits 1 (`column.c:302-307`'s `exit(1)`).
        ("column", COLUMN, 1, b"column: unknown option -- \xff\n"),
        // comm: `comm.c:195-199`'s `usage()` also exits 1, but its line
        // **hardwires the program name** in the string (no `getprogname()`), while
        // this row pins the letter slot of the libc line — the two lines have
        // different origins, see `comm_face`'s
        // `comm_reports_the_raw_option_byte_before_the_hardcoded_usage_line`.
        ("comm", COMM, 1, b"comm: unknown option -- \xff\n"),
        // cut: its usage is three lines and exits 1 (`cut.c:290-296`), and those
        // three lines have no program-name slot at all — this row pins only the
        // letter slot of the libc line.
        ("cut", CUT, 1, b"cut: unknown option -- \xff\n"),
        // head: its usage is one line and carries the program name
        // (`head.c:199-203` passes `getprogname()` as `%s`, unlike the hardwired
        // names in `cut`/`comm`), exit 1.
        ("head", HEAD, 1, b"head: unknown option -- \xff\n"),
        // join: its usage is **two lines** and exits 1 (`join.c:630-637`), while the
        // letter reader (`src/join.rs`'s `char::from(b[1])`) and the rendering
        // (`line_bytes`) were already byte-wise — this row turned the argv side right.
        ("join", JOIN, 1, b"join: unknown option -- \xff\n"),
        // jot: its `usage()` exits 1 (`jot.c:314-319`, ending in `exit(1)`), while
        // this shell's reader (`src/jot.rs`'s `char::from(bytes[pos])`) and rendering
        // (`line_bytes`) were already byte-wise — this row turned the argv side right
        // by replacing `support::args()` with `args_bytes()`.
        ("jot", JOT, 1, b"jot: unknown option -- \xff\n"),
        // nl: its `usage()` is **two lines** (the first ends in `\n\t`) and exits 1
        // (`nl.c:400-405`); this shell's option ring lives entirely in the thin
        // shell, so the letter slot and the seven echoes are pinned by `nl_face`,
        // and this row pins only "no more abort + the letter slot is the raw byte".
        ("nl", NL, 1, b"nl: unknown option -- \xff\n"),
        // pr, whose row is **worded differently from the rest of the family**: `pr`
        // does not use libc's `getopt` but links its own
        // `minix3/usr.bin/pr/egetopt.c` — `:148` prints `illegal option -- %c`
        // (libc prints `unknown option` at `getopt.c:103`). The same raw byte really
        // does yield different sentences under the two parsers, and flattening that
        // with a "family rule" would get the wording wrong; this row pins that
        // shell's own line (the letter slot is still one byte, `ff`), while the three
        // usage lines and the ten spaces are pinned by `pr_face`'s
        // `the_four_invalid_value_echoes_and_the_operand_message_carry_the_argv_bytes`
        // and `letters_outside_the_c_optstring_print_the_egetopt_line_then_the_usage`.
        ("pr", PR, 1, b"pr: illegal option -- \xff\n"),
        // sort, where the wording returns to the libc branch: `sort.c:156` calls
        // `getopt(argc, argv, SORT_OPTS)` (`getopt.c:101-104`'s `unknown option`),
        // the counterpart to `pr`'s own `egetopt` printing `illegal option` on the
        // row above — two wordings side by side in one table are the concrete proof
        // that a "family rule" would get sentences wrong. Its usage is **one line**
        // with a run of thirteen spaces inside (`sort.c:399-412`), and it exits
        // **2**, not 1.
        ("sort", SORT, 2, b"sort: unknown option -- \xff\n"),
        // split: `split.c:80` also calls libc's
        // `getopt(argc, argv, "0123456789b:l:a:n:")`, so the sentence is still
        // `unknown option` (contrast `pr`'s `illegal option`). Its usage is **one
        // line** and exits **1** (`split.c:355-361`'s `exit(1)`); the three legs
        // beyond the letter slot (the value slots of the four `illegal X count.`
        // sentences, the reported chunk name and the input file name) are pinned by
        // `split_face`'s `the_diagnostic_and_the_chunk_name_carry_the_argv_bytes`.
        ("split", SPLIT, 1, b"split: unknown option -- \xff\n"),
        // tail: `tail.c:108`'s `getopt(argc, argv, "Fb:c:fn:r")` is libc's, so the
        // sentence is still `unknown option`. Its usage is **one line**, capitalized
        // `Usage:`, with the program name (`tail.c:299-305`), exit 1. The four legs
        // beyond the letter slot (the value slots of the three
        // `illegal offset -- %s` sentences, the whole token carried by `obsolete()`'s
        // `illegal option -- %s`, the rewritten token, and the file name in the
        // diagnostic line) are pinned by `tail_face`'s
        // `the_offset_echo_the_token_echo_and_the_file_name_carry_the_argv_bytes`.
        ("tail", TAIL, 1, b"tail: unknown option -- \xff\n"),
        // unexpand: `unexpand.c:86`'s `getopt(argc, argv, "at:")` is libc's, so the
        // sentence is still `unknown option`; its usage is one line, capitalized
        // `Usage:`, exit 1 (`unexpand.c:69-73`). The six tabstop echoes beyond the
        // letter slot and the file-name slot of the operand's `Cannot open` sentence
        // are pinned by `unexpand_face`'s
        // `the_tabstop_echoes_and_the_operand_name_carry_the_argv_bytes`.
        (
            "unexpand",
            UNEXPAND,
            1,
            b"unexpand: unknown option -- \xff\n",
        ),
        // unifdef: `unifdef.c:273`'s
        // `getopt(argc, argv, "i:D:U:I:o:cdeklst")` is libc's, so the sentence is
        // still `unknown option`; but its own usage **exits 2** (`:373-378`'s
        // `exit(2)`) and that line hardwires the program name in the string (no
        // `getprogname()`) — this row pins the libc line, while usage and the
        // file-name slots of the two `can't open %s` sentences are pinned by
        // `unifdef_face`'s `the_operand_and_output_names_carry_the_argv_bytes`.
        ("unifdef", UNIFDEF, 2, b"unifdef: unknown option -- \xff\n"),
        // vis: `vis.c:73`'s `getopt(argc, argv, "bcde:F:fhlMmNnoSstw")` is libc's,
        // exit 1; its usage line carries `getprogname()` (`vis.c:136-139`), so the two
        // lines share one origin (the registered `VIS-USAGE-PROGNAME-HARDCODED` notes
        // that this shell's usage hardwires `vis`). Its `-e` value slot is the
        // family's first verdict leg: the extra list is a byte-wise set, and folding
        // changes the encoding answer; pinned by `vis_face`'s
        // `the_extra_list_the_letter_and_the_operand_name_carry_the_argv_bytes`.
        ("vis", VIS, 1, b"vis: unknown option -- \xff\n"),
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

/// An invalid byte inside an **operand** is ordinary data: the reference's
/// `tr a\xffb c` exits 0 and prints `cccccc`, and this layer matches. When the
/// operand is taken as data, the text seam and the byte seam yield the same byte
/// string, so this test pins "does not blow up" rather than "the letter slot" — the
/// latter belongs to the table above.
#[test]
fn a_raw_byte_inside_an_operand_is_ordinary_data() {
    let set1 = [b"a".as_slice(), &[FF][..], b"b"].concat();
    let (code, out, err) = run(TR, &[&set1, b"c"], b"aabbcc\n");
    assert_eq!(code, 0, "实际 stderr {err:?}");
    assert_eq!(out, b"cccccc\n");
    assert!(err.is_empty(), "实际 {err:?}");
}

/// An invalid byte **consumed as a value** by `-d` is no error at all: the
/// reference's `paste -d\xff` exits 0 with no output (the old zero-operand rule:
/// neither reads standard input nor reports anything).
#[test]
fn a_raw_byte_consumed_as_an_option_value_is_not_an_error() {
    let dflag = [b"-d".as_slice(), &[FF][..]].concat();
    let (code, out, err) = run(PASTE, &[&dflag], b"aabbcc\n");
    assert_eq!(code, 0, "实际 stderr {err:?}");
    assert!(out.is_empty(), "实际 {out:?}");
    assert!(err.is_empty(), "实际 {err:?}");
}

/// An accumulating option ring (`seq.c:101-113` accumulates `errflg` and does not
/// stop at the first bad letter) would, on the **text seam**, treat the three bytes
/// produced by lossy folding as three letters and print three lines; with `seq` on
/// `bin_support::args_bytes` the bare `ff` on the command line is one letter, so the
/// output matches the reference: **one line, letter slot the raw byte**, followed by
/// one usage line. This test pins the post-fix shape so that a change back to
/// `String` cannot quietly bring the three lines back.
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

/// The closing face: the text `args()` entry is gone, and the remaining twelve
/// shells without a letter face also use `args_bytes()` — their argv byte leg is
/// down to the **operand name** (the slot in the message and the string handed to
/// the open entry). This face feeds each of the ten shells that can name an operand
/// an operand containing a bare `ff` and pins the name slot in the message to the
/// **raw byte** (the folded `ef bf bd` no longer appears); colcrt's and fmt's
/// open-failure sentences have no name slot, so they have no observable byte leg and
/// are not listed.
#[test]
fn the_plain_shells_operand_echo_carries_the_argv_bytes() {
    let ff: Vec<u8> = [b"nosuch".as_slice(), &[0xff][..], b".txt".as_slice()].concat();
    // diff needs two operands before it reaches the open (one operand lands straight
    // on usage), and the first side must **open successfully** before the second
    // side's error can be reported — so hand it a scratch file; look's
    // single-operand form follows C's `argc == 2` special case and takes that
    // operand as the query word (rejecting non-ASCII first), so the file-name leg
    // needs two operands to be reachable.
    let mut scratch = std::env::temp_dir();
    scratch.push(format!("argv-bytes-side-{}", std::process::id()));
    std::fs::write(&scratch, b"side a\n").expect("writing the scratch file");
    let scratch_os = std::os::unix::ffi::OsStrExt::as_bytes(scratch.as_os_str());
    let pair_args: Vec<&[u8]> = vec![scratch_os, &ff];
    let args: [&[u8]; 1] = [&ff];
    let rows: [(&str, &str, i32, Vec<u8>); 10] = [
        ("wc", WC, 1, b"wc: nosuch\xff.txt\n".to_vec()),
        ("expand", EXPAND, 1, b"expand: Cannot open `nosuch\xff.txt'\n".to_vec()),
        ("diff", DIFF, 2, b"diff: nosuch\xff.txt\n".to_vec()),
        ("hexdump", HEXDUMP, 1, b"hexdump: nosuch\xff.txt\n".to_vec()),
        ("unvis", UNVIS, 0, b"unvis: nosuch\xff.txt\n".to_vec()),
        ("asa", ASA, 0, b"nosuch\xff.txt: cannot open\n".to_vec()),
        ("tsort", TSORT, 1, b"tsort: nosuch\xff.txt\n".to_vec()),
        ("look", LOOK, 1, b"look: Can't open nosuch\xff.txt\n".to_vec()),
        ("lam", LAM, 1, b"lam: open nosuch\xff.txt\n".to_vec()),
        (
            "patch",
            PATCH,
            2,
            b"patch: target file `nosuch\xff.txt' waits on the open-existing seam (edge E-CMDSYSFACE)\n".to_vec(),
        ),
    ];
    for (name, exe, want_code, want_err) in rows {
        let argv: &[&[u8]] = if matches!(name, "diff" | "look") {
            &pair_args
        } else {
            &args
        };
        let (code, out, err) = run(exe, argv, b"aabbcc\n");
        assert_eq!(code, want_code, "{name} 在裸字节操数上的退码");
        assert_eq!(err, want_err, "{name} 的操数名位该是原字节");
        assert_eq!(out, Vec::<u8>::new(), "{name} 的标准输出");
    }
    let _ = std::fs::remove_file(&scratch);
}
