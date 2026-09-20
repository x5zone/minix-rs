//! Shared doing-half helpers for this crate's `src/bin/` programs.
//!
//! Included per binary through `#[path = "../bin_support.rs"]`; it never
//! enters the library target, keeping `minix-fileops` free of any
//! system-call import. The hosted-versus-target split lives in the two
//! seams the echo template documents (`src/bin/echo.rs`), now carried
//! here as `cfg`-twin pairs so each binary swaps both in one sweep:
//!
//! - argv: hosted builds read `std::env::args` (invalid UTF-8 aborts the
//!   process, the std contract); freestanding builds read the birth-chain
//!   descriptor through `minix_rt::args` (raw initial-stack bytes, no
//!   allocator behind it yet — `minix-rt/src/crt0.rs` `args()`), rendered
//!   lossily into `String`s. C passes argv as raw bytes and never
//!   validates; the lossy render is this layer's text-tool contract, and
//!   the divergence is pinned here rather than spread over the binaries.
//! - termination: hosted builds exit through the host runtime, because
//!   `minix_sys::exit` deliberately spins when no process manager answers
//!   (the C `_exit` last resort, `minix3/minix/lib/libc/sys/_exit.c`),
//!   which would hang every hosted run; freestanding builds swap to
//!   `minix_sys::exit`.
//!
//! Writes go through `minix_sys::write` on both sides; transport failures
//! short-circuit to a typed `Err` (edge E-SYSCALL-SIGN), so hosted runs
//! observe honest failures on both channels.

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

/// Program arguments without `argv[0]` conventions applied — index 0 is
/// the program name, exactly as C's `argv`.
#[cfg(all(not(test), target_os = "none"))]
pub fn args() -> alloc::vec::Vec<alloc::string::String> {
    minix_rt::crt0::args()
        .map(|raw| alloc::string::String::from_utf8_lossy(raw).into_owned())
        .collect()
}

/// Hosted twin of [`args`] (see the module header for the seam contract).
#[cfg(any(test, not(target_os = "none")))]
pub fn args() -> alloc::vec::Vec<alloc::string::String> {
    std::env::args().collect()
}

/// Terminates the process with an exit status (see the module header).
#[cfg(all(not(test), target_os = "none"))]
pub fn terminate(code: i32) -> ! {
    minix_sys::exit(code)
}

/// Hosted twin of [`terminate`] (see the module header for the seam
/// contract).
#[cfg(any(test, not(target_os = "none")))]
pub fn terminate(code: i32) -> ! {
    std::process::exit(code)
}

/// Best-effort write to standard error (the C `warnx` channel); write
/// failures are ignored exactly as stdio's are in the C utilities.
/// Hosted twin: real stdio — the pre-freestanding `println!`/`eprintln!`
/// semantics these binaries carried before the seam landed (hosted runs
/// are host tools and test fixtures; the kernel transport is not on the
/// hosted output path).
#[cfg(any(test, not(target_os = "none")))]
pub fn warn(bytes: &[u8]) {
    use std::io::Write as _;
    let _ = std::io::stderr().write_all(bytes);
}

/// Hosted twin of [`emit`].
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

/// Writes the slice to standard output, reporting success without
/// exiting (the form the printf engine's sink needs).
/// Hosted twin of [`write_ok`]: real stdio.
#[cfg(any(test, not(target_os = "none")))]
pub fn write_ok(bytes: &[u8]) -> bool {
    use std::io::Write as _;
    std::io::stdout().write_all(bytes).is_ok()
}

#[cfg(all(not(test), target_os = "none"))]
pub fn write_ok(bytes: &[u8]) -> bool {
    minix_sys::write(STDOUT, bytes).is_ok()
}

/// Writes the slice to standard output, exiting 1 on failure.
#[cfg(all(not(test), target_os = "none"))]
pub fn emit(bytes: &[u8]) {
    if minix_sys::write(STDOUT, bytes).is_err() {
        terminate(1);
    }
}
