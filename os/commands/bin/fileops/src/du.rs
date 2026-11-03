//! `du` — disk usage: the pure counting, unit and walk-decision layer.
//!
//! Ground truth: `minix3/usr.bin/du/du.c`. The option scan is `getopt` with
//! `"HLPacd:ghikmnrsx"` at line 90, the flag interactions at 173 to 182, the
//! unit choice at 189 to 192, and the per-visit accounting at 214 to 249. The
//! `COUNT` macro at line 66 says what is summed: an inode count under `-i`,
//! otherwise `st_blocks` (512-byte blocks, as the file system reports them). The
//! filesystem legs — `lstat` and the directory walk — live in the thin program
//! `src/bin/du.rs`, which walks with [`crate::dents`] the way `rm -r`, `cp -R`
//! and `chmod -R` do.
//!
//! What is decided here:
//!
//! - the option face, including C's mutual exclusions (`-a` with `-s`/`-d`,
//!   `-s` with `-d`), the depth window `0..=SHRT_MAX`, and the `atoi` shape of
//!   the `-d` value;
//! - `BLOCKSIZE` interpretation, which is `getbsize`'s job in C
//!   (`minix3/lib/libc/gen/getbsize.c:66-108`) and is reproduced here because
//!   this tree has no libc to call;
//! - the printed text: the raw count under `-i`, `humanize_number` over
//!   `units * 512` under `-h`, and `howmany(units, blocksize / 512)` otherwise
//!   (`du.c:258-278`);
//! - the hard-link dedup table (`linkchk`, `du.c:280-351`), which decides
//!   whether a multiply-linked file is counted again;
//! - `-n`'s `UF_NODUMP` skip (`du.c:198-209`).
//!
//! Not here, and refused by the program: `-H`/`-L`, which pick the traversal mode
//! `du.c:166-171` acts on, and `-x`, which is `FTS_XDEV` and needs mount-point
//! knowledge. `-h` goes through [`crate::humanize`], this crate's port of
//! NetBSD's `humanize_number`.

use crate::humanize::{HN_AUTOSCALE, HN_B, HN_DECIMAL, HN_NOSPACE, humanize_number};
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

/// The unit the file system counts in: `st_blocks` is in 512-byte blocks, and
/// `du.c:192` divides the chosen block size by that same number.
pub const BLOCK_UNIT: u64 = 512;

/// `minix3/sys/sys/stat.h:203` (`UF_NODUMP`), the flag `-n` skips on.
pub const UF_NODUMP: u32 = 0x0001;

/// `du.c:89`: with no `-d` and no `-s`, every depth prints.
pub const DEPTH_UNBOUNDED: i32 = i32::MAX;

/// The largest `-d` value C accepts: `SHRT_MAX`, checked at `du.c:112`.
pub const DEPTH_MAX: i32 = 32767;

/// Why the command line is not a `du` invocation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DuOption {
    /// A letter whose facility this tree has not landed yet: `-h`
    /// (`humanize_number`), `-H`/`-L` (the logical walk).
    NotWired(char),
    /// Usage and exit 1, which is also the answer to a conflicting flag pair
    /// (`du.c:174-181`) and to an out-of-range `-d` (`du.c:112-116`).
    Usage,
}

/// The resolved command line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DuPlan {
    /// `-a`: list every file, not just directories.
    pub list_files: bool,
    /// `-c`: print a grand total for all operands at the end.
    pub grand_total: bool,
    /// `-i`: count inodes (one per entry) instead of blocks.
    pub inodes: bool,
    /// `-h`: render the count through `humanize_number`, as `prstat` does
    /// (`du.c:266-272`). It loses to `-i`, whose raw count is printed first.
    pub human: bool,
    /// `-n`: skip anything with `UF_NODUMP` and do not descend into it.
    pub skip_nodump: bool,
    /// The deepest level that still prints: `-s` is 0, `-d N` is `N`.
    pub depth: i32,
    /// The unit the total is reported in, already divided by 512
    /// (`du.c:192`): 1 by default, 2 for `-k`, 2048 for `-m`, 2097152 for `-g`.
    pub blocksize: u64,
    /// `-x` (`FTS_XDEV`, `du.c:144-146`): do not descend into directories
    /// whose device differs from the operand the walk started from. The
    /// mount-point directory itself is still counted and printed — fts(3)
    /// returns it with its children pruned, so its own blocks stand.
    pub xdev: bool,
    /// `-H` (`FTS_COMFOLLOW`, `du.c:166-167`): follow a symlink given as an
    /// operand — the traversal root is the link's target, and everything
    /// below stays physical (`-L`'s full-logical walk is the other flag).
    pub comfollow: bool,
    /// The operands, defaulting to the single name `.` (`du.c:184-187`).
    pub roots: Vec<String>,
}

