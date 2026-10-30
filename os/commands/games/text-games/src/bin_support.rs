//! Shared doing-half helpers for this crate's `src/bin/` programs.
//!
//! Included per binary through `#[path = "../bin_support.rs"]`; it never
//! enters the library target, keeping `minix_text_games` free of any system-call
//! import. The hosted-versus-target split lives in the two seams the echo
//! template documents (`os/commands/bin/fileops/src/bin/echo.rs`), carried
//! here as `cfg`-twin pairs so each binary swaps both in one sweep: argv
//! via `std::env::args` (hosted) or the `minix-rt` birth-chain descriptor
//! (`minix_rt::crt0::args`, raw initial-stack bytes rendered lossily — C
//! passes argv as raw bytes and never validates, so the lossy render is
//! this layer's text-tool contract), termination via the host runtime
//! (hosted) or `minix_sys::exit` (target). File access and the wall clock
//! follow the same hosted/target twin shape where a binary needs them.
//! Writes go through `minix_sys::write` on both sides; transport failures
//! short-circuit to a typed `Err` (edge E-SYSCALL-SIGN), so hosted runs
//! observe honest failures on both channels.

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
/// The byte-form exit of the argument seam: C treats `argv` as a byte array from start
/// to finish, so every leg that hands an option letter or an operand back as the
/// original bytes must avoid lossy folding — `String::from_utf8_lossy` folds a bare `ff`
/// into the three bytes `ef bf bd` (the residual measured on `random -\xff`).
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

/// The byte form of [`prog_name`]: `argv[0]` comes back as its raw bytes, cut to the
/// segment after the last slash (the same basename rule [`prog_name`] hands to
/// `minix_sys::prog_name`).
pub fn prog_name_bytes(argv: &[alloc::vec::Vec<u8>]) -> &[u8] {
    let raw = argv.first().map_or(&[][..], |name| name.as_slice());
    match raw.iter().rposition(|&byte| byte == b'/') {
        Some(cut) => &raw[cut + 1..],
        None => raw,
    }
}

/// Every environment string as raw `KEY=VALUE` bytes, in order (C `environ`).
/// The `ACRONYMDB` read by `minix3/games/wtf/wtf:47-53` is one such string: the script
/// takes the whole string first and then splits it on whitespace. The target leg comes
/// from the same birth chain as [`args`], and the hosted leg re-joins into the same
/// shape.
#[cfg(all(not(test), target_os = "none"))]
pub fn envs() -> alloc::vec::Vec<alloc::vec::Vec<u8>> {
    minix_rt::crt0::envs().map(|raw| raw.to_vec()).collect()
}

