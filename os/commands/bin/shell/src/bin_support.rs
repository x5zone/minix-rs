//! Shared doing-half helpers for this crate's `src/bin/` programs.
//!
//! Included per binary through `#[path = "../bin_support.rs"]`; it never
//! enters the library target, keeping `minix_shell` free of any
//! system-call import. The hosted-versus-target split lives in the two
//! seams the echo template documents
//! (`os/commands/bin/fileops/src/bin/echo.rs`), carried here as
//! `cfg`-twin pairs so each binary swaps both in one sweep: argv via
//! `std::env::args` (hosted) or the `minix-rt` birth-chain descriptor
//! (`minix_rt::crt0::args`, raw initial-stack bytes rendered lossily — C
//! passes argv as raw bytes and never validates, so the lossy render is
//! this layer's text-tool contract), termination via the host runtime
//! (hosted) or `minix_sys::exit` (target). The environment walk follows
//! the same twin shape: `std::env::vars` (hosted) or the ps_strings
//! environment half (`minix_rt::crt0::envs`), each `KEY=VALUE` entry
//! split at its first `=`.

// Each binary includes this module and uses the subset it needs; the
// unused helpers in any one binary are intentional, not drift.
#![allow(dead_code)]

use minix_sys::Fd;

/// Standard input, POSIX `STDIN_FILENO`.
pub const STDIN: Fd = 0;
/// Standard output, POSIX `STDOUT_FILENO`.
pub const STDOUT: Fd = 1;
/// Standard error, POSIX `STDERR_FILENO`.
pub const STDERR: Fd = 2;

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

/// The process environment as `KEY=VALUE` strings, in order.
#[cfg(all(not(test), target_os = "none"))]
pub fn envs() -> alloc::vec::Vec<alloc::string::String> {
    minix_rt::crt0::envs()
        .map(|raw| alloc::string::String::from_utf8_lossy(raw).into_owned())
        .collect()
}

/// Hosted twin of [`envs`] (see the module header for the seam contract).
#[cfg(any(test, not(target_os = "none")))]
pub fn envs() -> alloc::vec::Vec<alloc::string::String> {
    std::env::vars().map(|(k, v)| format!("{k}={v}")).collect()
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
pub fn warn(bytes: &[u8]) {
    let _ = minix_sys::write(STDERR, bytes);
}

/// Writes the slice to standard output, exiting 1 on failure.
pub fn emit(bytes: &[u8]) {
    if minix_sys::write(STDOUT, bytes).is_err() {
        terminate(1);
    }
}

/// Reads a whole file's bytes (the script source face).
///
/// Hosted builds read the host filesystem (`std::fs`, no kernel in the
/// loop); the target build walks the same open/read/close ladder through
/// the `minix_sys` transport. The error is erased to the failure signal
/// the caller maps onto its own diagnostic.
#[cfg(any(test, not(target_os = "none")))]
pub fn read_file(path: &str) -> Result<alloc::vec::Vec<u8>, ()> {
    std::fs::read(path).map_err(|_| ())
}

#[cfg(all(not(test), target_os = "none"))]
pub fn read_file(path: &str) -> Result<alloc::vec::Vec<u8>, ()> {
    let fd = minix_sys::open(path, 0, 0).map_err(|_| ())?;
    let mut out = alloc::vec::Vec::new();
    let mut chunk = [0u8; 4096];
    loop {
        match minix_sys::read(fd, &mut chunk) {
            Ok(0) => break,
            Ok(count) => out.extend_from_slice(&chunk[..count]),
            Err(_) => {
                let _ = minix_sys::close(fd);
                return Err(());
            }
        }
    }
    let _ = minix_sys::close(fd);
    Ok(out)
}
