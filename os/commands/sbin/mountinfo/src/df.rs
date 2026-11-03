//! `df` display core: a filesystem's block accounting from a `statvfs`
//! record into the report rows.
//!
//! Ground truth: `minix3/bin/df/df.c` — the conversion macro `fsbtoblk`
//! (lines 350-353, scale a filesystem-block count to the report block size
//! without overflow), the block-size environment parser `getbsize`
//! (`minix3/lib/libc/gen/getbsize.c`), the per-mount arithmetic (`prtstat`,
//! lines 460-509), the POSIX `-P` line (`:471-477`), the aligned default
//! layout and its inode columns (`:491-509`), the human-readable columns
//! (`prthuman`, `:323-346`), and the verbose `-G` record (`:365-408`).
//!
//! This half is pure integers and strings — it takes the numbers the caller
//! read out of a [`minix_types::StatvfsBuf`] and emits the report bytes, so
//! the arithmetic is unit-tested without a filesystem or the IPC layer. The
//! block size is a parameter: C carries it in the `blocksize` local selected
//! by `-k`/`-m`/`-g` or `getbsize`.

use crate::MountError;
use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use minix_fileops::humanize::{HN_AUTOSCALE, HN_B, HN_DECIMAL, HN_NOSPACE, humanize_number};
use minix_types::{StatvfsBuf, statvfs_off};

/// The block sizes `df`'s option letters select (`df.c:99-121`).
pub const BLOCK_512: u64 = 512;
/// `-k`'s 1024-byte report block.
pub const BLOCK_K: u64 = 1024;
/// `-m`'s 1 MiB report block.
pub const BLOCK_M: u64 = 1024 * 1024;
/// `-g`'s 1 GiB report block.
pub const BLOCK_G: u64 = 1024 * 1024 * 1024;

/// `getbsize`'s ceiling (`MAXB` = 1 GiB, `getbsize.c:29`).
pub const MAX_BLOCKSIZE: u64 = BLOCK_G;

/// C's `if (maxwidth < 12) maxwidth = 12;` (`df.c:416`): the mount-name
/// column never shrinks below twelve characters, so the `"Filesystem"` label
/// and the widest short name keep the same column.
pub const MAXWIDTH_FLOOR: usize = 12;

/// `MNT_LOCAL` (`minix3/sys/sys/fstypes.h:152`): the filesystem is stored
/// locally. `-l` keeps only entries that carry it.
pub const MNT_LOCAL: u64 = 0x0000_1000;

/// `MNT_IGNORE` (`minix3/sys/sys/fstypes.h:97`): the entry is hidden from a
/// plain `df`; `-a` shows it.
pub const MNT_IGNORE: u64 = 0x0010_0000;

/// The mount-name column width C uses: the widest name, floored at twelve.
pub fn clamped_width(maxwidth: usize) -> usize {
    maxwidth.max(MAXWIDTH_FLOOR)
}

/// The POSIX `-P` header line with the active block-size label
/// (`df.c:441-447`).
pub fn p_header(block_label: &str) -> String {
    format!("Filesystem {block_label} Used Available Capacity Mounted on\n")
}

/// The result of the `getbsize` port: the block size, the header label
/// (`"512-blocks"`, `"1K-blocks"`, …) and the warnings C's `warnx` would
/// print (`getbsize.c:60-88`).
pub struct Bsize {
    /// The report block size in bytes.
    pub blocksize: u64,
    /// The `"%ld%s-blocks"` label (`getbsize.c:90-92`).
    pub header: String,
    /// `warnx` message bodies, in C's order (program name excluded).
    pub warnings: Vec<String>,
}

/// Scan the leading integer the way C's `strtol` does, returning the value
/// (saturated to `i64`) and the unconverted tail.
fn strtol_prefix(text: &str) -> (i64, &str) {
    let bytes = text.as_bytes();
    let mut i = 0;
    while i < bytes.len() && bytes[i].is_ascii_whitespace() {
        i += 1;
    }
    let start = i;
    let mut negative = false;
    if i < bytes.len() && (bytes[i] == b'+' || bytes[i] == b'-') {
        negative = bytes[i] == b'-';
        i += 1;
    }
    let digits_start = i;
    while i < bytes.len() && bytes[i].is_ascii_digit() {
        i += 1;
    }
    if i == digits_start {
        // No conversion: C leaves `endptr` at the original string.
        return (0, text);
    }
    let mut magnitude: i128 = 0;
    for &byte in &bytes[digits_start..i] {
        magnitude = magnitude * 10 + i128::from(byte - b'0');
        if magnitude > i128::from(i64::MAX) + 1 {
            magnitude = i128::from(i64::MAX) + 1;
            break;
        }
    }
    let value = if negative { -magnitude } else { magnitude };
    let value = if value > i128::from(i64::MAX) {
        i64::MAX
    } else if value < i128::from(i64::MIN) {
        i64::MIN
    } else {
        value as i64
    };
    (value, &text[start + (i - start)..])
}

