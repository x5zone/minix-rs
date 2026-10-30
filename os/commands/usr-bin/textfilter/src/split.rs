//! Chunk computation for `split`.
//!
//! Ground truth: `minix3/usr.bin/split/split.c` (NetBSD): the input
//! splits into chunks of `DEFLINE` (1000, split.c:56) lines — `-l`
//! moves the count — and the chunk files are named by a suffix that
//! enumerates `aa`, `ab`, ... `zz` (`sfxlen` defaults to 2,
//! split.c:61). This module computes the chunks (which lines go to
//! which chunk) and the suffix sequence; the doing half's file writes
//! wait for the gated create/write face on the host, so the doing half
//! can only report what it would create. The `-a` suffix-length, `-b` byte
//! count (`split.c:99-108`), and `-n` chunk-count (`split.c:123-127`) flags are
//! all wired here (`lines_bytes` / `chunks_bytes` / `chunk_bytes` / `chunks_n`).
//! Both line-mode faces are byte-addressed, as `split2` is: it scans the read
//! buffer for `\n` and never checks an encoding, so undecodable bytes are
//! ordinary data. The two streaming cutters below ([`ByteCutter`] for `-b`,
//! [`LineCutter`] for the line face) carry only counters across reads, so
//! neither one needs the whole input resident.

use alloc::string::String;
use alloc::vec::Vec;

/// Default lines per chunk (`DEFLINE`, split.c:56).
pub const DEFAULT_LINES: usize = 1000;

/// Default suffix length (`sfxlen`, split.c:61).
pub const DEFAULT_SFXLEN: usize = 2;

/// Splits raw input into lines, each one **keeping** the `\n` that ends it
/// (`split.c:262-267`: `split2` writes `Ce - Cs + 1` bytes, so the terminator
/// travels with its line). A tail without a terminator stays without one, and
/// input ending in a newline yields no empty trailing line.
pub fn lines_bytes(data: &[u8]) -> Vec<&[u8]> {
    let mut out = Vec::new();
    let mut start = 0usize;
    for (index, byte) in data.iter().enumerate() {
        if *byte == b'\n' {
            out.push(&data[start..index + 1]);
            start = index + 1;
        }
    }
    if start < data.len() {
        out.push(&data[start..]);
    }
    out
}

/// Computes the chunks: each entry holds that chunk's lines, terminators and
/// all (the last one may be short). `lines_per_chunk` of zero is a caller error
/// the doing half rejects.
pub fn chunks_bytes<'a>(lines: &[&'a [u8]], lines_per_chunk: usize) -> Vec<Vec<&'a [u8]>> {
    let mut chunks = Vec::new();
    if lines_per_chunk == 0 {
        return chunks;
    }
    let mut cursor = 0usize;
    while cursor < lines.len() {
        let end = (cursor + lines_per_chunk).min(lines.len());
        chunks.push(lines[cursor..end].to_vec());
        cursor = end;
    }
    chunks
}

/// Splits raw input into fixed-size byte chunks of `n` bytes (the last may be
/// short); `n == 0` or empty input yields no chunks. The fixed-size cousin
/// of [`chunks_bytes`], mirroring `split.c` `split1(bytecnt, …)` behind `-b`.
pub fn chunk_bytes(data: &[u8], n: usize) -> Vec<&[u8]> {
    let mut out = Vec::new();
    if n == 0 {
        return out;
    }
    let mut cursor = 0usize;
    while cursor < data.len() {
        let end = (cursor + n).min(data.len());
        out.push(&data[cursor..end]);
        cursor = end;
    }
    out
}

/// Splits `data` into exactly `n` byte chunks (`-n`, C `split3` → `split1
/// (size/n, n)`): the first `n-1` chunks are `size/n` bytes and the last
/// absorbs the remainder, so all `n` files are non-empty. `None` when `n == 0`
/// or `n > data.len()` (C `errx("can't split into more than %d files")`).
pub fn chunks_n(data: &[u8], n: usize) -> Option<Vec<&[u8]>> {
    if n == 0 || n > data.len() {
        return None;
    }
    let base = data.len() / n;
    let mut out = Vec::with_capacity(n);
    let mut cursor = 0usize;
    for _ in 0..n - 1 {
        out.push(&data[cursor..cursor + base]);
        cursor += base;
    }
    out.push(&data[cursor..]);
    Some(out)
}

/// The file suffix for chunk `index` (0 based) at width `width`: `width`
/// lowercase letters enumerating the base-26 space (`aa`..`zz` for the C
/// default `sfxlen` 2). `None` when the sequence is exhausted (past the
/// `26^width`-th chunk) or `width` is zero.
pub fn suffix(index: usize, width: usize) -> Option<String> {
    if width == 0 {
        return None;
    }
    let capacity = 26usize.checked_pow(width as u32)?;
    if index >= capacity {
        return None;
    }
    let mut out = Vec::with_capacity(width);
    let mut value = index;
    for place in (0..width).rev() {
        let divisor = 26usize.pow(place as u32);
        out.push(b'a' + (value / divisor) as u8);
        value %= divisor;
    }
    Some(out.into_iter().map(|b| b as char).collect())
}

/// Parsed `split` command line: the resolved line count and the file operands
/// in order (a bare `-` names standard input).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SplitArgs<'a> {
    pub lines_per_chunk: usize,
    /// `-b` byte count per chunk (`bytecnt`, split.c:76); `Some` selects the
    /// byte-addressed mode over the line `lines_per_chunk` default.
    pub byte_size: Option<usize>,
    /// `-n` chunk count (`chunks`, split.c:77); `Some` selects `split3`
    /// (exactly this many byte chunks), mutually exclusive with `-b`/`-l`.
    pub chunk_count: Option<usize>,
    /// `-a` suffix width (`sfxlen`, split.c:61 default 2).
    pub sfxlen: usize,
    /// The file operands exactly as `argv` handed them over. The prefix that
    /// names every output chunk (`*argv++`, `split.c:143`) is a **filename**, so
    /// the bytes are the whole point: folding them through `str` would rewrite a
    /// non-UTF-8 prefix before it reaches `open`/`creat`.
    pub operands: Vec<&'a [u8]>,
}