/// `du.c:88-192` plus the `BLOCKSIZE` reading `getbsize` performs: parse the
/// options and resolve the unit. `env_blocksize` is the value of `BLOCKSIZE` if
/// one is set.
///
/// The second result holds the warnings C would have printed from inside
/// `getbsize`, in order: a library that writes to standard error is not what this
/// half is for, so the program forwards them.
pub fn parse_args(
    args: &[&str],
    env_blocksize: Option<&str>,
) -> Result<(DuPlan, Vec<String>), DuOption> {
    let mut aflag = false;
    let mut dflag = false;
    let mut sflag = false;
    let mut grand_total = false;
    let mut inodes = false;
    let mut human = false;
    let mut skip_nodump = false;
    let mut xdev = false;
    let mut comfollow = false;
    let mut depth = DEPTH_UNBOUNDED;
    let mut unit: Option<u64> = None;
    let mut i = 0;
    while i < args.len() {
        let a = args[i];
        if a == "--" {
            i += 1;
            break;
        }
        let bytes = a.as_bytes();
        if bytes.len() < 2 || bytes[0] != b'-' {
            break; // BSD getopt stops at the first non-option
        }
        let mut k = 1;
        while k < bytes.len() {
            match bytes[k] {
                // `-P` is the physical default the walk already takes; `-r` is
                // accepted and ignored by C itself (`du.c:139-140`).
                b'P' | b'r' => {}
                b'a' => aflag = true,
                b'c' => grand_total = true,
                b'i' => inodes = true,
                b'n' => skip_nodump = true,
                b's' => sflag = true,
                b'k' => unit = Some(1024),
                b'm' => unit = Some(1024 * 1024),
                b'g' => unit = Some(1024 * 1024 * 1024),
                b'd' => {
                    dflag = true;
                    // `du.c:111` hands `optarg` to `atoi`, so the value is the
                    // following argv entry and a non-numeric one reads as zero.
                    let value = match args.get(i + 1) {
                        Some(v) => *v,
                        None => return Err(DuOption::Usage),
                    };
                    let parsed = atoi(value);
                    if !(0..=DEPTH_MAX).contains(&parsed) {
                        // `du.c:112-116`: warn, then usage.
                        return Err(DuOption::Usage);
                    }
                    depth = parsed;
                    i += 1;
                }
                b'h' => human = true,
                b'x' => xdev = true,
                b'H' => comfollow = true,
                b'L' => return Err(DuOption::NotWired(b'L' as char)),
                _ => return Err(DuOption::Usage),
            }
            k += 1;
        }
        i += 1;
    }

    // `du.c:173-182`: the three listing modes are mutually exclusive, and `-s`
    // is spelled as a depth of zero.
    if (aflag && (sflag || dflag)) || (sflag && dflag) {
        return Err(DuOption::Usage);
    }
    if sflag {
        depth = 0;
    }

    let mut warnings = Vec::new();
    let blocksize_bytes = match unit {
        Some(bytes) => bytes,
        None => blocksize_from_env(env_blocksize, &mut warnings),
    };
    let roots: Vec<String> = if i >= args.len() {
        Vec::from([String::from(".")])
    } else {
        args[i..].iter().map(|s| String::from(*s)).collect()
    };
    Ok((
        DuPlan {
            list_files: aflag,
            grand_total,
            inodes,
            human,
            skip_nodump,
            xdev,
            comfollow,
            depth,
            blocksize: blocksize_bytes / BLOCK_UNIT,
            roots,
        },
        warnings,
    ))
}

/// The descent test `-x` drives (`fts(3)`'s `FTS_XDEV` compare, requested at
/// `du.c:144-146`): prune a directory's children exactly when the flag is on
/// and the directory's device differs from the operand's device.
pub fn crosses_device(xdev: bool, entry_dev: u64, root_dev: u64) -> bool {
    xdev && entry_dev != root_dev
}

