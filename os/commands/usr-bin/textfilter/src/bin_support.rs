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
//! runtime (hosted) or `minix_sys::exit` (target). Reads and writes on the
//! target go through `minix_sys::read`/`write`; hosted runs go through
//! `std::io`/`std::fs` so a developer (and the `tests/` face this crate keeps)
//! can drive a binary end to end and compare it with the C expectations written
//! into each golden case. Transport failures on the target short-circuit to a
//! typed `Err`.

// Each binary includes this module and uses the subset it needs; the
// unused helpers in any one binary are intentional, not drift.
#![allow(dead_code)]

use alloc::vec::Vec;

use minix_sys::Fd;
#[cfg(all(not(test), target_os = "none"))]
use minix_sys::{read, write};

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
/// `std::env::args()` panics on a non-UTF-8 argument, which would make this leg
/// diverge from the target leg (which renders lossily); `args_os()` keeps the raw
/// bytes so both legs apply the same lossy render — C passes argv as bytes and
/// never validates them.
#[cfg(any(test, not(target_os = "none")))]
pub fn args() -> alloc::vec::Vec<alloc::string::String> {
    use std::os::unix::ffi::OsStrExt as _;
    std::env::args_os()
        .map(|raw| alloc::string::String::from_utf8_lossy(raw.as_bytes()).into_owned())
        .collect()
}

/// The byte-shaped exit: C treats `argv` as a byte array from end to end, and
/// `getopt`'s letters come from byte stepping
/// (`minix3/lib/libc/stdlib/getopt.c:optopt = *place++`), so any leg that must hand
/// back an option letter or an operand **as raw bytes** cannot go through [`args`] —
/// its `String::from_utf8_lossy` folds a bare `ff` into the three bytes `ef bf bd`.
#[cfg(all(not(test), target_os = "none"))]
pub fn args_bytes() -> alloc::vec::Vec<alloc::vec::Vec<u8>> {
    minix_rt::crt0::args().map(<[u8]>::to_vec).collect()
}

/// Hosted twin of [`args_bytes`]: `args_os()` hands back exactly the same raw bytes.
#[cfg(any(test, not(target_os = "none")))]
pub fn args_bytes() -> alloc::vec::Vec<alloc::vec::Vec<u8>> {
    use std::os::unix::ffi::OsStrExt as _;
    std::env::args_os()
        .map(|raw| raw.as_bytes().to_vec())
        .collect()
}

/// The byte form of [`prog_name`]: `argv[0]` comes back as raw bytes, cut to the
/// segment **after the last slash** — the same basename rule [`prog_name`] delegates
/// to `minix_sys::prog_name` (a message's program name must not carry the caller's
/// path prefix).
pub fn prog_name_bytes(argv: &[alloc::vec::Vec<u8>]) -> &[u8] {
    let raw = argv.first().map_or(&[][..], |name| name.as_slice());
    match raw.iter().rposition(|&byte| byte == b'/') {
        Some(cut) => &raw[cut + 1..],
        None => raw,
    }
}

