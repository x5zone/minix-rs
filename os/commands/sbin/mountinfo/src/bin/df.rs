//! Minix-RS `df` — the doing half over `minix_mountinfo::df`.
//!
//! Ground truth: `minix3/bin/df/df.c`. With no operands `df` asks the VFS for
//! every mounted file system (`getmntinfo` → `getvfsstat`, which the tree
//! serves with `VFS_GETVFSSTAT`) and prints one row per record; with file
//! operands it resolves each to a mount point (`stat`/`getmntpt`, `:163-205`)
//! and prints the filesystem behind it. The block arithmetic, the layouts and
//! the `BLOCKSIZE` parser live in the library (`crate::df`); this program
//! drives the VFS calls, applies the `-a`/`-l`/`-n`/`-t` table filters
//! (`regetmntinfo`, `:277-322`) and emits rows.
//!
//! Wired: the whole usage string — `[-agln] [-Ghkm|-ihkm|-Pk] [-t type]`.
//! `-k`/`-m`/`-g` select the report block; no size letter means C's
//! `getbsize` (512 bytes, or `BLOCKSIZE` from the environment); `-h` switches
//! to humanised figures; `-i` appends the inode columns; `-P` is the POSIX
//! row (only 512- and 1024-byte blocks); `-G` is the verbose SunOS record;
//! `-a` keeps `MNT_IGNORE` entries, `-l` keeps local ones, `-t` keeps the
//! named types (`no` prefix inverts), and `-n` prints the cached table
//! instead of re-reading each mount with `statvfs`.
//!
//! Mechanical deviations: C's `getmntinfo(MNT_WAIT)` is one VFS call that
//! refreshes every record; this tree's `getvfsstat` wrapper only exposes the
//! cached form, so the refresh is one `statvfs` per mount (`:307-314`), the
//! same request the fallback loop makes. A failed refresh keeps the cached
//! record rather than printing whatever the buffer held, which is the only
//! sane reading of C's unchecked `(void)statvfs(...)`. The two fatal
//! mount-table sentences keep this shell's existing wording instead of
//! rendering `strerror`.

#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

extern crate alloc;

#[path = "../bin_support.rs"]
mod support;

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
use minix_mountinfo::df::{
    BLOCK_512, BLOCK_G, BLOCK_K, BLOCK_M, TypeList, default_header, format_block_row, format_g_row,
    format_h_row, format_i_row, format_p_row, getbsize, h_header, hidden_by_filter, p_header,
    parse_type_list, type_selected,
};
use minix_sys::{Stat, StatvfsBuf, stat, statvfs, statvfs_off};

/// Matches the VFS mount table width (`NR_MNTS`).
const MAX_MOUNTS: usize = 16;

/// `S_IFBLK` (`minix3/sys/sys/stat.h:141`): the operand names a device.
const S_IFBLK: u32 = 0o060000;

/// `S_IFMT` (`minix3/sys/sys/stat.h:138`): the file-type mask.
const S_IFMT: u32 = 0o170000;

fn zeroed_stat() -> Stat {
    // SAFETY: Stat is a plain byte struct of integers; all-zero is a valid bit
    // pattern that `stat` overwrites on success.
    unsafe { core::mem::zeroed() }
}

/// The program name C's `getprogname()` would report.
fn program_name(argv: &[String]) -> &str {
    argv.first()
        .map(|arg| arg.rsplit('/').next().unwrap_or(arg))
        .unwrap_or("df")
}

fn warn_line(prog: &str, sentence: &str) {
    let mut line = String::from(prog);
    line.push_str(": ");
    line.push_str(sentence);
    line.push('\n');
    support::warn(line.as_bytes());
}

fn fail_line(message: &str) -> ! {
    let mut line = String::from(message);
    line.push('\n');
    support::warn(line.as_bytes());
    support::terminate(1)
}

/// C's `errx(EXIT_FAILURE, …)`: program name, then the sentence.
fn failx(prog: &str, sentence: &str) -> ! {
    warn_line(prog, sentence);
    support::terminate(1)
}

/// C's `usage()` (`df.c:512-519`): the capital `Usage:` line and exit 1.
fn usage(prog: &str) -> ! {
    support::warn(
        format!("Usage: {prog} [-agln] [-Ghkm|-ihkm|-Pk] [-t type] [file | file_system ...]\n")
            .as_bytes(),
    );
    support::terminate(1)
}

