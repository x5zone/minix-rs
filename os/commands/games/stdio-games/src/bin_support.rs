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

use minix_sys::{EIO, Errno, Fd, write};

/// Standard input, POSIX `STDIN_FILENO`.
pub const STDIN: Fd = 0;
/// Standard output, POSIX `STDOUT_FILENO`.
pub const STDOUT: Fd = 1;
/// Standard error, POSIX `STDERR_FILENO`.
pub const STDERR: Fd = 2;

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
/// Hosted twin of [`emit`]: real stdio (see [`warn`]).
#[cfg(any(test, not(target_os = "none")))]
pub fn emit(bytes: &[u8]) {
    use std::io::Write as _;
    if std::io::stdout().write_all(bytes).is_err() {
        terminate(1);
    }
}

#[cfg(all(not(test), target_os = "none"))]
pub fn emit(bytes: &[u8]) {
    if !write_chunk(STDOUT, bytes) {
        terminate(1);
    }
}

/// The same single write as [`emit`], but handing the failing number back to the
/// caller: C's `caesar` calls `err(EXIT_FAILURE, "<stdout>")` as soon as
/// `putchar` returns `EOF` (`caesar.c:103-107`, `:116-120`), and the suffix
/// carries that number — flattening it to "exit 1" would drop the only thing C
/// says. The hosted side takes `raw_os_error()` and falls back to `EIO` when it
/// is absent (the same rule as the twin seams `read_file` and `stdin_chunk`).
#[cfg(any(test, not(target_os = "none")))]
pub fn write_stdout(bytes: &[u8]) -> Result<(), Errno> {
    use std::io::Write as _;
    std::io::stdout()
        .write_all(bytes)
        .map_err(|e| Errno::from_i32(e.raw_os_error().unwrap_or(EIO)))
}

#[cfg(all(not(test), target_os = "none"))]
pub fn write_stdout(bytes: &[u8]) -> Result<(), Errno> {
    write(STDOUT, bytes).map(|_| ())
}

/// C's `fflush(stdout)`: `arithmetic.c:231-232` flushes once after printing a
/// prompt without a trailing newline, otherwise whoever reads it never sees the
/// question. The host's `std::io::stdout()` is line buffered, so without the
/// flush the prompt stays in the buffer and an interactive command hanging on a
/// pipe never returns (this is exactly what the end-to-end gate hangs on); on
/// the target every `emit` call is one `write(2)` with nothing to flush, hence
/// the no-op.
#[cfg(any(test, not(target_os = "none")))]
pub fn flush_stdout() {
    use std::io::Write as _;
    let _ = std::io::stdout().flush();
}

#[cfg(all(not(test), target_os = "none"))]
pub fn flush_stdout() {}

/// Reads all of standard input and tells "end of input" from "read failed":
/// C's `caesar` does not check while reading and looks at `ferror(stdin)` only
/// after all output (`caesar.c:205-208`), where it is `errx` rather than
/// `err` — because the stdio error flag carries no number. Returning the bytes
/// read so far together with that number is what lets the caller keep that
/// order and that family distinction. Both sides ride the existing
/// `stdin_chunk` seam, so neither needs rewriting.
pub fn read_stdin_all() -> (alloc::vec::Vec<u8>, Option<Errno>) {
    let mut input = alloc::vec::Vec::new();
    let mut chunk = [0u8; 4096];
    loop {
        match stdin_chunk(&mut chunk) {
            Ok(0) => return (input, None),
            Ok(count) => input.extend_from_slice(&chunk[..count]),
            Err(e) => return (input, Some(e)),
        }
    }
}

/// Best-effort write to standard error — the C `fprintf(stderr, ...)` channel.
///
/// Write failures are ignored exactly as stdio's are in the C games, which is why the
/// C callers exit immediately afterwards rather than checking (`morse.c:140-141`).
/// Hosted twin: the process' real stderr, which is what lets an integration test drive
/// the binary the way a shell would.
#[cfg(any(test, not(target_os = "none")))]
pub fn warn(bytes: &[u8]) {
    use std::io::Write as _;
    // The hosted channel is unchecked for the same reason C's is: a failed diagnostic
    // must not rewrite the exit status the C caller already chose.
    #[allow(clippy::let_underscore_must_use)]
    let _ = std::io::stderr().write_all(bytes);
}

