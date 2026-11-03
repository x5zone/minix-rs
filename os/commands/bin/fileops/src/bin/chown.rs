//! Minix-RS `chown` / `chgrp` — the doing half over `minix_fileops::chown`.
//!
//! Ground truth: `minix3/sbin/chown/chown.c`, one source compiled under two
//! names; `ischown = myname[2] == 'o'` (`chown.c:94`) selects `chown` (a
//! `[user][:group]` spec) vs `chgrp` (the whole argument is a group). The
//! argument grammar — the `:`/`.` split, the empty-part "leave unchanged" rule,
//! the numeric `#`/`id()` parse — lives in `minix_fileops::chown` (host-tested
//! there); this program supplies the filesystem leg: resolve the spec to a
//! numeric `(uid, gid)`, then `chown(2)` each operand (`chown.c:212`
//! `change_owner(p->fts_accpath, uid, gid)` on the current path).
//!
//! Wired: `chown [-f] [-v] owner[:group] file ...` and `chgrp [-f] [-v] group
//! file ...`, numeric owner and group only. A group or user named rather than
//! numbered needs the password DB (`getpwnam`/`getgrnam`, `chown.c:252/:266`),
//! which this tree does not wrap; as in C without that database, the lookup
//! fails and the token falls to `id()`, whose verdict (`chown.c:288`,
//! `"%s: invalid %s name"`, prefixed by the program name) is what the bin
//! prints before exiting non-zero. `(uid -1)`/`(gid -1)` from an empty half
//! means "keep that id" (`chown.c:154-156`), and the VFS `do_chown` gate
//! enforces the same (`protect.rs:146`) — so an unset half is passed as
//! `u32::MAX`, matching C's `(uid_t)-1` exactly.
//!
//! Wired: `--reference=rfile` (or `--reference rfile`) copies the owner/group
//! off an existing file instead of an owner spec (`chown.c:181-189`) — the
//! non-recursive form, which needs only `stat` (the reference itself is not
//! rewritten). `-v` echoes each changed operand (`chown.c:237-240`).
//!
//! Declared residual (refused up front, like `chmod`'s `-R`): `-R` and
//! `-H`/`-L`/`-P` (the recursive hierarchy walk and its traversal modes,
//! `chown.c:98-155`, needing fts(3)) and `-h` (`lchown`, `chown.c:121-130`,
//! not wrapped). With `-R`, C also walks the reference's own subtree; that
//! pairing is covered by the same fts(3) residual, so `--reference` here is
//! non-recursive only.

#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

extern crate alloc;

#[path = "../bin_support.rs"]
mod support;

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
use minix_fileops::chown::{ChownError, OwnerIds, parse_owner_spec};
use minix_fileops::{cp, dents};
use minix_sys::{Stat, chown, close, getdents, lstat, open, stat};

/// C's `(uid_t)-1` / `(gid_t)-1` initial value (`chown.c:158-159`): an unset
/// half that `chown(2)` reads as "leave this id alone".
const KEEP: u32 = u32::MAX;

fn warn(bytes: &[u8]) {
    support::warn(bytes);
}

fn usage(ischown: bool) -> ! {
    let msg: &str = if ischown {
        "usage: chown [-R [-H | -L | -P]] [-fhv] owner[:group] file ...\n"
    } else {
        "usage: chgrp [-R [-H | -L | -P]] [-fhv] group file ...\n"
    };
    warn(msg.as_bytes());
    support::terminate(1);
}

/// The `-f`/`-v` flags, the `--reference` file (if given), and the non-numeric
/// option letters C accepts but this build refuses. Mirrors `chown.c:98`
/// `getopt_long("HLPRfhv", {reference})`.
struct Flags<'a> {
    force: bool,
    verbose: bool,
    /// `-R`: the recursive hierarchy walk (`chown.c:115-116`, `:144-157`).
    recursive: bool,
    /// `-H`: follow the command-line operands only (`FTS_COMFOLLOW`).
    comfollow: bool,
    /// `-L`: follow every symlink (`FTS_LOGICAL`).
    logical: bool,
    /// `-h`: act on the symlink itself. Minix's `lchown` emulation makes a
    /// symlink a silent success (`tools/compat/lchown.c:50-59`).
    hflag: bool,
    /// `--reference=rfile` (or `--reference rfile`): copy owner/group off an
    /// existing file instead of an owner spec (`chown.c:181-189`).
    reference: Option<&'a str>,
}

