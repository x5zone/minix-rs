//! Shared doing-half helpers for this crate's `src/bin/` programs.
//!
//! Included per binary through `#[path = "../bin_support.rs"]`; it never
//! enters the library target, keeping `minix-fileops` free of any
//! system-call import. The hosted-versus-target seams are the same two the
//! echo template documents (`src/bin/echo.rs`): argv via `std::env::args`
//! and termination via the host runtime, both swapping in one sweep when
//! no_std program images land.

use minix_sys::Fd;

/// Standard error, POSIX `STDERR_FILENO`.
pub const STDERR: Fd = 2;
/// Standard output, POSIX `STDOUT_FILENO`.
pub const STDOUT: Fd = 1;

/// Best-effort write to standard error (the C `warnx` channel); write
/// failures are ignored exactly as stdio's are in the C utilities.
pub fn warn(bytes: &[u8]) {
    let _ = minix_sys::write(STDERR, bytes);
}

/// Writes the slice to standard output, reporting success without
/// exiting (the form the printf engine's sink needs).
pub fn write_ok(bytes: &[u8]) -> bool {
    minix_sys::write(STDOUT, bytes).is_ok()
}

/// Terminates the process with an exit status (see the module header).
pub fn terminate(code: i32) -> ! {
    std::process::exit(code)
}

/// Writes the slice to standard output, exiting 1 on failure.
pub fn emit(bytes: &[u8]) {
    if minix_sys::write(STDOUT, bytes).is_err() {
        terminate(1);
    }
}
