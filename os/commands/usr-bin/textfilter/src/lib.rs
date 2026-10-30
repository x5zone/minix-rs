#![cfg_attr(not(test), no_std)]
// Mechanical gate for the silently-swallowed-error family (seventh crate, same
// program as minix-sys/rt/fs/kernel/sef/fs-rt): textfilter currently has exactly
// one `let _ = write!` into a String, in `cmp.rs` (only allocation can fail, and
// C regular.c's printf does not branch either). A new unexplained `let _ =`
// reports an extra hit immediately.
#![warn(clippy::let_underscore_must_use)]

//! Text filtering and data processing core for Minix-RS commands.
//!
//! Covers `rewrite-notes/18-stage-commands/07-text-filter.md`:
//! the line oriented tools in `minix3/usr.bin/` (`head`, `tail`, `sort`,
//! `uniq`, `wc`, `cut`, `tr`, `expand`, `fold`, `rev`, `seq`, `tee`, ...).
//! Every tool here reads lines and writes lines; the only thing that
//! differs is the per line decision. This crate owns those decisions as
//! pure functions over borrowed text:
//!
//! - [`window`]: head/tail line windows behind the [`window::LineWindow`]
//!   trait.
//! - [`count`]: the `wc` counters (lines, words, bytes, longest line).
//! - [`cut`]: field list parsing and column selection.
//! - [`tr`]: character set translation, deletion, and squeezing over the
//!   [`tr::CharClass`] trait.
//! - [`uniq`]: adjacent duplicate handling (`-c` counts, `-d`/`-u`
//!   selection).
//!
//! Sorting (`sort`), comparison (`cmp`, `diff`, `patch`), pagination, and
//! number formatting stay with later stages: `sort` needs ordering plus
//! buffering policy, the comparators need file pairing, and the formatters
//! need output width policy. The line algorithms here are the shared
//! foundation they will all reuse.
//!
//! Everything borrows from the input and uses fixed size buffers: no heap,
//! `no_std` throughout.

// The `sort` option parser materialises its key list; every other module
// stays allocation free.
extern crate alloc;

pub mod asa;
pub mod cksum;
pub mod cmp;
pub mod col;
pub mod colcrt;
pub mod colrm;
pub mod column;
pub mod comm;
pub mod count;
pub mod cut;
pub mod diff;
pub mod digest;
pub mod expand;
pub mod floatfmt;
pub mod fmt;
pub mod fold;
pub mod hexdump;
pub mod join;
pub mod jot;
pub mod lam;
pub mod look;
pub mod nl;
pub mod paste;
pub mod patch;
pub mod pr;
pub mod rev;
pub mod seq;
pub mod sort;
pub mod split;
pub mod tr;
pub mod tsort;
pub mod unexpand;
pub mod unifdef;
pub mod uniq;
pub mod units;
pub mod vis;
pub mod window;

/// Errors produced by this crate, mapped to classic Unix error numbers.
///
/// 22 marks malformed input (`EINVAL`): bad field lists, bad counts, bad
/// sets. All bounds are explicit fixed capacities; overflow is an error,
// never a silent truncation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextError {
    /// Malformed input.
    InvalidArgument,
    /// A fixed buffer proved too small for the result.
    TooLong,
}

impl TextError {
    /// The classic Unix error number for this failure.
    pub fn as_errno(self) -> i32 {
        match self {
            TextError::InvalidArgument => 22,
            TextError::TooLong => 12,
        }
    }
}

