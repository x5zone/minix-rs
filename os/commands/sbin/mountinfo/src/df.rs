//! `df` display core: a filesystem's block accounting from a `statvfs`
//! record into the POSIX `-P` report row.
//!
//! Ground truth: `minix3/bin/df/df.c` — the conversion macro `fsbtoblk`
//! (lines 350-353, scale a filesystem-block count to the report block size
//! without overflow), the per-mount arithmetic (`prtstat`, lines
//! 460-477: `used = f_blocks - f_bfree`, `bavail = f_bfree - f_bresvd`,
//! `availblks = bavail + used`), and the `-P` output line
//! (`printf("%s %lld %lld %lld %s%% %s\n", f_mntfromname, total, used,
//! avail, capacity, f_mntonname)`, lines 471-477). Capacity is BSD
//! `strspct`'s round-up percentage, with the empty filesystem rendered
//! `100` (the `availblks == 0 ? full` arm at line 476).
//!
//! This half is pure integers and strings — it takes the numbers the caller
//! read out of a [`minix_sys::StatvfsBuf`] and emits the report bytes, so
//! the arithmetic is unit-tested without a filesystem or the IPC layer. The
//! report block size is fixed at 1024 (the POSIX `-P`/`-k` view); the
//! human-readable (`-h`) and legacy 512-block aligned formats are a declared
//! subset and fall to the executor as `unwired` if requested.

use crate::MountError;
use alloc::format;
use alloc::string::{String, ToString};
use minix_fileops::humanize::{HN_AUTOSCALE, HN_B, HN_DECIMAL, HN_NOSPACE, humanize_number};

/// The `-P`/POSIX report block size in bytes.
pub const REPORT_BLOCK_SIZE: u64 = 1024;

/// The POSIX `-P` header line (`prtstat` Pflag branch, `df.c:448-449`).
pub const P_HEADER: &str = "Filesystem 1024-blocks Used Available Capacity Mounted on\n";

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

/// Render one `-P` report row into `out`, returning the byte count written.
///
/// `blocks`/`bfree`/`bresvd` are raw filesystem-block counts and `frsize` the
/// fragment size — exactly the `statvfs` fields `prtstat` reads. The names
/// come from the record's `f_mntfromname` / `f_mntonname`.
pub fn format_p_row(
    from: &str,
    to: &str,
    blocks: u64,
    bfree: u64,
    bresvd: u64,
    frsize: u64,
    out: &mut [u8],
) -> Result<usize, MountError> {
    let used = blocks.saturating_sub(bfree);
    let bavail = bfree.saturating_sub(bresvd);
    let availblks = bavail + used;
    let total = fsbtoblk(blocks, frsize, REPORT_BLOCK_SIZE);
    let used_blk = fsbtoblk(used, frsize, REPORT_BLOCK_SIZE);
    let avail_blk = fsbtoblk(bavail, frsize, REPORT_BLOCK_SIZE);
    let cap = capacity_percent(used, availblks);

    let mut w = Sink { out, pos: 0 };
    w.text(from)?;
    w.byte(b' ')?;
    w.number(total)?;
    w.byte(b' ')?;
    w.number(used_blk)?;
    w.byte(b' ')?;
    w.number(avail_blk)?;
    w.byte(b' ')?;
    w.number(cap)?;
    w.text("% ")?;
    w.text(to)?;
    w.byte(b'\n')?;
    Ok(w.pos)
}

/// Header of the `-h` report (`df.c:449-456`, the non-`-P` arm): the
/// filesystem column is `maxwidth + 6` wide because C widens it by
/// `headerlen - 10` with `header = "Size"` (`:447-452`).
pub fn h_header(maxwidth: usize) -> String {
    let width = maxwidth + 6;
    format!(
        "{:<width$} Size       Used      Avail %Cap Mounted on\n",
        filesystem_field(width)
    )
}

/// C's `%-*.*s` prints at most `width` characters of `"Filesystem"` — the
/// precision truncates the label on tables narrow enough to trip it.
fn filesystem_field(width: usize) -> &'static str {
    let label = "Filesystem";
    if label.len() > width {
        &label[..width]
    } else {
        label
    }
}

