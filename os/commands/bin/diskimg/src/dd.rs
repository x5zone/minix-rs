//! `dd` operand parsing and copy planning.
//!
//! Ground truth: `minix3/bin/dd/args.c` lines 105 to 121 (seventeen
//! operand words: `bs`, `cbs`, `conv`, `count`, `files`, `ibs`, `if`,
//! `iflag`, `iseek`, `msgfmt`, `obs`, `of`, `oflag`, `oseek`, `progress`,
//! `seek`, `skip`), conversions in `conv.c`, and the manual (`dd.1` lines
//! 81 to 262: `files` copies N tape files, `iseek`/`oseek` are synonyms of
//! `skip`/`seek`, `msgfmt` picks `human` or `posix` reports, `progress`
//! switches progress display, `iflag`/`oflag` carry comma separated open
//! words validated against the `olist` table at `args.c:439`).
//!
//! An invocation is `dd [if=in] [of=out] [bs=n] ...`: block sizes (with
//! `K`/`M`/`G` suffixes and `x` multiplication, e.g. `bs=1Mx2`), counts in
//! blocks, skips in input blocks, seeks in output blocks, and conversion
//! lists. This module parses operands into a [`CopyPlan`] (all sizes
//! resolved to bytes, conversions to flags); the byte pumping stays with
//! the execution layer over [`crate::device::BlockDevice`].

use crate::ImageError;

/// Conversion flags (`conv=` list).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Conversions {
    bits: u16,
}

impl Conversions {
    const NO_TRUNC: u16 = 1;
    const SYNC: u16 = 2;
    const NO_ERROR: u16 = 4;
    const LCASE: u16 = 8;
    const UCASE: u16 = 16;
    const SWAB: u16 = 32;

    /// Empty conversion set.
    pub fn empty() -> Self {
        Conversions { bits: 0 }
    }

    /// `notrunc`: keep the output past the copied tail.
    pub fn no_trunc(self) -> bool {
        self.bits & Self::NO_TRUNC != 0
    }

    /// `sync`: pad short reads with zeros.
    pub fn sync(self) -> bool {
        self.bits & Self::SYNC != 0
    }

    /// `noerror`: continue past read errors.
    pub fn no_error(self) -> bool {
        self.bits & Self::NO_ERROR != 0
    }

    /// `lcase` / `ucase`: fold case.
    pub fn lower(self) -> bool {
        self.bits & Self::LCASE != 0
    }

    /// See [`Conversions::lower`].
    pub fn upper(self) -> bool {
        self.bits & Self::UCASE != 0
    }

    /// `swab`: swap adjacent byte pairs.
    pub fn swap(self) -> bool {
        self.bits & Self::SWAB != 0
    }
}

/// One parsed `dd` invocation: every size resolved to bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CopyPlan<'a> {
    /// Input path (`None` means standard input).
    pub input: Option<&'a str>,
    /// Output path (`None` means standard output).
    pub output: Option<&'a str>,
    /// Input block size in bytes.
    pub input_block: u64,
    /// Output block size in bytes.
    pub output_block: u64,
    /// Input blocks to copy (`None` means all).
    pub count: Option<u64>,
    /// Input files to copy (`files=`, tape only; `None` means one).
    pub files: Option<u64>,
    /// Input blocks to skip first (`skip` and `iseek` share it).
    pub skip: u64,
    /// Output blocks to skip first (`seek` and `oseek` share it).
    pub seek: u64,
    /// Open words for the input (`iflag=`), validated spellings.
    pub input_flags: IoFlags<'a>,
    /// Open words for the output (`oflag=`), validated spellings.
    pub output_flags: IoFlags<'a>,
    /// Human (`true`) or POSIX (`false`) summary reports (`msgfmt=`).
    pub human_messages: bool,
    /// Progress display on (`progress=` non zero).
    pub progress: bool,
    /// Conversions to apply.
    pub conversions: Conversions,
}