/// The option letters, all of which C accepts (`df.c:93`).
#[derive(Default)]
struct Options {
    /// `-a`: keep `MNT_IGNORE` records.
    aflag: bool,
    /// `-G`: verbose SunOS record.
    gflag: bool,
    /// `-h`: humanised block figures.
    hflag: bool,
    /// `-i`: append the inode columns.
    iflag: bool,
    /// `-l`: keep only local file systems.
    lflag: bool,
    /// `-n`: print the cached table.
    nflag: bool,
    /// `-P`: POSIX row.
    pflag: bool,
    /// The explicit report block size (`-k`/`-m`/`-g`), or 0 for `getbsize`.
    usize: u64,
    /// The `-t` type list.
    typelist: Option<TypeList>,
}

/// One mount-table record with the two names pulled out for the layouts.
struct Record {
    from: String,
    on: String,
    buf: StatvfsBuf,
}

impl Record {
    fn from_buf(buf: StatvfsBuf) -> Self {
        Self {
            from: buf.get_name(statvfs_off::MNTFROMNAME),
            on: buf.get_name(statvfs_off::MNTONNAME),
            buf,
        }
    }
}

/// `getmntpt` (`df.c:220-231`): a device name resolves to the mount point
/// whose `f_mntfromname` matches it. `Err(())` is C's fatal
/// `err(EXIT_FAILURE, "Can't get mount information")` when the table itself
/// cannot be read.
fn getmntpt(name: &str) -> Result<Option<String>, ()> {
    let mut mounts = [StatvfsBuf::new(); MAX_MOUNTS];
    let count = minix_sys::getvfsstat(&mut mounts).map_err(|_| ())?;
    Ok(mounts[..count.min(MAX_MOUNTS)]
        .iter()
        .find(|buf| buf.get_name(statvfs_off::MNTFROMNAME) == name)
        .map(|buf| buf.get_name(statvfs_off::MNTONNAME)))
}

/// `regetmntinfo`'s single-record refresh (`df.c:305-315`): re-read the mount
/// unless `-n`, and keep the cached record when the fresh one belongs to a
/// different layer (the `f_mntfromname` guard) or the call failed.
fn refreshed(record: Record, nflag: bool) -> Record {
    if nflag {
        return record;
    }
    let mut fresh = StatvfsBuf::new();
    match statvfs(&record.on, &mut fresh) {
        Ok(()) => {
            let refreshed = Record::from_buf(fresh);
            if refreshed.from == record.from {
                refreshed
            } else {
                record
            }
        }
        Err(_) => record,
    }
}

/// The no-operand table path: fetch the cached table, then apply
/// `regetmntinfo` (`df.c:294-322`). The `-a`-only shortcut also refreshes
/// every record, exactly like the fallback loop does one at a time.
fn collect_mounts(options: &Options) -> Result<Vec<Record>, ()> {
    let mut mounts = [StatvfsBuf::new(); MAX_MOUNTS];
    let count = minix_sys::getvfsstat(&mut mounts).map_err(|_| ())?;
    let records: Vec<Record> = mounts[..count.min(MAX_MOUNTS)]
        .iter()
        .map(|buf| Record::from_buf(*buf))
        .collect();

    let list = options.typelist.as_ref();
    let shortcut = options.aflag && !options.lflag && list.is_none();
    if shortcut {
        return Ok(records
            .into_iter()
            .map(|record| refreshed(record, options.nflag))
            .collect());
    }
    Ok(records
        .into_iter()
        .filter(|record| {
            !hidden_by_filter(
                options.aflag,
                options.lflag,
                list,
                record.buf.get_u64(statvfs_off::FLAG),
                &record.buf.get_name(statvfs_off::FSTYPENAME),
            )
        })
        .map(|record| refreshed(record, options.nflag))
        .collect())
}

