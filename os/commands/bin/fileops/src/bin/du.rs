//! Minix-RS `du` — the doing half over `minix_fileops::du`.
//!
//! Ground truth: `minix3/usr.bin/du/du.c`. The decisions — what to count, in
//! which unit, at which depth, and whether a hard-linked file has been counted
//! already — live in `minix_fileops::du`; this program supplies the `lstat` and
//! `getdents` legs and drives the walk, replacing fts(3) with this crate's own
//! directory decoder the way `rm -r`, `cp -R` and `chmod -R` do.
//!
//! Wired: `du [-P] [-a | -d depth | -s] [-chijklmnpr] [file ...]`, with the
//! operand defaulting to `.` and with `-H`/`-L`/`-x` refused up front for want of
//! the logical walk and the mount-boundary test. `-h` renders through
//! [`minix_fileops::humanize`], the crate's port of `humanize_number`. The walk is physical, which is C's default
//! (`du.c:88`), so a symbolic link is counted as the link the way `du`'s man page
//! describes it for `-P`.
//!
//! C's per-visit arithmetic (`du.c:211-249`) maps onto recursion rather than onto
//! fts's two visits per directory: a directory's total is its own `COUNT` plus its
//! children's totals, which is exactly what the `fts_number` chain accumulates, and
//! the single print per directory happens after that sum, where C's `FTS_DP` arm
//! prints. Two consequences are worth stating: a directory is never reported twice
//! (C skips its post-order `FTS_D` visit by ignoring it), and the `-c` total is the
//! sum over the returned subtree counts.
//!
//! Two deliberate differences from C, both in parts of the run a user rarely
//! reaches: an unreadable directory is reported and still counted, which is C's
//! `FTS_DNR` path (`du.c:230-235`) — the report itself is wired to the shared
//! renderer (`err_line` in the `minix-types` crate), because C's line is
//! `warnx("%s: %s", p->fts_path, strerror(p->fts_errno))` (`:233`), which renders
//! to exactly `du: <path>: <sentence>`; and the descriptor leaks in C's failure
//! paths are closed here. Unlike every other command in this family, `du` only
//! reads, so its real-machine check does not wait for a writable image — the
//! seeding and the on-machine totals are registered as this batch's follow-up.
//! A failed directory read is fatal, as in C: `if (errno) err(1, "fts_read")`
//! (`du.c:251-253`) prints `du: fts_read: <sentence>` and leaves the run, so the
//! totals of the directories already walked are never printed.

#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

extern crate alloc;

#[path = "../bin_support.rs"]
mod support;

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
use minix_fileops::cp;
use minix_fileops::dents;
use minix_fileops::du::{
    self, DuOption, DuPlan, LinkSet, count, printed_text, prints_directory, prints_file,
    skipped_as_nodump,
};
use minix_sys::{Errno, Fd, Stat, close, getdents, lstat, open};

/// The refill buffer for the walk (C's fts keeps its own; the size is not
/// observable, it only bounds how many records arrive per call).
const WALK_BUF: usize = 4096;

fn zeroed() -> Stat {
    // SAFETY: Stat is a plain byte struct of integers; all-zero is a valid bit
    // pattern that `lstat` overwrites on success.
    unsafe { core::mem::zeroed() }
}

/// C's `warnx("%s: %s", p->fts_path, strerror(p->fts_errno))` (`du.c:233`), the
/// `FTS_DNR`/`FTS_ERR`/`FTS_NS` arm: program name, the path, that number's
/// sentence — the same three fields `warn(3)` produces, which is why this goes to
/// the shared `err_line` renderer rather than a hand-built line.
fn warn_path(prog: &str, path: &str, code: Errno) {
    support::warn_line(prog, path, code);
}

/// The value of `name` in the environment, as C's `getenv` would find it: the
/// first entry that carries the prefix, with the `NAME=` removed.
fn env_value(name: &str) -> Option<String> {
    let prefix = format!("{name}=");
    support::envs().iter().find_map(|entry| {
        let text = core::str::from_utf8(entry).ok()?;
        text.strip_prefix(prefix.as_str()).map(String::from)
    })
}

/// One walk position (`du.c:197-249`): the entry's own count, its children's
/// totals when it is a directory, and the print this visit owes. Returns the count
/// in 512-byte blocks (or inodes, under `-i`) and whether anything failed.
fn du_visit(
    prog: &str,
    path: &str,
    level: i32,
    plan: &DuPlan,
    links: &mut LinkSet,
    root_dev: u64,
) -> (i64, bool) {
    let mut sb = zeroed();
    if let Err(e) = lstat(path, &mut sb) {
        // `du.c:230-235` (`FTS_NS`/`FTS_ERR`): report, count the failure, and
        // contribute nothing.
        warn_path(prog, path, e);
        return (0, true);
    }
    if skipped_as_nodump(plan, sb.st_flags) {
        // `du.c:198-209`: neither printed nor descended into.
        return (0, false);
    }

    let own = count(plan.inodes, sb.st_blocks);
    if cp::is_dir(sb.st_mode) {
        // `-x` (`du.c:145`, fts's `FTS_XDEV`): the directory keeps its own
        // count and its line, but a device other than the operand's stops
        // the descent — fts returns it with the children pruned.
        let (children, failed) = if du::crosses_device(plan.xdev, sb.st_dev, root_dev) {
            (0, false)
        } else {
            descend(prog, path, level + 1, plan, links, root_dev)
        };
        let total = own.saturating_add(children);
        if prints_directory(level, plan) {
            report(path, total, plan);
        }
        return (total, failed);
    }

    // `du.c:237-239`: a file whose link count is above one is `break`-ed out of
    // the switch when its (device, inode) pair has been seen before, so it is
    // neither counted nor printed again. Files with a single name never consult
    // the table, exactly as in C.
    if sb.st_nlink > 1 && links.seen_before(sb.st_dev, sb.st_ino) {
        return (0, false);
    }
    if prints_file(level, plan) {
        report(path, own, plan);
    }
    (own, false)
}

