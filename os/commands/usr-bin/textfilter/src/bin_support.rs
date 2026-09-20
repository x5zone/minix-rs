//! Shared doing-half helpers for this crate's `src/bin/` programs.
//!
//! Included per binary through `#[path = "../bin_support.rs"]`; it never
//! enters the library target, keeping `minix-textfilter` free of any
//! system-call import. The hosted-versus-target split lives in the two
//! seams the echo template documents (`os/commands/bin/fileops/src/bin/echo.rs`),
//! carried here as `cfg`-twin pairs so each binary swaps both in one sweep:
//! argv via `std::env::args` (hosted) or the `minix-rt` birth-chain
//! descriptor (`minix_rt::crt0::args`, raw initial-stack bytes rendered
//! lossily — C passes argv as raw bytes and never validates, so the lossy
//! render is this layer's text-tool contract), termination via the host
//! runtime (hosted) or `minix_sys::exit` (target). Reads and writes go
//! through `minix_sys::read`/`write` only; transport failures short-circuit
//! to a typed `Err` (edge E-SYSCALL-SIGN), so hosted runs observe honest
//! failures on both channels.

// Each binary includes this module and uses the subset it needs; the
// unused helpers in any one binary are intentional, not drift.
#![allow(dead_code)]

use alloc::vec::Vec;

use minix_sys::{read, write, Fd};

/// Standard input, POSIX `STDIN_FILENO`.
pub const STDIN: Fd = 0;
/// Standard error, POSIX `STDERR_FILENO`.
pub const STDERR: Fd = 2;
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

/// Writes the slice to standard output, reporting success without
/// exiting (the form the yes loop's sink needs).
pub fn write_ok(bytes: &[u8]) -> bool {
    write(STDOUT, bytes).is_ok()
}

/// Best-effort write to standard error (the C `warnx` channel); write
/// failures are ignored exactly as stdio's are in the C utilities.
pub fn warn(bytes: &[u8]) {
    let _ = write(STDERR, bytes);
}

/// Reads all of standard input into one vector.
///
/// The tr semantics are stream-shaped (squeeze runs can cross any read
/// boundary), so the doing half collects first and transforms once — the
/// same hosted-first trade the primes bound documents. A read failure
/// ends collection; the failure itself is not reported (the C filters
/// exit through their own error paths once the transport reports
/// honestly).
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
///
/// The counting totals are 64 bit (`Counter`), so the formatter is too.
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
/// every line and the final empty piece after a terminating newline (the
/// comparison shape C `uniq` and `cut` work in).
pub fn lines_of(input: &[u8]) -> Vec<&str> {
    if input.is_empty() {
        return Vec::new();
    }
    let text = core::str::from_utf8(input).unwrap_or("");
    let mut pieces: Vec<&str> = text.split('\n').collect();
    if input.ends_with(b"\n") {
        // The piece after the final newline is empty, not a line; interior
        // empty lines stay real lines.
        pieces.pop();
    }
    pieces
}

/// Reads a whole file's bytes (the `-f`/operand file face).
///
/// Hosted builds read the host filesystem (`std::fs`, no kernel in the
/// loop); the target build walks the same open/read/close ladder through
/// the `minix_sys` transport. The error is erased to the failure signal
/// every caller maps onto its own diagnostic.
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

/// Writes bytes over a whole file (the `unifdef` in-place shape).
///
/// Hosted builds write through `std::fs`; the target build opens with
/// `O_WRONLY` (0x1, `fcntl.h:65`) `|` `O_TRUNC` (0x400, `fcntl.h:100`)
/// and writes once — no create: the input was opened first, so the file
/// exists.
#[cfg(any(test, not(target_os = "none")))]
pub fn write_file(path: &str, bytes: &[u8]) -> Result<(), ()> {
    std::fs::write(path, bytes).map_err(|_| ())
}

#[cfg(all(not(test), target_os = "none"))]
pub fn write_file(path: &str, bytes: &[u8]) -> Result<(), ()> {
    let fd = minix_sys::open(path, 0x1 | 0x400, 0).map_err(|_| ())?;
    let done = minix_sys::write(fd, bytes);
    let _ = minix_sys::close(fd);
    done.map(|_| ()).map_err(|_| ())
}

/// Wall-clock microseconds since the epoch, when a clock face answers.
///
/// Hosted builds read the host clock; the target build reads the kernel
/// `SYS_TIMES` face — boot epoch seconds times one million plus real-time
/// ticks times ten thousand (tick rate `DEFAULT_HZ = 100`,
/// `os/kernel/src/clock.rs:685`). This serves the jobs C utilities cover
/// with `time(2)` and seed mixes: entropy and coarse stamps, not a
/// monotonic measurement.
#[cfg(any(test, not(target_os = "none")))]
pub fn epoch_micros() -> Option<u64> {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .ok()
        .map(|duration| duration.as_micros() as u64)
}

#[cfg(all(not(test), target_os = "none"))]
pub fn epoch_micros() -> Option<u64> {
    minix_sys::syscall::sys_times(
        &minix_sys::syscall::DirectKernelCallTransport,
        minix_sys::syscall::SELF,
    )
    .ok()
    .map(|times| times.boot_time * 1_000_000 + times.real_ticks * 10_000)
}