impl<'a> CopyPlan<'a> {
    /// Defaults: standard input to standard output, 512 byte blocks,
    /// everything copied, POSIX messages, no progress, no conversions.
    pub fn defaults() -> Self {
        CopyPlan {
            input: None,
            output: None,
            input_block: 512,
            output_block: 512,
            count: None,
            files: None,
            skip: 0,
            seek: 0,
            input_flags: IoFlags::empty(),
            output_flags: IoFlags::empty(),
            human_messages: false,
            progress: false,
            conversions: Conversions::empty(),
        }
    }

    /// Total input bytes the plan copies (`None` means unbounded).
    pub fn copy_bytes(&self) -> Option<u64> {
        self.count
            .and_then(|count| count.checked_mul(self.input_block))
    }
}

/// Validated open words (`iflag=`/`oflag=`): spellings checked against the
/// `olist` table (`args.c:439`), at most 8 kept in order. Side placement
/// warnings (input only words on the output and vice versa) belong to the
/// executor, which reproduces the C warn-and-continue rules; the plan keeps
/// what the user typed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IoFlags<'a> {
    /// Validated words in encounter order.
    pub words: [&'a str; 8],
    /// How many of `words` are used.
    pub count: usize,
}

impl<'a> IoFlags<'a> {
    /// No words.
    pub fn empty() -> Self {
        IoFlags {
            words: [""; 8],
            count: 0,
        }
    }
}

/// Open words from the `olist` table (`args.c:439-461`): spelling source
/// of truth. Side rules (`directory` fits neither, `rdonly`/`rdwr`/
/// `rsync` fit input only, `append`/`creat`/`dsync` fit output only) are
/// documented here and enforced with warnings by the executor.
pub const IO_FLAG_WORDS: [&str; 20] = [
    "alt_io", "append", "async", "cloexec", "creat", "direct", "directory", "dsync", "excl",
    "exlock", "noctty", "nofollow", "nonblock", "nosigpipe", "rdonly", "rdwr", "rsync", "search",
    "shlock", "sync",
];

/// Summary report styles (`msgfmt=`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MsgFormat {
    /// POSIX summary (default).
    Posix,
    /// Human readable extended summary.
    Human,
}

/// Parse `dd` operands (`name=value` words) into a plan. Unknown names,
/// malformed sizes, and unknown conversions are errors; later words
/// override earlier ones (matching left to right processing).
pub fn parse_operands<'a>(args: &[&'a str]) -> Result<CopyPlan<'a>, ImageError> {
    let mut plan = CopyPlan::defaults();
    for arg in args {
        let (name, value) = arg.split_once('=').ok_or(ImageError::InvalidArgument)?;
        match name {
            "if" => plan.input = Some(non_empty(value)?),
            "of" => plan.output = Some(non_empty(value)?),
            "bs" => {
                let size = parse_size(value)?;
                plan.input_block = size;
                plan.output_block = size;
            }
            "ibs" => plan.input_block = parse_size(value)?,
            "obs" => plan.output_block = parse_size(value)?,
            "cbs" => {
                plan.input_block = parse_size(value)?;
                plan.output_block = parse_size(value)?;
            }
            "count" => plan.count = Some(parse_count(value)?),
            "files" => plan.files = Some(parse_count(value)?),
            "skip" | "iseek" => plan.skip = parse_count(value)?,
            "seek" | "oseek" => plan.seek = parse_count(value)?,
            "iflag" => plan.input_flags = parse_ioflags(value)?,
            "oflag" => plan.output_flags = parse_ioflags(value)?,
            "msgfmt" => {
                plan.human_messages = match value {
                    "human" => true,
                    "posix" => false,
                    _ => return Err(ImageError::InvalidArgument),
                }
            }
            "progress" => plan.progress = parse_count(value)? != 0,
            "conv" => plan.conversions = parse_conversions(value)?,
            _ => return Err(ImageError::InvalidArgument),
        }
    }
    if plan.input_block == 0 || plan.output_block == 0 {
        return Err(ImageError::InvalidArgument);
    }
    Ok(plan)
}

fn non_empty(value: &str) -> Result<&str, ImageError> {
    if value.is_empty() {
        return Err(ImageError::InvalidArgument);
    }
    Ok(value)
}