/// Port of `getbsize` (`minix3/lib/libc/gen/getbsize.c`): `BLOCKSIZE` is a
/// decimal count with an optional one-letter `K`/`M`/`G` suffix, floored at
/// 512 bytes and capped at 1 GiB. Unset or empty means 512.
pub fn getbsize(env: Option<&str>) -> Bsize {
    let Some(text) = env.filter(|text| !text.is_empty()) else {
        return Bsize {
            blocksize: BLOCK_512,
            header: String::from("512-blocks"),
            warnings: Vec::new(),
        };
    };
    let mut warnings = Vec::new();
    let (value, rest) = strtol_prefix(text);
    if value < 0 {
        // `goto underflow` (`getbsize.c:42`): C prints only the minimum line.
        warnings.push(format!("{text}: minimum blocksize is 512"));
        return Bsize {
            blocksize: BLOCK_512,
            header: String::from("512-blocks"),
            warnings,
        };
    }
    let mut n = if value == 0 { 1 } else { value as u64 };
    let mut form = "";
    let mut mul = 1u64;
    let mut max = MAX_BLOCKSIZE;
    let suffix = rest.as_bytes();
    let malformed = if suffix.len() > 1 {
        true
    } else {
        match suffix.first().copied() {
            None => false,
            Some(b'G' | b'g') => {
                form = "G";
                mul = BLOCK_G;
                max = MAX_BLOCKSIZE / BLOCK_G;
                false
            }
            Some(b'K' | b'k') => {
                form = "K";
                mul = BLOCK_K;
                max = MAX_BLOCKSIZE / BLOCK_K;
                false
            }
            Some(b'M' | b'm') => {
                form = "M";
                mul = BLOCK_M;
                max = MAX_BLOCKSIZE / BLOCK_M;
                false
            }
            Some(_) => true,
        }
    };
    if malformed {
        // `fmterr` (`getbsize.c:70-73`): warn, then fall through to the two
        // range checks with `n = 512, mul = 1, max = 0`.
        warnings.push(format!("{text}: unknown blocksize"));
        n = 512;
        mul = 1;
        max = 0;
    }
    if n > max {
        warnings.push(String::from("maximum blocksize is 1G"));
        n = max;
    }
    let blocksize = n.saturating_mul(mul);
    if blocksize < BLOCK_512 {
        warnings.push(format!("{text}: minimum blocksize is 512"));
        return Bsize {
            blocksize: BLOCK_512,
            header: String::from("512-blocks"),
            warnings,
        };
    }
    Bsize {
        blocksize,
        header: format!("{n}{form}-blocks"),
        warnings,
    }
}

/// Scale `num` filesystem blocks of fragment size `fsbs` to `bs` report
/// blocks, mirroring `fsbtoblk` (`df.c:350-353`).
///
/// When the fragment size is below the report block size, divide the count by
/// how many fragments fill a report block; otherwise multiply by how many
/// report blocks one fragment spans. Zero fragment size falls to the multiply
/// arm as in the C guard (`fsbs != 0` short-circuits the divide).
pub fn fsbtoblk(num: u64, fsbs: u64, bs: u64) -> u64 {
    if fsbs != 0 && fsbs < bs {
        match bs.checked_div(fsbs) {
            Some(scale) if scale != 0 => num / scale,
            _ => num,
        }
    } else {
        num.saturating_mul(fsbs.checked_div(bs).unwrap_or(0))
    }
}

/// Filesystem usage as a whole-percentage, rounded up, matching BSD
/// `strspct` as `df` uses it; an empty filesystem reports `100`
/// (`df.c:476,496`).
pub fn capacity_percent(used: u64, availblks: u64) -> u64 {
    if availblks == 0 {
        100
    } else {
        used.saturating_mul(100).div_ceil(availblks)
    }
}

/// C's `%-*.*s` for the mount-name column: truncate to `width` bytes and pad
/// on the right (`df.c:488`, `:491`). The truncation is byte-wise because C
/// prints a `char` array; a truncated multi-byte sequence is rendered
/// lossily rather than panicking.
fn name_field(name: &str, width: usize) -> String {
    let bytes = name.as_bytes();
    let taken = &bytes[..bytes.len().min(width)];
    let mut text = String::from_utf8_lossy(taken).into_owned();
    while text.len() < width {
        text.push(' ');
    }
    text
}

