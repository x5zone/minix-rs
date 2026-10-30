#![cfg_attr(not(test), no_std)]
// The mechanical gate against silent error swallowing (the seventh crate running the
// same program as minix-sys/rt/fs/kernel/sef/fs-rt): in textfilter only `cmp.rs` has a
// single `let _ = write!` into a String (only the allocation can fail, and C's regular.c
// has no branch around the matching printf either). A new `let _ =` without a reason is
// reported on the spot.
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

/// The line libc's `getopt` prints itself, before returning `'?'`.
///
/// The line belongs to no single command: as long as the optstring does not begin with
/// `:` (none in this family does), `getopt` prints one sentence in two places and only
/// then hands `'?'` to the caller's `usage()`: an unknown letter through
/// `minix3/lib/libc/stdlib/getopt.c:101-104`, a value-taking option missing its value
/// through `:132-136`. Both put the program name first (`getprogname()`) and write the
/// letter without quotes — glibc on this host writes `invalid option -- 'x'`, so taking
/// the host as baseline would be wrong; the truth for this sentence can only come from
/// the C source. Two commands each writing the same sentence would carry one fact twice
/// (flattening either one changes no behavior), so it is collected here.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GetoptReport {
    /// `getopt.c:101-104`: an unknown letter (the libc version).
    UnknownOption(char),
    /// `egetopt.c:144-151`: an unknown letter (the `egetopt` version `pr` carries). The
    /// wording differs from libc's line, so the two cannot share one variant: `pr` does
    /// not use libc's `getopt`, it links `minix3/usr.bin/pr/egetopt.c`, and that file
    /// writes `illegal option`. The same letter yields different sentences under the two
    /// parsers, and flattening them with a "family rule" would get the wording wrong.
    IllegalOption(char),
    /// `getopt.c:132-136` (libc) and `egetopt.c:196-205` (`pr`) — the two sentences are
    /// word for word identical, so they share one variant. (`egetopt` has a third
    /// sentence, `illegal '+' delimiter with option`, at `:158-167`, used only when no
    /// number follows `+`; this shell's own `+` leg reports `+page number ...`.)
    MissingValue(char),
}

impl GetoptReport {
    /// The wording of that line and the letter it reports — both rendering entries share
    /// this one copy, so the wording is carried once.
    fn wording(self) -> (&'static str, char) {
        match self {
            GetoptReport::UnknownOption(letter) => ("unknown option -- ", letter),
            GetoptReport::IllegalOption(letter) => ("illegal option -- ", letter),
            GetoptReport::MissingValue(letter) => ("option requires an argument -- ", letter),
        }
    }

    /// The whole text of that line (with the newline). The caller writes it to standard
    /// error and then appends its own usage.
    ///
    /// The prefix is always the program name: libc's two sentences pass `getprogname()`,
    /// while `egetopt` computes the last segment of `argv[0]` itself with
    /// `strrchr(*nargv, '/')` (`egetopt.c:145-147`, `:159-161`, `:197-199`) — the same
    /// result.
    ///
    /// The letter is rendered through `char`'s `Display`, so the range `0x80..=0xFF`
    /// produces two bytes (its UTF-8 encoding) whereas C prints one bare byte — the
    /// difference is in rendering; see [`GetoptReport::line_bytes`].
    #[must_use]
    pub fn line(self, prog: &str) -> alloc::string::String {
        let (wording, letter) = self.wording();
        alloc::format!("{prog}: {wording}{letter}\n")
    }