/// Terminates the process with an exit status (see the module header).
/// Hosted twin: real stdio — the pre-freestanding `println!`/`eprintln!`
/// semantics (hosted runs are host tools and test fixtures; the kernel
/// transport is not on the hosted output path).
#[cfg(any(test, not(target_os = "none")))]
pub fn warn(bytes: &[u8]) {
    use std::io::Write as _;
    //[SD19-gate] the hosted warn channel is isomorphic to C's unchecked
    // `fputs(msg, stderr)`: `write_all` can return EPIPE, and the main path chose
    // "print the diagnostic" over "report a failed diagnostic", so the failure does
    // not roll back the main semantics. This matches the non-returning-semantics
    // handling of the same family in `os/libs/minix-sys/src/lib.rs`.
    #[allow(clippy::let_underscore_must_use)]
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
#[cfg(all(not(test), target_os = "none"))]
pub fn emit(bytes: &[u8]) {
    if write(STDOUT, bytes).is_err() {
        terminate(1);
    }
}

/// Writes the slice to standard output and hands back the error that stopped it,
/// if any — the form a program needs when it reports a failed write the way C's
/// `check()` does (`minix3/usr.bin/colrm/colrm.c:123-130` names the *stream*,
/// `stdin` or `stdout`, and exits 1). Whatever the device took stays taken;
/// only the stopping error comes back.
#[cfg(any(test, not(target_os = "none")))]
pub fn emit_error(bytes: &[u8]) -> Option<minix_sys::Errno> {
    use std::io::Write as _;
    match std::io::stdout().write_all(bytes) {
        Ok(()) => None,
        Err(failure) => Some(errno_of(&failure)),
    }
}

/// Target leg of [`emit_error`]. A single `write` may take fewer bytes than asked
/// for, so the remainder is pushed until the device either takes it all or stops
/// taking it; a write that transfers nothing without failing is reported as `EIO`,
/// the same fallback [`errno_of`] uses for a hosted error carrying no raw code.
#[cfg(all(not(test), target_os = "none"))]
pub fn emit_error(bytes: &[u8]) -> Option<minix_sys::Errno> {
    let mut left = bytes;
    while !left.is_empty() {
        match write(STDOUT, left) {
            Ok(0) => return Some(minix_sys::Errno::EIO),
            Ok(count) => left = &left[count..],
            Err(code) => return Some(code),
        }
    }
    None
}

/// Writes the slice to standard output, reporting success without
/// exiting (the form the yes loop's sink needs).
#[cfg(any(test, not(target_os = "none")))]
pub fn write_ok(bytes: &[u8]) -> bool {
    use std::io::Write as _;
    std::io::stdout().write_all(bytes).is_ok()
}

#[cfg(all(not(test), target_os = "none"))]
pub fn write_ok(bytes: &[u8]) -> bool {
    write(STDOUT, bytes).is_ok()
}

/// Best-effort write to standard error (the C `warnx` channel); write
/// failures are ignored exactly as stdio's are in the C utilities.
#[cfg(all(not(test), target_os = "none"))]
pub fn warn(bytes: &[u8]) {
    let _ = write(STDERR, bytes);
}

/// Reads all of standard input into one vector, **discarding** any read failure.
///
/// The tr semantics are stream-shaped (squeeze runs can cross any read
/// boundary), so the doing half collects first and transforms once — the
/// same hosted-first trade the primes bound documents. Hosted runs read the
/// process' real standard input, which is what lets an integration test drive
/// the binary the way a shell would. A caller that has to report a failed read
/// the way C does takes [`read_stdin_error`] instead, which hands back the bytes
/// and the error together.
#[cfg(any(test, not(target_os = "none")))]
pub fn read_stdin() -> Vec<u8> {
    read_stdin_error().0
}

/// Reads all of standard input and returns what arrived together with the
/// error that stopped it, if the read stopped for a reason other than end of
/// file.
///
/// C's line readers do not treat a failed read as the end of the input: after
/// the loop they test the stream and report which stream failed — `rev.c:101-104`
/// runs `if (ferror(fp)) warn("%s", filename)` against a `filename` that holds
/// `"stdin"` until an operand takes it away. So the shape is
/// one `warn` line naming the stream plus a nonzero exit status, and whatever
/// was read before the failure has already been processed — which is why the
/// bytes collected so far come back alongside the number instead of being
/// dropped with it.
#[cfg(any(test, not(target_os = "none")))]
pub fn read_stdin_error() -> (Vec<u8>, Option<minix_sys::Errno>) {
    use std::io::Read as _;
    let mut input = Vec::new();
    match std::io::stdin().read_to_end(&mut input) {
        Ok(_) => (input, None),
        Err(failure) => (input, Some(errno_of(&failure))),
    }
}

#[cfg(all(not(test), target_os = "none"))]
pub fn read_stdin() -> Vec<u8> {
    read_stdin_error().0
}

#[cfg(all(not(test), target_os = "none"))]
pub fn read_stdin_error() -> (Vec<u8>, Option<minix_sys::Errno>) {
    let mut input = Vec::new();
    let mut chunk = [0u8; 4096];
    loop {
        match read(STDIN, &mut chunk) {
            Ok(0) => return (input, None),
            Ok(count) => input.extend_from_slice(&chunk[..count]),
            Err(code) => return (input, Some(code)),
        }
    }
}

/// Reads one line from standard input the way C's `fgets(buf, limit + 1, stdin)` does:
/// it stops **after consuming** a newline (which is not returned), or as soon as
/// `limit` bytes are collected — in that case the next byte stays in the stream and
/// belongs to the following call. `None` means an immediate EOF with nothing
/// buffered, which is what lets a prompting program tell "no more input" from
/// "empty line" (the latter is `Some` of an empty vector, and the program still
/// processes it).
///
/// The hosted leg reads byte-wise from the real process standard input so an
/// integration test can drive a prompting binary; the target leg goes through
/// `minix_sys::read` on the same descriptor. Blocking per line, not a slurp:
/// a program that prompts and then reads must not wait for EOF first.
#[cfg(any(test, not(target_os = "none")))]
pub fn read_line_bounded(limit: usize) -> Option<Vec<u8>> {
    use std::io::Read as _;
    let mut stdin = std::io::stdin();
    let mut line = Vec::new();
    let mut byte = [0u8; 1];
    while line.len() < limit {
        match stdin.read(&mut byte) {
            Ok(1) if byte[0] == b'\n' => return Some(line),
            Ok(1) => line.push(byte[0]),
            _ => break,
        }
    }
    if line.is_empty() { None } else { Some(line) }
}

#[cfg(all(not(test), target_os = "none"))]
pub fn read_line_bounded(limit: usize) -> Option<Vec<u8>> {
    let mut line = Vec::new();
    let mut byte = [0u8; 1];
    while line.len() < limit {
        match read(STDIN, &mut byte) {
            Ok(1) if byte[0] == b'\n' => return Some(line),
            Ok(1) => line.push(byte[0]),
            _ => break,
        }
    }
    if line.is_empty() { None } else { Some(line) }
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
/// The line leg lives on the library side: every line returned by
/// `minix_textfilter::window::lines_of` carries whether the input had that newline,
/// and the shells take it only from this one entry point rather than restating the
/// last-line rule in each of the five shells.
pub fn lines_of(input: &[u8]) -> Vec<minix_textfilter::window::Line<'_>> {
    minix_textfilter::window::lines_of(input)
}

/// Reads a whole file's bytes (the `-f`/operand file face).
///
/// Hosted builds read the host filesystem (`std::fs`, no kernel in the
/// loop); the target build walks the same open/read/close ladder through
/// the `minix_sys` transport.
///
/// The error slot carries **that number** (not a unit): these C commands' error legs
/// are `err(1, "%s", *argv)` (e.g. `join.c:244`, `:249`), which prints it; erasing it
/// to a unit leaves only "something failed". The hosted side takes the number from
/// `raw_os_error()`, falling back to `EIO` when absent (the same rule as the
/// `stdio-games` read seam's twin faces).
#[cfg(any(test, not(target_os = "none")))]
pub fn read_file(path: &str) -> Result<alloc::vec::Vec<u8>, minix_sys::Errno> {
    std::fs::read(path).map_err(|e| {
        minix_sys::Errno::from_i32(e.raw_os_error().unwrap_or(minix_sys::Errno::EIO.to_i32()))
    })
}

#[cfg(all(not(test), target_os = "none"))]
pub fn read_file(path: &str) -> Result<alloc::vec::Vec<u8>, minix_sys::Errno> {
    let fd = minix_sys::open(path, 0, 0)?;
    let mut out = alloc::vec::Vec::new();
    let mut chunk = [0u8; 4096];
    loop {
        match minix_sys::read(fd, &mut chunk) {
            Ok(0) => break,
            Ok(count) => out.extend_from_slice(&chunk[..count]),
            Err(e) => {
                let _ = minix_sys::close(fd);
                return Err(e);
            }
        }
    }
    let _ = minix_sys::close(fd);
    Ok(out)
}

/// C's `time(2)`: a wall clock with second granularity. `cksum`'s `-t` leg uses it to
/// compute the `Time = %ld seconds` and `Speed = %lld bytes/second` lines
/// (`minix3/usr.bin/cksum/md5.c:93`, `:107-113`). The hosted side uses `std::time`,
/// and the target side the second slot of `minix_sys::gettimeofday` — the same rule as
/// the `read_file` cfg twins: the hosted door must run, and the target door must be
/// able to obtain a number. When no number is available it returns 0 rather than `-1`:
/// C's `time()` returns `(time_t)-1` on failure and `cksum.c` never checks it, so this
/// leg prints diagnostic numbers and does not warrant an error leg of its own; the
/// only difference between `0` and `-1` is the reading on failure.
#[cfg(any(test, not(target_os = "none")))]
pub fn wall_seconds() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_secs() as i64)
}