/// Scan the leading option run. Returns the flags and the index of the first
/// non-option operand, or `Err` for a refused/unrecognised option (the caller
/// turns a refusal into a message and usage into the usage text).
fn parse_flags<'a>(args: &[&'a str]) -> Result<(Flags<'a>, usize), &'static str> {
    let mut flags = Flags {
        force: false,
        verbose: false,
        recursive: false,
        comfollow: false,
        logical: false,
        hflag: false,
        reference: None,
    };
    let mut i = 0;
    while i < args.len() {
        let a = args[i];
        if !a.starts_with('-') || a == "-" {
            break;
        }
        if a == "--" {
            i += 1;
            break;
        }
        // Long option `--reference[=rfile]` (`chown.c:74-75` required_argument).
        if let Some(rest) = a.strip_prefix("--") {
            if let Some(eq) = rest.strip_prefix("reference=") {
                flags.reference = Some(eq);
                i += 1;
                continue;
            } else if rest == "reference" {
                // The value is the next argument; refuse if it is absent.
                let next = args.get(i + 1).copied().ok_or("usage")?;
                flags.reference = Some(next);
                i += 2;
                continue;
            }
            return Err("usage");
        }
        // Each `-X` cluster letter (`chown.c:98` optstring "HLPRfhv").
        for b in a[1..].bytes() {
            match b {
                b'f' => flags.force = true,
                b'v' => flags.verbose = true,
                // `-P` is the physical default the walk already takes
                // (`chown.c:144`); the other four select behaviour.
                b'P' => {}
                b'R' => flags.recursive = true,
                b'H' => flags.comfollow = true,
                b'L' => flags.logical = true,
                b'h' => flags.hflag = true,
                _ => return Err("usage"),
            }
        }
        i += 1;
    }
    Ok((flags, i))
}

/// `change_owner` (`chown.c:186`) under `-h`: Minix's `lchown` is an
/// emulation (`minix3/tools/compat/lchown.c:50-59`) — it `lstat`s the path
/// and treats a symlink as a **silent success** (nothing changes), falling
/// back to `chown` for everything else. Without `-h` the plain `chown(2)`
/// follows symlinks, which is C's default.
fn change_owner(path: &str, owner: u32, group: u32, hflag: bool) -> Result<(), minix_sys::Errno> {
    if hflag {
        let mut sb: Stat = unsafe { core::mem::zeroed() };
        lstat(path, &mut sb)?;
        if cp::is_link(sb.st_mode) {
            return Ok(());
        }
    }
    chown(path, owner, group)
}

/// One fts-style walk over an operand (`chown.c:189-240`): directories are
/// changed **after** their children (`FTS_DP`), symlinks found by a physical
/// walk are skipped unless `-h`, and `-L` follows every symlink with the
/// ancestor `(dev, ino)` chain guarding against cycles (`FTS_DC`, silently
/// ignored as in the other walkers).
fn walk_operand(
    flags: &Flags<'_>,
    path: &str,
    owner: u32,
    group: u32,
    chain: &mut Vec<(u64, u64)>,
) -> bool {
    walk_entry(flags, path, 0, owner, group, chain)
}

fn walk_entry(
    flags: &Flags<'_>,
    path: &str,
    level: i32,
    owner: u32,
    group: u32,
    chain: &mut Vec<(u64, u64)>,
) -> bool {
    let mut sb: Stat = unsafe { core::mem::zeroed() };
    let followed = flags.logical || (flags.comfollow && level == 0);
    let looked_up = if followed {
        stat(path, &mut sb)
    } else {
        lstat(path, &mut sb)
    };
    if looked_up.is_err() {
        // `FTS_NS`: warn and skip (`chown.c:210-215`).
        if !flags.force {
            warn(format!("chown: {path}: cannot change owner\n").as_bytes());
        }
        return true;
    }

    if cp::is_dir(sb.st_mode) {
        let on_chain = flags.logical && chain.contains(&(sb.st_dev, sb.st_ino));
        if on_chain {
            return false; // `FTS_DC`: silently ignored.
        }
        let mut failed = false;
        if flags.recursive {
            chain.push((sb.st_dev, sb.st_ino));
            failed |= walk_children(flags, path, level, owner, group, chain);
            chain.pop();
        }
        // The directory itself at its post-order visit (`FTS_DP`).
        failed | report_change(flags, path, owner, group)
    } else if !followed && cp::is_link(sb.st_mode) {
        // `FTS_SL`: ignored unless `-h`, which then meets the silent
        // symlink success of the emulated `lchown`.
        if !flags.hflag {
            return false;
        }
        report_change(flags, path, owner, group)
    } else {
        report_change(flags, path, owner, group)
    }
}

/// One directory's children (`fts_read`'s descent): each entry is visited
/// with the parent's traversal state.
fn walk_children(
    flags: &Flags<'_>,
    dir: &str,
    level: i32,
    owner: u32,
    group: u32,
    chain: &mut Vec<(u64, u64)>,
) -> bool {
    let fd = match open(dir, cp::O_RDONLY, 0) {
        Ok(fd) => fd,
        Err(_) => {
            // `FTS_DNR`: warn, still change the directory itself
            // (`chown.c:199-203`).
            if !flags.force {
                warn(format!("chown: {dir}: cannot change owner\n").as_bytes());
            }
            return true;
        }
    };
    let mut failed = false;
    let mut buf = [0u8; 4096];
    loop {
        let transferred = match getdents(fd, &mut buf) {
            Ok(0) => break,
            Ok(n) => n,
            Err(_) => {
                // `chown.c:239-240`: `err(EXIT_FAILURE, "fts_read")`.
                support::warn(b"chown: fts_read failed\n");
                support::terminate(1);
            }
        };
        let mut cursor = 0;
        while cursor < transferred {
            let name = match dents::next_record(&buf, cursor) {
                Ok(Some((entry, next))) => {
                    cursor = next;
                    if entry.ino == 0 {
                        continue;
                    }
                    match core::str::from_utf8(entry.name) {
                        Ok(name) if dents::is_descendable(name) => name,
                        _ => continue,
                    }
                }
                Ok(None) | Err(_) => break,
            };
            failed |= walk_entry(
                flags,
                &dents::join_path(dir, name),
                level + 1,
                owner,
                group,
                chain,
            );
        }
    }
    let _ = close(fd);
    failed
}

/// Apply `change_owner` and echo under `-v` (`chown.c:227-235`).
fn report_change(flags: &Flags<'_>, path: &str, owner: u32, group: u32) -> bool {
    match change_owner(path, owner, group, flags.hflag) {
        Ok(()) => {
            if flags.verbose {
                support::emit(format!("{path}\n").as_bytes());
            }
            false
        }
        Err(_) => {
            if !flags.force {
                warn(format!("chown: {path}: cannot change owner\n").as_bytes());
            }
            true
        }
    }
}

/// The whole program body; both `main` forms call it and it never returns.
fn run() -> ! {
    let argv: Vec<String> = support::args();
    let args: Vec<&str> = argv.iter().map(String::as_str).collect();

    // `ischown = myname[2] == 'o'` (`chown.c:94`). argv[0] may be a bare name
    // or a path; C's getprogname strips the directory, so split on the last
    // '/' and index into the basename.
    let progname = args
        .first()
        .map(|s| s.rsplit('/').next().unwrap_or(s))
        .unwrap_or("");
    let ischown = progname.as_bytes().get(2) == Some(&b'o');

    let (flags, after_opts) = match parse_flags(&args[1.min(args.len())..]) {
        Ok(v) => v,
        Err(_) => usage(ischown),
    };
    // `chown.c:150-155`: the logical walk and acting on links themselves
    // contradict each other.
    if flags.recursive && flags.logical && flags.hflag {
        warn(b"chown: the -L and -h options may not be specified together.\n");
        support::terminate(1);
    }
    // `parse_flags` walked `args[1..]`, so its consumed count is relative to
    // that slice — the operands start one past argv[0] plus the options.
    let operands = &args[(1 + after_opts).min(args.len())..];

    // Owner/group come from either `--reference` (stat the model file, no
    // owner-spec operand — `chown.c:181-189`) or the leading owner spec
    // (`chown.c:161-177`). The two are exclusive (C `if (reference == NULL)`).
    let (owner, group, files): (u32, u32, &[&str]) = if let Some(ref_path) = flags.reference {
        // `chown.c:183-188`: stat the reference, then `if (ischown)
        // uid = st.st_uid; gid = st.st_gid;`. A chgrp keeps uid unchanged.
        // `chown.c:141`: at least one file operand must remain after options.
        if operands.is_empty() {
            usage(ischown);
        }
        let mut st: Stat = unsafe { core::mem::zeroed() };
        if stat(ref_path, &mut st).is_err() {
            warn(format!("chown: cannot stat '{ref_path}'\n").as_bytes());
            support::terminate(1);
        }
        let owner = if ischown { st.st_uid } else { KEEP };
        (owner, st.st_gid, operands)
    } else {
        // `chown.c:141`: need at least the owner spec plus one file.
        if operands.len() < 2 {
            usage(ischown);
        }
        let ids = match parse_owner_spec(operands[0], ischown) {
            Ok(ids) => ids,
            // `errx`/`err` prefix the program name (`chown.c:288`), so the
            // verdict renders as "chown: <name>: invalid <user|group> name"
            // — or "chgrp: ..." when invoked as chgrp. With no password DB
            // every named token lands here, matching C in this environment.
            Err(ChownError::Invalid(msg)) => {
                warn(format!("{progname}: {msg}\n").as_bytes());
                support::terminate(1);
            }
        };
        let OwnerIds { uid, gid } = ids;
        (uid.unwrap_or(KEEP), gid.unwrap_or(KEEP), &operands[1..])
    };

    let mut any_failed = false;
    for path in files {
        // C opens one fts walk per operand (`chown.c:189`); `-f` hides the
        // warnings but still sets the exit status (`:230-234`).
        let mut chain: Vec<(u64, u64)> = Vec::new();
        any_failed |= walk_operand(&flags, path, owner, group, &mut chain);
    }
    support::terminate(if any_failed { 1 } else { 0 });
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