    /// The byte form of the same line: the letter goes out as one bare byte.
    ///
    /// C's line passes `optopt`, and `optopt = *place++` (`getopt.c:79`, `:95`) takes one
    /// byte of argv, which `fprintf`'s `%c` then writes out unchanged
    /// (`getopt.c:101-104`). The C `rev` prints, for `rev -é` (`é` is the two bytes
    /// `c3 a9`), `rev: unknown option -- ` + the bare byte `c3` + a newline, that one
    /// line only (`rev.c:72-74` calls `usage()` and exits on `'?'`, never returning to
    /// `getopt` for the second byte).
    ///
    /// A code point in `0x00..=0xFF` is written as that one byte — C's letter slot is one
    /// byte to begin with, so this range matches the method's purpose exactly. Above
    /// `0xFF` C has no byte to print: such a code point can only come from a shell whose
    /// parsing half still takes characters, so it falls back to the character's UTF-8
    /// encoding, identical to [`GetoptReport::line`]. The fallback is for the staged
    /// migration: `line`'s callers are spread over more than twenty shells, only one can
    /// change per round, and the round that swaps the rendering should not also change
    /// the messages of shells whose letter handling has not been swapped yet.
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

/// The option loop for an empty optstring: hands back the operands after
/// `argv += optind`.
///
/// `rev.c:70` and `colrm.c:67` both write `getopt(argc, argv, "")`, the same shape word
/// for word, so this branch lives here and serves both commands (the same reason as
/// [`GetoptReport`]: one fact carried once). An empty optstring does not mean "no option
/// loop"; it has three consequences, all fixed by libc `getopt`:
/// ① `strchr("", <letter>)` is always `NULL`, so any token starting with `-` lands in
/// `minix3/lib/libc/stdlib/getopt.c:95-104`'s unknown-letter branch — libc prints one line
/// first and then returns `'?'`;
/// ② `--` at `:81-85` ends the option loop and advances `optind` first, so it is not
/// itself an operand;
/// ③ a bare `-` at `:87-92` ends the option loop because the optstring has no `-` but
/// does not advance `optind`, so it is an operand itself (what the two commands do with
/// that operand differs and belongs to their shells).
///
/// No permutation: as soon as a non-option token arrives, `:74-78` returns -1 at once and
/// it and every token after it come back as operands. Only the first token is examined:
/// the caller's `while` cannot run a second time (it exits through `usage()` on `'?'` and
/// ends on -1), so this is a single test rather than a loop.
///
/// This branch does not test "value-taking option missing its value": an empty optstring
/// has no option that takes a value, so the whole of `getopt.c:112-141` is unreachable,
/// and only [`GetoptReport::UnknownOption`] can come back.
///
/// What is reported is the first byte after the dash, not the first character:
/// `optopt = *place++` at `getopt.c:79` steps on a `char *`, one byte at a time. C `rev`
/// reports the bare byte `c3` for `rev -é` (`é`'s second byte `a9` never enters the
/// message), whereas a character-based read would report U+00E9. The same approach is
/// used earlier in `src/col.rs:parse_col_flags`.
/// This byte form of the empty-optstring operand loop (the shared `&str` twin was
/// deleted with the family's last shell) keeps the same three-shape test and hands back
/// the original bytes. The letter is still the first byte after the dash —
/// `char::from(byte)` takes it into the report, which is exactly C's `unsigned char`
/// promoted to `int`; [`GetoptReport::line_bytes`] then writes it back out as that one
/// byte.
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

/// C's `atoi`: skip leading `isspace`, take one optional sign, read a run of decimal
/// digits, and ignore trailing junk; no digit at all means 0.
///
/// It lives at the crate root and is shared by several commands (the same reason as
/// [`GetoptReport`]: one fact carried once) — `col`'s `-l` (`col.c`'s `atoi(optarg)`)
/// and `column`'s `-c` (`column.c:96`) call the same libc function with the same test.
///
/// So `3x` is a valid 3 (C `col`/`column` accept it the same way), while `abc`, `0` and
/// `-2` yield 0, 0 and -2; overflow saturates at both ends of `i32` the way C's `strtol`
/// truncates (`saturating_*`), because `atoi`'s return value is an `int` to begin with.
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
    use super::{GetoptReport, empty_optstring_operands_bytes};