/// The operand path (`df.c:163-205`): resolve each operand to a mount point,
/// then read that mount; the `-l`/`-t` mismatches are warnings that drop the
/// record, and a failed read warns the operand.
fn collect_operands(prog: &str, operands: &[&str], options: &Options) -> Result<Vec<Record>, ()> {
    let mut records = Vec::new();
    for path in operands {
        let mut sb = zeroed_stat();
        let mountpoint = match stat(path, &mut sb) {
            Ok(()) if sb.st_mode & S_IFMT == S_IFBLK => match getmntpt(path)? {
                Some(point) => Some(point),
                None => Some(String::from(*path)),
            },
            Ok(()) => Some(String::from(*path)),
            Err(_) => getmntpt(path)?,
        };
        let Some(mountpoint) = mountpoint else {
            // `warn("%s", *argv)` (`df.c:198`): the path and the failed
            // `stat`'s number; this shell keeps its number-less wording.
            let mut line = String::from(prog);
            line.push_str(": ");
            line.push_str(path);
            line.push('\n');
            support::warn(line.as_bytes());
            continue;
        };
        let mut buf = StatvfsBuf::new();
        match statvfs(&mountpoint, &mut buf) {
            Ok(()) => {
                let flag = buf.get_u64(statvfs_off::FLAG);
                if options.lflag && flag & minix_mountinfo::df::MNT_LOCAL == 0 {
                    // `warnx("Warning: %s is not a local %s", …)` (`df.c:186-188`).
                    warn_line(prog, &format!("Warning: {path} is not a local file system"));
                } else if !type_selected(
                    options.typelist.as_ref(),
                    &buf.get_name(statvfs_off::FSTYPENAME),
                ) {
                    // `warnx("Warning: %s mounted as a %s %s", …)` (`df.c:190-194`).
                    warn_line(
                        prog,
                        &format!(
                            "Warning: {path} mounted as a {} file system",
                            buf.get_name(statvfs_off::FSTYPENAME)
                        ),
                    );
                } else {
                    records.push(Record::from_buf(buf));
                }
            }
            Err(_) => {
                let mut line = String::from(prog);
                line.push_str(": ");
                line.push_str(path);
                line.push('\n');
                support::warn(line.as_bytes());
            }
        }
    }
    Ok(records)
}