/// Render one `-P` report row into `out`, returning the byte count written.
///
/// `blocks`/`bfree`/`bresvd` are raw filesystem-block counts and `frsize` the
/// fragment size — exactly the `statvfs` fields `prtstat` reads. `blocksize`
/// is the report unit the caller selected, not necessarily 1024: C prints a
/// 512-block figure when `BLOCKSIZE` is unset and `-P` is given.
#[allow(clippy::too_many_arguments)] // the fields mirror C's statvfs reads in prtstat
pub fn format_p_row(
    from: &str,
    to: &str,
    blocksize: u64,
    blocks: u64,
    bfree: u64,
    bresvd: u64,
    frsize: u64,
    out: &mut [u8],
) -> Result<usize, MountError> {
    let used = blocks.saturating_sub(bfree);
    let bavail = bfree.saturating_sub(bresvd);
    let availblks = bavail + used;
    let total = fsbtoblk(blocks, frsize, blocksize);
    let used_blk = fsbtoblk(used, frsize, blocksize);
    let avail_blk = fsbtoblk(bavail, frsize, blocksize);
    let cap = capacity_percent(used, availblks).to_string();
    let mut w = Sink { out, pos: 0 };
    w.text(from)?;
    w.byte(b' ')?;
    w.number(total)?;
    w.byte(b' ')?;
    w.number(used_blk)?;
    w.byte(b' ')?;
    w.number(avail_blk)?;
    w.byte(b' ')?;
    w.text(&cap)?;
    w.text("% ")?;
    w.text(to)?;
    w.byte(b'\n')?;
    Ok(w.pos)
}

/// Render one aligned row without inode columns (`df.c:491-498`): the
/// `maxwidth`-wide mount-from name (floored at twelve, `:416`), three
/// ten-wide block figures, and the three-wide capacity.
#[allow(clippy::too_many_arguments)] // the fields mirror C's statvfs reads in prtstat
pub fn format_block_row(
    from: &str,
    to: &str,
    maxwidth: usize,
    blocksize: u64,
    blocks: u64,
    bfree: u64,
    bresvd: u64,
    frsize: u64,
    out: &mut [u8],
) -> Result<usize, MountError> {
    let width = clamped_width(maxwidth);
    let used = blocks.saturating_sub(bfree);
    let bavail = bfree.saturating_sub(bresvd);
    let availblks = bavail + used;
    let total = fsbtoblk(blocks, frsize, blocksize);
    let used_blk = fsbtoblk(used, frsize, blocksize);
    let avail_blk = fsbtoblk(bavail, frsize, blocksize);
    let cap = capacity_percent(used, availblks).to_string();
    let mut w = Sink { out, pos: 0 };
    w.text(&name_field(from, width))?;
    w.byte(b' ')?;
    for value in [total, used_blk, avail_blk] {
        pad_number(&mut w, value, 10)?;
    }
    w.byte(b' ')?;
    for _ in cap.len()..3 {
        w.byte(b' ')?;
    }
    w.text(&cap)?;
    w.text("% ")?;
    w.text(to)?;
    w.byte(b'\n')?;
    Ok(w.pos)
}

/// Header of the non-`-P` report (`df.c:432-456`): the filesystem column is
/// `maxwidth + (10 - headerlen)` wide, floored through `clamped_width`.
/// `iflag` appends the inode columns (`:455-456`).
pub fn default_header(maxwidth: usize, size_label: &str, iflag: bool) -> String {
    // C's `maxwidth - (headerlen - 10)` is signed; a long `getbsize` label
    // can drive it below the name width but never below zero in practice.
    let width = clamped_width(maxwidth) as isize + 10 - size_label.len() as isize;
    let width = width.max(0) as usize;
    let mut text = format!(
        "{:<width$} {}       Used      Avail %Cap",
        filesystem_field(width),
        size_label
    );
    if iflag {
        text.push_str("    iUsed   iAvail %iCap");
    }
    text.push_str(" Mounted on\n");
    text
}

/// C's `%-*.*s` prints at most `width` characters of `"Filesystem"` — the
/// precision truncates the label on tables narrow enough to trip it.
fn filesystem_field(width: usize) -> String {
    let label = "Filesystem";
    let bytes = label.as_bytes();
    let taken = &bytes[..bytes.len().min(width)];
    let mut text = String::from_utf8_lossy(taken).into_owned();
    while text.len() < width {
        text.push(' ');
    }
    text
}

/// Header of the `-h` report: `"Size"` in the block-label slot
/// (`df.c:449-452`).
pub fn h_header(maxwidth: usize) -> String {
    default_header(maxwidth, "Size", false)
}