/// `du.c:258-278` (`prstat`): the number, a tab, the name. Which number — the
/// raw count under `-i`, the humanised text under `-h`, the unit-rounded blocks
/// otherwise — is `printed_text`'s decision, since that is the part with a
/// shape worth testing.
fn report(path: &str, units: i64, plan: &DuPlan) {
    support::emit(format!("{}\t{path}\n", printed_text(units, plan)).as_bytes());
}

/// Walk one directory's entries. An unopenable directory is C's `FTS_DNR`
/// (`du.c:230-235`): reported, counted as a failure, and the caller still adds the
/// directory's own blocks.
fn descend(
    prog: &str,
    dir: &str,
    level: i32,
    plan: &DuPlan,
    links: &mut LinkSet,
    root_dev: u64,
) -> (i64, bool) {
    let fd: Fd = match open(dir, cp::O_RDONLY, 0) {
        Ok(fd) => fd,
        Err(e) => {
            // C's `FTS_DNR` arm reports the same three-field line and carries on.
            warn_path(prog, dir, e);
            return (0, true);
        }
    };

    let mut total: i64 = 0;
    let mut failed = false;
    let mut buf = [0u8; WALK_BUF];
    loop {
        let transferred = match getdents(fd, &mut buf) {
            Ok(0) => break,
            Ok(n) => n,
            Err(e) => {
                // `du.c:251-253`: `if (errno) err(1, "fts_read")` — the C name of
                // the step, that number's sentence, and the run leaves; the totals
                // already gathered are never printed.
                support::warn_line(prog, "fts_read", e);
                support::terminate(1);
            }
        };
        let mut cursor = 0;
        while cursor < transferred {
            let entry = match dents::next_record(&buf, cursor) {
                Ok(Some((entry, next))) => {
                    cursor = next;
                    entry
                }
                // Buffer exhausted, or a record whose lengths contradict it:
                // stop this refill exactly where C's readdir returns NULL.
                Ok(None) | Err(_) => break,
            };
            if entry.ino == 0 {
                // A deleted slot: libc's readdir skips it (readdir.c:83-84).
                continue;
            }
            let name = match core::str::from_utf8(entry.name) {
                Ok(name) => name,
                Err(_) => continue,
            };
            if !dents::is_descendable(name) {
                continue;
            }
            let (units, child_failed) = du_visit(
                prog,
                &dents::join_path(dir, name),
                level,
                plan,
                links,
                root_dev,
            );
            total = total.saturating_add(units);
            failed |= child_failed;
        }
    }
    let _ = close(fd);
    (total, failed)
}

fn usage() -> ! {
    // `du.c:355-361`, with the refused letters still shown as C's usage line does.
    support::warn(b"usage: du [-H | -L | -P] [-a | -d depth | -s] [-cghikmnrx] [file ...]\n");
    support::terminate(1);
}

fn run() -> ! {
    let argv: Vec<String> = support::args();
    if argv.is_empty() {
        usage();
    }
    let args: Vec<&str> = argv.iter().map(String::as_str).collect();
    // C's `du` never calls `setprogname`; `getprogname()` still answers `argv[0]`'s
    // last segment because `crt0-common.c:156-165` seeds it from there, so every
    // `warnx` line starts with that string. C's own `usage()` (`du.c:356-362`) is a
    // hardcoded string and stays one.
    let prog = support::prog_name(&argv);

    let (plan, warnings) = match parse_with_env(&args[1..]) {
        Ok(v) => v,
        Err(DuOption::NotWired(c)) => {
            support::warn(format!("{prog}: -{c} not wired (logical walk, FTS_XDEV)\n").as_bytes());
            support::terminate(1);
        }
        Err(DuOption::Usage) => usage(),
    };
    // `getbsize` warns from inside the option processing, before any total.
    for text in warnings {
        support::warn(format!("{prog}: {text}\n").as_bytes());
    }

    let mut links = LinkSet::new();
    let mut grand: i64 = 0;
    let mut failed = false;
    for root in &plan.roots {
        // Each operand starts its own traversal, so `-x` compares against
        // this operand's device (fts roots one walk per operand).
        let mut sb = zeroed();
        let root_dev = match lstat(root, &mut sb) {
            Ok(_) => sb.st_dev,
            // A missing operand is reported by the walk itself; the boundary
            // flag is then irrelevant because nothing descends.
            Err(_) => 0,
        };
        let (units, this_failed) = du_visit(prog, root, 0, &plan, &mut links, root_dev);
        grand = grand.saturating_add(units);
        failed |= this_failed;
    }
    // `du.c:254-255`: the `-c` total is printed after every operand, as `total`.
    if plan.grand_total {
        report("total", grand, &plan);
    }
    support::terminate(if failed { 1 } else { 0 });
}

/// The option face with the environment already read, split out so the tests of
/// the deciding half do not have to touch the process environment.
fn parse_with_env(args: &[&str]) -> Result<(DuPlan, Vec<String>), DuOption> {
    du::parse_args(args, env_value("BLOCKSIZE").as_deref())
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
