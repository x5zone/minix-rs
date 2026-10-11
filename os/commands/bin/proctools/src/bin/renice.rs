//! Minix-RS `renice` — the doing half over `minix_proctools::renice`.
//!
//! Ground truth: `minix3/usr.bin/renice/renice.c`. The argument walk, base-0
//! number parsing, and priority clamping live in `minix_proctools::renice`;
//! this program supplies the syscall leg — for each resolved target read
//! `getpriority`, fold in the increment and clamp, `setpriority`, and print
//! `"<id>: old priority <n>, new priority <m>"` (`renice.c:136-156`), exiting
//! `0` only when every target succeeded (`errs == 0`, `renice.c:111`).
//!
//! Wired: `renice [-n] priority [[-p] pid ...] [[-g] pgrp ...]` — the process
//! and process-group scopes, both served by the wrapped `getpriority`/
//! `setpriority`. Unlike `nice`, `renice` acts on existing processes and never
//! execs, so its whole control path is reachable.
//!
//! Declared residual: the `-u user` scope resolves names with `getpwnam`, which
//! is not wrapped in this tree (the same blocker as `chown`/`chgrp`/`from`); a
//! `User` target is reported and counted as an error rather than silently
//! skipped. C's `warn`/`warnx` `strerror` suffixes are likewise not reproduced.

#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

extern crate alloc;

#[path = "../bin_support.rs"]
mod support;

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
use minix_proctools::renice::{Plan, PlanError, Target, clamp_prio};
use minix_sys::{getpriority, setpriority};

fn usage() -> ! {
    support::warn(
        b"usage: renice [ -n ] priority [[-p] pid ...] [[-g] pgrp ...] [[-u] user ...]\n",
    );
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
        Err(PlanError::BadPriority) => {
            support::warn(b"renice: Bad priority argument\n");
            support::terminate(1);
        }
    };

    let mut errs = 0usize;
    for target in &plan.targets {
        match target {
            Target::Numeric(scope, who) => {
                errs += donice(scope.code(), *who, plan.prio, plan.incr) as usize
            }
            Target::Bad(s) => {
                support::warn(format!("renice: Bad value argument: {s}\n").as_bytes());
                errs += 1;
            }
            Target::User(_) => {
                support::warn(b"renice: -u user lookup not wired (getpwnam)\n");
                errs += 1;
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