/// The gigabyte ceiling `getbsize.c:64` sets with `MAXB` ("No tera, peta, nor
/// exa"), and the three multipliers it knows.
const MAX_BYTES: u64 = 1024 * 1024 * 1024;

/// `getbsize` (`minix3/lib/libc/gen/getbsize.c:66-108`): `BLOCKSIZE` in bytes,
/// optionally suffixed with `K`/`M`/`G` in either case. Unset or empty is 512; a
/// negative value jumps to the minimum; anything unparsable warns "unknown
/// blocksize" and is then walked through the same two clamps, which is how C ends
/// up at 512 with three warnings for e.g. `10KB`.
pub fn blocksize_from_env(env: Option<&str>, warnings: &mut Vec<String>) -> u64 {
    let Some(text) = env.filter(|v| !v.is_empty()) else {
        return BLOCK_UNIT;
    };

    // `strtol(p, &ep, 10)` with the base-10 prefix scan of parse_long_decimal.
    let (value, suffix) = parse_long_decimal(text);
    if value < 0 {
        // `goto underflow` at getbsize.c:67-68.
        warnings.push(format!("{text}: minimum blocksize is 512"));
        return BLOCK_UNIT;
    }
    let mut n = if value == 0 { 1 } else { value as u64 };

    // `if (*ep && ep[1]) goto fmterr;`: at most one character may follow.
    let tail = suffix.as_bytes();
    let (mul, max): (u64, u64) = if tail.len() > 1 {
        warnings.push(format!("{text}: unknown blocksize"));
        (1, 0)
    } else {
        match tail.first() {
            // The ceiling `max` is the byte limit over the multiplier: a
            // gigabyte admits one of itself, and getbsize.c:64 caps at that.
            Some(b'G') | Some(b'g') => (MAX_BYTES, 1),
            Some(b'K') | Some(b'k') => (1024, MAX_BYTES / 1024),
            Some(b'M') | Some(b'm') => (1024 * 1024, MAX_BYTES / (1024 * 1024)),
            None => (1, MAX_BYTES),
            Some(_) => {
                warnings.push(format!("{text}: unknown blocksize"));
                (1, 0)
            }
        }
    };
    if n > max {
        // `getbsize.c:100-103`, where the message always quotes the gigabyte.
        // The message always quotes the gigabyte, as `getbsize.c:101` does.
        warnings.push(String::from("maximum blocksize is 1G"));
        n = max;
    }
    let blocksize = n.saturating_mul(mul);
    if blocksize < BLOCK_UNIT {
        // `getbsize.c:104-108` (`underflow:`).
        warnings.push(format!("{text}: minimum blocksize is 512"));
        return BLOCK_UNIT;
    }
    blocksize
}

/// `strtol` base 10 over an optional sign and a digit run: the value (saturated
/// to `i64` extremes the way `strtol` saturates to `LONG_MAX`), plus the
/// unconsumed remainder that stands for `ep`.
fn parse_long_decimal(text: &str) -> (i64, &str) {
    let bytes = text.as_bytes();
    let mut pos = 0;
    while pos < bytes.len() && bytes[pos].is_ascii_whitespace() {
        pos += 1;
    }
    let mut negative = false;
    if pos < bytes.len() && (bytes[pos] == b'+' || bytes[pos] == b'-') {
        negative = bytes[pos] == b'-';
        pos += 1;
    }
    let mut value: i64 = 0;
    let mut saturated = false;
    while pos < bytes.len() && bytes[pos].is_ascii_digit() {
        let digit = i64::from(bytes[pos] - b'0');
        if saturated {
            pos += 1;
            continue;
        }
        let signed = if negative { -digit } else { digit };
        value = match value.checked_mul(10).and_then(|v| v.checked_add(signed)) {
            Some(v) => v,
            None => {
                saturated = true;
                if negative { i64::MIN } else { i64::MAX }
            }
        };
        pos += 1;
    }
    (value, &text[pos..])
}

/// `atoi` as `du.c:111` uses it for `-d`: the same prefix scan, truncated to the
/// `int` range the C function returns.
fn atoi(text: &str) -> i32 {
    let (value, _) = parse_long_decimal(text);
    i32::try_from(value).unwrap_or(if value < 0 { i32::MIN } else { i32::MAX })
}

