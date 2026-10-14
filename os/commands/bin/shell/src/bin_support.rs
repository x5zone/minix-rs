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
/// 双生两面（`caesar`/`factor` 那批的判例）：宿主侧必须走进程自己的 stderr，
/// 因为 `minix_sys::write` 在宿主进的是替身传输——诊断会被吞掉，报文形状就无法在
/// 宿主端到端验证。真机侧仍是那条 `write(2)`。
#[cfg(any(test, not(target_os = "none")))]
pub fn warn(bytes: &[u8]) {
    use std::io::Write as _;
    #[allow(clippy::let_underscore_must_use)]
    let _ = std::io::stderr().write_all(bytes);
}

#[cfg(all(not(test), target_os = "none"))]
pub fn warn(bytes: &[u8]) {
    //[SD19-gate] SD-19: write stderr warn
    #[allow(clippy::let_underscore_must_use)]
    let _ = minix_sys::write(STDERR, bytes);
}

/// Writes the slice to standard output, exiting 1 on failure.
///
/// 与上面 `warn` 同一双生面纪律（批次二百三十七登记的那一条）：`emit` 以真机两面各写
/// 一次，因为 `minix_sys::write` 在宿主进的是替身传输——写会“成功”，而字节永远不会
/// 出现在进程自己的 stdout 上，于是输出面（`getopt` 的选项表、`hostname` 无操作数时
/// 印当前主机名、`sh` 的提示符）在宿主既看不到也钉不住。宿主面走进程自己的
/// `stdout`，写失败退 1（与真机面同一条判据）。
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

/// C 的 `getprogname()`：程序名是运行时刻从 `argv[0]` 上取的最后一个 `/` 之后那段
/// （`minix3/lib/csu/common/crt0-common.c:156-165`），没有斜杠就整段用。`err(3)`/`warn(3)`
/// 的那一行以它开头，所以这一层给的不是装饰而是 C 契约的一部分。
pub fn prog_name(argv: &[alloc::string::String]) -> &str {
    let raw = argv
        .first()
        .map(alloc::string::String::as_str)
        .unwrap_or("");
    minix_types::prog_name(raw)
}

/// 按 C 的 `err(3)` 报一句就走：`prog: label: <strerror 的句子>` 加一个换行，然后以
/// `eval` 终止。形状与句子都由 `minix-types` 那一份权威给出（`err_line` 转发到
/// `verrc.c` 的写法，句子取自 C 的 `sys_errlist`），这里只负责写与退。
pub fn err_exit(prog: &str, label: &str, code: minix_sys::Errno, eval: i32) -> ! {
    let mut line = [0u8; 128];
    let len = minix_types::err_line(prog, label, code, &mut line);
    warn(&line[..len.min(line.len())]);
    warn(b"\n");
    terminate(eval)
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::string::String;
    use alloc::vec::Vec;

    /// `getprogname()` 取的是 `argv[0]` 去掉目录的那段：`/bin/hostname` 报 `hostname`，
    /// 没有斜杠时整段照用（宿主侧跑测试时 `argv[0]` 是构建目录里的路径，同一条规则）。
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

    /// `err_line` 报的是需要的字节数，缓冲区不足时按 C 的截断走；这里钉住两件事：
    /// 完整时逐字等于 `verrc.c` 的形状，不足时写出的前缀仍是那一段而没有越界。
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