/// The inode section shared by the aligned and human rows (`df.c:499-508`):
/// ` %8 used %8 free %4 cap%%`. A filesystem with no inodes prints `"  0"`
/// when nothing is used and `"100"` otherwise (`:506`).
fn inode_section(w: &mut Sink<'_>, files: u64, ffree: u64) -> Result<(), MountError> {
    let used = files.saturating_sub(ffree);
    let cap = if files == 0 {
        if used == 0 {
            String::from("  0")
        } else {
            String::from("100")
        }
    } else {
        capacity_percent(used, files).to_string()
    };
    for (value, width) in [(used, 8usize), (ffree, 8usize)] {
        w.byte(b' ')?;
        let text = value.to_string();
        for _ in text.len()..width {
            w.byte(b' ')?;
        }
        w.text(&text)?;
    }
    w.byte(b' ')?;
    for _ in cap.len()..4 {
        w.byte(b' ')?;
    }
    w.text(&cap)?;
    w.text("%")?;
    Ok(())
}

fn pad_number(w: &mut Sink<'_>, value: u64, width: usize) -> Result<(), MountError> {
    let text = value.to_string();
    for _ in text.len()..width {
        w.byte(b' ')?;
    }
    w.text(&text)
}

/// Render one aligned (`-i` or the plain block layout) row (`df.c:491-512`):
/// the mount-from name, the ten-wide block figures, the capacity, the inode
/// triple, then the mount point.
#[allow(clippy::too_many_arguments)] // the fields mirror C's statvfs reads in prtstat
pub fn format_i_row(
    from: &str,
    to: &str,
    maxwidth: usize,
    blocksize: u64,
    blocks: u64,
    bfree: u64,
    bresvd: u64,
    frsize: u64,
    files: u64,
    ffree: u64,
    out: &mut [u8],
) -> Result<usize, MountError> {
    let width = clamped_width(maxwidth);
    let used = blocks.saturating_sub(bfree);
    let bavail = bfree.saturating_sub(bresvd);
    let availblks = bavail + used;
    let total_blk = fsbtoblk(blocks, frsize, blocksize);
    let used_blk = fsbtoblk(used, frsize, blocksize);
    let avail_blk = fsbtoblk(bavail, frsize, blocksize);
    let cap = capacity_percent(used, availblks).to_string();

    let mut w = Sink { out, pos: 0 };
    w.text(&name_field(from, width))?;
    w.byte(b' ')?;
    for value in [total_blk, used_blk, avail_blk] {
        pad_number(&mut w, value, 10)?;
    }
    w.byte(b' ')?;
    for _ in cap.len()..3 {
        w.byte(b' ')?;
    }
    w.text(&cap)?;
    w.text("%")?;
    inode_section(&mut w, files, ffree)?;
    w.text(" ")?;
    w.text(to)?;
    w.byte(b'\n')?;
    Ok(w.pos)
}

/// Render one `-h` report row (`df.c:491-509` with `prthuman`, `:323-346`).
/// The three figures are **bytes** — the block counts times the fragment
/// size — passed through the tree's `humanize_number` port with C's arguments
/// (`buf` five or six bytes depending on the sign, empty suffix,
/// `HN_AUTOSCALE` and `HN_B | HN_NOSPACE | HN_DECIMAL`, `:326-331`); each is
/// printed `"%s %6s"` after its pad. `iflag` appends the inode columns.
#[allow(clippy::too_many_arguments)] // the fields mirror C's statvfs reads in prtstat
pub fn format_h_row(
    from: &str,
    to: &str,
    maxwidth: usize,
    blocks: u64,
    bfree: u64,
    bresvd: u64,
    frsize: u64,
    files: u64,
    ffree: u64,
    iflag: bool,
    out: &mut [u8],
) -> Result<usize, MountError> {
    let width = clamped_width(maxwidth);
    let used = blocks.saturating_sub(bfree);
    let bavail = bfree.saturating_sub(bresvd);
    let availblks = bavail + used;
    let human = |bytes: u64| -> String {
        let buf_len = if (bytes as i64) < 0 { 6 } else { 5 };
        humanize_number(
            buf_len,
            bytes as i64,
            "",
            HN_AUTOSCALE,
            HN_B | HN_NOSPACE | HN_DECIMAL,
        )
        .unwrap_or_default()
    };
    let total = human(blocks.saturating_mul(frsize));
    let used_text = human(used.saturating_mul(frsize));
    let avail_text = human(bavail.saturating_mul(frsize));
    let cap = capacity_percent(used, availblks).to_string();
    // `%-*.*s ` + `%s %6s`×3 + ` %3s%%` + optional inode section + ` %s\n`.
    let mut w = Sink { out, pos: 0 };
    w.text(&name_field(from, width))?;
    w.byte(b' ')?;
    for (text, pad) in [(&total, "   "), (&used_text, "    "), (&avail_text, "    ")] {
        w.text(pad)?;
        w.byte(b' ')?;
        for _ in text.len()..6 {
            w.byte(b' ')?;
        }
        w.text(text)?;
    }
    w.byte(b' ')?;
    for _ in cap.len()..3 {
        w.byte(b' ')?;
    }
    w.text(&cap)?;
    w.text("%")?;
    if iflag {
        inode_section(&mut w, files, ffree)?;
    }
    w.text(" ")?;
    w.text(to)?;
    w.byte(b'\n')?;
    Ok(w.pos)
}