#[cfg(all(not(test), target_os = "none"))]
pub fn wall_seconds() -> i64 {
    minix_sys::gettimeofday().map_or(0, |(seconds, _)| seconds)
}

/// C's `getprogname()`: the last segment of `argv[0]` with the directory removed (the
/// rule is at `minix3/lib/csu/common/crt0-common.c:156-165`, authoritative in
/// `minix-types`' `prog_name`). The `err(3)`/`warn(3)`/`usage()` lines all start with
/// it.
pub fn prog_name(argv: &[alloc::string::String]) -> &str {
    let raw = argv
        .first()
        .map(alloc::string::String::as_str)
        .unwrap_or("");
    minix_sys::prog_name(raw)
}

/// The C `warn(3)`/`err(3)` line: `prog: label: <sentence>` plus a newline; it does
/// not exit after writing (`vwarnc.c` does not exit; the caller decides). The shape
/// and the sentence live in `minix-types`' authoritative `err_line` (this layer gets
/// it through `minix_sys::err_line`, alongside `perror_line`, which adds no program
/// name).
pub fn warn_line(prog: &str, label: &str, code: minix_sys::Errno) {
    // C's `vwarnc` prints the caller's string directly with
    // `vfprintf(stderr, fmt, ap)`, so its length is not bounded by a one-line stack
    // buffer (`minix3/lib/libc/gen/vwarnc.c:62`; `perror.c:67` likewise prints the
    // caller's string whole, and only the sentence goes through `strerror_r`'s
    // fixed-size table). The authoritative call returns "how many bytes should have
    // been written", counting even when the buffer does not fit, so this measures once
    // and then renders by that number — an over-long operand is never truncated.
    let mut sizing: [u8; 0] = [];
    let need = minix_sys::err_line(prog, label, code, &mut sizing);
    let mut line = alloc::vec![0u8; need];
    let len = minix_sys::err_line(prog, label, code, &mut line);
    warn(&line[..len.min(line.len())]);
    warn(b"\n");
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

/// Takes the number of a hosted `io::Error` as a C `errno`. These C commands' open legs
/// print exactly that number (`err(1, "%s", name)`/`warn("%s", name)` go through
/// `vwarn`, not `warnx`); without it only "something failed" remains. When
/// `raw_os_error()` is absent it falls back to `EIO` (the same rule as `read_file` and
/// the `stdio-games` read seam).
#[cfg(any(test, not(target_os = "none")))]
pub fn errno_of(error: &std::io::Error) -> minix_sys::Errno {
    minix_sys::Errno::from_i32(
        error
            .raw_os_error()
            .unwrap_or(minix_sys::Errno::EIO.to_i32()),
    )
}

/// A create-or-truncate write sink — C's `fopen(name, "w")`, which truncates the
/// file at open time, before any byte of the input has been read. Keeping the
/// handle open (instead of writing once at the end) is what makes that ordering
/// observable: an input that turns out to be unreadable still leaves the output
/// empty, exactly as the C's already-opened `FILE *` would at exit.
#[cfg(any(test, not(target_os = "none")))]
pub struct WriteSink(pub(crate) std::fs::File);

#[cfg(all(not(test), target_os = "none"))]
pub struct WriteSink {
    fd: minix_sys::Fd,
}

/// Opens an output operand, creating it if absent and truncating it if present.
/// `O_WRONLY` is `0x1` (`minix3/sys/sys/fcntl.h:65`), `O_CREAT` is `0x200` (`:99`)
/// and `O_TRUNC` is `0x400` (`:100`); the mode `0666` is what `fopen` hands to
/// `open(2)`, so the process umask still applies.
///
/// The error leg carries that number (not a unit): `uniq`'s output leg is
/// `file(argv[1], "w")` → `err(1, "%s", name)` (`uniq.c:216-221`), which is exactly
/// what gets reported.
#[cfg(any(test, not(target_os = "none")))]
pub fn create_sink(path: &str) -> Result<WriteSink, minix_sys::Errno> {
    std::fs::File::create(path)
        .map(WriteSink)
        .map_err(|e| errno_of(&e))
}

#[cfg(all(not(test), target_os = "none"))]
pub fn create_sink(path: &str) -> Result<WriteSink, minix_sys::Errno> {
    let fd = minix_sys::open(path, 0x1 | 0x200 | 0x400, 0o666)?;
    Ok(WriteSink { fd })
}

#[cfg(any(test, not(target_os = "none")))]
impl WriteSink {
    /// The handle is released on drop, matching C's stdio flushing at exit without
    /// reporting.
    ///
    /// The error leg hands back **the platform number**: `split.c:204`, `:216` and
    /// `:225` all do `err(1, "write")` when `bigwrite()` comes up short, and `err`
    /// appends exactly `errno` — "disk full" and "a real I/O error" are two different
    /// sentences, and flattening them into one `EIO` leaves only "something failed".
    /// Writes another chunk into the same already-open output file: after `split -b`
    /// became streaming, each read's bytes are written to whichever part of the chunk
    /// belongs to the current file (C's `bigwrite(ofd, Cs, bcnt)` has this shape).
    pub fn append(&mut self, bytes: &[u8]) -> Result<(), SinkFailure> {
        use std::io::Write as _;
        self.0
            .write_all(bytes)
            .map_err(|failure| SinkFailure::Write(errno_of(&failure)))
    }

    /// Finishes after the last write. The hosted leg cannot see a `close` failure
    /// (`std::fs::File` is closed by drop and returns no number), the same recorded
    /// difference as `finish`.
    pub fn seal(self) -> Result<(), SinkFailure> {
        Ok(())
    }

    pub fn finish(mut self, bytes: &[u8]) -> Result<(), SinkFailure> {
        use std::io::Write as _;
        // The hosted leg cannot see a `close` failure: `std::fs::File` is closed by
        // drop and returns no number, while C does `close(ofd)` and
        // `err(1, "%s", fname)` before opening the next chunk
        // (`minix3/usr.bin/split/split.c:319-322`). The target leg reports through
        // `close_outcome`.
        self.0
            .write_all(bytes)
            .map_err(|failure| SinkFailure::Write(errno_of(&failure)))
    }
}

#[cfg(all(not(test), target_os = "none"))]
impl WriteSink {
    /// Keeps writing until the piece is out; only a `write` returning 0 counts as real
    /// lack of progress — this is the loop shape of C's `bigwrite()`
    /// (`minix3/usr.bin/split/split.c:337-351`). The error leg carries the platform
    /// number, because the caller wants exactly the `err(1, "write")` sentence. The
    /// descriptor is released before returning; a `close` failure is not reported at
    /// this layer (a recorded difference).
    /// Writes another chunk into the same already-open output file, following the
    /// shape of C's `bigwrite()`.
    pub fn append(&mut self, bytes: &[u8]) -> Result<(), SinkFailure> {
        write_loop(self.fd, bytes).map_err(SinkFailure::Write)
    }

    /// Finishes and returns the `close` number: in `newfile()` C does
    /// `err(1, "%s", fname)` when `close(ofd) != 0` (`split.c:319-322`), with `fname`
    /// still naming the chunk just written.
    pub fn seal(self) -> Result<(), SinkFailure> {
        minix_sys::close(self.fd)
            .map(|_| ())
            .map_err(SinkFailure::Close)
    }

    pub fn finish(self, bytes: &[u8]) -> Result<(), SinkFailure> {
        let write = write_loop(self.fd, bytes).map_err(SinkFailure::Write);
        let close = minix_sys::close(self.fd)
            .map(|_| ())
            .map_err(SinkFailure::Close);
        close_outcome(write, close)
    }
}

/// The two failures of one output file: the write does not go through, and closing
/// after the write fails. The distinct labels are C's — `bigwrite()` coming up short is
/// `err(1, "write")` (`minix3/usr.bin/split/split.c:204`, `:216`, `:225`), and
/// `close(ofd) != 0` in `newfile()` is `err(1, "%s", fname)` (`:319-322`, where the
/// suffix has not advanced yet, so `fname` is still the **previous** chunk's name).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SinkFailure {
    Write(minix_sys::Errno),
    Close(minix_sys::Errno),
}

