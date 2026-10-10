//! Shared doing-half helpers for this crate's `src/bin/` programs.
//!
//! Included per binary through `#[path = "../bin_support.rs"]`; it never
//! enters the library target, keeping `minix-diskimg` free of any
//! system-call import (pure deciding half). The hosted-versus-target split
//! is the same `cfg`-twin pair the `echo` template documents: argv reads
//! `std::env::args` on host and the birth-chain `minix_rt::crt0::args` on
//! target; termination exits through the host runtime (so a hosted run does
//! not hang on `minix_sys::exit`'s spin) versus `minix_sys::exit` on target.

// Each binary includes this module and uses the subset it needs; the
// unused helpers in any one binary are intentional, not drift.
#![allow(dead_code)]

use minix_sys::Fd;

/// Standard input, POSIX `STDIN_FILENO`.
pub const STDIN: Fd = 0;
/// Standard error, POSIX `STDERR_FILENO`.
pub const STDERR: Fd = 2;
/// Standard output, POSIX `STDOUT_FILENO`.
pub const STDOUT: Fd = 1;

/// Program arguments with `argv[0]` intact (index 0 is the program name).
#[cfg(all(not(test), target_os = "none"))]
pub fn args() -> alloc::vec::Vec<alloc::string::String> {
    minix_rt::crt0::args()
        .map(|raw| alloc::string::String::from_utf8_lossy(raw).into_owned())
        .collect()
}

/// Hosted twin of [`args`].
#[cfg(any(test, not(target_os = "none")))]
pub fn args() -> alloc::vec::Vec<alloc::string::String> {
    std::env::args().collect()
}

/// Terminates the process with an exit status.
#[cfg(all(not(test), target_os = "none"))]
pub fn terminate(code: i32) -> ! {
    minix_sys::exit(code)
}

/// Hosted twin of [`terminate`].
#[cfg(any(test, not(target_os = "none")))]
pub fn terminate(code: i32) -> ! {
    std::process::exit(code)
}

/// Best-effort write to standard error (the C `warnx` channel); write
/// failures are ignored, as stdio's are in the C utilities.
#[cfg(any(test, not(target_os = "none")))]
pub fn warn(bytes: &[u8]) {
    use std::io::Write as _;
    //[SD19-gate] SD-19: hosted warn stderr
    #[allow(clippy::let_underscore_must_use)]
    let _ = std::io::stderr().write_all(bytes);
}

/// Hosted twin of [`emit`]: real stdout.
#[cfg(any(test, not(target_os = "none")))]
pub fn emit(bytes: &[u8]) {
    use std::io::Write as _;
    if std::io::stdout().write_all(bytes).is_err() {
        terminate(1);
    }
}

#[cfg(all(not(test), target_os = "none"))]
pub fn warn(bytes: &[u8]) {
    let _ = minix_sys::write(STDERR, bytes);
}

/// Writes the slice to standard output, exiting 1 on failure.
#[cfg(all(not(test), target_os = "none"))]
pub fn emit(bytes: &[u8]) {
    if minix_sys::write(STDOUT, bytes).is_err() {
        terminate(1);
    }
}
