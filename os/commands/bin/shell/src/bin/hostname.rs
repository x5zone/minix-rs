//! Minix-RS `hostname` — the doing half over `minix_shell::hostname` plus
//! the two sysctl wrappers `minix_sys::gethostname` and
//! `minix_sys::sethostname`.
//!
//! Ground truth: `minix3/bin/hostname/hostname.c`. `getopt(argc, argv,
//! "s")` toggles the short flag; more than one operand is a usage error;
//! the operand arm calls `sethostname(*argv, strlen(*argv))`; the
//! no-operand arm calls `gethostname(hostname, sizeof hostname)`,
//! clamps `hostname[sizeof(hostname) - 1] = '\0'`, trims at the first
//! dot under `-s`, and prints the buffer with a newline (see
//! [`minix_shell::hostname`] for the exact line map).
//!
//! Wired: `hostname [-s] [name-of-host]`. Refused with the same usage
//! text (a single letter cluster like `-sx` is not accepted; C's
//! `case '?'` reaches `usage()` for it too). The set arm goes through
//! [`minix_sys::sethostname`] which the MIB service answers; success
//! prints nothing and exits 0 (C's `exit(0)` is the shared terminal
//! step, so a bare `hostname newname` after a successful write does NOT
//! read back — C doesn't either).

#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

extern crate alloc;

#[path = "../bin_support.rs"]
mod support;

use alloc::format;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use minix_shell::ShellError;
use minix_shell::hostname::{Plan, parse_plan, strip_domain};
use minix_sys::sysctl::MAX_HOSTNAME_LEN;
use minix_sys::{Errno, gethostname, sethostname};

/// C `hostname.c:60` — the caller's buffer is
/// `char hostname[MAXHOSTNAMELEN + 1]`; the extra byte is C's NUL-clamp
/// room (`hostname.c:85`), sitting outside the `MAXHOSTNAMELEN` sysctl
/// bound. This tree does not NUL-terminate on the caller side, so the
/// extra slot lets the caller pass a strictly-larger sink without a
/// partial read if the service ever answers the full
/// [`MAX_HOSTNAME_LEN`] plus a terminator.
const HOSTNAME_BUF_LEN: usize = MAX_HOSTNAME_LEN + 1;

/// C's `usage()` (`hostname.c:94-100`): the `%s` in `usage: %s [-s] [name-of-host]` is
/// `getprogname()`, so the program name is the same source as the `err(3)` line; exit 1.
fn usage(prog: &str) -> ! {
    support::warn(format!("usage: {prog} [-s] [name-of-host]\n").as_bytes());
    support::terminate(1);
}

/// Fold a transport-or-service errno into the C `err(1, "…")` shape: a
/// one-line stderr report prefixed by the operation, then exit 1. C's
/// `err(1, "sethostname")`/`err(1, "gethostname")` (`hostname.c:81`/`:84`): that line's
/// shape is in `minix3/lib/libc/gen/verrc.c` — `getprogname(): ` + the caller's string +
/// `": "` + the `strerror` sentence, then exit 1. The program name is taken from `argv[0]`
/// with the directory stripped ([`support::prog_name`]), and the sentence comes from this
/// tree's `sys_errlist` ([`support::err_exit`] forwards to `minix_sys::err_line`).
fn report_err(prog: &str, operation: &str, errno: Errno) -> ! {
    support::err_exit(prog, operation, errno, 1);
}

/// The whole program body; both `main` forms call it and it never returns.
fn run() -> ! {
    let argv: Vec<String> = support::args();
    let prog = support::prog_name(&argv);
    // C `argv += optind; argc -= optind` yields operands only; the pure
    // walker in [`parse_plan`] handles the option scan, so feed it the
    // tail after `argv[0]`.
    let args: Vec<&str> = argv.iter().map(String::as_str).collect();
    let operands = args.get(1..).unwrap_or(&[]);
    let plan = match parse_plan(operands) {
        Ok(p) => p,
        Err(ShellError::InvalidSyntax) => usage(prog),
        // No other shape can occur today (the walker only returns
        // InvalidSyntax); the extra arm is here so a future ShellError
        // variant forces the call site to reconsider rather than
        // silently fall through.
        #[allow(unreachable_patterns)]
        Err(_) => usage(prog),
    };

    match plan {
        Plan::Write { name } => {
            if let Err(e) = sethostname(name.as_bytes()) {
                report_err(prog, "sethostname", e);
            }
            support::terminate(0);
        }
        Plan::Read { short } => {
            let mut buffer = vec![0u8; HOSTNAME_BUF_LEN];
            let mut written = 0usize;
            if let Err(e) = gethostname(&mut buffer, &mut written) {
                report_err(prog, "gethostname", e);
            }
            // C clamps the last byte of the `MAXHOSTNAMELEN + 1` buffer
            // to `'\0'` before printing; the equivalent here is to
            // truncate at the first NUL in the answered slice, which
            // covers both the "service already NUL-terminated" and the
            // "caller's buffer is fully used" shapes.
            let answered = &buffer[..written.min(buffer.len())];
            let text = match answered.iter().position(|b| *b == 0) {
                Some(end) => &answered[..end],
                None => answered,
            };
            let shown = if short { strip_domain(text) } else { text };
            support::emit(shown);
            support::emit(b"\n");
            support::terminate(0);
        }
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