/// Parse a block count (plain decimal, no suffixes: counts are exact).
fn parse_count(text: &str) -> Result<u64, ImageError> {
    if text.is_empty() || !text.bytes().all(|b| b.is_ascii_digit()) {
        return Err(ImageError::InvalidArgument);
    }
    let mut value: u64 = 0;
    for byte in text.bytes() {
        value = value
            .checked_mul(10)
            .and_then(|v| v.checked_add((byte - b'0') as u64))
            .ok_or(ImageError::InvalidArgument)?;
    }
    Ok(value)
}

/// Parse a size with optional unit suffix and `x` multiplication
/// (`1Mx2` is two mebibytes): factors apply left to right with overflow
/// errors instead of wraparound.
pub fn parse_size(text: &str) -> Result<u64, ImageError> {
    if text.is_empty() {
        return Err(ImageError::InvalidArgument);
    }
    let mut total: Option<u64> = None;
    for factor in text.split('x') {
        let part = parse_single(factor)?;
        total = Some(match total {
            None => part,
            Some(acc) => acc.checked_mul(part).ok_or(ImageError::InvalidArgument)?,
        });
    }
    total.ok_or(ImageError::InvalidArgument)
}

fn parse_single(text: &str) -> Result<u64, ImageError> {
    if text.is_empty() {
        return Err(ImageError::InvalidArgument);
    }
    let (digits, factor) = match text.as_bytes().last() {
        Some(b'K') | Some(b'k') => (&text[..text.len() - 1], 1024u64),
        Some(b'M') | Some(b'm') => (&text[..text.len() - 1], 1024u64 * 1024),
        Some(b'G') | Some(b'g') => (&text[..text.len() - 1], 1024u64 * 1024 * 1024),
        _ => (text, 1),
    };
    // A `c` suffix means bytes (factor 1, explicit); `w` means words
    // (factor 2). Both attach after the digits like units.
    let (digits, factor) = match digits.strip_suffix(['c', 'C']) {
        Some(rest) => (rest, 1),
        None => match digits.strip_suffix(['w', 'W']) {
            Some(rest) => (rest, 2),
            None => (digits, factor),
        },
    };
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return Err(ImageError::InvalidArgument);
    }
    let mut value: u64 = 0;
    for byte in digits.bytes() {
        value = value
            .checked_mul(10)
            .and_then(|v| v.checked_add((byte - b'0') as u64))
            .ok_or(ImageError::InvalidArgument)?;
    }
    value.checked_mul(factor).ok_or(ImageError::InvalidArgument)
}

/// Parse an `iflag=`/`oflag=` comma list: every word must name a known
/// open word (unknown words fail the whole invocation, matching the C
/// `unknown %s` error). At most 8 words are kept.
fn parse_ioflags<'a>(text: &'a str) -> Result<IoFlags<'a>, ImageError> {
    if text.is_empty() {
        return Err(ImageError::InvalidArgument);
    }
    let mut flags = IoFlags::empty();
    for word in text.split(',') {
        if !IO_FLAG_WORDS.contains(&word) {
            return Err(ImageError::InvalidArgument);
        }
        if flags.count >= flags.words.len() {
            return Err(ImageError::InvalidArgument);
        }
        flags.words[flags.count] = word;
        flags.count += 1;
    }
    Ok(flags)
}

fn parse_conversions(text: &str) -> Result<Conversions, ImageError> {    let mut conversions = Conversions::empty();
    if text.is_empty() {
        return Err(ImageError::InvalidArgument);
    }
    for word in text.split(',') {
        match word {
            "notrunc" => conversions.bits |= Conversions::NO_TRUNC,
            "sync" => conversions.bits |= Conversions::SYNC,
            "noerror" => conversions.bits |= Conversions::NO_ERROR,
            "lcase" => conversions.bits |= Conversions::LCASE,
            "ucase" => conversions.bits |= Conversions::UCASE,
            "swab" => conversions.bits |= Conversions::SWAB,
            "block" | "unblock" | "ascii" | "ebcdic" | "ibm" => {}
            _ => return Err(ImageError::InvalidArgument),
        }
    }
    Ok(conversions)
}