/// The line libc's `getopt` prints itself **before** returning `'?'`.
///
/// This line belongs to no command: as long as the optstring does not start with
/// `:` (none in this family does), `getopt` prints a sentence in two places and
/// only then hands `'?'` to the caller's `usage()`: out-of-set letters go through
/// `minix3/lib/libc/stdlib/getopt.c:101-104`, a value-taking option with a missing
/// value through `:132-136`. Both put the program name first (`getprogname()`), and
/// the letter is **not quoted** — the local glibc writes
/// `invalid option -- 'x'`, so the host is a wrong baseline; the truth of this
/// sentence can only come from the C source. Two commands each writing the same
/// sentence would carry the same fact twice (flattening either first changes no
/// behavior), so it lives here.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GetoptReport {
    /// `getopt.c:101-104`: out-of-set letter (libc version).
    UnknownOption(char),
    /// `egetopt.c:144-151`: out-of-set letter (`pr`'s own `egetopt` version). The
    /// wording **differs** from libc's line, so the two cannot share a variant:
    /// `pr` does not use libc's `getopt`; it links `minix3/usr.bin/pr/egetopt.c`,
    /// which writes `illegal option`. The same letter yields different sentences
    /// under the two parsers, and flattening them with a "family rule" would get
    /// the wording wrong.
    IllegalOption(char),
    /// `getopt.c:132-136` (libc) and `egetopt.c:196-205` (`pr`) — the sentences are
    /// verbatim identical, so they share one variant. (`egetopt` has a third
    /// sentence, `illegal '+' delimiter with option`, at `:158-167`, reached only
    /// when `+` is not followed by a number; this wrapper's `+` leg reports
    /// `+page number ...` itself.)
    MissingValue(char),
}

impl GetoptReport {
    /// The line's wording and the letter to report — both rendering entries share
    /// this, so the wording is carried once.
    fn wording(self) -> (&'static str, char) {
        match self {
            GetoptReport::UnknownOption(letter) => ("unknown option -- ", letter),
            GetoptReport::IllegalOption(letter) => ("illegal option -- ", letter),
            GetoptReport::MissingValue(letter) => ("option requires an argument -- ", letter),
        }
    }

    /// The complete line (with newline). The caller writes it to stderr and then
    /// appends its own usage.
    ///
    /// The prefix is always the program name: libc's two sentences pass
    /// `getprogname()`, while `egetopt` computes the last segment of `argv[0]`
    /// itself with `strrchr(*nargv, '/')` (`egetopt.c:145-147`, `:159-161`,
    /// `:197-199`) — the same result.
    ///
    /// The letter renders through `char`'s `Display`, so the `0x80..=0xFF` range
    /// produces **two** bytes (UTF-8 encoding) while C prints one bare byte — a
    /// rendering difference; see [`GetoptReport::line_bytes`].
    #[must_use]
    pub fn line(self, prog: &str) -> alloc::string::String {
        let (wording, letter) = self.wording();
        alloc::format!("{prog}: {wording}{letter}\n")
    }

    /// The **byte** form of the same line: write the letter as **one bare byte**.
    ///
    /// C's line passes `optopt`, and `optopt = *place++` (`getopt.c:79`, `:95`)
    /// takes **one byte** from argv, which `fprintf`'s `%c` writes out unchanged
    /// (`getopt.c:101-104`). Reference measurement `tmp/rev-309/rev_ref`: `rev -é`
    /// (`é` is the two bytes `c3 a9`) prints `rev: unknown option -- ` + the bare
    /// byte `c3` + newline, and nothing else (`rev.c:72-74` calls `usage()` and
    /// exits on `'?'`, never returning to `getopt` to read the second byte).
    ///
    /// A code point in `0x00..=0xFF` is written as **that one byte** — C's letter
    /// slot is a single byte to begin with, so this range matches the method's
    /// purpose exactly. For a code point **above** `0xFF` C has no byte to print:
    /// such code points can only come from a wrapper whose parse half still takes
    /// characters, so this falls back to the character's UTF-8 encoding, byte for
    /// byte identical to [`GetoptReport::line`]. The fallback exists for the
    /// **staged migration**: `line`'s callers are spread over more than twenty
    /// wrappers, one can change per round, and the round that changes rendering
    /// must not also change messages of wrappers whose parse method is unchanged.
    #[must_use]
    pub fn line_bytes(self, prog: &str) -> alloc::vec::Vec<u8> {
        let (wording, letter) = self.wording();
        let mut out = alloc::format!("{prog}: {wording}").into_bytes();
        match u8::try_from(u32::from(letter)) {
            Ok(byte) => out.push(byte),
            Err(_) => out.extend_from_slice(letter.encode_utf8(&mut [0u8; 4]).as_bytes()),
        }
        out.push(b'\n');
        out
    }
}

