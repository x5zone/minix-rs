//! Shared doing-half helpers for this crate's `src/bin/` programs.
//!
//! Included per binary through `#[path = "../bin_support.rs"]`; it never
//! enters the library target, keeping `minix-regex` free of any
//! system-call import. The hosted-versus-target split lives in the two
//! seams the echo template documents (`os/commands/bin/fileops/src/bin/echo.rs`),
//! carried here as `cfg`-twin pairs so each binary swaps both in one sweep:
//! argv via `std::env::args` (hosted) or the `minix-rt` birth-chain
//! descriptor (`minix_rt::crt0::args`, raw initial-stack bytes rendered
//! lossily), termination via the host runtime (hosted) or `minix_sys::exit`
//! (target). Transport failures short-circuit to a typed `Err` (edge
//! E-SYSCALL-SIGN), so hosted runs observe honest failures on both
//! channels.

// Each binary includes this module and uses the subset it needs; the
// unused helpers in any one binary are intentional, not drift.
#![allow(dead_code)]

use alloc::vec::Vec;
use minix_sys::{read, write, Fd};

/// Standard input, POSIX `STDIN_FILENO`.
pub const STDIN: Fd = 0;
/// Standard output, POSIX `STDOUT_FILENO`.
pub const STDOUT: Fd = 1;

/// Terminates the process with an exit status (see the module header).
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

/// Writes the slice to standard output, exiting 1 on failure.
pub fn emit(bytes: &[u8]) {
    if write(STDOUT, bytes).is_err() {
        terminate(1);
    }
}

/// Reads all of standard input into one vector.
///
/// Stream filters transform per line, so the doing half collects first and
/// iterates over the split; a read failure ends collection.
pub fn read_stdin() -> Vec<u8> {
    let mut input = Vec::new();
    let mut chunk = [0u8; 4096];
    loop {
        match read(STDIN, &mut chunk) {
            Ok(0) => break,
            Ok(count) => input.extend_from_slice(&chunk[..count]),
            Err(_) => break,
        }
    }
    input
}

/// Renders `value` in decimal into `out`, returning the written length.
pub fn utoa_u64(mut value: u64, out: &mut [u8]) -> usize {
    if value == 0 {
        out[0] = b'0';
        return 1;
    }
    let mut digits = [0u8; 20];
    let mut count = 0;
    while value > 0 {
        digits[count] = b'0' + (value % 10) as u8;
        value /= 10;
        count += 1;
    }
    for (index, digit) in digits[..count].iter().rev().enumerate() {
        out[index] = *digit;
    }
    count
}

/// Splits collected input into lines, dropping the trailing newline of
/// every line and the final empty piece after a terminating newline.
pub fn lines_of(input: &[u8]) -> Vec<&str> {
    if input.is_empty() {
        return Vec::new();
    }
    let text = core::str::from_utf8(input).unwrap_or("");
    let mut pieces: Vec<&str> = text.split('\n').collect();
    if input.ends_with(b"\n") {
        pieces.pop();
    }
    pieces
}
