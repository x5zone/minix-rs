//! Minix-RS `renice` — the doing half over `minix_proctools::renice`.
//!
//! Ground truth: `minix3/usr.bin/renice/renice.c`. The argument walk, base-0
//! number parsing, and priority clamping live in `minix_proctools::renice`;
//! this program supplies the syscall leg — for each resolved target read
//! `getpriority`, fold in the increment and clamp, `setpriority`, and print
//! `"<id>: old priority <n>, new priority <m>"` (`renice.c:136-156`), exiting
//! `0` only when every target succeeded (`errs == 0`, `renice.c:111`).
//!
//! Wired: `renice [-n] priority [[-p] pid ...] [[-g] pgrp ...] [[-u] user ...]`
//! — the process, process-group and user scopes. Unlike `nice`, `renice` acts
//! on existing processes and never execs, so its whole control path is
//! reachable. The `-u` name resolves through the shared password-file parser
//! (`minix_fileops::passwd`, the `getpwnam` deciding half): the database is
//! read from C `_PATH_PASSWD` (`/etc/passwd`) and the first matching account
//! supplies the uid. When the file cannot be read or the name is absent,
//! `getpwnam` answers NULL and C prints `warnx("%s: unknown user")` and
//! counts an error (`renice.c:96-105`) — the same face this program shows.
//!
//! Declared residual: C's `warn`/`warnx` `strerror` suffixes are not
//! reproduced (this tree carries no errno table on that seam).

#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

extern crate alloc;

#[path = "../bin_support.rs"]
mod support;

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
use minix_proctools::passwd;
use minix_proctools::renice::{Plan, PlanError, Scope, Target, clamp_prio};
use minix_sys::{close, getpriority, open, read, setpriority};

/// C `_PATH_PASSWD`: the text database this build reads directly (no
/// `pwd_mkdb` hash files exist here).
const PASSWD_PATH: &str = "/etc/passwd";
const READ_BUF: usize = 4096;

/// `getpwnam`'s doing half: read the whole database and hand the text to
/// the shared parser. `None` = the file is unavailable (the same face as a
/// NULL entry).
fn read_passwd_db() -> Option<String> {
    let fd = open(PASSWD_PATH, 0, 0).ok()?;
    let mut text = String::new();
    let mut chunk = [0u8; READ_BUF];
    loop {
        match read(fd, &mut chunk) {
            Ok(0) => break,
            Ok(n) => text.push_str(&String::from_utf8_lossy(&chunk[..n])),
            Err(_) => {
                let _ = close(fd);
                return None;
            }
        }
    }
    let _ = close(fd);
    Some(text)
}

fn usage() -> ! {
    // C `renice.c:158-162`: three fprintf pieces, capital `Usage:`, the
    // program name embedded, brackets verbatim.
    support::warn(b"Usage: renice [<priority> | -n <incr>] ");
    support::warn(b"[[-p] <pids>...] [-g <pgrp>...] ");
    support::warn(b"[-u <user>...]\n");
    support::terminate(1);
}

/// C `donice` for one numeric target. Returns true on failure (the error
/// counter). `getpriority` on a bare id (which 0 in the process scope means the
/// caller) mirrors `renice.c:138-154`.
fn donice(which: i32, who: i32, prio: i32, incr: bool) -> bool {
    let old = match getpriority(which, who) {
        Ok(v) => v,
        Err(_) => {
            support::warn(format!("renice: {who}: getpriority\n").as_bytes());
            return true;
        }
    };
    let new = clamp_prio(old, prio, incr);
    if setpriority(which, who, new).is_err() {
        support::warn(format!("renice: {who}: setpriority\n").as_bytes());
        return true;
    }
    support::emit(format!("{who}: old priority {old}, new priority {new}\n").as_bytes());
    false
}

fn run() -> ! {
    let argv: Vec<String> = support::args();
    let args: Vec<&str> = argv.iter().map(String::as_str).collect();

    let plan = match Plan::parse(&args[1.min(args.len())..]) {
        Ok(p) => p,
        Err(PlanError::Usage) => usage(),
        // C `getnum("priority", …)`'s `warnx("Bad %s argument: %s")`
        // (`renice.c:113-115`), then `return 1` from main.
        Err(PlanError::BadPriority(text)) => {
            support::warn(format!("renice: Bad priority argument: {text}\n").as_bytes());
            support::terminate(1);
        }
    };

    let mut errs = 0usize;
    for target in &plan.targets {
        match target {
            Target::Numeric(scope, who) => {
                errs += donice(scope.code(), *who, plan.prio, plan.incr) as usize
            }
            // C `renice.c:106-109`: `warnx("%s: bad value", *argv)` — the
            // token comes first, lowercase phrase.
            Target::Bad(s) => {
                support::warn(format!("renice: {s}: bad value\n").as_bytes());
                errs += 1;
            }
            Target::User(name) => {
                // C `renice.c:96-105`: `getpwnam` resolves the name; a NULL
                // answer (missing database, absent account) prints
                // `warnx("%s: unknown user")` and counts one error.
                let uid: Option<i32> = read_passwd_db()
                    .and_then(|db| passwd::lookup_in(&db, name).map(|a| a.uid as i32));
                match uid {
                    Some(who) => {
                        errs += donice(Scope::User.code(), who, plan.prio, plan.incr) as usize
                    }
                    None => {
                        support::warn(format!("renice: {name}: unknown user\n").as_bytes());
                        errs += 1;
                    }
                }
            }
        }
    }

    support::terminate(if errs == 0 { 0 } else { 1 });
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