/// The option loop for an empty optstring: return the operands after
/// `argv += optind`.
///
/// `rev.c:70` and `colrm.c:67` both write `getopt(argc, argv, "")`, and the two
/// shapes are verbatim identical, so this branch lives here and is shared by both
/// commands (same rationale as [`GetoptReport`]: one fact carried once).
/// An empty optstring does not mean "no option loop", and its consequences are
/// three, all fixed by libc `getopt`:
/// (1) `strchr("", <letter>)` is always `NULL`, so any token starting with `-`
/// falls into the out-of-set-letter branch at
/// `minix3/lib/libc/stdlib/getopt.c:95-104` — libc prints a line first, then
/// returns `'?'`;
/// (2) `--` ends the option loop at `:81-85` and **does `++optind` first**, so it
/// is not itself an operand;
/// (3) a bare `-` ends the option loop at `:87-92` because the optstring has no
/// `-`, **without advancing** `optind`, so it is an operand itself (how the two
/// commands handle that operand differs and belongs to their wrappers).
///
/// No permutation: as soon as a non-option token appears, `:74-78` returns -1
/// immediately and that token plus everything after it are returned as operands.
/// Only the **first** token is inspected: the caller's `while` cannot reach a
/// second one (`'?'` leads to `usage()` and exit, -1 ends the loop), so this is a
/// single decision rather than a loop.
///
/// This branch never judges "value-taking option missing its value": an empty
/// optstring has no option that takes a value, so the whole `getopt.c:112-141`
/// block is unreachable and only [`GetoptReport::UnknownOption`] can come back.
///
/// What is reported is the **first byte** after the dash, not the first
/// character: `optopt = *place++` at `getopt.c:79` steps over a `char *`, one byte
/// at a time. Reference measurement `rev -é` reports the bare byte `c3` (`é`'s
/// trailing `a9` never enters the message); taking a character would report
/// U+00E9. Same-method precedent: `src/col.rs:parse_col_flags`.
pub fn empty_optstring_operands<'a>(
    argv: &'a [&'a str],
) -> Result<alloc::vec::Vec<&'a str>, GetoptReport> {
    let Some(first) = argv.first() else {
        return Ok(alloc::vec::Vec::new());
    };
    let Some(flag) = first.strip_prefix('-') else {
        return Ok(argv.to_vec());
    };
    if flag == "-" {
        return Ok(argv[1..].to_vec());
    }
    let Some(byte) = flag.as_bytes().first().copied() else {
        return Ok(argv.to_vec());
    };
    Err(GetoptReport::UnknownOption(char::from(byte)))
}

/// Byte form of [`empty_optstring_operands`] (the A2 argument-seam half): the same
/// three-shape rule, except raw bytes come back. The letter is still the **first
/// byte** after the dash — `char::from(byte)` puts it in the report, which is
/// exactly how C promotes `unsigned char` to `int`; [`GetoptReport::line_bytes`]
/// then writes it back as that one byte.
pub fn empty_optstring_operands_bytes<'a>(
    argv: &'a [&'a [u8]],
) -> Result<alloc::vec::Vec<&'a [u8]>, GetoptReport> {
    let Some(first) = argv.first() else {
        return Ok(alloc::vec::Vec::new());
    };
    let Some(flag) = first.strip_prefix(b"-") else {
        return Ok(argv.to_vec());
    };
    if flag == &b"-"[..] {
        return Ok(argv[1..].to_vec());
    }
    let Some(&byte) = flag.first() else {
        return Ok(argv.to_vec());
    };
    Err(GetoptReport::UnknownOption(char::from(byte)))
}