/// C's `%#15lx`: the `0x` prefix is dropped for a zero value (C's `#` flag
/// only forces the prefix for nonzero conversions), then the text pads on the
/// left to `width`.
fn hex_prefixed(value: u64, width: usize) -> String {
    let body = if value == 0 {
        String::from("0")
    } else {
        format!("0x{value:x}")
    };
    let mut text = String::new();
    for _ in body.len()..width {
        text.push(' ');
    }
    text.push_str(&body);
    text
}

/// Render one verbose `-G` record (`df.c:365-408`). C's argument order on the
/// third line is `(f_ffree, f_files)` while the labels read "total files" and
/// "free files"; that swap is part of the observed output and is reproduced.
pub fn format_g_row(buf: &StatvfsBuf, out: &mut [u8]) -> Result<usize, MountError> {
    let on = buf.get_name(statvfs_off::MNTONNAME);
    let from = buf.get_name(statvfs_off::MNTFROMNAME);
    let bsize = buf.get_u64(statvfs_off::BSIZE);
    let frsize = buf.get_u64(statvfs_off::FRSIZE);
    let blocks = buf.get_u64(statvfs_off::BLOCKS);
    let bfree = buf.get_u64(statvfs_off::BFREE);
    let bavail = buf.get_u64(statvfs_off::BAVAIL);
    let files = buf.get_u64(statvfs_off::FILES);
    let ffree = buf.get_u64(statvfs_off::FFREE);
    let fsid = buf.get_u64(statvfs_off::FSID);
    let fstype = buf.get_name(statvfs_off::FSTYPENAME);
    let flag = buf.get_u64(statvfs_off::FLAG);
    let namemax = buf.get_u64(statvfs_off::NAMEMAX);
    let owner = buf.get_u32(statvfs_off::OWNER);
    let syncwrites = buf.get_u64(statvfs_off::SYNCWRITES);
    let asyncwrites = buf.get_u64(statvfs_off::ASYNCWRITES);
    let text = format!(
        "{on:>10} ({from:<12}): {bsize:>7} block size {frsize:>12} frag size\n\
         {blocks:>10} total blocks {bfree:>10} free blocks  {bavail:>10} available\n\
         {ffree:>10} total files  {files:>10} free files {fsid:>12x} filesys id\n\
         {fstype:>10} fstype  {flag} flag  {namemax:>17} filename length\n\
         {owner:>10} owner {syncwrites:>17} syncwrites {asyncwrites:>12} asyncwrites\n\n",
        on = on,
        from = from,
        bsize = bsize as i64,
        frsize = frsize as i64,
        blocks = blocks as i64,
        bfree = bfree as i64,
        bavail = bavail as i64,
        ffree = ffree as i64,
        files = files as i64,
        fsid = fsid,
        fstype = fstype,
        flag = hex_prefixed(flag, 15),
        namemax = namemax as i64,
        owner = owner,
        syncwrites = syncwrites as i64,
        asyncwrites = asyncwrites as i64,
    );
    let mut w = Sink { out, pos: 0 };
    w.text(&text)?;
    Ok(w.pos)
}

/// A `-t` type list (`df.c:246-274`): `"mfs,ext2"` keeps those types,
/// `"nomfs,ext2"` drops them. C splits on commas without trimming, so an
/// empty element can never match.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeList {
    /// `"no"` prefix present: the list names types to exclude.
    pub negated: bool,
    /// The comma-separated names.
    pub names: Vec<String>,
}

/// Why a `-t` argument could not be accepted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TypeListError {
    /// C's `errx(EXIT_FAILURE, "empty type list")` (`df.c:250-251`).
    Empty,
}

/// Parse a `-t` argument (`df.c:246-274`).
pub fn parse_type_list(spec: &str) -> Result<TypeList, TypeListError> {
    if spec.is_empty() {
        return Err(TypeListError::Empty);
    }
    let (negated, body) = match spec.strip_prefix("no") {
        Some(body) => (true, body),
        None => (false, spec),
    };
    Ok(TypeList {
        negated,
        names: body.split(',').map(String::from).collect(),
    })
}

/// C's `selected` (`df.c:233-243`): with no list every type passes; with a
/// list the type must equal one of the names (C's `strncmp` over a
/// NUL-terminated name is exact equality), and a `"no"` list inverts the
/// answer.
pub fn type_selected(list: Option<&TypeList>, fstype: &str) -> bool {
    let Some(list) = list else {
        return true;
    };
    let found = list.names.iter().any(|name| name == fstype);
    if list.negated { !found } else { found }
}