/// The choice between the two legs is written as an explicit table because a door has
/// to be able to pin it: the host cannot see a `close` failure, the target has both.
/// The rule is "write failure wins" — C `err`s out on the spot where `bigwrite` comes
/// up short, so it never reaches the next `close`.
pub fn close_outcome(
    write: Result<(), SinkFailure>,
    close: Result<(), SinkFailure>,
) -> Result<(), SinkFailure> {
    match (write, close) {
        (Err(failure), _) => Err(failure),
        (Ok(()), Err(failure)) => Err(failure),
        (Ok(()), Ok(())) => Ok(()),
    }
}

/// Writes `bytes` to the end: `Ok(0)` means the descriptor takes no more bytes, and
/// with no platform number to return the result falls back to `EIO`.
fn write_loop(fd: minix_sys::Fd, mut left: &[u8]) -> Result<(), minix_sys::Errno> {
    while !left.is_empty() {
        match minix_sys::write(fd, left) {
            Ok(0) => return Err(minix_sys::Errno::EIO),
            Ok(count) => left = &left[count..],
            Err(code) => return Err(code),
        }
    }
    Ok(())
}

/// The two failure legs of one input operand. C prints them in two places: `fopen`
/// failing inside `file(argv[0], "r")` is `err(1, "%s", *argv)`
/// (`minix3/usr.bin/split/split.c:136-138`), and `read` failing in the read loop is
/// `err(1, "read")` (`:185-190`, `:252-257`) — one operand, two different sentences.
/// Merging them conflates "the file does not exist" with "it opens but cannot be
/// read", and the latter is exactly testable on a host: a directory opens fine and
/// fails to read (`EISDIR`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum InputFailure {
    Open(minix_sys::Errno),
    Read(minix_sys::Errno),
}