/// C's `atoi`: skip leading `isspace`, allow an optional sign, take a run of
/// decimal digits, **ignore any trailing junk**, and yield 0 when there is no
/// digit at all.
///
/// Lives at the crate root, shared by several commands (same rationale as
/// [`GetoptReport`]: one fact carried once) — `col`'s `-l` (`atoi(optarg)` in
/// `col.c`) and `column`'s `-c` (`column.c:96`) call the same libc function, so
/// the rule is identical.
///
/// So `3x` is a legal 3 (the reference accepts it too), while `abc`, `0`, `-2`
/// yield 0, 0, -2; overflow saturates to the `i32` bounds (`saturating_*`) the way
/// C's `strtol` does, because `atoi`'s return value is an `int` to begin with.
#[must_use]
pub fn atoi(text: &[u8]) -> i32 {
    let bytes = text;
    let mut cursor = 0usize;
    while matches!(
        bytes.get(cursor),
        Some(b' ') | Some(b'\t') | Some(b'\n') | Some(0x0b) | Some(0x0c) | Some(b'\r')
    ) {
        cursor += 1;
    }
    let negative = match bytes.get(cursor) {
        Some(b'+') => {
            cursor += 1;
            false
        }
        Some(b'-') => {
            cursor += 1;
            true
        }
        _ => false,
    };
    let mut value: i32 = 0;
    while cursor < bytes.len() && bytes[cursor].is_ascii_digit() {
        value = value
            .saturating_mul(10)
            .saturating_add(i32::from(bytes[cursor] - b'0'));
        cursor += 1;
    }
    if negative { -value } else { value }
}

#[cfg(test)]
mod tests {
    use super::{GetoptReport, empty_optstring_operands, empty_optstring_operands_bytes};

    /// The two lines' exact text is pinned here (single source), because the
    /// wrapper-side gates can only prove "it rendered", not the wording. The truth
    /// comes from `minix3/lib/libc/stdlib/getopt.c:101-104` and `:132-136`:
    /// `"%s: unknown option -- %c\n"`,
    /// `"%s: option requires an argument -- %c\n"`, with the letter unquoted in
    /// both.
    #[test]
    fn getopt_report_lines_match_the_libc_format_strings() {
        assert_eq!(
            GetoptReport::UnknownOption('x').line("cut"),
            "cut: unknown option -- x\n"
        );
        assert_eq!(
            GetoptReport::MissingValue('f').line("tail"),
            "tail: option requires an argument -- f\n"
        );
        // The program name comes from `getprogname()` (or the segment `egetopt`
        // computes itself), not a hard-coded command name.
        assert_eq!(
            GetoptReport::UnknownOption('3').line("head"),
            "head: unknown option -- 3\n"
        );
        // `pr`'s own `egetopt` writes a different sentence for out-of-set letters
        // (`egetopt.c:148`), one word off from the shape above; substituting the
        // family wording would get the sentence wrong.
        assert_eq!(
            GetoptReport::IllegalOption('x').line("pr"),
            "pr: illegal option -- x\n"
        );
        // The missing-value sentence is verbatim identical on both sides
        // (`egetopt.c:202` and `getopt.c:134`), so the two branches share one
        // variant.
        assert_eq!(
            GetoptReport::MissingValue('w').line("pr"),
            "pr: option requires an argument -- w\n"
        );
    }