    /// The exact text of those two lines is pinned here (single source), because the
    /// shell-side gate can only prove that something was rendered, not the wording. The
    /// truth comes from `minix3/lib/libc/stdlib/getopt.c:101-104` and `:132-136`:
    /// `"%s: unknown option -- %c\n"` and `"%s: option requires an argument -- %c\n"`,
    /// with no quotes around the letter in either.
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
        // computes itself), not from a hard-coded command name.
        assert_eq!(
            GetoptReport::UnknownOption('3').line("head"),
            "head: unknown option -- 3\n"
        );
        // `pr`'s own `egetopt` writes a different sentence for an unknown letter
        // (`egetopt.c:148`), one word away from the form above; substituting the family
        // wording would get the sentence wrong.
        assert_eq!(
            GetoptReport::IllegalOption('x').line("pr"),
            "pr: illegal option -- x\n"
        );
        // The missing-value sentence is word for word the same on both sides
        // (`egetopt.c:202` and `getopt.c:134`), so the two branches share one variant.
        assert_eq!(
            GetoptReport::MissingValue('w').line("pr"),
            "pr: option requires an argument -- w\n"
        );
    }

    /// The byte-form branch: the letter is one bare byte, not its UTF-8.
    ///
    /// The truth is what C `rev` prints (for `rev -é` it prints `unknown option -- ` +
    /// `c3`); the mechanism is `optopt = *place++` at `getopt.c:79` and `%c` at `:103`.
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
        // For the ASCII form the two entries are identical: the migration must not touch
        // it.
        assert_eq!(
            GetoptReport::UnknownOption('Z').line_bytes("rev"),
            GetoptReport::UnknownOption('Z').line("rev").into_bytes()
        );
        // A code point in `0x00..=0xFF` is always written as that one byte: U+00E9 prints
        // `e9`, one byte shorter than `line`'s UTF-8 form (`c3 a9`).
        assert_eq!(
            GetoptReport::UnknownOption('\u{00e9}').line_bytes("rev"),
            b"rev: unknown option -- \xe9\n".to_vec()
        );
        // The other side of the fork is pinned too: `line` prints two bytes for the same
        // code point, which is the declared deviation.
        assert_eq!(
            GetoptReport::UnknownOption('\u{00c3}')
                .line("rev")
                .into_bytes(),
            b"rev: unknown option -- \xc3\x83\n".to_vec(),
            "`line` 的 char 渲染比 C 多一个字节——A2 的射程"
        );
        // A code point above 0xFF (reachable only while the parsing half still takes
        // characters) falls back to UTF-8 and matches `line` exactly — so the round that
        // swaps rendering does not change the messages of shells that have not swapped
        // their letter handling.
        assert_eq!(
            GetoptReport::UnknownOption('\u{fffd}').line_bytes("rev"),
            GetoptReport::UnknownOption('\u{fffd}')
                .line("rev")
                .into_bytes()
        );
    }

    /// The byte form of the empty optstring recognizes a single invalid UTF-8 byte:
    /// that `ff` is the letter and never takes the lossy-folding path (`args()` would
    /// fold it to U+FFFD first, making the letter `ef`).
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
        // The text-seam folding comparison lost its carrier when `args()` was deleted:
        // that leg no longer exists, so the "letter off by one byte" residual has no
        // source left.
    }

    /// The empty-optstring branch reports the first byte after the dash (the shared
    /// `&str` form was deleted with the family's last shell, and every pin of this
    /// three-shape test moved to the byte form).
    #[test]
    fn empty_optstring_names_the_first_byte_of_the_flag() {
        assert_eq!(
            empty_optstring_operands_bytes(&[b"-\xc3\xa9"]),
            Err(GetoptReport::UnknownOption('\u{00c3}')),
            "`é` 是 c3 a9 两字节，libc 报 c3 而不是整个字符"
        );
        // A cluster still reports the first byte: an ASCII x in front is where it lands.
        assert_eq!(
            empty_optstring_operands_bytes(&[b"-x\xc3\xa9"]),
            Err(GetoptReport::UnknownOption('x'))
        );
        // ASCII and the three shapes (`--` consuming itself, a bare `-` and `--x`) are
        // unaffected by this rule.
        assert_eq!(
            empty_optstring_operands_bytes(&[b"-Z"]),
            Err(GetoptReport::UnknownOption('Z'))
        );
        assert_eq!(
            empty_optstring_operands_bytes(&[b"--x"]),
            Err(GetoptReport::UnknownOption('-'))
        );
        assert_eq!(
            empty_optstring_operands_bytes(&[b"--", b"f"]),
            Ok(vec![b"f".as_slice()])
        );
        assert_eq!(
            empty_optstring_operands_bytes(&[b"-"]),
            Ok(vec![b"-".as_slice()])
        );
    }
}
