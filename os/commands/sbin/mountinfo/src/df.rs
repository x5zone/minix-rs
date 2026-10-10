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
//! read out of a [`minix_types::StatvfsBuf`] and emits the report bytes, so
//! the arithmetic is unit-tested without a filesystem or the IPC layer. The
//! report block size is fixed at 1024 (the POSIX `-P`/`-k` view); the
//! human-readable (`-h`) and legacy 512-block aligned formats are a declared
//! subset and fall to the executor as `unwired` if requested.

use crate::MountError;

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
        assert_eq!(row("/dev/root", "/", 1000, 400, 0, 1024), "/dev/root 1000 600 400 60% /\n");
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
        assert_eq!(row("/dev/sd1", "/mnt", 1000, 500, 100, 1024), "/dev/sd1 1000 500 400 56% /mnt\n");
    }

    #[test]
    fn test_header_is_posix_standard() {
        assert_eq!(P_HEADER, "Filesystem 1024-blocks Used Available Capacity Mounted on\n");
    }
}