    /// The byte-form branch: the letter is **one bare byte**, not its UTF-8
    /// encoding.
    ///
    /// The truth is the reference measurement (`tmp/rev-309/rev_ref` with `rev -é`
    /// gives `unknown option -- ` + `c3`); the mechanism is `optopt = *place++` at
    /// `getopt.c:79` and `%c` at `:103`.
    #[test]
    fn getopt_report_bytes_write_the_letter_as_one_byte() {
        assert_eq!(
            GetoptReport::UnknownOption('\u{00c3}').line_bytes("rev"),
            b"rev: unknown option -- \xc3\n".to_vec(),
            "0xC3 只能是一个字节；按 char 渲染会多出第二个字节 0x83"
        );
        assert_eq!(
            GetoptReport::IllegalOption('\u{0080}').line_bytes("pr"),
            b"pr: illegal option -- \x80\n".to_vec()
        );
        assert_eq!(
            GetoptReport::MissingValue('f').line_bytes("tail"),
            b"tail: option requires an argument -- f\n".to_vec(),
            "另外两句的措辞与 line 同一份来源"
        );
        // The ASCII shape is identical through both entries: the migration must
        // not touch it.
        assert_eq!(
            GetoptReport::UnknownOption('Z').line_bytes("rev"),
            GetoptReport::UnknownOption('Z').line("rev").into_bytes()
        );
        // Every code point in `0x00..=0xFF` is written as **that one byte**: U+00E9
        // prints `e9`, one byte less than `line`'s UTF-8 form (`c3 a9`).
        assert_eq!(
            GetoptReport::UnknownOption('\u{00e9}').line_bytes("rev"),
            b"rev: unknown option -- \xe9\n".to_vec()
        );
        // The other side of the fork is pinned too: `line` prints two bytes for the
        // same code point — that is the logged deviation.
        assert_eq!(
            GetoptReport::UnknownOption('\u{00c3}')
                .line("rev")
                .into_bytes(),
            b"rev: unknown option -- \xc3\x83\n".to_vec(),
            "`line` 的 char 渲染比 C 多一个字节——A2 的射程"
        );
        // A code point above 0xFF (reachable only while the parse half still takes
        // characters) falls back to UTF-8, the same string as `line` — so the
        // "change rendering" round does not alter messages of wrappers whose
        // "parse method" has not changed yet.
        assert_eq!(
            GetoptReport::UnknownOption('\u{fffd}').line_bytes("rev"),
            GetoptReport::UnknownOption('\u{fffd}')
                .line("rev")
                .into_bytes()
        );
    }

    /// The byte form of the empty optstring recognizes an **invalid UTF-8 single
    /// byte**: that `ff` is the letter, not something routed through lossy folding
    /// (`args()` would fold it to U+FFFD first, making the letter `ef`).
    #[test]
    fn empty_optstring_bytes_name_an_invalid_utf8_option_byte() {
        assert_eq!(
            empty_optstring_operands_bytes(&[&b"-\xff"[..]]),
            Err(GetoptReport::UnknownOption('\u{00ff}')),
            "裸字节 ff 应当在字母位上，而不是折成 U+FFFD"
        );
        assert_eq!(
            GetoptReport::UnknownOption('\u{00ff}').line_bytes("rev"),
            b"rev: unknown option -- \xff\n".to_vec()
        );
        // When the same argument comes in through `args()`'s folding leg, the letter
        // slot is no longer that byte — the difference between the two shapes is the
        // residual tracked in A/B.
        let folded: alloc::borrow::Cow<'_, str> =
            alloc::string::String::from_utf8_lossy(&[b'-', 0xff][..]);
        assert_eq!(
            empty_optstring_operands(&[folded.as_ref()]),
            Err(GetoptReport::UnknownOption('\u{00ef}')),
            "折叠腿交回的是 efbfbd 三字节，取第一个字节就成了 ef——这正是 A/B 里那条字母位差一位的成因"
        );
    }

    /// The empty-optstring branch reports the first **byte** after the dash.
    #[test]
    fn empty_optstring_names_the_first_byte_of_the_flag() {
        assert_eq!(
            empty_optstring_operands(&["-\u{e9}"]),
            Err(GetoptReport::UnknownOption('\u{00c3}')),
            "`é` 是 c3 a9 两字节，libc 报 c3 而不是整个字符"
        );
        // Clustering still reports the first byte: with ASCII `x` first, it lands on
        // `x`.
        assert_eq!(
            empty_optstring_operands(&["-x\u{e9}"]),
            Err(GetoptReport::UnknownOption('x'))
        );
        // ASCII and three shapes (`--` consuming itself, bare `-`, and `--x`) are
        // unaffected by this change.
        assert_eq!(
            empty_optstring_operands(&["-Z"]),
            Err(GetoptReport::UnknownOption('Z'))
        );
        assert_eq!(
            empty_optstring_operands(&["--x"]),
            Err(GetoptReport::UnknownOption('-'))
        );
        assert_eq!(empty_optstring_operands(&["--", "f"]), Ok(vec!["f"]));
        assert_eq!(empty_optstring_operands(&["-"]), Ok(vec!["-"]));
    }
}