/// `split` argument failure, each arm mirroring a distinct C path so the caller
/// can reproduce C's message and exit code.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SplitError<'a> {
    /// The `default: usage()` of `split.c:80-130` (both cases are counted:
    /// `-l 3 -l 4` reaches `if (numlines != 0) usage()`, and more than two
    /// operands reaches `if (*argv != NULL) usage()`).
    Usage,
    /// A letter outside the set: libc prints one line **before** returning
    /// `'?'` (`getopt.c:101-104`), and C's `default:` then appends the usage —
    /// hence **two lines**. `-Z`, `-Zb` and `--x` (which reports `-`) all take
    /// this branch.
    UnknownOption(char),
    /// A value-taking option without its value (`-l`/`-b`/`-a`/`-n` with no
    /// next word): libc prints `option requires an argument -- <letter>`
    /// (`getopt.c:132-136`) and then returns `'?'`; the caller's `default:`
    /// appends the usage — two lines again.
    MissingValue(char),
    /// A `-l`/`-N` value that is not all-decimal or is zero: C `errx(1, "%s:
    /// illegal line count.", …)`; carries the offending value for the message.
    IllegalLineCount(&'a [u8]),
    /// A `-b` value that is not decimal (with optional `k`/`m` unit) or is
    /// zero: C `errx(1, "%s: illegal byte count.", …)` (split.c:99-108).
    IllegalByteCount(&'a [u8]),
    /// A `-n` value that is not all-decimal or is zero: C `errx(1, "%s:
    /// illegal number of chunks.", …)` (split.c:123-127).
    IllegalChunkCount(&'a [u8]),
    /// A `-a` value that is not all-decimal or is zero: C `errx(1, "%s:
    /// illegal suffix length.", …)` (split.c:117-121).
    IllegalSuffixLength(&'a [u8]),
}

/// An all-decimal, nonzero line count, or `None` (the C `strtoull` + `*ep !=
/// '\0'` + `== 0` rejection). An over-wide digit run (past `usize`) is also
/// `None` here — an absurd input C would saturate, treated as illegal rather
/// than panicking.
fn line_count(v: &[u8]) -> Option<usize> {
    if v.is_empty() || !v.iter().all(u8::is_ascii_digit) {
        return None;
    }
    decimal(v)
}

/// A decimal digit run folded one byte at a time, nonzero, and `None` once it
/// no longer fits `usize` — the same answer the old `parse::<usize>()` gave for
/// an over-wide run (absurd input C saturates; here it is illegal, never a
/// panic).
fn decimal(digits: &[u8]) -> Option<usize> {
    let mut acc = 0usize;
    for &byte in digits {
        acc = acc.checked_mul(10)?.checked_add(usize::from(byte - b'0'))?;
    }
    if acc > 0 { Some(acc) } else { None }
}

/// A `-b` byte count: decimal with an optional lowercase `k`/`m` unit
/// (×1024 / ×1048576), nonzero (`split.c:99-108`). `None` for an illegal or
/// zero count, or a product that overflows `usize` (absurd input).
fn byte_count(v: &[u8]) -> Option<usize> {
    let (digits, mul) = match *v.last()? {
        b'k' => (&v[..v.len() - 1], 1024usize),
        b'm' => (&v[..v.len() - 1], 1024 * 1024),
        _ => (v, 1usize),
    };
    if digits.is_empty() || !digits.iter().all(u8::is_ascii_digit) {
        return None;
    }
    decimal(digits).and_then(|n| n.checked_mul(mul))
}

/// Fold `split`'s argv tail faithfully to BSD `getopt("0123456789b:l:a:n:")`
/// (`split.c:80`).
///
/// Parity over the old whole-token matcher (`arg == "-l"`, value forced onto a
/// separate word): `-l` now takes its value either attached (`-l100`) or as the
/// next word (`-l 100`), and an all-digit `-N` is the obsolete line-count
/// kludge (`split.c:82-98`, applied only while the count is unset so a later
/// `-l` still trips the `numlines != 0` usage as C does). `--` ends option
/// scanning; a bare `-` and any non-option token are operands. `-a` sets the
/// suffix width, `-b` the byte chunk size, and `-n` the chunk count
/// (attached or next word); the three split modes are mutually exclusive.
pub fn parse_split_args<'a>(args: &'a [&'a [u8]]) -> Result<SplitArgs<'a>, SplitError<'a>> {
    let mut lines_per_chunk = DEFAULT_LINES;
    let mut sfxlen = DEFAULT_SFXLEN;
    let mut byte_size: Option<usize> = None;
    let mut chunk_count: Option<usize> = None;
    let mut set = false;
    let mut operands: Vec<&'a [u8]> = Vec::new();

    let mut index = 0usize;
    while index < args.len() {
        let arg = args[index];
        if arg == b"--" {
            operands.extend_from_slice(&args[index + 1..]);
            break;
        }
        let bytes = arg;
        if bytes.len() > 1 && bytes[0] == b'-' {
            let tail = &arg[1..];
            // C's legacy form `-<digits>`: all six letters `0-9` are **legal**
            // in the optstring, and once the first digit comes back,
            // `split.c:86-90` takes the digits from `argv[optind] + 1` (the
            // whole token minus that `-`), so `-1000` sets 1000 lines and
            // `-12x` lands on `12x: illegal line count.`, while `-3b5` hands
            // the whole `3b5` to `strtoull` (it is not just "an all-digit
            // tail").
            if bytes[1].is_ascii_digit() {
                if !set {
                    match line_count(tail) {
                        Some(n) => {
                            lines_per_chunk = n;
                            set = true;
                        }
                        None => return Err(SplitError::IllegalLineCount(tail)),
                    }
                }
                index += 1;
                continue;
            }
            match bytes[1] {
                b'l' => {
                    // C `case 'l'`: `if (numlines != 0) usage()`.
                    if set {
                        return Err(SplitError::Usage);
                    }
                    // Value attached after `l` (`-l100`) or the next word
                    // (`-l 100`).
                    let rest = &tail[1..];
                    let value = if rest.is_empty() {
                        index += 1;
                        match args.get(index) {
                            Some(v) => *v,
                            // `-l` with no next word: libc prints the
                            // missing-value line first (`getopt.c:132-136`).
                            None => return Err(SplitError::MissingValue('l')),
                        }
                    } else {
                        rest
                    };
                    match line_count(value) {
                        Some(n) => lines_per_chunk = n,
                        None => return Err(SplitError::IllegalLineCount(value)),
                    }
                    set = true;
                }
                b'a' => {
                    // C `case 'a'`: value attached (`-a3`) or next word (`-a 3`),
                    // all-digit and nonzero, else "illegal suffix length".
                    let rest = &tail[1..];
                    let value = if rest.is_empty() {
                        index += 1;
                        match args.get(index) {
                            Some(v) => *v,
                            None => return Err(SplitError::MissingValue('a')),
                        }
                    } else {
                        rest
                    };
                    match line_count(value) {
                        Some(n) => sfxlen = n,
                        None => return Err(SplitError::IllegalSuffixLength(value)),
                    }
                }
                b'b' => {
                    // C `case 'b'`: value attached (`-b1000`) or next word
                    // (`-b 1000`), decimal with optional k/m, nonzero.
                    let rest = &tail[1..];
                    let value = if rest.is_empty() {
                        index += 1;
                        match args.get(index) {
                            Some(v) => *v,
                            None => return Err(SplitError::MissingValue('b')),
                        }
                    } else {
                        rest
                    };
                    match byte_count(value) {
                        Some(n) => byte_size = Some(n),
                        None => return Err(SplitError::IllegalByteCount(value)),
                    }
                }
                b'n' => {
                    // C `case 'n'`: all-decimal, nonzero, no trailing char
                    // (no k/m unit) — `split.c:123-127`.
                    let rest = &tail[1..];
                    let value = if rest.is_empty() {
                        index += 1;
                        match args.get(index) {
                            Some(v) => *v,
                            None => return Err(SplitError::MissingValue('n')),
                        }
                    } else {
                        rest
                    };
                    match line_count(value) {
                        Some(n) => chunk_count = Some(n),
                        None => return Err(SplitError::IllegalChunkCount(value)),
                    }
                }
                // A letter outside the set: `getopt.c:95-104` prints one line,
                // then `split.c:128-129`'s `default: usage()` appends the
                // second. What is handed on is the **first byte** after the
                // `-` (both a cluster and `--x` report only the first:
                // measured, `-Zb` reports `Z` and `--x` reports `-`;
                // `getopt.c:79`'s `optopt = *place++` takes one byte at a time
                // and `:103`'s `%c` writes it out verbatim, so `é` (`c3 a9`)
                // reports `c3`).
                _ => {
                    return Err(SplitError::UnknownOption(char::from(bytes[1])));
                }
            }
            index += 1;
            continue;
        }
        // **No rearrangement**: as soon as a non-option token arrives, C's
        // option loop ends on the spot (`getopt.c:74-78` returns -1 when it
        // does not see `-`), and that token plus everything after it fall into
        // the `argv += optind` slice — the first is the input file, the second
        // the prefix, and a third one is usage (`split.c:135-150`). So the `-Z`
        // in `split in -Z` is a **prefix**, not an option (reference
        // measurement: it tries to open `in` and reports
        // `split: in: No such file or directory`).
        operands.extend_from_slice(&args[index..]);
        break;
    }

    // The third operand is **not** judged on this leg. C's order is: open the
    // input first (`split.c:135-139`, where a failed open calls
    // `err(1, "%s", *argv)` on the spot), then take the prefix (`:143`), and
    // only then `if (*argv != NULL) usage()` (`:148-149`). This layer cannot
    // see whether the open succeeded, so judging here would report
    // `split <unopenable> <prefix> <third>` as usage, whereas the reference
    // reports that file name. The check therefore lives on the shell side,
    // after the open.
    // C: `if (numlines == 0) numlines = DEFLINE; else if (bytecnt || chunks) usage()`;
    // `if (bytecnt && chunks) usage()` — the three modes are mutually exclusive.
    if set && (byte_size.is_some() || chunk_count.is_some()) {
        return Err(SplitError::Usage);
    }
    if byte_size.is_some() && chunk_count.is_some() {
        return Err(SplitError::Usage);
    }
    Ok(SplitArgs {
        lines_per_chunk,
        byte_size,
        chunk_count,
        sfxlen,
        operands,
    })
}

/// A **streaming** cutter for fixed-size byte chunks: it turns each read
/// buffer into pieces of two kinds, "write to the current chunk" and "start a
/// new chunk", following `split1` in
/// `minix3/usr.bin/split/split.c:175-236`.
///
/// The only thing C carries across two `read` calls is one counter (`bcnt`:
/// how many bytes the current chunk already holds); it keeps no data, so the
/// same two numbers are all this layer needs: bytes written and whether the
/// current chunk is still open.
/// One key detail comes from C's `else file_open = 0` (`split.c:228-230`):
/// after a chunk is filled **exactly**, that chunk counts as closed, and the
/// first bytes of the next read belong to a **new** chunk rather than
/// continuing into the file just finished.
pub struct ByteCutter {
    size: usize,
    filled: usize,
    open: bool,
}

/// One stretch of bytes to write out and the chunk it belongs in: when
/// `opens_new_file` is true, the previous chunk is closed first, the suffix
/// advances by one, and a new file is created (C's `newfile()`,
/// `split.c:309-335`).
#[derive(Debug, PartialEq, Eq)]
pub struct CutPiece<'a> {
    pub bytes: &'a [u8],
    pub opens_new_file: bool,
}

impl ByteCutter {
    /// Build a cutter of `size` bytes per chunk. A zero `size` is the
    /// caller's error, the same convention as [`chunk_bytes`] returning an
    /// empty result for `n == 0`.
    pub fn new(size: usize) -> Self {
        Self {
            size,
            filled: 0,
            open: false,
        }
    }

    /// Cut one read buffer `piece` into several pieces; their order is the
    /// write order.
    pub fn take<'a>(&mut self, piece: &'a [u8]) -> Vec<CutPiece<'a>> {
        let mut out = Vec::new();
        if self.size == 0 {
            return out;
        }
        let mut rest = piece;
        while !rest.is_empty() {
            let opens_new_file = !self.open;
            if opens_new_file {
                self.open = true;
                self.filled = 0;
            }
            let room = self.size - self.filled;
            let give = room.min(rest.len());
            out.push(CutPiece {
                bytes: &rest[..give],
                opens_new_file,
            });
            self.filled += give;
            rest = &rest[give..];
            if self.filled == self.size {
                self.open = false;
                self.filled = 0;
            }
        }
        out
    }
}