/// `regetmntinfo`'s filter (`df.c:294-306`): whether a table record is
/// dropped before printing. `-a` keeps `MNT_IGNORE` entries, `-l` keeps only
/// `MNT_LOCAL` ones, and `-t` keeps the selected types.
pub fn hidden_by_filter(
    aflag: bool,
    lflag: bool,
    list: Option<&TypeList>,
    flag: u64,
    fstype: &str,
) -> bool {
    (!aflag && flag & MNT_IGNORE != 0)
        || (lflag && flag & MNT_LOCAL == 0)
        || !type_selected(list, fstype)
}

/// Fixed-buffer byte sink, failing closed when full (no allocation; the
/// caller sizes the buffer, mirroring how the C stdio buffer is simply large).
struct Sink<'a> {
    out: &'a mut [u8],
    pos: usize,
}

impl<'a> Sink<'a> {
    fn byte(&mut self, b: u8) -> Result<(), MountError> {
        if self.pos >= self.out.len() {
            return Err(MountError::InvalidArgument);
        }
        self.out[self.pos] = b;
        self.pos += 1;
        Ok(())
    }

    fn text(&mut self, s: &str) -> Result<(), MountError> {
        for &b in s.as_bytes() {
            self.byte(b)?;
        }
        Ok(())
    }