/// Reads one operand: returns the **bytes that arrived** together with **the arm that
/// stopped it** (if any).
///
/// The arrived bytes come back too because C reads, splits and writes as it goes — by
/// the time `read` fails the earlier chunks are already on disk, so the caller finishes
/// those chunks, then adds the `err(1, "read")` line and exits 1.
#[cfg(any(test, not(target_os = "none")))]
pub fn read_operand(path: &str) -> (Vec<u8>, Option<InputFailure>) {
    use std::io::Read as _;
    let mut file = match std::fs::File::open(path) {
        Ok(handle) => handle,
        Err(failure) => {
            return (Vec::new(), Some(InputFailure::Open(errno_of(&failure))));
        }
    };
    let mut input = Vec::new();
    match file.read_to_end(&mut input) {
        Ok(_) => (input, None),
        Err(failure) => (input, Some(InputFailure::Read(errno_of(&failure)))),
    }
}

/// The byte count the input carries: the `-n` leg asks the kernel for it before reading.
///
/// C's shape is `fstat(ifd, &sb)`, with `err(1, "stat")` on failure, after which
/// `sb.st_size` both judges the chunk-count limit and computes the chunk width
/// (`minix3/usr.bin/split/split.c:293-300`). A pipe or character device has `st_size`
/// 0, so "split into 3 pieces" fails right there — not because data cannot be read but
/// because the **length cannot be asked for**. When `path` is `None` or `"-"`,
/// standard input is queried.
///
/// The hosted leg asks by path (standard input goes through `/dev/stdin`, which yields
/// the same number as C's `fstat(0)`: a named file gives its length, pipes and null
/// devices give 0), because std gives a non-root process no `unsafe`-free channel to
/// take status of a descriptor. The target leg is `fstat`/`stat` itself. One fact, two
/// carriers.
#[cfg(any(test, not(target_os = "none")))]
pub fn input_size(path: Option<&str>) -> Result<u64, minix_sys::Errno> {
    std::fs::metadata(path.unwrap_or("/dev/stdin"))
        .map(|status| status.len())
        .map_err(|failure| errno_of(&failure))
}

/// The target leg: `fstat(0)` for standard input and `stat(path)` for a named file —
/// what C does at that point (`split.c:293` takes the descriptor already opened, and a
/// named file's descriptor is the one opened at `split.c:137`).
#[cfg(all(not(test), target_os = "none"))]
pub fn input_size(path: Option<&str>) -> Result<u64, minix_sys::Errno> {
    // `Stat`'s alignment padding is private, so outside the crate the value can only
    // start from the repository's existing zeroed-value shape (as in
    // `commands/bin/fileops/src/bin/test.rs:71`).
    let mut status: minix_sys::Stat = unsafe { core::mem::zeroed() };
    let outcome = match path {
        None | Some("-") => minix_sys::fstat(STDIN, &mut status),
        Some(name) => minix_sys::stat(name, &mut status),
    };
    outcome.map(|_| status.st_size.max(0) as u64)
}

#[cfg(all(not(test), target_os = "none"))]
pub fn read_operand(path: &str) -> (Vec<u8>, Option<InputFailure>) {
    let mut input = Vec::new();
    let mut chunk = [0u8; 4096];
    let fd = match minix_sys::open(path, 0, 0) {
        Ok(handle) => handle,
        Err(code) => return (input, Some(InputFailure::Open(code))),
    };
    let outcome = loop {
        match read(fd, &mut chunk) {
            Ok(0) => break Ok(()),
            Ok(count) => input.extend_from_slice(&chunk[..count]),
            Err(code) => break Err(InputFailure::Read(code)),
        }
    };
    let _ = minix_sys::close(fd);
    (input, outcome.err())
}

/// The twin seam for chunked reads: read at most `chunk_len` bytes at a time, hand
/// what arrives to `take`, and stop at end of input or on a read failure.
///
/// This is the read shape the `split -b` leg needs: C's `split1` loops on
/// `read(ifd, bfr, MAXBSIZE)`, and across reads the only state kept is the counter
/// "how many bytes of the current chunk are written" — no data
/// (`minix3/usr.bin/split/split.c:175-236`). When it returns `Some(InputFailure)`, the
/// chunks already handed over are **not taken back** — C reads and writes as it goes,
/// so chunks written before the failure remain on disk. Open failures and read
/// failures come back on separate arms because C's sentences differ: the first reports
/// the operand with `err(1, "%s", *argv)`, the second that word with `err(1, "read")`
/// (`:137-138` and `:188-190`).
#[cfg(any(test, not(target_os = "none")))]
pub fn stream_input<F>(path: Option<&str>, chunk_len: usize, take: F) -> Option<InputFailure>
where
    F: FnMut(&[u8]),
{
    let mut reader = match path {
        None | Some("-") => HostedInput::Stdin(std::io::stdin()),
        Some(name) => match std::fs::File::open(name) {
            Ok(opened) => HostedInput::File(opened),
            Err(failure) => return Some(InputFailure::Open(errno_of(&failure))),
        },
    };
    drive_reads(&mut reader, chunk_len, take)
        .err()
        .map(|failure| InputFailure::Read(errno_of(&failure)))
}

