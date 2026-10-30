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
///
/// Twin faces (the precedent set by the `caesar`/`factor` batch): the hosted
/// side must go through the process's own stderr, because `minix_sys::write`
/// on the host enters the stand-in transport — the diagnostic would be
/// swallowed and the report shape could not be verified end to end on the
/// host. The target side still uses that `write(2)`.
#[cfg(any(test, not(target_os = "none")))]
pub fn warn(bytes: &[u8]) {
    use std::io::Write as _;
    #[allow(clippy::let_underscore_must_use)]
    let _ = std::io::stderr().write_all(bytes);
}

#[cfg(all(not(test), target_os = "none"))]
pub fn warn(bytes: &[u8]) {
    //[SD19-gate] best-effort stderr warn: the write result is intentionally dropped
    #[allow(clippy::let_underscore_must_use)]
    let _ = minix_sys::write(STDERR, bytes);
}

/// Writes the slice to standard output, exiting 1 on failure.
///
/// Same twin-face discipline as `warn` above: `emit` is written once per
/// host/target face, because `minix_sys::write` on the host enters the
/// stand-in transport — the write "succeeds", yet the bytes never appear on
/// the process's own stdout, so the output face (the `getopt` option table,
/// `hostname` printing the current name with no operand, `sh`'s prompt)
/// could be neither seen nor pinned on the host. The hosted face goes
/// through the process's own `stdout`, exiting 1 on a failed write (the same
/// criterion as the target face).
#[cfg(any(test, not(target_os = "none")))]
pub fn emit(bytes: &[u8]) {
    use std::io::Write as _;
    if std::io::stdout().write_all(bytes).is_err() {
        terminate(1);
    }
}

#[cfg(all(not(test), target_os = "none"))]
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

/// C's `getprogname()`: the program name is the part of `argv[0]` after the
/// last `/`, taken at run time
/// (`minix3/lib/csu/common/crt0-common.c:156-165`); without a slash the whole
/// string is used. The `err(3)`/`warn(3)` line begins with it, so this layer
/// is part of the C contract, not decoration.
pub fn prog_name(argv: &[alloc::string::String]) -> &str {
    let raw = argv
        .first()
        .map(alloc::string::String::as_str)
        .unwrap_or("");
    minix_sys::prog_name(raw)
}

/// Report one line the way C's `err(3)` does and leave: `prog: label: <strerror
/// sentence>` plus a newline, then terminate with `eval`. Both the shape and
/// the sentence come from the authority in `minix-types` (`err_line` forwards
/// to the `verrc.c` form, the sentence is taken from C's `sys_errlist`); this
/// function only writes and exits.
pub fn err_exit(prog: &str, label: &str, code: minix_sys::Errno, eval: i32) -> ! {
    // C's `vwarnc` prints the caller's string with `vfprintf(stderr, fmt, ap)`
    // directly, so its length is not bounded by a one-line stack buffer
    // (`minix3/lib/libc/gen/vwarnc.c:62`; `perror.c:67` likewise prints the
    // caller's whole string, with only the sentence going through the
    // fixed-size `strerror_r` table). The authority returns "how many bytes
    // should have been written" — counted even when the buffer is too small —
    // so size once here, then render to that size: an overlong operand is
    // never truncated.
    let mut sizing: [u8; 0] = [];
    let need = minix_sys::err_line(prog, label, code, &mut sizing);
    let mut line = alloc::vec![0u8; need];
    let len = minix_sys::err_line(prog, label, code, &mut line);
    warn(&line[..len.min(line.len())]);
    warn(b"\n");
    terminate(eval)
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::string::String;
    use alloc::vec::Vec;

    /// `getprogname()` takes the directoryless segment of `argv[0]`:
    /// `/bin/hostname` reports `hostname`, and without a slash the whole string
    /// is used (when tests run on the host, `argv[0]` is a path inside the
    /// build directory, under the same rule).
    #[test]
    fn test_prog_name_takes_the_last_slash_segment() {
        let argv: Vec<String> = ["/bin/hostname", "up"]
            .iter()
            .map(|a| String::from(*a))
            .collect();
        assert_eq!(prog_name(&argv), "hostname");
        let bare: Vec<String> = ["domainname"].iter().map(|a| String::from(*a)).collect();
        assert_eq!(prog_name(&bare), "domainname");
        let trailing: Vec<String> = ["/usr/"].iter().map(|a| String::from(*a)).collect();
        assert_eq!(prog_name(&trailing), "");
        assert_eq!(prog_name(&Vec::new()), "");
    }

    /// `err_line` reports the **needed byte count**; if the buffer cannot hold
    /// it, only that much is written (this is the authority's own plumbing
    /// shape; C itself does not truncate — `vwarnc` and `perror` both print
    /// the caller's whole string through `fprintf`). Two things are pinned:
    /// when it fits, the bytes equal the `verrc.c` shape verbatim; when it does
    /// not, the written prefix is still that segment and stays in bounds — the
    /// caller uses this return value to size first and then render to size
    /// (see `err_exit`).
    #[test]
    fn test_err_line_shape_and_truncation() {
        let mut line = [0u8; 128];
        let len = minix_types::err_line(
            "hostname",
            "sethostname",
            minix_sys::Errno::from_i32(minix_types::EINVAL),
            &mut line,
        );
        assert_eq!(&line[..len], b"hostname: sethostname: Invalid argument");
        let mut tight = [0u8; 10];
        let need = minix_types::err_line(
            "hostname",
            "gethostname",
            minix_sys::Errno::from_i32(minix_types::EPERM),
            &mut tight,
        );
        assert!(need > tight.len(), "这一行放不下，报的是需要的长度");
        assert_eq!(&tight[..], b"hostname: ");
    }
}
