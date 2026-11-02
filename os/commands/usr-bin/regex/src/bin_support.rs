//! Shared doing-half helpers for this crate's `src/bin/` programs.
//!
//! Included per binary through `#[path = "../bin_support.rs"]`; it never
//! enters the library target, keeping `minix-regex` free of any
//! system-call import. The hosted-versus-target split lives in the seams the
//! echo template documents (`os/commands/bin/fileops/src/bin/echo.rs`),
//! carried here as `cfg`-twin pairs so each binary swaps all of them in one
//! sweep: argv via `std::env::args` (hosted) or the `minix-rt` birth-chain
//! descriptor (`minix_rt::crt0::args`, raw initial-stack bytes rendered
//! lossily), termination via the host runtime (hosted) or `minix_sys::exit`
//! (target), and the **input** legs ([`read_stdin`] / [`read_file`]) via
//! `std::io` / `std::fs` (hosted) or `minix_sys::{open, read, close}`
//! (target). Transport failures short-circuit to a typed `Err` (edge
//! E-SYSCALL-SIGN), so hosted runs observe honest failures on both
//! channels.

// Each binary includes this module and uses the subset it needs; the
// unused helpers in any one binary are intentional, not drift.
#![allow(dead_code)]

use alloc::string::{String, ToString};
use alloc::vec::Vec;
use minix_sys::Fd;
// The syscall carriers behind the target legs of the input and output seams;
// the hosted legs go through `std`, so nothing here is referenced there.
#[cfg(all(not(test), target_os = "none"))]
use minix_sys::{close, open, read, write};

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

/// The name C prints in its diagnostics: `getprogname()`, which the startup code
/// takes from `argv[0]` with the directory part removed — so `vwarnc`'s first
/// segment (`minix3/lib/libc/gen/vwarnc.c:3`) is the last path component, and a
/// renamed or symlinked invocation prints a different name from the same binary.
/// An invented `"sed: "` string does not move with `argv[0]` at all, which is how
/// the same defect showed up in `vis` and `nl`.
#[must_use]
pub fn prog_name(argv: &[alloc::string::String]) -> alloc::string::String {
    let raw = argv.first().map(String::as_str).unwrap_or("");
    match raw.rfind('/') {
        Some(cut) => raw[cut + 1..].to_string(),
        None => raw.to_string(),
    }
}

/// Terminates the process with an exit status (see the module header).
/// Hosted twin: real stdio — the pre-freestanding `println!`/`eprintln!`
/// semantics (hosted runs are host tools and test fixtures; the kernel
/// transport is not on the hosted output path).
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

/// Target twin of [`warn`]: writes the diagnostic to standard error.
#[cfg(all(not(test), target_os = "none"))]
pub fn warn(bytes: &[u8]) {
    let _ = write(STDERR, bytes);
}

/// Reads all of standard input into one vector.
///
/// Stream filters transform per line, so the doing half collects first and
/// iterates over the split; a read failure ends collection.
///
/// The hosted twin goes through `std::io::stdin`, because descriptor 0 on the
/// host is the real process standard input while `minix_sys::read` is a shim
/// that hands back nothing — a filter built on the host would otherwise run the
/// whole pipeline over an empty pattern space and still exit 0 (`sed 's/a/A/'`
/// measured exactly that: no output, exit 0, where the C reads and prints two
/// lines). Same leg, two carriers: the target still goes through the syscall.
#[cfg(any(test, not(target_os = "none")))]
pub fn read_stdin() -> Vec<u8> {
    use std::io::Read as _;
    let mut input = Vec::new();
    // A failed read ends collection with whatever arrived, which is the target
    // leg's behavior too, so both sides hand back the same shape.
    let _ = std::io::stdin().read_to_end(&mut input);
    input
}

#[cfg(all(not(test), target_os = "none"))]
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

/// Opens and drains one file operand, handing back the **`errno`** when the
/// operand could not be opened.
///
/// Both consumers report an unreadable operand with C's `warn("%s", name)`
/// (`minix3/usr.bin/sed/main.c:450`, `minix3/minix/usr.bin/grep/util.c:111`), and
/// `warn` is three segments — program name, the formatted text, then
/// `strerror(errnum)` — because it goes through `vwarnc`
/// (`minix3/lib/libc/gen/vwarnc.c:3-8`). That last segment is the number, so the
/// seam has to carry it rather than collapse every failure into one unit type.
///
/// The hosted twin opens through `std::fs`, for the same reason as
/// [`read_stdin`]: `minix_sys::open` on the host fails, so every file operand
/// would take the warning arm and a normal run would exit 1 with no output.
#[cfg(any(test, not(target_os = "none")))]
pub fn read_file(path: &str) -> Result<Vec<u8>, minix_sys::Errno> {
    use std::io::Read as _;
    let mut file = std::fs::File::open(path).map_err(|failure| errno_of(&failure))?;
    let mut data = Vec::new();
    // A read failure stops collection and keeps the bytes that arrived, the
    // same shape as the target loop below.
    let _ = file.read_to_end(&mut data);
    Ok(data)
}

#[cfg(all(not(test), target_os = "none"))]
pub fn read_file(path: &str) -> Result<Vec<u8>, minix_sys::Errno> {
    let fd = match open(path, 0, 0) {
        Ok(fd) => fd,
        Err(code) => return Err(code),
    };
    let mut data = Vec::new();
    let mut chunk = [0u8; 4096];
    loop {
        match read(fd, &mut chunk) {
            Ok(0) => break,
            Ok(count) => data.extend_from_slice(&chunk[..count]),
            Err(_) => break,
        }
    }
    // [SD19-gate] the fd is no longer used after close
    #[allow(clippy::let_underscore_must_use)]
    let _ = close(fd);
    Ok(data)
}

/// The `errno` behind a host I/O failure: the OS number when there is one, and
/// `EIO` for the failure kinds that carry no number here — which is what C's
/// `warn` would print too, since it reads the `errno` the failed call left set.
#[cfg(any(test, not(target_os = "none")))]
fn errno_of(error: &std::io::Error) -> minix_sys::Errno {
    minix_sys::Errno::from_i32(
        error
            .raw_os_error()
            .unwrap_or(minix_sys::Errno::EIO.to_i32()),
    )
}

/// C's `warn("%s", name)` line: `<程序名>: <名字>: <strerror 句子>\n`
/// (`vwarnc.c:3-8`, where the first segment comes from `getprogname()` — pass
/// [`prog_name`]'s result, not a literal command name).
pub fn warn_name(prog: &str, name: &str, code: minix_sys::Errno) {
    let sentence = minix_sys::Strerror(code).to_string();
    let mut line = Vec::with_capacity(prog.len() + name.len() + sentence.len() + 4);
    line.extend_from_slice(prog.as_bytes());
    line.extend_from_slice(b": ");
    line.extend_from_slice(name.as_bytes());
    line.extend_from_slice(b": ");
    line.extend_from_slice(sentence.as_bytes());
    line.push(b'\n');
    warn(&line);
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