/// The whole program body; both `main` forms call it and it never returns.
fn run() -> ! {
    let argv: Vec<String> = support::args();
    let prog = program_name(&argv);
    let args: Vec<&str> = argv.iter().map(String::as_str).collect();

    let mut options = Options::default();
    let mut operands: Vec<&str> = Vec::new();
    let mut index = 1;
    while index < args.len() {
        let arg = args[index];
        if arg == "--" {
            operands.extend(args[index + 1..].iter().copied());
            break;
        }
        if arg.len() > 1 && arg.starts_with('-') && arg != "-" {
            let bytes = arg.as_bytes();
            let mut k = 1;
            while k < bytes.len() {
                match bytes[k] {
                    b'a' => options.aflag = true,
                    b'g' => {
                        options.hflag = false;
                        options.usize = BLOCK_G;
                    }
                    b'G' => options.gflag = true,
                    b'h' => {
                        options.hflag = true;
                        options.usize = 0;
                    }
                    b'i' => options.iflag = true,
                    b'k' => {
                        options.hflag = false;
                        options.usize = BLOCK_K;
                    }
                    b'l' => options.lflag = true,
                    b'm' => {
                        options.hflag = false;
                        options.usize = BLOCK_M;
                    }
                    b'n' => options.nflag = true,
                    b'P' => options.pflag = true,
                    b't' => {
                        let attached = &arg[k + 1..];
                        let value = if attached.is_empty() {
                            index += 1;
                            match args.get(index) {
                                Some(value) => *value,
                                None => {
                                    warn_line(prog, "option requires an argument -- t");
                                    usage(prog);
                                }
                            }
                        } else {
                            attached
                        };
                        if options.typelist.is_some() {
                            failx(prog, "only one -t option may be specified.");
                        }
                        options.typelist = match parse_type_list(value) {
                            Ok(list) => Some(list),
                            Err(_) => failx(prog, "empty type list"),
                        };
                        // The rest of the cluster was the argument.
                        break;
                    }
                    _ => usage(prog),
                }
                k += 1;
            }
        } else {
            operands.push(arg);
        }
        index += 1;
    }

    // C's two conflict sentences (`df.c:140-145`), prefix included.
    if options.gflag && (options.pflag || options.iflag) {
        failx(prog, "only one of -G and -P or -i may be specified");
    }
    if options.pflag && options.iflag {
        failx(prog, "only one of -P and -i may be specified");
    }
    // `-P -h` reaches `assert(hflag == 0)` in C (`df.c:455`); this shell
    // refuses the pair by name instead of aborting.
    if options.pflag && options.hflag {
        failx(prog, "-P and -h cannot be combined");
    }

    let records = if operands.is_empty() {
        match collect_mounts(&options) {
            Ok(records) => records,
            Err(()) => fail_line(&format!(
                "{prog}: retrieving mounted file system information failed"
            )),
        }
    } else {
        match collect_operands(prog, &operands, &options) {
            Ok(records) => records,
            Err(()) => fail_line(&format!("{prog}: Can't get mount information")),
        }
    };
    if records.is_empty() {
        // C prints nothing and returns 0 when the filters leave no records.
        support::terminate(0);
    }
    let maxwidth = records
        .iter()
        .map(|record| record.from.len())
        .max()
        .unwrap_or(0);

    // C resolves the block size and prints the header only when the first
    // record is about to be printed (`prtstat`'s `timesthrough == 1`), so a
    // run with no records never consults `BLOCKSIZE` and never warns.
    let (blocksize, label) = if options.usize == BLOCK_K {
        (
            BLOCK_K,
            String::from(if options.pflag {
                "1024-blocks"
            } else {
                "1K-blocks"
            }),
        )
    } else if options.usize == BLOCK_M {
        (BLOCK_M, String::from("1M-blocks"))
    } else if options.usize == BLOCK_G {
        (BLOCK_G, String::from("1G-blocks"))
    } else if options.hflag {
        (BLOCK_512, String::from("Size"))
    } else {
        let size = getbsize(support::env("BLOCKSIZE").as_deref());
        for warning in &size.warnings {
            warn_line(prog, warning);
        }
        (size.blocksize, size.header)
    };
    if options.pflag && blocksize != BLOCK_K && blocksize != BLOCK_512 {
        // `df.c:441-445`: `-P` only accepts 512- and 1024-byte blocks.
        failx(prog, "non-standard block size incompatible with -P");
    }

    if !options.gflag {
        if options.pflag {
            support::emit(p_header(&label).as_bytes());
        } else if options.hflag {
            support::emit(h_header(maxwidth).as_bytes());
        } else {
            support::emit(default_header(maxwidth, &label, options.iflag).as_bytes());
        }
    }

    let mut out = [0u8; 512];
    for record in &records {
        let buf = &record.buf;
        let used = if options.gflag {
            format_g_row(buf, &mut out)
        } else if options.pflag {
            format_p_row(
                &record.from,
                &record.on,
                blocksize,
                buf.get_u64(statvfs_off::BLOCKS),
                buf.get_u64(statvfs_off::BFREE),
                buf.get_u64(statvfs_off::BRESVD),
                buf.get_u64(statvfs_off::FRSIZE),
                &mut out,
            )
        } else if options.hflag {
            format_h_row(
                &record.from,
                &record.on,
                maxwidth,
                buf.get_u64(statvfs_off::BLOCKS),
                buf.get_u64(statvfs_off::BFREE),
                buf.get_u64(statvfs_off::BRESVD),
                buf.get_u64(statvfs_off::FRSIZE),
                buf.get_u64(statvfs_off::FILES),
                buf.get_u64(statvfs_off::FFREE),
                options.iflag,
                &mut out,
            )
        } else if options.iflag {
            format_i_row(
                &record.from,
                &record.on,
                maxwidth,
                blocksize,
                buf.get_u64(statvfs_off::BLOCKS),
                buf.get_u64(statvfs_off::BFREE),
                buf.get_u64(statvfs_off::BRESVD),
                buf.get_u64(statvfs_off::FRSIZE),
                buf.get_u64(statvfs_off::FILES),
                buf.get_u64(statvfs_off::FFREE),
                &mut out,
            )
        } else {
            format_block_row(
                &record.from,
                &record.on,
                maxwidth,
                blocksize,
                buf.get_u64(statvfs_off::BLOCKS),
                buf.get_u64(statvfs_off::BFREE),
                buf.get_u64(statvfs_off::BRESVD),
                buf.get_u64(statvfs_off::FRSIZE),
                &mut out,
            )
        };
        if let Ok(used) = used {
            support::emit(&out[..used]);
        }
    }
    support::terminate(0);
}

#[cfg(all(not(test), target_os = "none"))]
#[unsafe(no_mangle)]
extern "Rust" fn main() -> i32 {
    run()
}

#[cfg(any(test, not(target_os = "none")))]
fn main() {
    run()
}
