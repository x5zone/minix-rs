//! Shared doing-half helpers for this crate's `src/bin/` programs.
//!
//! Included per binary through `#[path = "../bin_support.rs"]`; it never
//! enters the library target, keeping `minix-fileops` free of any
//! system-call import. The hosted-versus-target split lives in the two
//! seams the echo template documents (`src/bin/echo.rs`), now carried
//! here as `cfg`-twin pairs so each binary swaps both in one sweep:
//!
//! - argv: hosted builds read `std::env::args` (invalid UTF-8 aborts the
//!   process, the std contract); freestanding builds read the birth-chain
//!   descriptor through `minix_rt::args` (raw initial-stack bytes, no
//!   allocator behind it yet — `minix-rt/src/crt0.rs` `args()`), rendered
//!   lossily into `String`s. C passes argv as raw bytes and never
//!   validates; the lossy render is this layer's text-tool contract, and
//!   the divergence is pinned here rather than spread over the binaries.
//! - termination: hosted builds exit through the host runtime, because
//!   `minix_sys::exit` deliberately spins when no process manager answers
//!   (the C `_exit` last resort, `minix3/minix/lib/libc/sys/_exit.c`),
//!   which would hang every hosted run; freestanding builds swap to
//!   `minix_sys::exit`.
//!
//! Writes go through `minix_sys::write` on both sides; transport failures
//! short-circuit to a typed `Err` (edge E-SYSCALL-SIGN), so hosted runs
//! observe honest failures on both channels.

// Each binary includes this module and uses the subset it needs; the
// unused helpers in any one binary are intentional, not drift.
#![allow(dead_code)]

use minix_sys::Fd;

/// Standard input, POSIX `STDIN_FILENO`.
pub const STDIN: Fd = 0;
/// Standard error, POSIX `STDERR_FILENO`.
pub const STDERR: Fd = 2;
/// Standard output, POSIX `STDOUT_FILENO`.
pub const STDOUT: Fd = 1;

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
    //[SD19-gate] SD-19: hosted warn stderr
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
pub fn warn(bytes: &[u8]) {
    let _ = minix_sys::write(STDERR, bytes);
}

/// C 的 `getprogname()`：`argv[0]` 去掉目录的最后一段（住在 `minix-types` 的 errno
/// 表旁，本层经 `minix_sys::prog_name` 拿；规则见
/// `minix3/lib/csu/common/crt0-common.c:156-165`）。
/// `warn(3)`/`err(3)`/`usage()` 的那几行都以它开头。
pub fn prog_name(argv: &[alloc::string::String]) -> &str {
    let raw = argv
        .first()
        .map(alloc::string::String::as_str)
        .unwrap_or("");
    minix_sys::prog_name(raw)
}

/// Minix3 的 `perror(3)` 那一行：`<label>: <句子>`（**不带程序名**，见
/// `minix3/lib/libc/stdio/perror.c`），写完不换行由本函数补换行、不退——退不退由调用者定。
pub fn perror_line(label: &str, code: minix_sys::Errno) {
    // C 的 `perror` 把调用者的串整个 `fprintf` 出去，只有句子走定长的 `strerror_r`
    // 表（`minix3/lib/libc/stdio/perror.c:57-67`）。权威 `perror_line` 交回的是「本该
    // 写多少字节」——缓冲装不下也照数——所以这里先量一次、再按那个数渲染，
    // 超长操作数不会被截断。
    let mut sizing: [u8; 0] = [];
    let need = minix_sys::perror_line(label, code, &mut sizing);
    let mut line = alloc::vec![0u8; need];
    let len = minix_sys::perror_line(label, code, &mut line);
    warn(&line[..len.min(line.len())]);
    warn(b"\n");
}

/// C 的 `warn(3)` 那一行：`prog: label: <strerror 的句子>` 加一个换行，写完继续跑
/// （`vwarn.c` 不退，退不退由调用者决定）。形状与句子都由 `minix_sys::err_line` 转发的那
/// 一份权威给出（住在 `minix-types` 的 errno 表旁，对位 `minix3/lib/libc/gen/verrc.c` 与 C
/// 的 `sys_errlist`）。
pub fn warn_line(prog: &str, label: &str, code: minix_sys::Errno) {
    // C 的 `vwarnc` 直接用 `vfprintf(stderr, fmt, ap)` 打调用者的串，长度不进方程
    // （`minix3/lib/libc/gen/vwarnc.c:62`）——不先量长度就会把超长操作数截在缓冲
    // 边界上。`err_line` 交回的是需要的字节数，故先拿它量一次、再按这个数渲染。
    let mut sizing: [u8; 0] = [];
    let need = minix_sys::err_line(prog, label, code, &mut sizing);
    let mut line = alloc::vec![0u8; need];
    let len = minix_sys::err_line(prog, label, code, &mut line);
    warn(&line[..len.min(line.len())]);
    warn(b"\n");
}

/// Writes the slice to standard output, reporting success without
/// exiting (the form the printf engine's sink needs).
/// Hosted twin of [`write_ok`]: real stdio.
#[cfg(any(test, not(target_os = "none")))]
pub fn write_ok(bytes: &[u8]) -> bool {
    use std::io::Write as _;
    std::io::stdout().write_all(bytes).is_ok()
}

#[cfg(all(not(test), target_os = "none"))]
pub fn write_ok(bytes: &[u8]) -> bool {
    minix_sys::write(STDOUT, bytes).is_ok()
}

/// Reads one interactive answer the way the C file commands do: the first
/// character decides (`y`/`Y` is yes), the rest of the line is drained, and
/// end-of-file or a read failure counts as "no". Ground truth: `rm.c:573-576`
/// (`first = ch = getchar(); while (ch != '\n' && ch != EOF) ch = getchar();`),
/// the same shape `mv.c:195-201` and `cp/utils.c:123-131` use.
pub fn answer_yes() -> bool {
    let mut buf = [0u8; 64];
    let first = match minix_sys::read(STDIN, &mut buf) {
        Ok(0) | Err(_) => return false,
        Ok(n) => buf[0..n].first().copied().unwrap_or(b'\n'),
    };
    if first != b'\n' {
        loop {
            match minix_sys::read(STDIN, &mut buf) {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    if buf[..n].contains(&b'\n') {
                        break;
                    }
                }
            }
        }
    }
    first == b'y' || first == b'Y'
}

/// Writes the slice to standard output, exiting 1 on failure.
#[cfg(all(not(test), target_os = "none"))]
pub fn emit(bytes: &[u8]) {
    if minix_sys::write(STDOUT, bytes).is_err() {
        terminate(1);
    }
}
