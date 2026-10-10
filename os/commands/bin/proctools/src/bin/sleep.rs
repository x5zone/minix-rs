//! Minix-RS `sleep` — the doing half over `minix_proctools::sleep`.
//!
//! Ground truth: `minix3/bin/sleep/sleep.c`. The operand→duration decision
//! lives in the library (`plan`); this program parses the (option-free) command
//! line, and for a positive duration calls `nanosleep`.
//!
//! Wired: `sleep seconds` — a single operand (no options; `getopt(argc, argv,
//! "")` sends any leading `-` to usage, `sleep.c:81-90`). A non-positive integer
//! exits 0 without sleeping; a malformed/non-positive float operand exits 1 with
//! the usage line; otherwise the seconds+nanoseconds plan is handed to
//! `minix_sys::nanosleep` and any error is reported. Declared residual: C's
//! `SIGINFO` progress report (`report_request`) and the EINTR-remaining resume
//! loop (`sleep.c:126-136`) need signal-handler registration this build does not
//! wire, so the call is issued once.

#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

extern crate alloc;

#[path = "../bin_support.rs"]
mod support;

use alloc::string::String;
use alloc::vec::Vec;
use minix_proctools::sleep::{SleepPlan, plan};

fn usage() -> ! {
    support::warn(b"usage: sleep seconds\n");
    support::terminate(1);
}

/// The whole program body; both `main` forms call it and it never returns.
fn run() -> ! {
    let argv: Vec<String> = support::args();
    let args: Vec<&str> = argv.iter().map(String::as_str).collect();

    // getopt("") with no accepted letters: any leading option is a usage error.
    let i = 1;
    if i < args.len() && args[i].starts_with('-') && args[i].len() > 1 {
        usage();
    }
    if args.len() - i != 1 {
        usage();
    }

    match plan(args[i].as_bytes()) {
        SleepPlan::Bad => usage(),
        SleepPlan::NoSleep => support::terminate(0),
        SleepPlan::Sleep { secs, nanos } => match minix_sys::nanosleep(secs, nanos) {
            Ok(()) => support::terminate(0),
            Err(_) => {
                support::warn(b"sleep: nanosleep failed\n");
                support::terminate(1);
            }
        },
    }
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