/// Byte source/sink the copy loop drives — the seam that keeps the pump
/// host-testable over a mock and lets the binary bind it to a real file.
/// `read` returns bytes read (`0` is end of file); `write` consumes the whole
/// slice or reports an error.
pub trait Io {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, ImageError>;
    fn write(&mut self, buf: &[u8]) -> Result<(), ImageError>;
}

/// Copy tallies `dd`'s trailing summary reports (POSIX block counts, matching
/// the C `if`/`of` line: full and partial blocks each way, plus recovered
/// read errors).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CopyStats {
    /// Whole input blocks read.
    pub full_in: u64,
    /// Final short input block (0 or 1 unless `sync`).
    pub part_in: u64,
    /// Whole output blocks written.
    pub full_out: u64,
    /// Final short output block.
    pub part_out: u64,
    /// Read errors recovered under `conv=noerror`.
    pub read_errors: u64,
}

/// Pump `count`/all input blocks from `source` to `sink` honouring `bs`
/// (the `ibs == obs` case — the `bs=` operand `dd` is overwhelmingly used
/// with). `skip`/`seek` advance their streams first; a read error stops
/// unless `noerror`, which skips the block and retries; `sync` pads a short
/// final read to a full block before writing.
///
/// Declared residual of this slice: the asymmetric `ibs != obs` blocking
/// machine and the `conv=` case/`swab`/`block`/`unblock`/`cbs` transforms run
/// in the C through separate buffers and a byte carry (`dd.c`/`conv.c`); this
/// pump covers the unconverted `bs=` path, and the binary declines an
/// `ibs != obs` request rather than emit a mis-shaped copy. `scratch` must be
/// at least `bs` bytes.
pub fn pump(
    plan: &CopyPlan<'_>,
    scratch: &mut [u8],
    source: &mut dyn Io,
    sink: &mut dyn Io,
) -> Result<CopyStats, ImageError> {
    let bs = plan.input_block as usize;
    if bs == 0 || bs > scratch.len() || plan.input_block != plan.output_block {
        return Err(ImageError::InvalidArgument);
    }
    let mut stats = CopyStats::default();

    // `skip`: discard that many input blocks (C reads-and-drops on unseekable
    // input; a real file's `lseek` yields the same bytes to copy).
    for _ in 0..plan.skip {
        if source.read(&mut scratch[..bs])? == 0 {
            break;
        }
    }
    // `seek`: advance the output by whole blocks of NUL (the C zero-fills a
    // sparse head unless `oflag` says otherwise).
    for _ in 0..plan.seek {
        for byte in scratch[..bs].iter_mut() {
            *byte = 0;
        }
        sink.write(&scratch[..bs])?;
    }

    let limit = plan.count.unwrap_or(u64::MAX);
    let mut copied = 0u64;
    loop {
        if copied >= limit {
            break;
        }
        let read = match read_block(source, &mut scratch[..bs]) {
            Ok(n) => n,
            Err(_) => {
                if plan.conversions.no_error() {
                    stats.read_errors += 1;
                    copied += 1;
                    continue;
                }
                break;
            }
        };
        if read == 0 {
            break; // end of input
        }
        let partial = read < bs;
        let synced = if partial && plan.conversions.sync() {
            scratch[read..bs].fill(0);
            bs
        } else if partial {
            read
        } else {
            bs
        };
        sink.write(&scratch[..synced])?;
        copied += 1;
        if read < bs {
            stats.part_in += 1;
            stats.part_out += 1;
            break; // short read is EOF of the input
        }
        stats.full_in += 1;
        stats.full_out += 1;
    }
    Ok(stats)
}