/// Hosted twin of [`envs`].
#[cfg(any(test, not(target_os = "none")))]
pub fn envs() -> alloc::vec::Vec<alloc::vec::Vec<u8>> {
    std::env::vars()
        .map(|(key, value)| {
            let mut bytes = key.into_bytes();
            bytes.push(b'=');
            bytes.extend(value.into_bytes());
            bytes
        })
        .collect()
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

/// Writes the slice to standard output, exiting 1 on failure.
#[cfg(all(not(test), target_os = "none"))]
pub fn emit(bytes: &[u8]) {
    if minix_sys::write(STDOUT, bytes).is_err() {
        terminate(1);
    }
}

/// Writes the slice to standard output and **hands the failing number back** to the
/// caller instead of exiting: the sentence and the exit code belong to the caller,
/// this layer only picks up the number. Hosted twin goes through `std::io::stdout`
/// and reports an error that carries no raw code as `EIO`, the same fallback
/// [`read_stdin_error`] uses on the reading side.
#[cfg(any(test, not(target_os = "none")))]
pub fn emit_error(bytes: &[u8]) -> Option<minix_sys::Errno> {
    use std::io::Write as _;
    match std::io::stdout().write_all(bytes) {
        Ok(()) => None,
        Err(failure) => Some(minix_sys::Errno::from_i32(
            failure
                .raw_os_error()
                .unwrap_or(minix_sys::Errno::EIO.to_i32()),
        )),
    }
}

/// Target twin of [`emit_error`] (see the module header for the seam contract).
/// A single `write` may take fewer bytes than asked for, so the remainder is pushed
/// until the device either takes it all or stops taking it; a write that transfers
/// nothing without failing is reported as `EIO`.
#[cfg(all(not(test), target_os = "none"))]
pub fn emit_error(bytes: &[u8]) -> Option<minix_sys::Errno> {
    let mut left = bytes;
    while !left.is_empty() {
        match minix_sys::write(STDOUT, left) {
            Ok(0) => return Some(minix_sys::Errno::EIO),
            Ok(count) => left = &left[count..],
            Err(code) => return Some(code),
        }
    }
    None
}

/// C's `err(3)`/`warn(3)` line: `prog: label: <sentence>` plus a newline, without
/// exiting (whether to exit is the caller's decision). Both the shape and the sentence
/// live in the one authority, `minix-types`'s `err_line` (the same source as the
/// same-named piece in `textfilter`; C has only one `verrc.c` too).
pub fn warn_line(prog: &str, label: &str, code: minix_sys::Errno) {
    // Measure once, then render against that count: `err_line` returns how many bytes
    // should be written, so an over-long operand is never truncated (C's `vwarnc` prints
    // the caller's whole string to stderr, unbounded by a one-line stack buffer).
    let mut sizing: [u8; 0] = [];
    let need = minix_sys::err_line(prog, label, code, &mut sizing);
    let mut line = alloc::vec![0u8; need];
    let len = minix_sys::err_line(prog, label, code, &mut line);
    warn(&line[..len.min(line.len())]);
    warn(b"\n");
}

/// Reads standard input to the end and hands the failing number back to the caller,
/// rather than treating a failure as end of input.
///
/// C's path is a `getchar()` loop reading to EOF (`minix3/games/random/random.c:127`);
/// stdio keeps the failure on its books and the end reports it with `err(2, "stdin")`.
/// This layer only obtains the number; the sentence and the exit code belong to the
/// caller. The hosted leg goes through `std::io::stdin` (falling back to `EIO` when it
/// has no number, the same rule as the `textfilter` read seam), and the target leg reads
/// chunk by chunk on the same descriptor. With the twins in place, a thin shell's filter
/// path is testable on the host too.
#[cfg(any(test, not(target_os = "none")))]
pub fn read_stdin_error() -> (alloc::vec::Vec<u8>, Option<minix_sys::Errno>) {
    use std::io::Read as _;
    let mut input = alloc::vec::Vec::new();
    match std::io::stdin().read_to_end(&mut input) {
        Ok(_) => (input, None),
        Err(failure) => (
            input,
            Some(minix_sys::Errno::from_i32(
                failure
                    .raw_os_error()
                    .unwrap_or(minix_sys::Errno::EIO.to_i32()),
            )),
        ),
    }
}

/// Target twin of [`read_stdin_error`] (see the module header for the seam contract).
#[cfg(all(not(test), target_os = "none"))]
pub fn read_stdin_error() -> (alloc::vec::Vec<u8>, Option<minix_sys::Errno>) {
    let mut input = alloc::vec::Vec::new();
    let mut chunk = [0u8; 4096];
    loop {
        match minix_sys::read(STDIN, &mut chunk) {
            Ok(0) => return (input, None),
            Ok(count) => input.extend_from_slice(&chunk[..count]),
            Err(code) => return (input, Some(code)),
        }
    }
}

/// Which of the two legs of opening and draining an operand failed. Keeping them
/// apart is what lets a shell say "cannot open" and "read error" as different
/// sentences while both legs stay reachable in a hosted run.
#[derive(Debug, PartialEq, Eq)]
pub enum OperandFailure {
    /// The path could not be opened; the number is what C's `err(1, ...)` appends
    /// as the trailing sentence, so the caller can render the whole line.
    Open(minix_sys::Errno),
}

/// Reads an operand file, reporting only a failure to open it; a read that breaks
/// partway is reported as the end of the file. That is C's shape: `fortune` loads
/// with `fgets(line, sizeof line, fp->inf) != NULL`
/// (`minix3/games/fortune/fortune/fortune.c:268`, `:293`), and the file never asks
/// `ferror`, so a broken read looks exactly like an
/// ordinary end of input — no extra sentence, and no different exit code. Hosted twin:
/// real `std::fs` — without it a file-reading game binary cannot be run, and so
/// cannot be checked, on the host at all (the target face below is the one the
/// kernel actually serves).
#[cfg(any(test, not(target_os = "none")))]
pub fn slurp_operand(path: &str) -> Result<alloc::vec::Vec<u8>, OperandFailure> {
    use std::io::Read as _;
    let mut file = std::fs::File::open(path).map_err(|failure| {
        OperandFailure::Open(minix_sys::Errno::from_i32(
            failure
                .raw_os_error()
                .unwrap_or(minix_sys::Errno::EIO.to_i32()),
        ))
    })?;
    let mut image = alloc::vec::Vec::new();
    // A failed read counts as end of input: C only sees `fgets` return NULL
    if file.read_to_end(&mut image).is_err() {
        return Ok(image);
    }
    Ok(image)
}

/// Target twin of [`slurp_operand`] (see the module header for the seam contract):
/// `minix_sys::open` then repeated `minix_sys::read` on the same descriptor, and the
/// descriptor is closed on both ways out.
#[cfg(all(not(test), target_os = "none"))]
pub fn slurp_operand(path: &str) -> Result<alloc::vec::Vec<u8>, OperandFailure> {
    let fd = match minix_sys::open(path, 0, 0) {
        Ok(fd) => fd,
        Err(code) => return Err(OperandFailure::Open(code)),
    };
    let mut image = alloc::vec::Vec::new();
    let mut chunk = [0u8; 4096];
    loop {
        match minix_sys::read(fd, &mut chunk) {
            Ok(0) => break,
            Ok(count) => image.extend_from_slice(&chunk[..count]),
            Err(_) => break,
        }
    }
    let _ = minix_sys::close(fd);
    Ok(image)
}

/// Wall-clock microseconds since the epoch, when a clock face answers.///
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

#[cfg(all(not(test), target_os = "none"))]
pub fn epoch_secs() -> Option<u64> {
    minix_sys::syscall::sys_times(
        &minix_sys::syscall::DirectKernelCallTransport,
        minix_sys::syscall::SELF,
    )
    .ok()
    .map(|times| times.boot_time + times.real_ticks / 100)
}
