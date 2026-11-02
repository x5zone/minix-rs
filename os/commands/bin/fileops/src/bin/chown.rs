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
use minix_sys::{Stat, chown, stat};

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
                b'R' | b'H' | b'L' | b'P' | b'h' => return Err("unwired"),
                _ => return Err("usage"),
            }
        }
        i += 1;
    }
    Ok((flags, i))
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
        Err("unwired") => {
            warn(b"recursive/link handling not wired (-R/-H/-L/-P/-h)\n");
            support::terminate(1);
        }
        Err(_) => usage(ischown),
    };
    let operands = &args[after_opts.min(args.len())..];

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
        // `chown.c:212` change_owner per operand; `-f` hides the warning but
        // still sets the exit status (`chown.c:230-234`).
        match chown(path, owner, group) {
            Ok(()) => {
                if flags.verbose {
                    support::emit(format!("{path}\n").as_bytes());
                }
            }
            Err(_) => {
                if !flags.force {
                    warn(format!("chown: {path}: cannot change owner\n").as_bytes());
                }
                any_failed = true;
            }
        }
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