/// Fill `buf` from `source` until full or a short/zero read; returns the bytes
/// held (shorter than `buf.len()` at EOF, and a partial read on mid-block
/// error so `noerror` can advance past it).
fn read_block(source: &mut dyn Io, buf: &mut [u8]) -> Result<usize, ImageError> {
    let mut filled = 0usize;
    while filled < buf.len() {
        match source.read(&mut buf[filled..])? {
            0 => break,
            n => filled += n,
        }
    }
    Ok(filled)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_full_invocation() {
        let plan = parse_operands(&["if=/dev/c0d0", "of=disk.img", "bs=1M", "count=10"]).unwrap();
        assert_eq!(plan.input, Some("/dev/c0d0"));
        assert_eq!(plan.output, Some("disk.img"));
        assert_eq!(plan.input_block, 1024 * 1024);
        assert_eq!(plan.copy_bytes(), Some(10 * 1024 * 1024));
    }

    #[test]
    fn test_multiplication_and_suffixes() {
        assert_eq!(parse_size("1Mx2"), Ok(2 * 1024 * 1024));
        assert_eq!(parse_size("512"), Ok(512));
        assert_eq!(parse_size("4k"), Ok(4096));
        assert_eq!(parse_size("2w"), Ok(4));
        assert_eq!(parse_size("1c"), Ok(1));
    }

    #[test]
    fn test_skip_seek_count() {
        let plan = parse_operands(&["skip=1", "seek=2", "count=3"]).unwrap();
        assert_eq!((plan.skip, plan.seek, plan.count), (1, 2, Some(3)));
        assert_eq!(plan.copy_bytes(), Some(3 * 512));
    }

    #[test]
    fn test_conversions() {
        let plan = parse_operands(&["conv=notrunc,sync,noerror"]).unwrap();
        assert!(plan.conversions.no_trunc());
        assert!(plan.conversions.sync());
        assert!(plan.conversions.no_error());
        assert!(!plan.conversions.upper());
    }

    #[test]
    fn test_bad_operands_rejected() {
        assert_eq!(parse_operands(&["bogus=1"]), Err(ImageError::InvalidArgument));
        assert_eq!(parse_operands(&["if="]), Err(ImageError::InvalidArgument));
        assert_eq!(parse_operands(&["bs=0x10"]), Err(ImageError::InvalidArgument));
        assert_eq!(parse_operands(&["conv=bogus"]), Err(ImageError::InvalidArgument));
        assert_eq!(parse_operands(&["bs=0"]), Err(ImageError::InvalidArgument));
        assert_eq!(parse_operands(&["iflag=bogus"]), Err(ImageError::InvalidArgument));
        assert_eq!(parse_operands(&["msgfmt=bogus"]), Err(ImageError::InvalidArgument));
    }

    #[test]
    fn test_extended_operands() {
        // `iseek`/`oseek` are synonyms of `skip`/`seek` (shared handlers
        // in the C table); `files`, flags, message format, and progress
        // each land on their own plan field.
        let plan = parse_operands(&[
            "iseek=1",
            "oseek=2",
            "files=3",
            "iflag=direct,sync",
            "oflag=append",
            "msgfmt=human",
            "progress=1",
        ])
        .unwrap();
        assert_eq!((plan.skip, plan.seek, plan.files), (1, 2, Some(3)));
        assert_eq!(plan.input_flags.count, 2);
        assert_eq!(plan.output_flags.words[0], "append");
        assert!(plan.human_messages);
        assert!(plan.progress);
        let plain = parse_operands(&["progress=0"]).unwrap();
        assert!(!plain.progress);
        assert!(!plain.human_messages);
    }

    /// In-memory `Io` over a byte source and a collected sink.
    struct MockIo {
        src: Vec<u8>,
        pos: usize,
        out: Vec<u8>,
        fail_after: Option<usize>,
    }

    impl MockIo {
        fn reader(src: &[u8]) -> Self {
            MockIo { src: src.to_vec(), pos: 0, out: Vec::new(), fail_after: None }
        }
        fn writer() -> Self {
            MockIo { src: Vec::new(), pos: 0, out: Vec::new(), fail_after: None }
        }
    }

    impl Io for MockIo {
        fn read(&mut self, buf: &mut [u8]) -> Result<usize, ImageError> {
            if let Some(n) = self.fail_after
                && self.pos >= n
            {
                return Err(ImageError::InvalidArgument);
            }
            let avail = self.src[self.pos..].len().min(buf.len());
            buf[..avail].copy_from_slice(&self.src[self.pos..self.pos + avail]);
            self.pos += avail;
            Ok(avail)
        }
        fn write(&mut self, buf: &[u8]) -> Result<(), ImageError> {
            self.out.extend_from_slice(buf);
            Ok(())
        }
    }

    fn pump_with(plan: &CopyPlan<'_>, src: &[u8], bs: usize) -> (CopyStats, Vec<u8>) {
        let mut source = MockIo::reader(src);
        let mut sink = MockIo::writer();
        let mut scratch = vec![0u8; bs];
        let stats = pump(plan, &mut scratch, &mut source, &mut sink).unwrap();
        (stats, sink.out)
    }

    #[test]
    fn test_pump_copies_everything() {
        let plan = parse_operands(&["bs=4"]).unwrap();
        let (stats, out) = pump_with(&plan, b"abcdefghijklmnop", 4);
        assert_eq!((stats.full_in, stats.part_in, stats.full_out), (4, 0, 4));
        assert_eq!(out, b"abcdefghijklmnop".to_vec());
    }

    #[test]
    fn test_pump_count_limits_input_blocks() {
        let plan = parse_operands(&["bs=4", "count=2"]).unwrap();
        let (stats, out) = pump_with(&plan, b"abcdefghijklmnop", 4);
        assert_eq!((stats.full_in, stats.full_out), (2, 2));
        assert_eq!(out, b"abcdefgh".to_vec());
    }

    #[test]
    fn test_pump_skip_and_seek() {
        let plan = parse_operands(&["bs=2", "skip=1", "seek=1"]).unwrap();
        let (stats, out) = pump_with(&plan, b"abcd", 2);
        // skip one 2-byte block -> copy "cd"; seek one block -> two NULs first
        // (the seek pad and the skip drop are positioning, not counted blocks).
        assert_eq!(out, vec![0, 0, b'c', b'd']);
        assert_eq!((stats.full_in, stats.full_out), (1, 1));
    }

    #[test]
    fn test_pump_partial_read_at_eof() {
        let plan = parse_operands(&["bs=4"]).unwrap();
        let (stats, out) = pump_with(&plan, b"abcdef", 4);
        assert_eq!((stats.full_in, stats.part_in), (1, 1));
        assert_eq!(out, b"abcdef".to_vec());
    }

    #[test]
    fn test_pump_sync_pads_final_block() {
        let plan = parse_operands(&["bs=4", "conv=sync"]).unwrap();
        let (stats, out) = pump_with(&plan, b"abcdef", 4);
        // sync pads the trailing "ef" to a full 4-byte block; the padded final
        // block counts as a partial transfer, not a second full one.
        assert_eq!(out, vec![b'a', b'b', b'c', b'd', b'e', b'f', 0, 0]);
        assert_eq!((stats.full_in, stats.part_in, stats.part_out), (1, 1, 1));
    }

    #[test]
    fn test_pump_noerror_recovers_read_error() {
        // `count` bounds the retry loop (as real `dd conv=noerror` relies on
        // the input ending); the first block reads, then two recovered errors.
        let plan = parse_operands(&["bs=2", "count=3", "conv=noerror"]).unwrap();
        let mut source = MockIo::reader(b"abcd");
        source.fail_after = Some(2); // error from block 2 on
        let mut sink = MockIo::writer();
        let mut scratch = vec![0u8; 2];
        let stats = pump(&plan, &mut scratch, &mut source, &mut sink).unwrap();
        assert_eq!(stats.read_errors, 2);
        assert_eq!(sink.out, b"ab".to_vec());
    }

    #[test]
    fn test_pump_rejects_asymmetric_block_sizes() {
        let plan = parse_operands(&["ibs=512", "obs=1024"]).unwrap();
        let mut source = MockIo::reader(b"xx");
        let mut sink = MockIo::writer();
        let mut scratch = vec![0u8; 1024];
        assert_eq!(pump(&plan, &mut scratch, &mut source, &mut sink), Err(ImageError::InvalidArgument));
    }
}