#[cfg(all(not(test), target_os = "none"))]
pub fn warn(bytes: &[u8]) {
    let _ = write(STDERR, bytes);
}

/// Reads all of standard input.
///
/// C's games walk the stream a `getchar()` at a time; collecting first is the hosted
/// trade already documented for the primes bound and the text filters — the byte
/// sequence a run consumes is unchanged, only when it is consumed. A read failure ends
/// collection without being reported itself, because `getchar()` returns EOF there too.
#[cfg(any(test, not(target_os = "none")))]
pub fn read_stdin() -> alloc::vec::Vec<u8> {
    use std::io::Read as _;
    let mut input = alloc::vec::Vec::new();
    #[allow(clippy::let_underscore_must_use)]
    let _ = std::io::stdin().read_to_end(&mut input);
    input
}

#[cfg(all(not(test), target_os = "none"))]
pub fn read_stdin() -> alloc::vec::Vec<u8> {
    use minix_sys::read;
    let mut input = alloc::vec::Vec::new();
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

/// Reads one chunk of standard input into `buf`, returning the byte count.
///
/// This is the seam the line reader drives, and the hosted twin is what makes a
/// game's standard-input face testable at all: `minix_sys::read` on the host goes
/// through the kernel transport (a mock there) rather than the process' real standard
/// input — the same trap the text filters documented and the morse shell closed in the
/// previous batch. `Ok(0)` is end of input and `Err(errno)` a failed read: C's games
/// distinguish the two, because `factor` leaves with `err(1, "stdin")` and `ppt` with
/// `err(1, "fgets")` when the stream *errored* (`factor.c:157-161`, `ppt.c:110-111`) and
/// simply finish when it merely ended. That distinction needs the number the read left
/// behind, so the seam carries it on both twins — the hosted one takes it from the OS,
/// the freestanding one from `minix_sys::read`.
#[cfg(any(test, not(target_os = "none")))]
pub fn stdin_chunk(buf: &mut [u8]) -> Result<usize, Errno> {
    use std::io::Read as _;
    std::io::stdin().read(buf).map_err(|error| {
        // `raw_os_error()` is the errno the failed `read(2)` left; `None` means the error
        // came from somewhere else, and C has no such case here.
        Errno::from_i32(error.raw_os_error().unwrap_or(EIO))
    })
}

#[cfg(all(not(test), target_os = "none"))]
pub fn stdin_chunk(buf: &mut [u8]) -> Result<usize, Errno> {
    use minix_sys::read;
    read(STDIN, buf)
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

/// Renders `value` in **lowercase** hexadecimal into `out`, returning the written length.
///
/// The counterpart of `primes -h` (`printf("%" PRIx64, …)` at
/// `primes.c:255`/`:327`): C uses `x`, not `X`, and no `0x` prefix, so 17
/// prints `11` and 11 prints `b` — the same twin-seam shape as `utoa`, keeping
/// the formatting stack out of the deciding half.
pub fn utox(mut value: u32, out: &mut [u8]) -> usize {
    if value == 0 {
        out[0] = b'0';
        return 1;
    }
    let mut digits = [0u8; 8];
    let mut count = 0;
    while value > 0 {
        let digit = (value & 0xf) as u8;
        digits[count] = if digit < 10 {
            b'0' + digit
        } else {
            b'a' + digit - 10
        };
        value >>= 4;
        count += 1;
    }
    for (index, digit) in digits[..count].iter().rev().enumerate() {
        out[index] = *digit;
    }
    count
}

/// C's `warn(3)`/`err(3)` line: `prog: label: <sentence>` plus one newline,
/// written without exiting (`vwarnc.c` does not exit; the caller decides). Both
/// the shape and the sentence live in the `minix-types` authority (`err_line`),
/// alongside `perror_line`, which omits the program name. Only the arms that
/// need an errno suffix (`primes`' `err(1, "%s", argv[i])`) use it.
/// Rendering of C's `err(3)` line, with the authority next to the errno table:
/// forwards to [`minix_sys::err_line`] so each command family does not keep its
/// own copy of the shape. Only the bins' unit tests call through this re-export
/// (`clippy --workspace` builds without `--tests`, where it would look unused —
/// hence the `cfg_attr`).
#[cfg_attr(not(test), allow(unused_imports))]
pub use minix_sys::err_line;

/// C's `warn(3)`/`err(3)` line: `prog: label: <sentence>` plus one newline,
/// written without exiting (`vwarnc.c` does not exit; the caller decides). Both
/// the shape and the sentence live in the `minix-types` authority (`err_line`),
/// alongside `perror_line`, which omits the program name. Only the arms that
/// need an errno suffix (`primes`' `err(1, "%s", argv[i])`) use it.
pub fn warn_line(prog: &str, label: &str, code: minix_sys::Errno) {
    // C's `vwarnc` prints the caller's string with a direct
    // `vfprintf(stderr, fmt, ap)`, whose length is not capped by a one-line
    // stack buffer (`minix3/lib/libc/gen/vwarnc.c:62`; `perror.c:67` likewise
    // writes the caller's whole string, and only the sentence goes through
    // `strerror_r`'s fixed-size table). The authority returns "how many bytes
    // should be written", and that count holds even when the buffer does not
    // fit — so this measures once, then renders to that size, and an oversized
    // operand is never truncated.
    let mut sizing: [u8; 0] = [];
    let need = minix_sys::err_line(prog, label, code, &mut sizing);
    let mut line = alloc::vec![0u8; need];
    let len = minix_sys::err_line(prog, label, code, &mut line);
    warn(&line[..len.min(line.len())]);
    warn(b"\n");
}

/// C's `getprogname()`: the last path component of `argv[0]` (the rule is at
/// `minix3/lib/csu/common/crt0-common.c:156-165`, and the decision itself lives
/// only in that authority). The program names in `usage()` and in the
/// `err`/`warn` lines all come from here; this layer only splits `argv` into
/// that component and hands it over, the same shape as the identically named
/// entries in `fileops`/`textfilter`.
pub fn prog_name(argv: &[alloc::string::String]) -> &str {
    let raw = argv
        .first()
        .map(alloc::string::String::as_str)
        .unwrap_or("");
    minix_sys::prog_name(raw)
}

/// Line assembly over a caller-provided buffer and an injected reader.
///
/// The reader returns `Ok(0)` for end of input and `Err(errno)` for a failed
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

impl<'a, R: FnMut(&mut [u8]) -> Result<usize, Errno>> LineReader<'a, R> {
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

    /// Loads the next line, returning `false` at end of input and `Err(errno)`
    /// when the injected reader fails — the two are different outcomes, as the
    /// paragraph on [`stdin_chunk`] explains.
    ///
    /// The previous line is dropped here (lazy consumption), so the bytes
    /// behind [`LineReader::line`] stay untouched until this call.
    pub fn next_line(&mut self) -> Result<bool, Errno> {
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

    /// C's `getchar()`: one byte, not a line. The pending line from the last
    /// [`LineReader::next_line`] is dropped first — the same lazy consumption
    /// [`LineReader::next_line`] applies — so a caller that has just read `"ab\n"`
    /// gets the byte that follows it, exactly where C's stdio cursor sits.
    ///
    /// `None` is C's `EOF`. Here the two read outcomes are deliberately not separated:
    /// `arithmetic.c:177`'s `while(!getchar())` looks only at the returned byte — it checks
    /// neither `feof` nor `ferror` — so a zero byte loops on, while any other byte *and*
    /// both end-of-input and a failed read end the loop.
    pub fn next_byte(&mut self) -> Option<u8> {
        if self.line_len > 0 {
            let drop = self.line_len;
            self.line_len = 0;
            self.consume(drop);
        }
        loop {
            if self.filled > 0 {
                let byte = self.buf[0];
                self.consume(1);
                return Some(byte);
            }
            if self.source_done {
                return None;
            }
            match (self.reader)(&mut self.buf[self.filled..]) {
                Ok(0) | Err(_) => self.source_done = true,
                Ok(read) => self.filled += read,
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
    // Only `test_err_line_writes_c_shape_with_the_tables_sentence` needs these
    // two numbers; the production shell does not, so they are imported on the
    // test side.
    use minix_types::{EINVAL, EISDIR};

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
        assert_eq!(
            lines(b"ab\ncd\n", 16),
            vec![b"ab\n".to_vec(), b"cd\n".to_vec()]
        );
    }

    #[test]
    fn test_next_byte_takes_one_byte_after_the_pending_line() {
        // C's `getchar()` and `fgets()` share one stdio cursor: taking one byte
        // after reading a line yields the byte right after that line, not the
        // start of the next line.
        let mut source: &[u8] = b"ab\ncd\ne";
        let mut storage = [0u8; 16];
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
        assert!(reader.next_line().unwrap());
        assert_eq!(reader.line(), b"ab\n");
        assert_eq!(reader.next_byte(), Some(b'c'), "跳过刚交付的那一行");
        assert!(reader.next_line().unwrap());
        assert_eq!(reader.line(), b"d\n", "剩下的半个行仍按整行交付");
        assert_eq!(reader.next_byte(), Some(b'e'));
        assert_eq!(reader.next_byte(), None, "尽头就是 C 的 EOF");
    }

    #[test]
    fn test_final_fragment_without_newline_is_a_line() {
        assert_eq!(lines(b"ab\ncd", 16), vec![b"ab\n".to_vec(), b"cd".to_vec()]);
    }

    #[test]
    fn test_oversized_input_returns_full_buffer_lines() {
        // Capacity 4: "abcd" fills the buffer without a newline and is
        // returned as its own line, then "e\n" follows.
        assert_eq!(
            lines(b"abcde\n", 4),
            vec![b"abcd".to_vec(), b"e\n".to_vec()]
        );
    }

    #[test]
    fn test_empty_input_yields_no_lines() {
        assert!(lines(b"", 16).is_empty());
    }

    #[test]
    fn test_reader_failure_ends_stream_with_error() {
        let mut storage = [0u8; 16];
        let mut reader =
            LineReader::new(|_chunk: &mut [u8]| Err(Errno::from_i32(EIO)), &mut storage);
        assert!(reader.next_line().is_err());
    }

    #[test]
    fn test_err_line_writes_c_shape_with_the_tables_sentence() {
        // The shape of `verrc.c`: program name, colon space, the caller's
        // string, colon space, and the `strerror` sentence. The sentence comes
        // from this tree's table, so 21 is `Is a directory` and 22 is
        // `Invalid argument`.
        let mut out = [0u8; 96];
        let len = err_line("factor", "stdin", Errno::from_i32(EISDIR), &mut out);
        assert_eq!(&out[..len], b"factor: stdin: Is a directory");
        let len = err_line("ppt", "fgets", Errno::from_i32(EINVAL), &mut out);
        assert_eq!(&out[..len], b"ppt: fgets: Invalid argument");
        // A number outside the table takes C's `Unknown error: %u`, whose
        // value is read as `unsigned` (a negative wraps into a large number).
        let len = err_line("ppt", "fgets", Errno::from_i32(200), &mut out);
        assert_eq!(&out[..len], b"ppt: fgets: Unknown error: 200");
        let len = err_line("factor", "stdin", Errno::from_i32(-1), &mut out);
        assert_eq!(&out[..len], b"factor: stdin: Unknown error: 4294967295");
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
