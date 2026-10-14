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
//! typed `Err` (edge E-SYSCALL-SIGN).

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
#[cfg(any(test, not(target_os = "none")))]
pub fn args() -> alloc::vec::Vec<alloc::string::String> {
    std::env::args().collect()
}

/// Terminates the process with an exit status (see the module header).
/// Hosted twin: real stdio — the pre-freestanding `println!`/`eprintln!`
/// semantics (hosted runs are host tools and test fixtures; the kernel
/// transport is not on the hosted output path).
#[cfg(any(test, not(target_os = "none")))]
pub fn warn(bytes: &[u8]) {
    use std::io::Write as _;
    //[SD19-gate] hosted warn 通道与 C `fputs(msg, stderr)` 无检查同构：`write_all` 可 EPIPE，
    // 主路径已选 “输出提示” 而非 “报提示失败”，失败不回滚主语义。与 `os/libs/minix-sys/src/lib.rs`
    // 同族的“不返回语义”处理一致。
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

/// Reads all of standard input into one vector.
///
/// The tr semantics are stream-shaped (squeeze runs can cross any read
/// boundary), so the doing half collects first and transforms once — the
/// same hosted-first trade the primes bound documents. A read failure
/// ends collection; the failure itself is not reported (the C filters
/// exit through their own error paths once the transport reports
/// honestly). Hosted runs read the process' real standard input, which is
/// what lets an integration test drive the binary the way a shell would.
#[cfg(any(test, not(target_os = "none")))]
pub fn read_stdin() -> Vec<u8> {
    use std::io::Read as _;
    let mut input = Vec::new();
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
/// the `minix_sys` transport.
///
/// 错误位带的是**那个号**（不是单位）：C 的这些命令的报错腿是 `err(1, "%s", *argv)`
/// （如 `join.c:244`、`:249`），印的就是它，擦成单位只剩「出错了」三个字。宿主侧的号
/// 取 `raw_os_error()`，没有它时退到 `EIO`（与 `stdio-games` 那条读缝双生面同一口径）。
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

/// C 的 `getprogname()`：`argv[0]` 去掉目录的最后一段（规则在
/// `minix3/lib/csu/common/crt0-common.c:156-165`，权威在 `minix-types` 的 `prog_name`）。
/// `err(3)`/`warn(3)`/`usage()` 的那几行都以它开头。
pub fn prog_name(argv: &[alloc::string::String]) -> &str {
    let raw = argv
        .first()
        .map(alloc::string::String::as_str)
        .unwrap_or("");
    minix_sys::prog_name(raw)
}

/// C 的 `warn(3)`/`err(3)` 那一行：`prog: label: <句子>` 加一个换行，写完不退
/// （`vwarnc.c` 不退，退不退由调用者定）。形状与句子都住在 `minix-types` 的 `err_line`
/// 那一份权威里（本层经 `minix_sys::err_line` 拿，与不缀程序名的 `perror_line` 并列）。
pub fn warn_line(prog: &str, label: &str, code: minix_sys::Errno) {
    // C 的 `vwarnc` 直接用 `vfprintf(stderr, fmt, ap)` 打调用者的串，长度不受一行栈缓冲
    // 限制（`minix3/lib/libc/gen/vwarnc.c:62`；`perror.c:67` 也一样把调用者的串整个打出去，
    // 只有句子走 `strerror_r` 的定长表）。权威交回的是「本该写多少字节」——缓冲装不下也照数
    // ——所以这里先量一次、再按那个数渲染，超长操作数不会被截断。
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

/// A create-or-truncate write sink — C's `fopen(name, "w")`, which truncates the
/// file at open time, before any byte of the input has been read. Keeping the
/// handle open (instead of writing once at the end) is what makes that ordering
/// observable: an input that turns out to be unreadable still leaves the output
/// empty, exactly as the C's already-opened `FILE *` would at exit.
#[cfg(any(test, not(target_os = "none")))]
pub struct WriteSink(std::fs::File);

#[cfg(all(not(test), target_os = "none"))]
pub struct WriteSink {
    fd: minix_sys::Fd,
}

/// Opens an output operand, creating it if absent and truncating it if present.
/// `O_WRONLY` is `0x1` (`minix3/sys/sys/fcntl.h:65`), `O_CREAT` is `0x200` (`:99`)
/// and `O_TRUNC` is `0x400` (`:100`); the mode `0666` is what `fopen` hands to
/// `open(2)`, so the process umask still applies.
#[cfg(any(test, not(target_os = "none")))]
pub fn create_sink(path: &str) -> Result<WriteSink, ()> {
    std::fs::File::create(path).map(WriteSink).map_err(|_| ())
}

#[cfg(all(not(test), target_os = "none"))]
pub fn create_sink(path: &str) -> Result<WriteSink, ()> {
    let fd = minix_sys::open(path, 0x1 | 0x200 | 0x400, 0o666).map_err(|_| ())?;
    Ok(WriteSink { fd })
}

#[cfg(any(test, not(target_os = "none")))]
impl WriteSink {
    /// Writes the collected bytes; the handle is closed when it drops, matching C
    /// where stdio flushes at exit and reports nothing.
    pub fn finish(mut self, bytes: &[u8]) -> Result<(), ()> {
        use std::io::Write as _;
        self.0.write_all(bytes).map_err(|_| ())
    }
}

#[cfg(all(not(test), target_os = "none"))]
impl WriteSink {
    /// Writes the collected bytes and releases the descriptor. C never checks
    /// the stdio write, so a caller that wants to report a failure does so with
    /// its own diagnostic — see the note on `create_sink`.
    pub fn finish(self, bytes: &[u8]) -> Result<(), ()> {
        let done = minix_sys::write(self.fd, bytes);
        let _ = minix_sys::close(self.fd);
        done.map(|_| ()).map_err(|_| ())
    }
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
#[cfg(any(test, not(target_os = "none")))]
pub fn open_input(path: &str) -> Result<ReadSource, ()> {
    std::fs::File::open(path).map(ReadSource).map_err(|_| ())
}

#[cfg(all(not(test), target_os = "none"))]
pub fn open_input(path: &str) -> Result<ReadSource, ()> {
    // `O_RDONLY` is zero, the flags `open(path, 0, 0)` already passes.
    minix_sys::open(path, 0, 0)
        .map(|fd| ReadSource { fd })
        .map_err(|_| ())
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
/// (`minix3/sys/sys/unistd.h:168-171`). The hosted twin reads the owner-write bit
/// instead of the real-uid check, which is the same answer for the files the hosted
/// tests create.
#[cfg(any(test, not(target_os = "none")))]
pub fn writable(path: &str) -> bool {
    use std::os::unix::fs::PermissionsExt as _;
    std::fs::metadata(path)
        .map(|found| found.permissions().mode() & 0o200 != 0)
        .unwrap_or(false)
}

#[cfg(all(not(test), target_os = "none"))]
pub fn writable(path: &str) -> bool {
    minix_sys::access(path, 0x02).is_ok()
}

/// Creates a fresh file for writing, failing if the name is already taken — C's
/// `mkstemp` (`sort.c:340`), whose `O_EXCL` is `0x800` and whose mode is `0600`
/// (`minix3/sys/sys/fcntl.h:101`, `minix3/lib/libc/stdio/gettemp.c`).
#[cfg(any(test, not(target_os = "none")))]
pub fn create_exclusive(path: &str) -> Result<WriteSink, ()> {
    use std::os::unix::fs::OpenOptionsExt as _;
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)
        .map(WriteSink)
        .map_err(|_| ())
}

#[cfg(all(not(test), target_os = "none"))]
pub fn create_exclusive(path: &str) -> Result<WriteSink, ()> {
    // `O_WRONLY|O_CREAT|O_EXCL` is `0x1|0x200|0x800`.
    minix_sys::open(path, 0x1 | 0x200 | 0x800, 0o600)
        .map(|fd| WriteSink { fd })
        .map_err(|_| ())
}

/// `lchmod(path, mode)` in C's sense: set the permission bits by path
/// (`minix3/lib/libc/sys/lchmod.c`; the target has no `lchmod` wrapper, so this
/// goes through `chmod`, which differs only for a symlink — a name `sort` has just
/// created for itself).
#[cfg(any(test, not(target_os = "none")))]
pub fn set_mode(path: &str, mode: u32) -> Result<(), ()> {
    use std::os::unix::fs::PermissionsExt as _;
    let permissions = std::fs::Permissions::from_mode(mode);
    std::fs::set_permissions(path, permissions).map_err(|_| ())
}

#[cfg(all(not(test), target_os = "none"))]
pub fn set_mode(path: &str, mode: u32) -> Result<(), ()> {
    minix_sys::chmod(path, mode).map_err(|_| ())
}

/// C's `link(outfile, outpath)`: the second name appears without moving the bytes,
/// which is why C unlinks the destination first (`sort.c:375-378`).
#[cfg(any(test, not(target_os = "none")))]
pub fn hard_link(from: &str, to: &str) -> Result<(), ()> {
    std::fs::hard_link(from, to).map_err(|_| ())
}

#[cfg(all(not(test), target_os = "none"))]
pub fn hard_link(from: &str, to: &str) -> Result<(), ()> {
    minix_sys::link(from, to).map_err(|_| ())
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