/// The two read sources of `stream_input`'s hosted leg: standard input and a file
/// opened by name. They differ only in what the `Read` is; the read loop exists once,
/// so a door can feed `drive_reads` directly instead of opening a real file.
///
/// All three (the read source, its `Read` impl, and `drive_reads`) mention `std`, so
/// they must carry the same `cfg` gate as the hosted leg: this file is pulled into
/// every bin through `#[path]`, and in a freestanding build a `no_std` bin fails to
/// compile the moment it sees an ungated `std` (a lesson learned the hard way: the
/// three-architecture bin compile gate stays red until
/// `cargo build --bins --target x86_64-unknown-none` passes).
#[cfg(any(test, not(target_os = "none")))]
enum HostedInput {
    Stdin(std::io::Stdin),
    File(std::fs::File),
}

#[cfg(any(test, not(target_os = "none")))]
impl std::io::Read for HostedInput {
    fn read(&mut self, out: &mut [u8]) -> std::io::Result<usize> {
        match self {
            Self::Stdin(stdin) => stdin.read(out),
            Self::File(file) => file.read(out),
        }
    }
}

/// Takes `chunk_len`-sized pieces from any `Read` and hands them to `take`; at end of
/// input it finishes, and on a mid-way failure it **hands over the accumulated piece
/// first** and only then reports the error — that ordering contract is exactly C's
/// `split1` read-as-you-write shape (`minix3/usr.bin/split/split.c:184-204`), and it
/// is the only way to pin the shape on target hardware, where an input of "two good
/// reads then a failure" cannot be produced.
#[cfg(any(test, not(target_os = "none")))]
pub fn drive_reads<R, F>(reader: &mut R, chunk_len: usize, mut take: F) -> std::io::Result<()>
where
    R: std::io::Read,
    F: FnMut(&[u8]),
{
    use std::io::Read as _;
    if chunk_len == 0 {
        return Ok(());
    }
    let mut buffer = vec![0u8; chunk_len];
    let mut filled = 0usize;
    loop {
        if filled == chunk_len {
            take(&buffer[..filled]);
            filled = 0;
        }
        match reader.read(&mut buffer[filled..]) {
            Ok(0) => break,
            Ok(count) => filled += count,
            Err(failure) => {
                if filled > 0 {
                    take(&buffer[..filled]);
                }
                return Err(failure);
            }
        }
    }
    if filled > 0 {
        take(&buffer[..filled]);
    }
    Ok(())
}

/// The target leg goes through `minix_sys::read`; otherwise the shape matches the
/// hosted leg: chunks already handed over are not taken back, and the open and read
/// arms keep their own numbers. Standard input (including a bare `-`) is neither
/// opened nor closed.
#[cfg(all(not(test), target_os = "none"))]
pub fn stream_input<F>(path: Option<&str>, chunk_len: usize, mut take: F) -> Option<InputFailure>
where
    F: FnMut(&[u8]),
{
    if chunk_len == 0 {
        return None;
    }
    let mut buffer = alloc::vec![0u8; chunk_len];
    let name = match path {
        None | Some("-") => None,
        Some(name) => Some(name),
    };
    let fd = match name {
        None => STDIN,
        Some(name) => match minix_sys::open(name, 0, 0) {
            Ok(handle) => handle,
            Err(code) => return Some(InputFailure::Open(code)),
        },
    };
    let mut filled = 0usize;
    let failure = loop {
        if filled == chunk_len {
            take(&buffer[..filled]);
            filled = 0;
        }
        match read(fd, &mut buffer[filled..]) {
            Ok(0) => break None,
            Ok(count) => filled += count,
            Err(code) => {
                if filled > 0 {
                    take(&buffer[..filled]);
                }
                break Some(InputFailure::Read(code));
            }
        }
    };
    if filled > 0 {
        take(&buffer[..filled]);
    }
    if name.is_some() {
        let _ = minix_sys::close(fd);
    }
    failure
}

/// An input operand that has been opened but not yet read.
///
/// The split exists because C opens the input *before* it opens (and thereby
/// truncates) the output operand, and reads only afterwards:
/// `ifp = file(argv[0], "r"); ofp = file(argv[1], "w")`. Holding the handle lets
/// a filter keep that order, which is observable in the degenerate
/// `uniq file file` case — C sees an empty stream there, and a shell-out
/// implementation that reads first would echo the old bytes back.
#[cfg(any(test, not(target_os = "none")))]
pub struct ReadSource(std::fs::File);

#[cfg(all(not(test), target_os = "none"))]
pub struct ReadSource {
    fd: minix_sys::Fd,
}

/// Opens an input operand read-only (C's `fopen(name, "r")`), without reading it.
/// The error leg is the same as `create_sink`: it carries the `errno`, not a unit.
#[cfg(any(test, not(target_os = "none")))]
pub fn open_input(path: &str) -> Result<ReadSource, minix_sys::Errno> {
    std::fs::File::open(path)
        .map(ReadSource)
        .map_err(|e| errno_of(&e))
}