    fn number(&mut self, value: u64) -> Result<(), MountError> {
        let mut digits = [0u8; 20];
        let mut n = value;
        let mut len = 0usize;
        loop {
            digits[len] = b'0' + (n % 10) as u8;
            len += 1;
            n /= 10;
            if n == 0 {
                break;
            }
        }
        for d in digits[..len].iter().rev() {
            self.byte(*d)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(
        from: &str,
        to: &str,
        blocksize: u64,
        blocks: u64,
        bfree: u64,
        bresvd: u64,
        frsize: u64,
    ) -> String {
        let mut buf = [0u8; 256];
        let used =
            format_p_row(from, to, blocksize, blocks, bfree, bresvd, frsize, &mut buf).unwrap();
        String::from_utf8(buf[..used].to_vec()).unwrap()
    }

    #[test]
    fn test_p_row_exact_layout() {
        // 1000 blocks, 400 free, 0 reserved, frsize == report block (1024):
        // used 600, avail 400, availblks 1000, cap ceil(60000/1000)=60.
        assert_eq!(
            row("/dev/root", "/", BLOCK_K, 1000, 400, 0, 1024),
            "/dev/root 1000 600 400 60% /\n"
        );
        // The same record at `-P` with the 512 default: every figure doubles.
        assert_eq!(
            row("/dev/root", "/", BLOCK_512, 1000, 400, 0, 1024),
            "/dev/root 2000 1200 800 60% /\n"
        );
    }

    #[test]
    fn test_fsbtoblk_scales_small_fragments() {
        // frsize 512 below the 1024 report block → divide by 2.
        assert_eq!(fsbtoblk(2000, 512, 1024), 1000);
        assert_eq!(fsbtoblk(1024, 1024, 1024), 1024);
        // Above the report block: multiply by the fragment/report ratio.
        assert_eq!(fsbtoblk(4, 4096, 1024), 16);
    }

    #[test]
    fn test_capacity_rounds_up_and_honours_empty() {
        // 601/1000 → 60.1% rounds up to 61.
        assert_eq!(capacity_percent(601, 1000), 61);
        assert_eq!(capacity_percent(0, 0), 100); // empty filesystem arm
        assert_eq!(capacity_percent(500, 1000), 50);
    }

    #[test]
    fn test_reserved_blocks_reduce_available() {
        // 1000 blocks, 500 free, 100 reserved → avail 400, used 500,
        // availblks 900, cap ceil(50000/900)=56.
        assert_eq!(
            row("/dev/sd1", "/mnt", BLOCK_K, 1000, 500, 100, 1024),
            "/dev/sd1 1000 500 400 56% /mnt\n"
        );
    }

    #[test]
    fn test_header_is_posix_standard() {
        assert_eq!(
            p_header("1024-blocks"),
            "Filesystem 1024-blocks Used Available Capacity Mounted on\n"
        );
        assert_eq!(
            p_header("512-blocks"),
            "Filesystem 512-blocks Used Available Capacity Mounted on\n"
        );
    }

    /// The plain aligned row (`df.c:491-498`) with C's twelve-character
    /// floor: names shorter than twelve pad to twelve.
    #[test]
    fn block_row_uses_the_aligned_layout() {
        let mut out = [0u8; 256];
        let used =
            format_block_row("rootfs", "/", 6, BLOCK_K, 1000, 400, 0, 1024, &mut out).unwrap();
        assert_eq!(
            core::str::from_utf8(&out[..used]).unwrap(),
            "rootfs             1000       600       400  60% /\n"
        );
    }

    /// The `-h` row: C's `%-*.*s` filesystem column, three `pad + %6s`
    /// humanised figures (`df.c:323-346`, `:491-509`), then ` %3s%%` and the
    /// mount point. 4096000/2457600/1228800 bytes humanise to 3.9M/2.3M/1.2M
    /// and 600/900 rounds up to 67%.
    #[test]
    fn h_row_matches_the_c_columns() {
        let mut out = [0u8; 256];
        let used = format_h_row(
            "rootfs", "/", 6, 1000, 400, 100, 4096, 0, 0, false, &mut out,
        )
        .unwrap();
        assert_eq!(
            core::str::from_utf8(&out[..used]).unwrap(),
            "rootfs             3.9M       2.3M       1.2M  67% /\n"
        );
    }

    /// `-h -i` is legal in C (`iflag` only appends the inode section), so the
    /// human row carries both.
    #[test]
    fn h_row_appends_the_inode_section_when_asked() {
        let mut out = [0u8; 256];
        let used = format_h_row(
            "rootfs", "/", 6, 1000, 400, 100, 4096, 100, 40, true, &mut out,
        )
        .unwrap();
        assert_eq!(
            core::str::from_utf8(&out[..used]).unwrap(),
            "rootfs             3.9M       2.3M       1.2M  67%       60       40   60% /\n"
        );
    }

    /// The `-h` header widens the filesystem column by six (`headerlen - 10`
    /// with `header = "Size"`) after the twelve-character floor.
    #[test]
    fn h_header_widens_the_filesystem_column() {
        assert_eq!(
            h_header(6),
            "Filesystem         Size       Used      Avail %Cap Mounted on\n"
        );
        assert_eq!(
            h_header(20),
            "Filesystem                 Size       Used      Avail %Cap Mounted on\n"
        );
    }

    /// The `-i` row: ten-wide block figures, the capacity, then
    /// ` %8 used %8 free %4 pct%%` (`df.c:491-512`); 600/900 rounds to 67%
    /// and 60/100 to 60%.
    #[test]
    fn i_row_lays_out_the_inode_columns() {
        let mut out = [0u8; 256];
        let used = format_i_row(
            "rootfs", "/", 6, BLOCK_K, 1000, 400, 100, 4096, 100, 40, &mut out,
        )
        .unwrap();
        assert_eq!(
            core::str::from_utf8(&out[..used]).unwrap(),
            "rootfs             4000      2400      1200  67%       60       40   60% /\n"
        );
    }

    /// The `-i` header appends the inode section; the label column widens by
    /// `headerlen - 10` (`df.c:449-456`) on top of the twelve-character floor.
    #[test]
    fn i_header_appends_the_inode_section() {
        assert_eq!(
            default_header(10, "1K-blocks", true),
            "Filesystem    1K-blocks       Used      Avail %Cap    iUsed   iAvail %iCap Mounted on\n"
        );
        // A long `getbsize` label narrows the column until C's precision
        // truncates "Filesystem".
        assert_eq!(
            default_header(12, "1073741824-blocks", true),
            "Files 1073741824-blocks       Used      Avail %Cap    iUsed   iAvail %iCap Mounted on\n"
        );
    }

    /// `getbsize`: unset and empty mean 512; the suffix letters scale; the
    /// floor and ceiling warn exactly as C's `getbsize.c` does.
    #[test]
    fn getbsize_matches_the_c_parser() {
        let b = getbsize(None);
        assert_eq!((b.blocksize, b.header.as_str()), (512, "512-blocks"));
        assert!(b.warnings.is_empty());

        let b = getbsize(Some(""));
        assert_eq!(b.blocksize, 512);

        let b = getbsize(Some("1k"));
        assert_eq!((b.blocksize, b.header.as_str()), (1024, "1K-blocks"));
        assert!(b.warnings.is_empty());

        let b = getbsize(Some("1M"));
        assert_eq!((b.blocksize, b.header.as_str()), (BLOCK_M, "1M-blocks"));

        let b = getbsize(Some("1g"));
        assert_eq!(b.blocksize, BLOCK_G);
        assert_eq!(b.header, "1G-blocks");

        let b = getbsize(Some("512"));
        assert_eq!((b.blocksize, b.header.as_str()), (512, "512-blocks"));

        // Below the floor: warn and fall back to 512.
        let b = getbsize(Some("256"));
        assert_eq!(b.blocksize, 512);
        assert_eq!(b.warnings, ["256: minimum blocksize is 512"]);

        // Negative goes straight to the underflow warning.
        let b = getbsize(Some("-1"));
        assert_eq!(b.blocksize, 512);
        assert_eq!(b.warnings, ["-1: minimum blocksize is 512"]);

        // A malformed suffix warns twice, in C's order, and lands on 512.
        let b = getbsize(Some("abc"));
        assert_eq!(b.blocksize, 512);
        assert_eq!(
            b.warnings,
            [
                "abc: unknown blocksize",
                "maximum blocksize is 1G",
                "abc: minimum blocksize is 512"
            ]
        );

        // Above the ceiling: clamp to 1 GiB with the maximum warning.
        let b = getbsize(Some("2g"));
        assert_eq!((b.blocksize, b.header.as_str()), (BLOCK_G, "1G-blocks"));
        assert_eq!(b.warnings, ["maximum blocksize is 1G"]);
    }

    /// The `-G` record reproduces C's five lines, including the
    /// `(f_ffree, f_files)` argument swap on the "total files" line.
    #[test]
    fn g_record_matches_the_verbose_layout() {
        let mut buf = StatvfsBuf::new();
        buf.set_name(statvfs_off::MNTONNAME, "/");
        buf.set_name(statvfs_off::MNTFROMNAME, "/dev/root");
        buf.set_name(statvfs_off::FSTYPENAME, "mfs");
        buf.set_u64(statvfs_off::BSIZE, 4096);
        buf.set_u64(statvfs_off::FRSIZE, 1024);
        buf.set_u64(statvfs_off::BLOCKS, 1000);
        buf.set_u64(statvfs_off::BFREE, 400);
        buf.set_u64(statvfs_off::BAVAIL, 380);
        buf.set_u64(statvfs_off::FILES, 100);
        buf.set_u64(statvfs_off::FFREE, 40);
        buf.set_u64(statvfs_off::FSID, 0x1234);
        buf.set_u64(statvfs_off::FLAG, 0x1000);
        buf.set_u64(statvfs_off::NAMEMAX, 255);
        buf.set_u32(statvfs_off::OWNER, 0);
        buf.set_u64(statvfs_off::SYNCWRITES, 7);
        buf.set_u64(statvfs_off::ASYNCWRITES, 9);
        let mut out = [0u8; 512];
        let used = format_g_row(&buf, &mut out).unwrap();
        assert_eq!(
            core::str::from_utf8(&out[..used]).unwrap(),
            "         / (/dev/root   ):    4096 block size         1024 frag size\n\
             \x20     1000 total blocks        400 free blocks         380 available\n\
             \x20       40 total files         100 free files         1234 filesys id\n\
             \x20      mfs fstype           0x1000 flag                255 filename length\n\
             \x20        0 owner                 7 syncwrites            9 asyncwrites\n\n"
        );
    }

    /// C's `%#lx` prints a bare `0` for zero; nonzero values gain `0x` and
    /// the whole text is right-aligned to the field width.
    #[test]
    fn the_flag_column_matches_c_printf() {
        assert_eq!(hex_prefixed(0, 15), "              0");
        assert_eq!(hex_prefixed(0x1000, 15), "         0x1000");
    }

    #[test]
    fn type_list_parsing_and_selection() {
        let list = parse_type_list("mfs,ext2").unwrap();
        assert!(!list.negated);
        assert!(type_selected(Some(&list), "mfs"));
        assert!(type_selected(Some(&list), "ext2"));
        assert!(!type_selected(Some(&list), "vbfs"));
        assert!(type_selected(None, "anything"));

        let list = parse_type_list("nomfs").unwrap();
        assert!(list.negated);
        assert!(!type_selected(Some(&list), "mfs"));
        assert!(type_selected(Some(&list), "ext2"));

        // No trimming: an empty element is a name that never matches.
        let list = parse_type_list("mfs,,").unwrap();
        assert_eq!(list.names, ["mfs", "", ""]);

        assert_eq!(parse_type_list(""), Err(TypeListError::Empty));
    }

    #[test]
    fn the_filter_matches_regetmntinfo() {
        let list = parse_type_list("mfs").unwrap();
        // Hidden unless -a; local only with -l; type gate with -t.
        assert!(hidden_by_filter(false, false, None, MNT_IGNORE, "mfs"));
        assert!(!hidden_by_filter(true, false, None, MNT_IGNORE, "mfs"));
        assert!(hidden_by_filter(false, true, None, 0, "mfs"));
        assert!(!hidden_by_filter(false, true, None, MNT_LOCAL, "mfs"));
        assert!(!hidden_by_filter(false, false, Some(&list), 0, "mfs"));
        assert!(hidden_by_filter(false, false, Some(&list), 0, "vbfs"));
    }
}