/// A **streaming** cutter for fixed-line-count chunks: it turns each read
/// buffer into pieces of two kinds, "write to the current chunk" and "start a
/// new chunk", following `split2` in
/// `minix3/usr.bin/split/split.c:242-282`.
///
/// The protocol is the same as [`ByteCutter`]'s; only the cut point changes
/// from a byte count to a newline count. Two numbers are still all that
/// crosses a `read` boundary (how many whole lines the current chunk has
/// counted, and whether it is still open), and **no data at all** is kept: a
/// line spanning two buffers is written into the same still-open chunk in two
/// pieces, so there is no reassembly buffer here, and there should not be one
/// — it would drag the resident footprint back up to "the longest line".
pub struct LineCutter {
    numlines: usize,
    lines: usize,
    open: bool,
}

impl LineCutter {
    /// Build a cutter of `numlines` lines per chunk. A zero `numlines` is the
    /// caller's error, the same convention as [`ByteCutter::new`] for a zero
    /// `size`.
    pub fn new(numlines: usize) -> Self {
        Self {
            numlines,
            lines: 0,
            open: false,
        }
    }

    /// Cut one read buffer `piece` into several pieces; their order is the
    /// write order.
    pub fn take<'a>(&mut self, piece: &'a [u8]) -> Vec<CutPiece<'a>> {
        let mut out = Vec::new();
        if self.numlines == 0 {
            return out;
        }
        let mut start = 0usize;
        for (index, byte) in piece.iter().enumerate() {
            if *byte != b'\n' {
                continue;
            }
            self.lines += 1;
            if self.lines != self.numlines {
                continue;
            }
            self.lines = 0;
            out.push(CutPiece {
                bytes: &piece[start..index + 1],
                opens_new_file: !self.open,
            });
            // After a cut point there is no "still-open chunk" any more: the
            // next piece decides for itself via `!open` whether to start a new
            // one, so "the cut point falls in the middle of a piece" and "the
            // cut point lands exactly at the end of a piece" share one path.
            self.open = false;
            start = index + 1;
        }
        if start < piece.len() {
            out.push(CutPiece {
                bytes: &piece[start..],
                opens_new_file: !self.open,
            });
            self.open = true;
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The fixture's argv, exactly as `std::env::args_os()` hands it over: a
    /// `Vec` of byte slices. Written through `str` literals here because these
    /// shapes are readable that way; the three forms whose **bytes** are the
    /// point (`-\u{e9}` and its two siblings) spell the `c3 a9` pair out instead.
    fn argv<'a>(words: &[&'a str]) -> Vec<&'a [u8]> {
        words.iter().map(|word| word.as_bytes()).collect()
    }

    #[test]
    fn test_default_chunks_of_thousand() {
        let mut data = Vec::new();
        for _ in 0..2500 {
            data.extend_from_slice(b"line\n");
        }
        let lines = lines_bytes(&data);
        let chunks = chunks_bytes(&lines, DEFAULT_LINES);
        assert_eq!(chunks.len(), 3);
        assert_eq!(chunks[0].len(), 1000);
        assert_eq!(chunks[1].len(), 1000);
        assert_eq!(chunks[2].len(), 500);
        assert_eq!(chunks[2][499], &data[data.len() - 5..]);
    }

    #[test]
    fn test_explicit_count_and_empty_input() {
        let data = *b"1\n2\n3\n4\n5\n";
        let lines = lines_bytes(&data);
        let groups = chunks_bytes(&lines, 2);
        assert_eq!(groups.len(), 3);
        assert_eq!(groups[2], vec![&b"5\n"[..]]);
        assert!(chunks_bytes(&[], 10).is_empty());
    }

    #[test]
    fn test_zero_count_is_no_chunks() {
        let lines = lines_bytes(b"a\n");
        assert!(chunks_bytes(&lines, 0).is_empty());
    }

    #[test]
    fn lines_bytes_carries_the_terminator_with_its_line() {
        assert_eq!(lines_bytes(b"a\nb\n"), vec![&b"a\n"[..], &b"b\n"[..]]);
        // An empty line is a line of its own, with its terminator as its whole
        // content; a last line without a newline stays without one.
        assert_eq!(
            lines_bytes(b"a\n\nb\n"),
            vec![&b"a\n"[..], &b"\n"[..], &b"b\n"[..]]
        );
        assert_eq!(lines_bytes(b"a\nb"), vec![&b"a\n"[..], &b"b"[..]]);
        assert_eq!(lines_bytes(b"a"), vec![&b"a"[..]]);
        assert!(lines_bytes(b"").is_empty());
    }

    #[test]
    fn lines_bytes_is_not_an_encoding_check() {
        // Undecodable bytes are ordinary data (C counts the newlines in the
        // read buffer).
        assert_eq!(
            lines_bytes(b"aa\xffbb\ncc\xfe\n"),
            vec![&b"aa\xffbb\n"[..], &b"cc\xfe\n"[..]]
        );
    }

    #[test]
    fn test_suffix_enumerates_two_letters() {
        // Default width=2: aa..zz unchanged character for character (zero
        // regression).
        assert_eq!(suffix(0, 2).as_deref(), Some("aa"));
        assert_eq!(suffix(1, 2).as_deref(), Some("ab"));
        assert_eq!(suffix(25, 2).as_deref(), Some("az"));
        assert_eq!(suffix(26, 2).as_deref(), Some("ba"));
        assert_eq!(suffix(675, 2).as_deref(), Some("zz"));
        assert_eq!(suffix(676, 2), None);
    }

    #[test]
    fn test_suffix_width_follows_sfxlen() {
        // -a 1: a single letter a..z (capacity 26).
        assert_eq!(suffix(0, 1).as_deref(), Some("a"));
        assert_eq!(suffix(25, 1).as_deref(), Some("z"));
        assert_eq!(suffix(26, 1), None);
        // -a 3: three letters starting at aaa; index 26 = "aab".
        assert_eq!(suffix(0, 3).as_deref(), Some("aaa"));
        assert_eq!(suffix(1, 3).as_deref(), Some("aab"));
        assert_eq!(suffix(26 * 26 - 1, 3).as_deref(), Some("azz"));
        assert_eq!(suffix(26 * 26 * 26, 3), None); // past the 26^3 enumeration
        // width 0 is illegal → None.
        assert_eq!(suffix(0, 0), None);
    }

    #[test]
    fn parse_default_and_separated_value() {
        // No arguments: the default of 1000 lines. Separated values (still
        // accepted).
        let w_d = argv(&[]);
        let d = parse_split_args(&w_d).unwrap();
        assert_eq!(d.lines_per_chunk, DEFAULT_LINES);
        assert!(d.operands.is_empty());
        let w_a = argv(&["-l", "100", "f"]);
        let a = parse_split_args(&w_a).unwrap();
        assert_eq!(a.lines_per_chunk, 100);
        assert_eq!(a.operands, vec![b"f".as_slice()]);
    }

    #[test]
    fn parse_attached_and_obsolete_accepted() {
        // Key regression: the old front door rejected these two legal forms.
        let w_a = argv(&["-l100", "f"]);
        let a = parse_split_args(&w_a).unwrap();
        assert_eq!(a.lines_per_chunk, 100);
        assert_eq!(a.operands, vec![b"f".as_slice()]);
        let w_b = argv(&["-1000", "in", "out"]);
        let b = parse_split_args(&w_b).unwrap();
        assert_eq!(b.lines_per_chunk, 1000);
        assert_eq!(b.operands, vec![b"in".as_slice(), b"out".as_slice()]);
        // A bare `-` is an operand; `--` ends scanning.
        let w_c = argv(&["-l", "5", "-", "pfx"]);
        let c = parse_split_args(&w_c).unwrap();
        assert_eq!(c.operands, vec![b"-".as_slice(), b"pfx".as_slice()]);
        let w_e = argv(&["--", "-l", "-x"]);
        let e = parse_split_args(&w_e).unwrap();
        assert_eq!(e.operands, vec![b"-l".as_slice(), b"-x".as_slice()]);
        // The obsolete form after a `-l` is ignored (C only interprets it
        // `if (numlines == 0)`).
        let w_f = argv(&["-l", "3", "-5"]);
        let f = parse_split_args(&w_f).unwrap();
        assert_eq!(f.lines_per_chunk, 3);
    }

    #[test]
    fn parse_suffix_length_minus_a() {
        // -a takes a value: attached `-a3` and separated `-a 3` both set
        // sfxlen=3; the default is 2.
        assert_eq!(parse_split_args(&argv(&["-a3", "f"])).unwrap().sfxlen, 3);
        assert_eq!(
            parse_split_args(&argv(&["-a", "3", "f"])).unwrap().sfxlen,
            3
        );
        // Once -a consumes its value, the `3` does not leak out as an operand:
        // `-a3 f` leaves just the one operand `f`.
        assert_eq!(
            parse_split_args(&argv(&["-a3", "f"])).unwrap().operands,
            vec![b"f".as_slice()]
        );
        // Without -a, sfxlen defaults to 2.
        assert_eq!(
            parse_split_args(&argv(&["f"])).unwrap().sfxlen,
            DEFAULT_SFXLEN
        );
        // Illegal suffix length (0 / not all digits): matches C's "illegal
        // suffix length".
        assert_eq!(
            parse_split_args(&argv(&["-a0"])).err(),
            Some(SplitError::IllegalSuffixLength(b"0"))
        );
        assert_eq!(
            parse_split_args(&argv(&["-a", "x"])).err(),
            Some(SplitError::IllegalSuffixLength(b"x"))
        );
    }

    #[test]
    fn parse_byte_count_minus_b() {
        // -b takes a value: attached or separated, with k/m units. The line
        // count stays at its default (byte mode does not count lines).
        assert_eq!(
            parse_split_args(&argv(&["-b", "100"])).unwrap().byte_size,
            Some(100)
        );
        assert_eq!(
            parse_split_args(&argv(&["-b1000"])).unwrap().byte_size,
            Some(1000)
        );
        assert_eq!(
            parse_split_args(&argv(&["-b", "4k"])).unwrap().byte_size,
            Some(4096)
        );
        assert_eq!(
            parse_split_args(&argv(&["-b2m"])).unwrap().byte_size,
            Some(2 * 1024 * 1024)
        );
        // Without -b, byte_size is None (line mode).
        assert_eq!(parse_split_args(&argv(&["f"])).unwrap().byte_size, None);
        // Illegal byte count (0 / non-digit): "illegal byte count".
        assert_eq!(
            parse_split_args(&argv(&["-b0"])).err(),
            Some(SplitError::IllegalByteCount(b"0"))
        );
        assert_eq!(
            parse_split_args(&argv(&["-b", "x"])).err(),
            Some(SplitError::IllegalByteCount(b"x"))
        );
        // -l together with -b (either order): C rules usage in the closing
        // `else if (bytecnt…)`.
        assert_eq!(
            parse_split_args(&argv(&["-l", "3", "-b", "10"])).err(),
            Some(SplitError::Usage)
        );
        assert_eq!(
            parse_split_args(&argv(&["-b", "10", "-l", "3"])).err(),
            Some(SplitError::Usage)
        );
    }

    #[test]
    fn chunk_bytes_splits_fixed_and_last_short() {
        let data = *b"abcdefghij";
        let c = chunk_bytes(&data, 4);
        assert_eq!(c.len(), 3);
        assert_eq!(c[0], b"abcd");
        assert_eq!(c[1], b"efgh");
        assert_eq!(c[2], b"ij");
        assert!(chunk_bytes(&data, 0).is_empty());
        assert!(chunk_bytes(b"", 5).is_empty());
        assert_eq!(chunk_bytes(&data, 20), vec![&data[..]]);
    }

    #[test]
    fn chunks_n_makes_exactly_n_with_remainder_in_last() {
        let data = *b"abcdefghij"; // 10 bytes
        let c = chunks_n(&data, 3).unwrap();
        assert_eq!(c.len(), 3);
        assert_eq!(c[0], b"abc"); // 10/3 = 3
        assert_eq!(c[1], b"def");
        assert_eq!(c[2], b"ghij"); // the last chunk absorbs the remainder
        // n == len: one byte per chunk.
        assert_eq!(
            chunks_n(b"abc", 3).unwrap(),
            vec![&b"a"[..], &b"b"[..], &b"c"[..]]
        );
        // n > len → None (C "can't split into more than %d files").
        assert_eq!(chunks_n(b"ab", 3), None);
        assert_eq!(chunks_n(b"", 1), None);
        assert_eq!(chunks_n(b"abc", 0), None);
    }

    #[test]
    fn parse_chunk_count_minus_n() {
        assert_eq!(
            parse_split_args(&argv(&["-n", "4"])).unwrap().chunk_count,
            Some(4)
        );
        assert_eq!(
            parse_split_args(&argv(&["-n4"])).unwrap().chunk_count,
            Some(4)
        );
        assert_eq!(parse_split_args(&argv(&["f"])).unwrap().chunk_count, None);
        assert_eq!(
            parse_split_args(&argv(&["-n0"])).err(),
            Some(SplitError::IllegalChunkCount(b"0"))
        );
        // The three modes are pairwise exclusive: -n together with -l / -b is
        // usage.
        assert_eq!(
            parse_split_args(&argv(&["-l", "3", "-n", "4"])).err(),
            Some(SplitError::Usage)
        );
        assert_eq!(
            parse_split_args(&argv(&["-b", "10", "-n", "4"])).err(),
            Some(SplitError::Usage)
        );
    }

    /// The unknown letter handed back is the **first byte** after the hyphen,
    /// not the first character: `-\u{e9}` (`c3 a9`) is U+00C3 in a `char`
    /// field. Two absolute readings: on the mechanism side, `getopt.c:79`'s
    /// `optopt = *place++` (stepping over a `char *`, one byte at a time) and
    /// `:103`'s `%c`; on the reference side, `-é` printed a single `303` (the
    /// byte-wise gate is in `tests/split_face.rs`).
    #[test]
    fn the_unknown_letter_is_the_first_byte_not_the_first_character() {
        assert_eq!(
            parse_split_args(&[b"-\xc3\xa9"]).err(),
            Some(SplitError::UnknownOption('\u{00c3}')),
            "`é` 是 c3 a9 两字节，libc 报 c3 而不是整个字符"
        );
        // The loop stops at the first bad letter: `-Zé` reports only `Z`.
        assert_eq!(
            parse_split_args(&[b"-Z\xc3\xa9"]).err(),
            Some(SplitError::UnknownOption('Z'))
        );
        // The ASCII shape must not be disturbed by this change.
        assert_eq!(
            parse_split_args(&argv(&["-q"])).err(),
            Some(SplitError::UnknownOption('q'))
        );
        // Counter-case: a value-taking letter swallows every following byte as
        // its value, so the letter position never comes up (the reference gives
        // the same sentence, `split: é: illegal line count.`, exit 1, no
        // usage).
        assert_eq!(
            parse_split_args(&[b"-l\xc3\xa9"]).err(),
            Some(SplitError::IllegalLineCount(b"\xc3\xa9"))
        );
    }

    #[test]
    fn parse_error_paths() {
        // The unknown letter is handed back as the letter itself, and the shell
        // prints the libc line before the usage (reference: `-Z` gives two
        // lines).
        assert_eq!(
            parse_split_args(&argv(&["-z", "f"])).err(),
            Some(SplitError::UnknownOption('z'))
        );
        // A cluster and `--x` report only the first byte (reference: `-Zb`
        // reports `Z`, `--x` reports `-`).
        assert_eq!(
            parse_split_args(&argv(&["-Zb"])).err(),
            Some(SplitError::UnknownOption('Z'))
        );
        assert_eq!(
            parse_split_args(&argv(&["--x"])).err(),
            Some(SplitError::UnknownOption('-'))
        );
        // A missing value is libc's other sentence (`getopt.c:132-136`), not
        // the usage line itself.
        assert_eq!(
            parse_split_args(&argv(&["-l"])).err(),
            Some(SplitError::MissingValue('l'))
        );
        assert_eq!(
            parse_split_args(&argv(&["-b"])).err(),
            Some(SplitError::MissingValue('b'))
        );
        assert_eq!(
            parse_split_args(&argv(&["-a"])).err(),
            Some(SplitError::MissingValue('a'))
        );
        assert_eq!(
            parse_split_args(&argv(&["-n"])).err(),
            Some(SplitError::MissingValue('n'))
        );
        // The legacy form takes the whole token minus that `-` (C's
        // `argv[optind] + 1`): a stray trailing character makes the whole
        // token an illegal line count rather than an unknown letter.
        assert_eq!(
            parse_split_args(&argv(&["-12x"])).err(),
            Some(SplitError::IllegalLineCount(b"12x"))
        );
        assert_eq!(
            parse_split_args(&argv(&["-3b5"])).err(),
            Some(SplitError::IllegalLineCount(b"3b5"))
        );
        // **No rearrangement**: as soon as a non-option token arrives, it and
        // everything after it are handed back as operands, so the `-Z` in
        // `split in -Z` is a prefix, not an option (reference measurement).
        assert_eq!(
            parse_split_args(&argv(&["in", "-Z"])).unwrap().operands,
            vec![b"in".as_slice(), b"-Z".as_slice()]
        );
        assert_eq!(
            parse_split_args(&argv(&["in", "-l"])).unwrap().operands,
            vec![b"in".as_slice(), b"-l".as_slice()],
            "操作数之后的 `-l` 不再被当选项读，而是当输出前缀"
        );
        assert_eq!(
            parse_split_args(&argv(&["in", "-l"]))
                .unwrap()
                .lines_per_chunk,
            DEFAULT_LINES,
            "没被当选项读就不改列数（C 的 `getopt` 见非选项即返 -1）"
        );
        assert_eq!(
            parse_split_args(&argv(&["in", "-l", "3"]))
                .unwrap()
                .operands,
            vec![b"in".as_slice(), b"-l".as_slice(), b"3".as_slice()],
            "不重排之后这是**三个**操作数：档名、前缀、多出来的一支。取数这一层不判它，\
             因为 C 的 `usage()`（`split.c:148-149`）排在开档（`:135-139`）之后，\
             而开档的成败只有壳侧知道"
        );
        assert_eq!(
            parse_split_args(&argv(&["a", "b", "c"])).unwrap().operands,
            vec![b"a".as_slice(), b"b".as_slice(), b"c".as_slice()],
            "多于两个操作数在库里只是三个操作数，那一记 usage 由壳侧在开档之后落下"
        );
        assert_eq!(
            parse_split_args(&argv(&["-l0"])).err(),
            Some(SplitError::IllegalLineCount(b"0"))
        );
        assert_eq!(
            parse_split_args(&argv(&["-lx"])).err(),
            Some(SplitError::IllegalLineCount(b"x"))
        );
        assert_eq!(
            parse_split_args(&argv(&["-l", "3", "-l", "4"])).err(),
            Some(SplitError::Usage)
        );
        assert_eq!(
            parse_split_args(&argv(&["-5", "-l", "3"])).err(),
            Some(SplitError::Usage)
        );
    }

    /// The cutter and the whole-buffer cut must be byte-for-byte equivalent:
    /// for the same input, no matter which boundaries split it into how many
    /// reads, grouping the pieces into files by `opens_new_file` must give
    /// exactly the result of one `chunk_bytes` cut. This test is the proof that
    /// streaming does not change visible behaviour; the read boundaries cover
    /// "across a piece", "exactly filling a chunk", and "one read spanning
    /// several pieces".
    #[test]
    fn the_cutter_agrees_with_the_whole_buffer_cut_at_any_read_boundary() {
        let data: Vec<u8> = (0u8..40).collect();
        for size in [1usize, 3, 5, 8, 40, 41] {
            for boundaries in [
                vec![data.len()],
                vec![1, 1, 1, data.len() - 3],
                vec![7, 7, 7, 7, 7, 5],
                vec![5, 5, 5, 5, 5, 5, 5, 5],
                vec![13, 13, 14],
                vec![40],
                vec![20, 20],
                vec![41],
            ] {
                let mut cutter = ByteCutter::new(size);
                let mut files: Vec<Vec<u8>> = Vec::new();
                let mut offset = 0usize;
                for step in &boundaries {
                    let piece = &data[offset..(offset + step).min(data.len())];
                    offset += step;
                    for part in cutter.take(piece) {
                        if part.opens_new_file || files.is_empty() {
                            files.push(Vec::new());
                        }
                        files.last_mut().unwrap().extend_from_slice(part.bytes);
                    }
                    if offset >= data.len() {
                        break;
                    }
                }
                let whole = chunk_bytes(&data, size)
                    .into_iter()
                    .map(|chunk| chunk.to_vec())
                    .collect::<Vec<Vec<u8>>>();
                assert_eq!(files, whole, "size={size} boundaries={boundaries:?}");
            }
        }
    }

    /// After an exact fill the current chunk counts as closed: the first bytes
    /// of the next read must belong to a **new** chunk. This comes from C's
    /// `else file_open = 0` (`split.c:228-230`) and is the shape this layer is
    /// most likely to get wrong.
    #[test]
    fn an_exact_fill_closes_the_file_so_the_next_read_starts_a_new_one() {
        let head = [1u8, 2, 3, 4];
        let tail = [5u8, 6];
        let mut cutter = ByteCutter::new(4);
        let first = cutter.take(&head);
        assert_eq!(
            first,
            vec![CutPiece {
                bytes: &[1, 2, 3, 4],
                opens_new_file: true
            }]
        );
        let second = cutter.take(&tail);
        assert_eq!(
            second,
            vec![CutPiece {
                bytes: &[5, 6],
                opens_new_file: true
            }],
            "切满之后再读，不许接着灌上一枚分片"
        );
    }

    /// When one read spans several chunks, each chunk starts its own file, and
    /// **no zero-length tail** is left behind (that is what C's `if (len)`
    /// means, `split.c:221-230`).
    #[test]
    fn one_read_spanning_several_chunks_leaves_no_empty_tail() {
        let mut cutter = ByteCutter::new(4);
        let ten: Vec<u8> = (0u8..10).collect();
        let pieces = cutter.take(&ten);
        assert_eq!(pieces.len(), 3, "10 字节按 4 切是 4/4/2 三段");
        assert!(pieces.iter().all(|p| p.opens_new_file));
        assert_eq!(pieces[0].bytes.len(), 4);
        assert_eq!(pieces[1].bytes.len(), 4);
        assert_eq!(pieces[2].bytes.len(), 2);
        assert!(!pieces.iter().any(|p| p.bytes.is_empty()));
    }

    /// When a read is shorter than one chunk, the next block must **continue
    /// inside the same chunk file**: this is the only shape in which
    /// cross-read state really matters (C keeps it in `bcnt`).
    #[test]
    fn a_short_read_continues_inside_the_same_chunk_file() {
        let head = [1u8, 2, 3];
        let rest = [4u8, 5, 6, 7, 8, 9, 10];
        let mut cutter = ByteCutter::new(8);
        let first = cutter.take(&head);
        assert_eq!(first.len(), 1);
        assert!(first[0].opens_new_file);
        let second = cutter.take(&rest);
        assert_eq!(second.len(), 2, "补足 8 之后再起新的一枚");
        assert!(!second[0].opens_new_file);
        assert_eq!(second[0].bytes, &[4, 5, 6, 7, 8]);
        assert!(second[1].opens_new_file);
        assert_eq!(second[1].bytes, &[9, 10]);
    }

    /// A zero `size` is the caller's error, the same convention as
    /// `chunk_bytes`: nothing is cut at all, rather than silently swallowing
    /// the input or spinning in place.
    #[test]
    fn a_zero_sized_cutter_produces_no_pieces() {
        let mut cutter = ByteCutter::new(0);
        assert!(cutter.take(&[1, 2, 3]).is_empty());
        assert_eq!(chunk_bytes(&[1u8, 2, 3], 0), Vec::<&[u8]>::new());
    }

    /// Group the pieces [`LineCutter`] hands back **strictly by
    /// `opens_new_file`**. The byte-leg equivalence gate kept an
    /// `|| files.is_empty()` escape; this one does not: the first piece must
    /// say for itself that it starts a new chunk, otherwise "no file was
    /// opened" and "the open timing is wrong" look the same to the gate.
    fn streaming_files(data: &[u8], piece_len: usize, numlines: usize) -> Vec<Vec<u8>> {
        let mut cutter = LineCutter::new(numlines);
        let mut files: Vec<Vec<u8>> = Vec::new();
        let mut offset = 0usize;
        while offset < data.len() {
            let piece = &data[offset..(offset + piece_len).min(data.len())];
            offset += piece_len;
            for part in cutter.take(piece) {
                assert!(!part.bytes.is_empty(), "切块器不许交回零长度的片段");
                if part.opens_new_file {
                    files.push(Vec::new());
                }
                files
                    .last_mut()
                    .expect("片段必属某一枚分片")
                    .extend_from_slice(part.bytes);
            }
        }
        files
    }

    /// The file sequence of one whole-buffer cut, i.e. the reference half of
    /// the streaming leg (the same method as `chunk_bytes` serving as the
    /// reference for the byte leg).
    fn whole_files(data: &[u8], numlines: usize) -> Vec<Vec<u8>> {
        chunks_bytes(&lines_bytes(data), numlines)
            .into_iter()
            .map(|chunk| chunk.concat())
            .collect()
    }

    /// The shape set: empty, one line without a newline, a single empty line, a
    /// last line without a newline, cuts exactly on a newline, empty lines in
    /// the middle, and undecodable bytes as data. Every length here is at most
    /// 40 bytes, so the gate below can try every piece length **one by one**.
    fn small_shapes() -> Vec<Vec<u8>> {
        vec![
            Vec::new(),
            b"a".to_vec(),
            b"\n".to_vec(),
            b"a\nb\nc".to_vec(),
            b"a\nb\nc\n".to_vec(),
            b"a\n\nb\n".to_vec(),
            b"aa\xffbb\ncc\xfe\ndd\n".to_vec(),
            (0..40u8)
                .map(|i| if i % 4 == 3 { b'\n' } else { b'x' })
                .collect(),
        ]
    }

    /// The equivalence gate (proof that streaming does not change visible
    /// behaviour, small shapes scanned over every piece length): for the same
    /// input, no matter which boundaries form the read blocks, the files handed
    /// back must be **byte for byte** the same as one whole-buffer cut. Piece
    /// lengths run over `1..=len+1`, so shapes like "the cut lands exactly at
    /// the end of a piece", "one line spans two pieces", and "the last byte is
    /// the newline" need no hand-picked cases.
    #[test]
    fn the_line_cutter_agrees_with_the_whole_buffer_cut_at_any_read_boundary() {
        for data in small_shapes() {
            for numlines in [1usize, 2, 3, 5] {
                let expected = whole_files(&data, numlines);
                for piece_len in 1..=data.len() + 1 {
                    assert_eq!(
                        streaming_files(&data, piece_len, numlines),
                        expected,
                        "numlines={numlines} piece_len={piece_len} len={}",
                        data.len()
                    );
                }
            }
        }
    }

    /// The same check again on the large shapes: they are too long to try every
    /// piece length, so a representative set of read boundaries is used — one
    /// byte per read, across the cut point, exactly at a line end, across read
    /// blocks, and longer than the input. The 97-byte line and the 60-byte line
    /// are there specifically for "one line spanning two reads". Small shapes
    /// are not in this list: the previous gate already scanned them at every
    /// piece length, and scanning them again would only test the same thing
    /// twice.
    #[test]
    fn the_line_cutter_agrees_on_the_big_shapes_at_the_awkward_read_sizes() {
        let lines = |width: usize, count: usize, mark: u8| -> Vec<u8> {
            (0..(width * count))
                .map(|i| if i % width == width - 1 { b'\n' } else { mark })
                .collect()
        };
        let shapes = vec![
            (0..401u32)
                .map(|i| if i % 100 == 99 { b'\n' } else { b'q' })
                .collect(),
            lines(97, 41, b'l'),
            lines(60, 1200, b'w'),
        ];
        for data in &shapes {
            for numlines in [1usize, 3, 64, 100, 1000] {
                let expected = whole_files(data, numlines);
                for piece_len in [
                    1usize,
                    2,
                    3,
                    5,
                    7,
                    8,
                    13,
                    16,
                    40,
                    64,
                    97,
                    100,
                    128,
                    1000,
                    4096,
                    65536,
                    65537,
                    data.len(),
                    data.len() + 1,
                ] {
                    assert_eq!(
                        streaming_files(data, piece_len, numlines),
                        expected,
                        "numlines={numlines} piece_len={piece_len} len={}",
                        data.len()
                    );
                }
            }
        }
    }

    /// The real read-block shape: C reads one `MAXBSIZE` per `read` (this
    /// shell's `STREAM_CHUNK` = 64 KiB), and 64 lines of 1 KiB fill exactly
    /// 64 KiB. So the cut point lands **exactly** on the boundary between two
    /// reads — this shape must produce exactly two chunks, and a third (empty)
    /// one means "the cut point owns the last byte" was implemented wrongly.
    #[test]
    fn the_real_read_block_shape_makes_two_files_not_three() {
        let data: Vec<u8> = (0..(1024 * 128u32))
            .map(|i| if i % 1024 == 1023 { b'\n' } else { b'z' })
            .collect();
        let files = streaming_files(&data, 64 * 1024, 64);
        assert_eq!(files.len(), 2, "128 KiB 的 1 KiB 行按 64 行切是两枚");
        assert!(files.iter().all(|file| file.len() == 64 * 1024));
        assert_eq!(files, whole_files(&data, 64));
    }

    /// A line spanning two reads: both halves must go into the **same** chunk,
    /// which stays open (that is what C's `if (Cs < Ce)` means,
    /// `split.c:276-280`); a new chunk must not start just because the newline
    /// has not arrived yet.
    #[test]
    fn a_line_straddling_two_reads_stays_in_one_chunk_file() {
        let mut cutter = LineCutter::new(1);
        let head = cutter.take(b"a");
        assert_eq!(
            head,
            vec![CutPiece {
                bytes: b"a",
                opens_new_file: true
            }]
        );
        let body = cutter.take(b"b");
        assert_eq!(
            body,
            vec![CutPiece {
                bytes: b"b",
                opens_new_file: false
            }],
            "行还没结束，不许另起一枚"
        );
        let tail = cutter.take(b"c\n");
        assert_eq!(
            tail,
            vec![CutPiece {
                bytes: b"c\n",
                opens_new_file: false
            }],
            "换行落在这一次读里，收尾仍属同一枚分片"
        );
    }

    /// A cut landing exactly on the last byte of a piece: the next chunk must
    /// **not** be opened yet; it opens only when the next read actually brings
    /// bytes (C's `if (len) newfile(); else file_open = 0;`,
    /// `split.c:271-274`). If the run stops here, it stops on this chunk
    /// without leaving an extra empty file behind.
    #[test]
    fn a_cut_on_the_last_byte_of_a_read_leaves_the_next_file_unopened() {
        let mut cutter = LineCutter::new(1);
        assert_eq!(
            cutter.take(b"a\n"),
            vec![CutPiece {
                bytes: b"a\n",
                opens_new_file: true
            }]
        );
        assert!(
            cutter.take(b"").is_empty(),
            "没有字节进来就不该开下一枚分片"
        );
        assert_eq!(
            cutter.take(b"b\n"),
            vec![CutPiece {
                bytes: b"b\n",
                opens_new_file: true
            }],
            "下一次真读到字节，才由那一段自己宣布另起一枚"
        );
    }

    /// A piece with no newline at all: the whole stretch is poured into the
    /// current chunk, and a new one starts only when no chunk is open.
    #[test]
    fn a_piece_without_any_newline_fills_the_open_chunk() {
        let mut cutter = LineCutter::new(2);
        assert_eq!(
            cutter.take(b"ab"),
            vec![CutPiece {
                bytes: b"ab",
                opens_new_file: true
            }]
        );
        assert_eq!(
            cutter.take(b"cd"),
            vec![CutPiece {
                bytes: b"cd",
                opens_new_file: false
            }]
        );
    }

    /// One read spanning several cut points opens one file per cut point, and
    /// every piece is non-empty (`numlines` of one is the shape most likely to
    /// produce a zero-length tail).
    #[test]
    fn several_cuts_in_one_piece_open_one_file_each() {
        let mut cutter = LineCutter::new(1);
        let pieces = cutter.take(b"a\nb\nc\n");
        assert_eq!(pieces.len(), 3);
        assert!(
            pieces.iter().all(|part| part.opens_new_file),
            "三个切点各起一枚"
        );
        assert!(pieces.iter().all(|part| !part.bytes.is_empty()));
        assert_eq!(
            pieces.iter().map(|part| part.bytes).collect::<Vec<_>>(),
            vec![&b"a\n"[..], &b"b\n"[..], &b"c\n"[..]]
        );
    }

    /// A zero `numlines` is the caller's error, the same convention as the byte
    /// leg: nothing is cut at all.
    #[test]
    fn a_zero_line_count_cutter_produces_no_pieces() {
        let mut cutter = LineCutter::new(0);
        assert!(cutter.take(b"a\nb\n").is_empty());
        assert!(whole_files(b"a\nb\n", 0).is_empty());
    }

    /// The value and operand legs hand back the **raw bytes** of argv, and the
    /// letter position is the first byte after the hyphen. On the C side all
    /// three legs work on bytes: `split.c:112`'s `errx(1, "%s: illegal line
    /// count.", optarg)` prints every byte of `optarg` itself, `getopt.c:79`'s
    /// `optopt = *place++` takes one byte at a time, and the file name
    /// `*argv++` (`:143`) goes straight into `creat`. After one fold, a bare
    /// `ff` at a value position becomes the three bytes `ef bf bd`, and the
    /// prefix is no longer that name before `open` is even reached.
    #[test]
    fn the_value_and_the_operand_legs_hand_back_the_argv_bytes() {
        assert_eq!(
            parse_split_args(&[b"-l\xff"]).err(),
            Some(SplitError::IllegalLineCount(b"\xff".as_slice()))
        );
        assert_eq!(
            parse_split_args(&[b"-b\xff"]).err(),
            Some(SplitError::IllegalByteCount(b"\xff".as_slice()))
        );
        assert_eq!(
            parse_split_args(&[b"-a\xff"]).err(),
            Some(SplitError::IllegalSuffixLength(b"\xff".as_slice()))
        );
        assert_eq!(
            parse_split_args(&[b"-n\xff"]).err(),
            Some(SplitError::IllegalChunkCount(b"\xff".as_slice()))
        );
        // Letter position: `char::from(0xff)` is U+00FF, the same value as the
        // byte libc's `%c` prints after handing back `optopt`, so a high byte
        // must not be folded into the replacement character here.
        assert_eq!(
            parse_split_args(&[b"-\xff"]).err(),
            Some(SplitError::UnknownOption('\u{00ff}')),
            "短横后第一枚字节 ff 交回 U+00FF，不是 U+FFFD"
        );
        // Operand leg: the raw prefix bytes come back along with the file name.
        assert_eq!(
            parse_split_args(&[b"in", b"p\xff"]).unwrap().operands,
            vec![b"in".as_slice(), b"p\xff".as_slice()]
        );
    }
}