#[cfg(all(not(test), target_os = "none"))]
pub fn open_input(path: &str) -> Result<ReadSource, minix_sys::Errno> {
    // `O_RDONLY` is zero, which is exactly the argument set `open(path, 0, 0)` already passes.
    minix_sys::open(path, 0, 0).map(|fd| ReadSource { fd })
}

impl ReadSource {
    /// Reads the handle to its end. A read failure ends collection, matching the
    /// honest-transport shape the rest of this layer uses.
    #[cfg(any(test, not(target_os = "none")))]
    pub fn read_to_end(mut self) -> Vec<u8> {
        use std::io::Read as _;
        let mut data = Vec::new();
        let _ = self.0.read_to_end(&mut data);
        data
    }

    #[cfg(all(not(test), target_os = "none"))]
    pub fn read_to_end(self) -> Vec<u8> {
        let mut data = Vec::new();
        let mut chunk = [0u8; 4096];
        loop {
            match read(self.fd, &mut chunk) {
                Ok(0) => break,
                Ok(count) => data.extend_from_slice(&chunk[..count]),
                Err(_) => break,
            }
        }
        let _ = minix_sys::close(self.fd);
        data
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

#[cfg(all(not(test), target_os = "none"))]
pub fn epoch_micros() -> Option<u64> {
    minix_sys::syscall::sys_times(
        &minix_sys::syscall::DirectKernelCallTransport,
        minix_sys::syscall::SELF,
    )
    .ok()
    .map(|times| times.boot_time * 1_000_000 + times.real_ticks * 10_000)
}

// ---------------------------------------------------------------------------
// Output-path verbs (`sort -o`)
//
// The `sort(1)` output leg asks four questions of a path and, on the branch that
// replaces an existing file, performs five operations in a fixed order
// (`minix3/usr.bin/sort/sort.c:320-379`). They are thin here — the *policy*,
// including which order C performs them and what it reports when one fails, lives
// in the command that reads those lines.
// ---------------------------------------------------------------------------

/// What C's `lstat(outpath, &st)` answers, narrowed to the two things its branch
/// tests: the permission bits it later copies with `lchmod` (`ALLPERMS`,
/// `minix3/sys/sys/stat.h:191`), and whether the path is a character or block
/// special (`S_ISCHR`/`S_ISBLK`, `:172-173`), which is what makes C write straight
/// through instead of spooling to a temporary. `None` is C's `lstat` failure, i.e.
/// no such path.
#[cfg(any(test, not(target_os = "none")))]
pub fn probe_path(path: &str) -> Option<(u32, bool)> {
    use std::os::unix::fs::{FileTypeExt as _, PermissionsExt as _};
    let found = std::fs::symlink_metadata(path).ok()?;
    let kind = found.file_type();
    Some((
        found.permissions().mode() & 0o7777,
        kind.is_char_device() || kind.is_block_device(),
    ))
}

#[cfg(all(not(test), target_os = "none"))]
pub fn probe_path(path: &str) -> Option<(u32, bool)> {
    // Every field of the ABI `struct stat` is an integer or a timespec of integers,
    // so an all-zero value is a valid instance — the same construction `pwd`, `test`
    // and `ln` use for this buffer.
    let mut buf: minix_sys::Stat = unsafe { core::mem::zeroed() };
    minix_sys::lstat(path, &mut buf).ok()?;
    // `S_IFCHR` is 0020000 and `S_IFBLK` 0060000 under `S_IFMT` 0170000
    // (`minix3/sys/sys/stat.h:140-173`).
    let kind = buf.st_mode & 0o170000;
    Some((buf.st_mode & 0o7777, kind == 0o020000 || kind == 0o060000))
}

/// C's `access(outpath, W_OK)` (`sort.c:336-337`), where `W_OK` is `0x02`
/// (`minix3/sys/sys/unistd.h:168-171`).
///
/// The error leg carries **that number**: C's arm is `err(2, "%s", outpath)`, which
/// prints `strerror(errno)`. The target arm takes `access(2)`'s number directly; the
/// hosted arm has no std wrapper for `access(2)`, so it uses the equivalent test "open
/// it for writing once" (without `O_CREAT`/`O_TRUNC`, touching neither content nor
/// metadata; on failure `raw_os_error()` is a real number such as `EACCES`/`ENOENT`).
#[cfg(any(test, not(target_os = "none")))]
pub fn writable(path: &str) -> Result<(), minix_sys::Errno> {
    match std::fs::OpenOptions::new().write(true).open(path) {
        Ok(_) => Ok(()),
        Err(error) => Err(errno_of(&error)),
    }
}

#[cfg(all(not(test), target_os = "none"))]
pub fn writable(path: &str) -> Result<(), minix_sys::Errno> {
    minix_sys::access(path, 0x02)
}

/// C's `access(outfile, F_OK)` (`sort.c:364-365`): does the name exist at all?
/// The error leg carries a number too, because that arm is `err(2, "%s", outfile)`.
/// `F_OK` is `0` (`minix3/sys/sys/unistd.h:168`).
#[cfg(any(test, not(target_os = "none")))]
pub fn exists(path: &str) -> Result<(), minix_sys::Errno> {
    std::fs::metadata(path)
        .map(|_| ())
        .map_err(|error| errno_of(&error))
}

#[cfg(all(not(test), target_os = "none"))]
pub fn exists(path: &str) -> Result<(), minix_sys::Errno> {
    minix_sys::access(path, 0)
}

/// Creates a fresh file for writing, failing if the name is already taken — C's
/// `mkstemp` (`sort.c:340`), whose `O_EXCL` is `0x800` and whose mode is `0600`
/// (`minix3/sys/sys/fcntl.h:101`, `minix3/lib/libc/stdio/gettemp.c`).
#[cfg(any(test, not(target_os = "none")))]
pub fn create_exclusive(path: &str) -> Result<WriteSink, minix_sys::Errno> {
    use std::os::unix::fs::OpenOptionsExt as _;
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)
        .map(WriteSink)
        .map_err(|e| errno_of(&e))
}

#[cfg(all(not(test), target_os = "none"))]
pub fn create_exclusive(path: &str) -> Result<WriteSink, minix_sys::Errno> {
    // `O_WRONLY|O_CREAT|O_EXCL` is `0x1|0x200|0x800`.
    minix_sys::open(path, 0x1 | 0x200 | 0x800, 0o600).map(|fd| WriteSink { fd })
}

/// `lchmod(path, mode)` in C's sense: set the permission bits by path
/// (`minix3/lib/libc/sys/lchmod.c`; the target has no `lchmod` wrapper, so this
/// goes through `chmod`, which differs only for a symlink — a name `sort` has just
/// created for itself). A failure returns the number, because C's arm is
/// `err(2, "cannot chmod %s: output left in %s", outpath, outfile)` (`sort.c:373-374`).
#[cfg(any(test, not(target_os = "none")))]
pub fn set_mode(path: &str, mode: u32) -> Result<(), minix_sys::Errno> {
    use std::os::unix::fs::PermissionsExt as _;
    let permissions = std::fs::Permissions::from_mode(mode);
    std::fs::set_permissions(path, permissions).map_err(|error| errno_of(&error))
}

#[cfg(all(not(test), target_os = "none"))]
pub fn set_mode(path: &str, mode: u32) -> Result<(), minix_sys::Errno> {
    minix_sys::chmod(path, mode)
}

/// C's `link(outfile, outpath)`: the second name appears without moving the bytes,
/// which is why C unlinks the destination first (`sort.c:375-378`). A number is
/// carried here too: `err(2, "cannot link %s: output left in %s", outpath, outfile)`
/// (`:379-380`).
#[cfg(any(test, not(target_os = "none")))]
pub fn hard_link(from: &str, to: &str) -> Result<(), minix_sys::Errno> {
    std::fs::hard_link(from, to).map_err(|error| errno_of(&error))
}

#[cfg(all(not(test), target_os = "none"))]
pub fn hard_link(from: &str, to: &str) -> Result<(), minix_sys::Errno> {
    minix_sys::link(from, to)
}

/// C's `unlink(path)`, used for both the old output name and the temporary.
#[cfg(any(test, not(target_os = "none")))]
pub fn remove_path(path: &str) -> Result<(), ()> {
    std::fs::remove_file(path).map_err(|_| ())
}

#[cfg(all(not(test), target_os = "none"))]
pub fn remove_path(path: &str) -> Result<(), ()> {
    minix_sys::unlink(path).map_err(|_| ())
}

#[cfg(all(test, not(target_os = "none")))]
mod write_leg_tests {
    use super::*;