/// `du.c:66` (`COUNT`): an inode count under `-i`, otherwise the file system's
/// 512-byte block count.
pub fn count(inodes: bool, st_blocks: i64) -> i64 {
    if inodes { 1 } else { st_blocks }
}

/// `du.c:258-278` (`prstat`): the raw count under `-i`, otherwise
/// `howmany(blocks, blocksize)` in the chosen unit. There is no `-h` branch here
/// because the program refuses that letter until `humanize_number` lands.
pub fn printed_value(units: i64, inodes: bool, blocksize: u64) -> i64 {
    if inodes {
        return units;
    }
    howmany(units, blocksize)
}

/// `du.c:258-278` (`prstat`): the text that goes before the tab and the name.
/// The `-i` form wins over `-h` exactly as C's first `if` does, and a
/// `humanize_number` that cannot fit its five-byte buffer leaves an empty field
/// behind — that is what C's `buf[0] = '\0'` followed by `return -1` produces
/// (`humanize_number.c:88-89` with `:146-148`), so it is reproduced rather than
/// papered over with a fallback number.
pub fn printed_text(units: i64, plan: &DuPlan) -> String {
    if plan.inodes {
        return format!("{units}");
    }
    if plan.human {
        let bytes = units * BLOCK_UNIT as i64;
        return humanize_number(
            HUMAN_BUF,
            bytes,
            "",
            HN_AUTOSCALE,
            HN_B | HN_NOSPACE | HN_DECIMAL,
        )
        .unwrap_or_default();
    }
    format!("{}", printed_value(units, false, plan.blocksize))
}

/// `du.c:266`: `char buf[5]`. The fifth byte is `snprintf`'s terminator, so a
/// `-h` rendering gets four characters of text at most.
pub const HUMAN_BUF: usize = 5;

/// The `howmany` macro of `sys/param.h`: round up to a whole unit.
pub fn howmany(units: i64, blocksize: u64) -> i64 {
    let blocksize = i64::try_from(blocksize).unwrap_or(i64::MAX);
    if blocksize <= 1 {
        return units;
    }
    (units + blocksize - 1) / blocksize
}

/// `du.c:224-227`: a directory's post-order visit prints when its level is within
/// `-d`/`-s`, or when it is the root of a traversal that is not listing files.
pub fn prints_directory(level: i32, plan: &DuPlan) -> bool {
    level <= plan.depth || (!plan.list_files && level == 0)
}

/// `du.c:241-248`: a non-directory prints when files are listed, or when it is
/// itself the operand (level zero of the traversal).
pub fn prints_file(level: i32, plan: &DuPlan) -> bool {
    plan.list_files || level == 0
}

/// `du.c:198-209` (`-n`): an entry carrying `UF_NODUMP` is neither reported nor
/// descended into — C calls `fts_set(FTS_SKIP)` and then `continue`.
pub fn skipped_as_nodump(plan: &DuPlan, st_flags: u32) -> bool {
    plan.skip_nodump && st_flags & UF_NODUMP != 0
}

/// `du.c:237-239` with `linkchk` at 280 to 351: a file with more than one name is
/// counted once. What is reproduced is the answer — "has this `(dev, ino)` pair
/// been seen before?" — not C's golden-ratio hash constants, which are not
/// observable; the all-zero pair is tracked outside the table, as `sawzero` is
/// (`du.c:300-303`).
#[derive(Debug, Default)]
pub struct LinkSet {
    slots: Vec<Option<(u64, u64)>>,
    mask: usize,
    used: usize,
    saw_zero: bool,
}

impl LinkSet {
    /// An empty table; the first insertion allocates `1 << 10` slots, the size C
    /// starts at (`du.c:310-311`).
    pub fn new() -> Self {
        Self::default()
    }

    /// Record the pair and report whether it was already recorded.
    pub fn seen_before(&mut self, dev: u64, ino: u64) -> bool {
        if dev == 0 && ino == 0 {
            return core::mem::replace(&mut self.saw_zero, true);
        }
        if self.slots.is_empty() || (self.used << 1) >= self.mask {
            self.grow();
        }
        let mut at = Self::mix(dev, ino) & self.mask;
        let step = (Self::mix(dev, ino) >> 13) | 1;
        loop {
            match self.slots[at] {
                None => {
                    self.slots[at] = Some((dev, ino));
                    self.used += 1;
                    return false;
                }
                Some((d, i)) if d == dev && i == ino => return true,
                Some(_) => at = (at + step) & self.mask,
            }
        }
    }

