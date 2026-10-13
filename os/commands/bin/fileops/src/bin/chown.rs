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
//! which this tree does not wrap, so it is refused by name through
//! `ChownError::NamedUnsupported` rather than resolved to a guessed id. `(uid
//! -1)`/`(gid -1)` from an empty half means "keep that id" (`chown.c:154-156`),
//! and the VFS `do_chown` gate enforces the same (`protect.rs:146`) — so an
//! unset half is passed as `u32::MAX`, matching C's `(uid_t)-1` exactly.
//!
//! Declared residual (refused up front, like `chmod`'s `-R`): `-R` and
//! `-H`/`-L`/`-P` (the recursive hierarchy walk and its traversal modes,
//! `chown.c:98-155`, needing fts(3)), `-h` (`lchown`, `chown.c:121-130`, not
//! wrapped), and `--reference=rfile` (copy owner off an existing file,
//! `chown.c:181-189`). `-v` echoes each changed operand (`chown.c:237-240`).

#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

extern crate alloc;

#[path = "../bin_support.rs"]
mod support;

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
use minix_fileops::chown::{ChownError, OwnerIds, parse_owner_spec};
use minix_sys::chown;

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

/// The `-f`/`-v` flags plus the two non-numeric option letters C accepts but
/// this build refuses. Mirrors `chown.c:98` `getopt("HLPRfhv")`.
struct Flags {
    force: bool,
    verbose: bool,
}

/// Scan the leading option run. Returns the flags and the index of the first
/// non-option operand, or `Err` for a refused/unrecognised option (the caller
/// turns a refusal into a message and usage into the usage text).
fn parse_flags(args: &[&str]) -> Result<(Flags, usize), &'static str> {
    let mut flags = Flags { force: false, verbose: false };
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
            warn(b"recursive/link handling not wired (-R/-H/-L/-P/-h, --reference)\n");
            support::terminate(1);
        }
        Err(_) => usage(ischown),
    };
    let operands = &args[after_opts.min(args.len())..];

    // `chown.c:141`: need at least the owner spec plus one file.
    if operands.len() < 2 {
        usage(ischown);
    }

    let ids = match parse_owner_spec(operands[0], ischown) {
        Ok(ids) => ids,
        Err(ChownError::NamedUnsupported(name)) => {
            warn(format!("chown: {name}: user/group name lookup needs the password DB\n").as_bytes());
            support::terminate(1);
        }
        Err(ChownError::Invalid(msg)) => {
            warn(format!("{msg}\n").as_bytes());
            support::terminate(1);
        }
    };
    let OwnerIds { uid, gid } = ids;
    let owner = uid.unwrap_or(KEEP);
    let group = gid.unwrap_or(KEEP);

    let mut any_failed = false;
    for path in &operands[1..] {
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
