//! Shared doing-half helpers for this crate's `src/bin/` programs.
//!
//! Included per binary through `#[path = "../bin_support.rs"]`; it never
//! enters the library target, keeping `minix-fileops` free of any
//! system-call import. The hosted-versus-target seams are the same two the
//! echo template documents (`src/bin/echo.rs`): argv via `std::env::args`
//! and termination via the host runtime, both swapping in one sweep when
//! no_std program images land.

use minix_sys::Fd;

/// Standard output, POSIX `STDOUT_FILENO`.
pub const STDOUT: Fd = 1;

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