    /// Double the table and re-place everything, keeping the load under a half as
    /// C does (`du.c:307-333`).
    fn grow(&mut self) {
        let slots = if self.mask == 0 {
            1usize << 10 // the starting size C picks at du.c:310-311
        } else {
            (self.mask + 1) * 2 // htshift++ at du.c:312-313
        };
        let old = core::mem::take(&mut self.slots);
        self.slots = alloc::vec![None; slots];
        self.mask = slots - 1;
        self.used = 0;
        for (dev, ino) in old.into_iter().flatten() {
            let mut at = Self::mix(dev, ino) & self.mask;
            let step = (Self::mix(dev, ino) >> 13) | 1;
            while self.slots[at].is_some() {
                at = (at + step) & self.mask;
            }
            self.slots[at] = Some((dev, ino));
            self.used += 1;
        }
    }

    /// Multiplicative hashing over the pair, the shape of C's `tmp * HTCONST`
    /// (`du.c:334-345`).
    fn mix(dev: u64, ino: u64) -> usize {
        let mut tmp = dev;
        tmp <<= 32;
        tmp |= ino;
        tmp = tmp.wrapping_mul(11_400_714_819_323_198_485);
        (tmp >> 32) as usize
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;

    /// `-H` is accepted and lands in the plan (the operand-follow flag).
    #[test]
    fn h_is_the_comfollow_flag() {
        assert!(plan(&["-H"]).comfollow);
        assert!(!plan(&[]).comfollow);
    }

    /// `-x` is accepted and lands in the plan.
    #[test]
    fn x_is_the_device_boundary_flag() {
        assert!(plan(&["-x"]).xdev);
        assert!(!plan(&[]).xdev);
    }

    /// The mount-boundary decision `-x` drives (the bin consults it before
    /// descending): the flag alone never prunes, a device match never prunes,
    /// and a different device under the flag prunes the descent. The
    /// mount-point directory itself keeps its own count either way.
    #[test]
    fn xdev_prunes_only_a_different_device_under_the_flag() {
        for (xdev, entry, root, prunes) in [
            (false, 1, 1, false),
            (false, 2, 1, false),
            (true, 1, 1, false),
            (true, 2, 1, true),
        ] {
            assert_eq!(
                crosses_device(xdev, entry, root),
                prunes,
                "{xdev}/{entry}/{root}"
            );
        }
    }

    fn plan(args: &[&str]) -> DuPlan {
        parse_args(args, None).unwrap().0
    }

    #[test]
    fn defaults_count_blocks_to_a_depth_of_everything() {
        let p = plan(&["dir"]);
        assert!(!p.list_files && !p.grand_total && !p.inodes && !p.skip_nodump);
        assert_eq!(p.depth, DEPTH_UNBOUNDED);
        assert_eq!(p.blocksize, 1, "the default unit is the 512-byte block");
        assert_eq!(p.roots, [String::from("dir")]);
    }

    #[test]
    fn no_operand_means_the_current_directory() {
        assert_eq!(plan(&[]).roots, [String::from(".")]);
        assert_eq!(plan(&["--"]).roots, [String::from(".")]);
    }

    #[test]
    fn units_are_divided_by_512() {
        assert_eq!(plan(&["-k", "d"]).blocksize, 2);
        assert_eq!(plan(&["-m", "d"]).blocksize, 2048);
        assert_eq!(plan(&["-g", "d"]).blocksize, 2_097_152);
        assert_eq!(plan(&["d"]).blocksize, 1);
    }

    #[test]
    fn listing_modes_are_mutually_exclusive() {
        // du.c:174-181: -a with -s or -d, and -s with -d.
        assert_eq!(parse_args(&["-a", "-s", "d"], None), Err(DuOption::Usage));
        assert_eq!(
            parse_args(&["-a", "-d", "1", "d"], None),
            Err(DuOption::Usage)
        );
        assert_eq!(
            parse_args(&["-s", "-d", "1", "d"], None),
            Err(DuOption::Usage)
        );
    }

    #[test]
    fn depth_comes_from_s_or_d() {
        assert_eq!(plan(&["-s", "d"]).depth, 0);
        assert_eq!(plan(&["-d", "2", "d"]).depth, 2);
        assert_eq!(plan(&["-d", "0", "d"]).depth, 0, "the shape of -s");
        assert_eq!(plan(&["-a", "d"]).depth, DEPTH_UNBOUNDED);
        assert!(plan(&["-a", "d"]).list_files);
    }

    #[test]
    fn atoi_shape_and_the_short_window_for_d() {
        // du.c:111-116: atoi accepts junk as zero, and the value must fit a short.
        assert_eq!(plan(&["-d", "abc", "d"]).depth, 0);
        assert_eq!(plan(&["-d", "32767", "d"]).depth, DEPTH_MAX);
        assert_eq!(
            parse_args(&["-d", "32768", "d"], None),
            Err(DuOption::Usage)
        );
        assert_eq!(parse_args(&["-d", "-1", "d"], None), Err(DuOption::Usage));
        assert_eq!(parse_args(&["-d"], None), Err(DuOption::Usage));
    }

    #[test]
    fn unwired_letters_are_named_and_the_noop_letters_are_taken() {
        // `-L` (the full-logical walk) is the remaining unwired letter; `-x`
        // and `-H` left this set as their facilities landed.
        assert_eq!(parse_args(&["-L", "d"], None), Err(DuOption::NotWired('L')));
        // -P is the physical walk this tree already does; -r is C's own no-op.
        assert_eq!(plan(&["-Pr", "d"]), plan(&["d"]));
        assert_eq!(
            parse_args(&["-z", "d"], None),
            Err(DuOption::Usage),
            "a letter outside the option string"
        );
    }

    #[test]
    fn clusters_and_operand_names_survive() {
        let p = plan(&["-cis", "a", "b"]);
        assert!(p.inodes && p.grand_total);
        assert_eq!(p.depth, 0);
        assert_eq!(p.roots, [String::from("a"), String::from("b")]);
        // BSD getopt does not permute: the first non-option ends the scan, and
        // everything after it is a name — dashes included.
        let p = plan(&["d", "-k"]);
        assert_eq!(p.roots, [String::from("d"), String::from("-k")]);
        assert_eq!(p.blocksize, 1, "the -k after a name is not an option");
    }

    #[test]
    fn blocksize_environment_is_read_the_way_getbsize_reads_it() {
        let mut w = Vec::new();
        assert_eq!(blocksize_from_env(None, &mut w), 512);
        assert_eq!(blocksize_from_env(Some(""), &mut w), 512);
        assert_eq!(blocksize_from_env(Some("1024"), &mut w), 1024);
        assert_eq!(blocksize_from_env(Some("4K"), &mut w), 4096);
        assert_eq!(blocksize_from_env(Some("2m"), &mut w), 2 * 1024 * 1024);
        assert_eq!(blocksize_from_env(Some("1G"), &mut w), MAX_BYTES);
        // A digit string with more than one trailing character is a format error
        // (getbsize.c:71-72), which lands on the floor through the two clamps.
        assert_eq!(blocksize_from_env(Some(" 8K "), &mut w), 512);
        assert!(
            w.iter()
                .any(|s| s.ends_with("unknown blocksize") && s.starts_with(" 8K ")),
            "{w:?}"
        );
    }

    #[test]
    fn environment_values_below_or_above_the_window_are_clamped_with_warnings() {
        let mut w = Vec::new();
        assert_eq!(blocksize_from_env(Some("8"), &mut w), 512);
        assert_eq!(w, vec![String::from("8: minimum blocksize is 512")]);

        let mut w = Vec::new();
        assert_eq!(blocksize_from_env(Some("-1"), &mut w), 512);
        assert_eq!(w, vec![String::from("-1: minimum blocksize is 512")]);

        // A suffix with junk after it walks all three branches, as the C
        // fall-through does: unknown, then the cap, then the floor.
        let mut w = Vec::new();
        assert_eq!(blocksize_from_env(Some("10KB"), &mut w), 512);
        assert_eq!(
            w,
            vec![
                String::from("10KB: unknown blocksize"),
                String::from("maximum blocksize is 1G"),
                String::from("10KB: minimum blocksize is 512"),
            ]
        );

        // Over the ceiling: capped to one gigabyte, which is a legal block size.
        let mut w = Vec::new();
        assert_eq!(blocksize_from_env(Some("9999999999G"), &mut w), MAX_BYTES);
        assert_eq!(w, vec![String::from("maximum blocksize is 1G")]);
    }

    #[test]
    fn counts_and_printed_values_follow_the_unit() {
        assert_eq!(count(false, 24), 24);
        assert_eq!(count(true, 24), 1, "-i counts the entry, not its blocks");
        assert_eq!(printed_value(24, false, 1), 24);
        assert_eq!(printed_value(24, false, 2), 12);
        assert_eq!(printed_value(25, false, 2), 13, "howmany rounds up");
        assert_eq!(printed_value(24, true, 2), 24, "-i ignores the unit");
        assert_eq!(howmany(0, 2), 0);
        assert_eq!(howmany(1, 2_097_152), 1);
    }

    #[test]
    fn printing_depths_mirror_the_two_arms() {
        let all = plan(&["d"]);
        assert!(prints_directory(0, &all));
        assert!(prints_directory(7, &all));
        assert!(
            !prints_file(3, &all),
            "without -a a deep file is folded into its directory"
        );
        assert!(
            prints_file(0, &all),
            "a file given as the operand is its own total"
        );

        let top = plan(&["-s"]);
        assert!(prints_directory(0, &top));
        assert!(!prints_directory(1, &top));
        assert!(!prints_file(2, &top));

        let listed = plan(&["-a"]);
        assert!(prints_file(2, &listed));
        assert!(prints_directory(9, &listed));
    }

    #[test]
    fn nodump_flag_skips_only_under_n() {
        let plain = plan(&["d"]);
        let skipping = plan(&["-n", "d"]);
        assert!(skipped_as_nodump(&skipping, UF_NODUMP));
        assert!(!skipped_as_nodump(&plain, UF_NODUMP));
        assert!(!skipped_as_nodump(&skipping, 0));
        assert!(!skipped_as_nodump(&skipping, UF_NODUMP << 3));
    }

    #[test]
    fn a_link_is_counted_once_however_many_names_it_has() {
        let mut seen = LinkSet::new();
        assert!(!seen.seen_before(3, 17), "first sight");
        assert!(
            seen.seen_before(3, 17),
            "the same inode through another name"
        );
        assert!(!seen.seen_before(3, 18), "another inode is another file");
        assert!(
            !seen.seen_before(4, 17),
            "the same inode number on another device is another file"
        );
    }

    #[test]
    fn the_zero_pair_is_tracked_outside_the_table() {
        // du.c:300-303: (0, 0) is never stored, and still counts as seen once.
        let mut seen = LinkSet::new();
        assert!(!seen.seen_before(0, 0));
        assert!(seen.seen_before(0, 0));
        assert!(!seen.seen_before(1, 1), "and it does not shadow real pairs");
    }

    #[test]
    fn h_flag_renders_the_prstat_shape() {
        let human = plan(&["-h", "d"]);
        assert!(human.human);
        // 24 blocks is what `du /etc` reads on the machine: 12 KiB.
        assert_eq!(printed_text(24, &human), String::from("12K"));
        let both = plan(&["-hi", "d"]);
        assert_eq!(
            printed_text(24, &both),
            String::from("24"),
            "-i prints the raw count first (du.c:263-265)"
        );
        let kilo = plan(&["-hk", "d"]);
        assert_eq!(
            printed_text(24, &kilo),
            String::from("12K"),
            "-h renders bytes, so the chosen unit does not enter the text"
        );
        assert_eq!(printed_text(24, &plan(&["d"])), String::from("24"));
        assert_eq!(printed_text(24, &plan(&["-k", "d"])), String::from("12"));
    }

    #[test]
    fn the_table_grows_and_keeps_every_answer_it_already_gave() {
        let mut seen = LinkSet::new();
        for ino in 0..5000u64 {
            assert!(!seen.seen_before(1, ino), "fresh inode {ino}");
        }
        for ino in 0..5000u64 {
            assert!(
                seen.seen_before(1, ino),
                "{ino} was seen before the table grew"
            );
        }
    }
}
