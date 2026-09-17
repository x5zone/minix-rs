//! Shared doing-half helpers for this crate's `src/bin/` programs.
//!
//! Included per binary through `#[path = "../bin_support.rs"]`; it never
//! enters the library target, keeping `minix-regex` free of any
//! system-call import. The hosted-versus-target seams are the same two the
//! echo template documents (`os/commands/bin/fileops/src/bin/echo.rs`):
//! argv via `std::env::args` and termination via the host runtime, both
//! swapping in one sweep when no_std program images land. Until the sign
//! mismatch in `perform_syscall` is fixed (edge E-SYSCALL-SIGN), hosted
//! runs observe fake successes on both channels.

use minix_sys::{read, write, Fd};

/// Standard input, POSIX `STDIN_FILENO`.
pub const STDIN: Fd = 0;
/// Standard output, POSIX `STDOUT_FILENO`.
pub const STDOUT: Fd = 1;

/// Terminates the process with an exit status (see the module header).
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
    let text = std::str::from_utf8(input).unwrap_or("");
    let mut pieces: Vec<&str> = text.split('\n').collect();
    if input.ends_with(b"\n") {
        pieces.pop();
    }
    pieces
}