/// Render one `-h` report row (`df.c:491-509` with `prthuman`,
/// `:323-346`). The three figures are **bytes** — the block counts times the
/// fragment size — passed through the tree's `humanize_number` port with C's
/// arguments (`buf` five or six bytes depending on the sign, empty suffix,
/// `HN_AUTOSCALE` and `HN_B | HN_NOSPACE | HN_DECIMAL`, `:326-331`); each is
/// printed `"%s %6s"` after its pad. The capacity string follows the P row's
/// rule (`full` = "100" when nothing is addressable, `df.c:476`).
/// The `-h`/`-i` header (the non-`-P` default layout). `iflag` appends the
/// inode columns section (`df.c:455-456`); the filesystem column widens by
/// `headerlen - 10` as in C (`:447-452`). `header` is `"Size"` for `-h` and
/// `"1K-blocks"` for the block display (`headerlen` 4 and 9 give the same
/// `maxwidth + 6` and `maxwidth + 1` widths C computes).
pub fn default_header(maxwidth: usize, size_label: &str, iflag: bool) -> String {
    let width = maxwidth + (10 - size_label.len());
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

/// Render one `-i` row (`df.c:491-512`): the ten-wide block figures, the
/// capacity, then the inode triple (` %8 used %8 free %4 pct%%`). The inode
/// capacity follows C's rule (`:499-508`): no inodes at all prints "100"
/// when something is used and "  0" otherwise; the figure uses the same
/// round-up arithmetic as the block capacity.
#[allow(clippy::too_many_arguments)] // the fields mirror C's statvfs reads in prtstat
pub fn format_i_row(
    from: &str,
    to: &str,
    maxwidth: usize,
    blocks: u64,
    bfree: u64,
    bresvd: u64,
    frsize: u64,
    files: u64,
    ffree: u64,
    out: &mut [u8],
) -> Result<usize, MountError> {
    let used = blocks.saturating_sub(bfree);
    let bavail = bfree.saturating_sub(bresvd);
    let availblks = bavail + used;
    let total_blk = fsbtoblk(blocks, frsize, REPORT_BLOCK_SIZE);
    let used_blk = fsbtoblk(used, frsize, REPORT_BLOCK_SIZE);
    let avail_blk = fsbtoblk(bavail, frsize, REPORT_BLOCK_SIZE);
    let cap = if availblks == 0 {
        String::from("100")
    } else {
        capacity_percent(used, availblks).to_string()
    };
    let used_inodes = files.saturating_sub(ffree);
    let inode_cap = if files == 0 {
        if used_inodes == 0 {
            String::from("  0")
        } else {
            String::from("100")
        }
    } else {
        capacity_percent(used_inodes, files).to_string()
    };

    let mut w = Sink { out, pos: 0 };
    w.text(from)?;
    for _ in from.len()..maxwidth {
        w.byte(b' ')?;
    }
    // `%10jd` three times (`df.c:493-497`), then the block capacity.
    for value in [total_blk, used_blk, avail_blk] {
        let text = value.to_string();
        w.byte(b' ')?;
        for _ in text.len()..10 {
            w.byte(b' ')?;
        }
        w.text(&text)?;
    }
    w.byte(b' ')?;
    for _ in cap.len()..3 {
        w.byte(b' ')?;
    }
    w.text(&cap)?;
    w.text("%")?;
    // ` %8jd %8jd %4s%%` (`df.c:499-508`).
    for value in [used_inodes, ffree] {
        w.byte(b' ')?;
        let text = value.to_string();
        for _ in text.len()..8 {
            w.byte(b' ')?;
        }
        w.text(&text)?;
    }
    w.byte(b' ')?;
    for _ in inode_cap.len()..4 {
        w.byte(b' ')?;
    }
    w.text(&inode_cap)?;
    w.text("% ")?;
    w.text(to)?;
    w.byte(b'\n')?;
    Ok(w.pos)
}

#[allow(clippy::too_many_arguments)] // the fields mirror C's statvfs reads in prtstat
pub fn format_h_row(
    from: &str,
    to: &str,
    maxwidth: usize,
    blocks: u64,
    bfree: u64,
    bresvd: u64,
    frsize: u64,
    out: &mut [u8],
) -> Result<usize, MountError> {
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
    let cap = if availblks == 0 {
        String::from("100")
    } else {
        capacity_percent(used, availblks).to_string()
    };
    // `%-*.*s ` + `%s %6s`×3 + ` %3s%%` + ` %s\n` (`df.c:491-508`).
    let mut w = Sink { out, pos: 0 };
    w.text(from)?;
    for _ in from.len()..maxwidth {
        w.byte(b' ')?;
    }
    // The from-column's own trailing space (`"%-*.*s "`).
    w.byte(b' ')?;
    for (text, pad) in [(&total, "   "), (&used_text, "    "), (&avail_text, "    ")] {
        // `prthumanval`'s `"%s %6s"`: the pad, one space, then the
        // six-wide right-aligned figure.
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
    w.text("% ")?;
    w.text(to)?;
    w.byte(b'\n')?;
    Ok(w.pos)
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

    fn row(from: &str, to: &str, blocks: u64, bfree: u64, bresvd: u64, frsize: u64) -> String {
        let mut buf = [0u8; 256];
        let used = format_p_row(from, to, blocks, bfree, bresvd, frsize, &mut buf).unwrap();
        String::from_utf8(buf[..used].to_vec()).unwrap()
    }

    #[test]
    fn test_p_row_exact_layout() {
        // 1000 blocks, 400 free, 0 reserved, frsize == report block (1024):
        // used 600, avail 400, availblks 1000, cap ceil(60000/1000)=60.
        assert_eq!(
            row("/dev/root", "/", 1000, 400, 0, 1024),
            "/dev/root 1000 600 400 60% /\n"
        );
    }

    #[test]
    fn test_fsbtoblk_scales_small_fragments() {
        // frsize 512 below the 1024 report block → divide by 2.
        assert_eq!(fsbtoblk(2000, 512, 1024), 1000);
        assert_eq!(fsbtoblk(1024, 1024, 1024), 1024);
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
            row("/dev/sd1", "/mnt", 1000, 500, 100, 1024),
            "/dev/sd1 1000 500 400 56% /mnt\n"
        );
    }

    #[test]
    fn test_header_is_posix_standard() {
        assert_eq!(
            P_HEADER,
            "Filesystem 1024-blocks Used Available Capacity Mounted on\n"
        );
    }

    /// The `-h` row: C's `%-*.*s` filesystem column, three `pad + %6s`
    /// humanised figures (`df.c:323-346`, `:491-509`), then ` %3s%%` and the
    /// mount point. 4096000/2457600/1228800 bytes humanise to 3.9M/2.3M/1.2M
    /// and 600/900 rounds up to 67%.
    #[test]
    fn h_row_matches_the_c_columns() {
        let mut out = [0u8; 256];
        let used = format_h_row("rootfs", "/", 6, 1000, 400, 100, 4096, &mut out).unwrap();
        assert_eq!(
            core::str::from_utf8(&out[..used]).unwrap(),
            "rootfs       3.9M       2.3M       1.2M  67% /\n"
        );
    }

    /// The `-h` header widens the filesystem column by six (`headerlen - 10`
    /// with `header = "Size"`, `df.c:447-452`).
    #[test]
    fn h_header_widens_the_filesystem_column() {
        assert_eq!(
            h_header(6),
            "Filesystem   Size       Used      Avail %Cap Mounted on\n"
        );
    }

    /// The `-i` row: ten-wide block figures, the capacity, then
    /// ` %8 used %8 free %4 pct%%` (`df.c:491-512`); 600/900 rounds to 67%
    /// and 60/100 to 60%.
    #[test]
    fn i_row_lays_out_the_inode_columns() {
        let mut out = [0u8; 256];
        let used = format_i_row("rootfs", "/", 6, 1000, 400, 100, 4096, 100, 40, &mut out).unwrap();
        assert_eq!(
            core::str::from_utf8(&out[..used]).unwrap(),
            "rootfs       4000       2400       1200  67%       60       40   60% /\n"
        );
    }

    /// The `-i` header appends the inode section; the label column widens by
    /// `headerlen - 10` (`df.c:449-456`) and C's precision truncates
    /// `"Filesystem"` on narrow tables.
    #[test]
    fn i_header_appends_the_inode_section() {
        assert_eq!(
            default_header(10, "1K-blocks", true),
            "Filesystem  1K-blocks       Used      Avail %Cap    iUsed   iAvail %iCap Mounted on\n"
        );
        assert_eq!(
            default_header(6, "1K-blocks", true),
            "Filesys 1K-blocks       Used      Avail %Cap    iUsed   iAvail %iCap Mounted on\n"
        );
    }
}
