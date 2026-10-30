//! Minix-RS `domainname` — the doing half over `minix_sys::getdomainname`
//! and `minix_sys::setdomainname`.
//!
//! Ground truth: `minix3/bin/domainname/domainname.c`. The `getopt(argc,
//! argv, "")` scan (line 65) admits no option letters — the switch only
//! has `case '?': default:`, so any dash-prefixed token that survives
//! `getopt` reaches `usage()`; `if (argc > 1) usage()` (line 71) refuses
//! more than one operand; the operand arm calls `setdomainname(*argv,
//! strlen(*argv))` (line 76-78); the no-operand arm calls
//! `getdomainname(domainname, sizeof domainname)` (line 80-81) and
//! prints the buffer with a newline (line 83). Unlike `hostname`,
//! there is no `-s` and no `MAXHOSTNAMELEN + 1` slack — the C buffer
//! is exactly `MAXHOSTNAMELEN` bytes (line 60), so we mirror that.
//!
//! The shape is the sibling of [`minix_shell::hostname`]'s: the pure
//! arg walk lives here (five lines) because domainname has no flag to
//! fold into a `Plan`, and the sysctl round trip rides the two
//! `{CTL_KERN, KERN_DOMAINNAME}` wrappers in `minix-sys`.

#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

extern crate alloc;

#[path = "../bin_support.rs"]
mod support;

use alloc::format;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use minix_sys::sysctl::MAX_HOSTNAME_LEN;
use minix_sys::{Errno, getdomainname, setdomainname};

/// C `domainname.c:60` — `char domainname[MAXHOSTNAMELEN]`, no slack.
const DOMAINNAME_BUF_LEN: usize = MAX_HOSTNAME_LEN;

/// C's `usage()` (`domainname.c:90-94`): the `%s` in `usage: %s [name-of-domain]`
/// is likewise `getprogname()`; `EXIT_FAILURE` is 1.
fn usage(prog: &str) -> ! {
    support::warn(format!("usage: {prog} [name-of-domain]\n").as_bytes());
    support::terminate(1);
}

/// C's `err(1, "getdomainname")`/`err(1, "setdomainname")` (`domainname.c:80`/
/// `:83`), the same convention as `hostname`: the sentence
/// `getprogname(): ` + the caller's string + `": "` + `strerror`, exit 1 (see
/// `minix3/lib/libc/gen/verrc.c` and [`support::err_exit`]).
fn report_err(prog: &str, operation: &str, errno: Errno) -> ! {
    support::err_exit(prog, operation, errno, 1);
}

/// The whole program body; both `main` forms call it and it never returns.
fn run() -> ! {
    let argv: Vec<String> = support::args();
    let prog = support::prog_name(&argv);
    let args: Vec<&str> = argv.iter().map(String::as_str).collect();
    // C's `getopt(argc, argv, "")` walks the argv tail looking for a
    // `-X` cluster; the empty optstring makes every option letter fall
    // through to `case '?'` → usage. Then `argc -= optind; argv +=
    // optind` leaves operands only, and `if (argc > 1) usage()`.
    let operands = args.get(1..).unwrap_or(&[]);
    for &a in operands {
        // Any surviving dash token (leading `--` included: C's getopt
        // stops scanning at `--` but here we've already stripped
        // `argv[0]`, so the token would be a name operand starting
        // with `-`). Refuse it as C would.
        if a.starts_with('-') && a != "-" {
            usage(prog);
        }
    }
    match operands {
        [] => {
            let mut buffer = vec![0u8; DOMAINNAME_BUF_LEN];
            let mut written = 0usize;
            if let Err(e) = getdomainname(&mut buffer, &mut written) {
                report_err(prog, "getdomainname", e);
            }
            // C prints `%s` of the buffer; the service's byte answer is
            // already NUL-terminated within `MAX_HOSTNAME_LEN` (the
            // VarStr write half we fixed in the hostname round appends
            // it verbatim), so truncate at the first NUL to keep parity
            // even if a caller writes an embedded terminator.
            let answered = &buffer[..written.min(buffer.len())];
            let text = match answered.iter().position(|b| *b == 0) {
                Some(end) => &answered[..end],
                None => answered,
            };
            support::emit(text);
            support::emit(b"\n");
            support::terminate(0);
        }
        [name] => {
            if let Err(e) = setdomainname(name.as_bytes()) {
                report_err(prog, "setdomainname", e);
            }
            support::terminate(0);
        }
        _ => usage(prog),
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
