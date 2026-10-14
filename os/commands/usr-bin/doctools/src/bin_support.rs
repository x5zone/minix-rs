//! Shared doing-half helpers for this crate's `src/bin/` programs.
//!
//! Included per binary through `#[path = "../bin_support.rs"]`; it never
//! enters the library target, keeping `minix_doctools` free of any system-call
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

/// Every environment string as raw `KEY=VALUE` bytes, in order (C `environ`).
/// Used by `which` to read `PATH` (`whereis.c:97`).
#[cfg(all(not(test), target_os = "none"))]
pub fn envs() -> alloc::vec::Vec<alloc::vec::Vec<u8>> {
    minix_rt::crt0::envs().map(|raw| raw.to_vec()).collect()
}

/// Hosted twin of [`envs`], re-joining each var into `KEY=VALUE` bytes.
#[cfg(any(test, not(target_os = "none")))]
pub fn envs() -> alloc::vec::Vec<alloc::vec::Vec<u8>> {
    std::env::vars()
        .map(|(k, v)| {
            let mut s = k.into_bytes();
            s.push(b'=');
            s.extend(v.into_bytes());
            s
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

/// Minix3 的 `perror(3)` 那一行：`<调用者给的串>: <句子>`——注意这张表里的
/// `perror` **不带程序名前缀**（`minix3/lib/libc/stdio/perror.c` 打的是
/// `fprintf(stderr, "%s%s%s\n", s, separator, buf)`），与缀程序名的
/// `err(3)`/`warn(3)` 是两件事。渲染与句子表都住在 `minix-types` 那一份权威里
/// （`perror_line`，与 `err_line`/`prog_name` 并列），本层经 `minix_sys::perror_line` 拿，
/// 本处只负责写与补换行；退不退由调用者定（C 的 `perror` 也不退）。
pub fn perror_line(label: &str, code: minix_sys::Errno) {
    let mut line = [0u8; 256];
    let len = minix_sys::perror_line(label, code, &mut line);
    warn(&line[..len.min(line.len())]);
    warn(b"\n");
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

#[cfg(all(not(test), target_os = "none"))]
pub fn epoch_secs() -> Option<u64> {
    minix_sys::syscall::sys_times(
        &minix_sys::syscall::DirectKernelCallTransport,
        minix_sys::syscall::SELF,
    )
    .ok()
    .map(|times| times.boot_time + times.real_ticks / 100)
}
