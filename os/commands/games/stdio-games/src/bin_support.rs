//! Shared doing-half helpers for this crate's `src/bin/` programs.
//!
//! Included per binary through `#[path = "../bin_support.rs"]`; it never
//! enters the library target, keeping `minix-stdio-games` free of any
//! system-call import. Two seams carry the hosted-versus-target split,
//! carried here as `cfg`-twin pairs so each binary swaps both in one sweep
//! (same decision as the echo template,
//! `os/commands/bin/fileops/src/bin/echo.rs`):
//!
//! - argv gathering uses `std::env::args` on hosted builds; the target
//!   build reads the birth-chain descriptor through `minix_rt::crt0::args`
//!   (raw initial-stack bytes rendered lossily into `String`s).
//! - `terminate` exits through the host runtime on hosted builds, because
//!   `minix_sys::exit` deliberately spins when no process manager answers
//!   (the C `_exit` last resort); the target build swaps it for
//!   `minix_sys::exit`.
//!
//! Writes and reads go through `minix_sys::write`/`read` only. Transport
//! failures short-circuit to a typed `Err` (edge E-SYSCALL-SIGN), so
//! hosted runs observe honest failures on both channels; on-target behavior
//! is unaffected.

// Each binary includes this module and uses the subset it needs; the
// unused helpers in any one binary are intentional, not drift.
#![allow(dead_code)]

use minix_sys::{write, Fd};

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

/// Writes the whole slice to the given descriptor as one call.
///
/// Games emit small fixed pieces (a punch line, a card row), where one
/// descriptor write is the C `putchar`-buffer shape. A failed write is
/// reported to the caller, which maps it to exit 1 (the games' uniform
/// failure exit); partial-write retries wait for the transport fix above.
pub fn write_chunk(fd: Fd, bytes: &[u8]) -> bool {
    write(fd, bytes).is_ok()
}

/// Writes the slice to standard output, exiting 1 on failure.
pub fn emit(bytes: &[u8]) {
    if !write_chunk(STDOUT, bytes) {
        terminate(1);
    }
}

/// Renders `value` in decimal into `out`, returning the written length.
///
/// Games print numbers (factors, primes, answers) without pulling a
/// formatting stack into the doing half.
pub fn utoa(mut value: u32, out: &mut [u8]) -> usize {
    if value == 0 {
        out[0] = b'0';
        return 1;
    }
    let mut digits = [0u8; 10];
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

/// Line assembly over a caller-provided buffer and an injected reader.
///
/// The reader returns `Ok(0)` for end of input and `Err(())` for a failed
/// read; both end the stream. A line is everything up to and including the
/// next `\n` (the C `fgets` shape the games are written against); the final
/// unterminated fragment counts as a line; a full buffer without `\n` is
/// returned as one line. The contents of `line()` stay valid until the next
/// [`LineReader::next_line`].
pub struct LineReader<'a, R> {
    reader: R,
    buf: &'a mut [u8],
    filled: usize,
    line_len: usize,
    source_done: bool,
}

impl<'a, R: FnMut(&mut [u8]) -> Result<usize, ()>> LineReader<'a, R> {
    /// Creates a reader over `buf` (its length is the line capacity, the
    /// C games' `fgets` buffer size).
    pub fn new(reader: R, buf: &'a mut [u8]) -> Self {
        LineReader {
            reader,
            buf,
            filled: 0,
            line_len: 0,
            source_done: false,
        }
    }

    /// Loads the next line, returning `false` at end of input and `Err(())`
    /// when the injected reader fails.
    ///
    /// The previous line is dropped here (lazy consumption), so the bytes
    /// behind [`LineReader::line`] stay untouched until this call.
    pub fn next_line(&mut self) -> Result<bool, ()> {
        self.consume(self.line_len);
        loop {
            if let Some(position) = self.buf[..self.filled].iter().position(|&b| b == b'\n') {
                self.line_len = position + 1;
                return Ok(true);
            }
            if self.filled == self.buf.len() {
                self.line_len = self.filled;
                return Ok(true);
            }
            if self.source_done {
                if self.filled == 0 {
                    return Ok(false);
                }
                self.line_len = self.filled;
                return Ok(true);
            }
            let read = (self.reader)(&mut self.buf[self.filled..])?;
            if read == 0 {
                self.source_done = true;
            } else {
                self.filled += read;
            }
        }
    }

    /// The line loaded by the last [`LineReader::next_line`] call.
    pub fn line(&self) -> &[u8] {
        &self.buf[..self.line_len]
    }

    /// Drops `count` consumed bytes by shifting the remainder forward.
    fn consume(&mut self, count: usize) {
        self.buf.copy_within(count..self.filled, 0);
        self.filled -= count;
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec::Vec;

    /// Runs the reader over canned input, returning every delivered line.
    fn lines(input: &[u8], capacity: usize) -> Vec<Vec<u8>> {
        let mut source = input;
        let mut storage = vec![0u8; capacity];
        let mut reader = LineReader::new(
            |chunk: &mut [u8]| {
                if source.is_empty() {
                    return Ok(0);
                }
                let take = source.len().min(chunk.len());
                chunk[..take].copy_from_slice(&source[..take]);
                source = &source[take..];
                Ok(take)
            },
            &mut storage,
        );
        let mut out = Vec::new();
        while reader.next_line().unwrap() {
            out.push(reader.line().to_vec());
        }
        out
    }

    #[test]
    fn test_lines_include_newline_and_split() {
        assert_eq!(lines(b"ab\ncd\n", 16), vec![b"ab\n".to_vec(), b"cd\n".to_vec()]);
    }

    #[test]
    fn test_final_fragment_without_newline_is_a_line() {
        assert_eq!(lines(b"ab\ncd", 16), vec![b"ab\n".to_vec(), b"cd".to_vec()]);
    }

    #[test]
    fn test_oversized_input_returns_full_buffer_lines() {
        // Capacity 4: "abcd" fills the buffer without a newline and is
        // returned as its own line, then "e\n" follows.
        assert_eq!(lines(b"abcde\n", 4), vec![b"abcd".to_vec(), b"e\n".to_vec()]);
    }

    #[test]
    fn test_empty_input_yields_no_lines() {
        assert!(lines(b"", 16).is_empty());
    }

    #[test]
    fn test_reader_failure_ends_stream_with_error() {
        let mut storage = [0u8; 16];
        let mut reader = LineReader::new(|_chunk: &mut [u8]| Err(()), &mut storage);
        assert!(reader.next_line().is_err());
    }

    #[test]
    fn test_utoa_shapes() {
        let mut out = [0u8; 12];
        let zero = utoa(0, &mut out);
        assert_eq!(&out[..zero], b"0");
        let five = utoa(12_345, &mut out);
        assert_eq!(&out[..five], b"12345");
    }
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