    /// This door pins "the number must not be flattened": wrap a read-only handle as a
    /// write leg and the host's `write` fails immediately with `EBADF` (number 9, the
    /// same on Linux and Minix — `EBADF 9` in `minix3/sys/include/errno.h`). If an
    /// implementation folded the failure into a unit or into `EIO`, these two
    /// assertions would diverge at once.
    #[test]
    fn the_write_leg_keeps_the_platform_number() {
        let dir = std::env::temp_dir().join(format!("bin-support-write-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("scratch dir");
        let path = dir.join("payload");
        std::fs::write(&path, b"seed").expect("seed file");
        let readable = std::fs::File::open(&path).expect("read-only handle");
        match WriteSink(readable).finish(b"more") {
            Err(SinkFailure::Write(code)) => {
                assert_eq!(code.to_i32(), 9, "应当原样交回 EBADF");
                assert_ne!(
                    code.to_i32(),
                    minix_sys::Errno::EIO.to_i32(),
                    "又被压成 EIO"
                );
            }
            Err(SinkFailure::Close(code)) => {
                panic!("写失败被记成了 close 失败：{}", code.to_i32())
            }
            Ok(()) => panic!("只读句柄上的写不可能成功"),
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
}

#[cfg(test)]
mod close_outcome_tests {
    use super::SinkFailure::{Close, Write};
    use super::close_outcome;

    /// The write failure wins: C exits on the spot where `bigwrite` comes up short and
    /// never reaches the next `close` (the ordering between
    /// `minix3/usr.bin/split/split.c:204` and `:319-322`).
    #[test]
    fn a_write_failure_outranks_a_close_failure() {
        let write = Err(Write(minix_sys::Errno::from_i32(28)));
        let close = Err(Close(minix_sys::Errno::from_i32(5)));
        assert_eq!(
            close_outcome(write, close),
            Err(Write(minix_sys::Errno::from_i32(28)))
        );
    }

    #[test]
    fn a_clean_write_still_reports_the_close_failure() {
        let close = Err(Close(minix_sys::Errno::from_i32(28)));
        assert_eq!(
            close_outcome(Ok(()), close),
            Err(Close(minix_sys::Errno::from_i32(28)))
        );
        assert_eq!(
            close_outcome(Ok(()), Ok(())),
            Ok(()),
            "两腿都干净时不许凭空造出失败"
        );
    }
}
