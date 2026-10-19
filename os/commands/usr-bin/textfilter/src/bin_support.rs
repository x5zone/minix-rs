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
    if line.is_empty() {
        None
    } else {
        Some(line)
    }
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
    if line.is_empty() {
        None
    } else {
        Some(line)
    }
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
pub fn lines_of(input: &[u8]) -> Vec<minix_textfilter::window::Line<'_>> {
    if input.is_empty() {
        return Vec::new();
    }
    let text = core::str::from_utf8(input).unwrap_or("");
    let mut pieces: Vec<minix_textfilter::window::Line<'_>> = text
        .split('\n')
        .map(|piece| minix_textfilter::window::Line::new(piece))
        .collect();
    if minix_textfilter::window::final_line_terminated(input) {
        // 换行结尾时 `split` 多交回一片空串，它是换行**之后**的东西，不是行；
        // 内部那片空串（两枚相邻换行之间）仍是真行。
        pieces.pop();
    } else if let Some(last) = pieces.last_mut() {
        // 末行在输入里没有换行：把这件事记在行上，写出的腿就不会去造它。
        *last = minix_textfilter::window::Line::raw(last.text(), false);
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

/// C 的 `time(2)`：秒粒度的墙钟。`cksum` 的 `-t` 那一腿用它算 `Time = %ld seconds` 与
/// `Speed = %lld bytes/second` 两行（`minix3/usr.bin/cksum/md5.c:93`、`:107-113`）。
/// 宿主侧走 `std::time`，真机侧走 `minix_sys::gettimeofday` 的秒位——与 `read_file` 那对
/// cfg 双生面同一口径：判据是同一条「宿主门要能跑、真机门要能取到号」。
/// 取不到号时交回 0 而不是 `-1`：C 的 `time()` 失败交回 `(time_t)-1` 而 `cksum.c` 从不查它，
/// 这一腿印的是诊断性数字，不值得为它加一条错腿；`0` 与 `-1` 的差别只在失败时的读数上。
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

/// 把宿主 `io::Error` 的号取成 C 的 `errno`。C 的这些命令的开档腿印的就是那个号
/// （`err(1, "%s", name)`/`warn("%s", name)` 走 `vwarn`，不是 `warnx`），没有号就只剩
/// 「出错了」三个字。拿不到 `raw_os_error()` 时退到 `EIO`（与 `read_file`、`stdio-games`
/// 那条读缝同一口径）。
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
/// 错误腿带的是那个号（不是单位）：`uniq` 的输出腿是 `file(argv[1], "w")` →
/// `err(1, "%s", name)`（`uniq.c:216-221`），报的正是它。
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
    /// 写进去的字节在 drop 时释放句柄，与 C 那边 stdio 在退出时冲刷、不回报一致。
    ///
    /// 错误腿交回的是**平台那个号**：C 的 `split.c:204`、`:216`、`:225` 三处都是
    /// `bigwrite()` 对不上数就 `err(1, "write")`，`err` 后缀打的正是 `errno`——
    /// 「盘写满」与「真的一次 I/O 错」是两句话，压成一个 `EIO` 就只剩「出错了」。
    /// 往同一枚已打开的输出文件里再写一段：`split -b` 流式化之后，一次读到的字节
    /// 属于当前分片的哪一截就写哪一截（C 的 `bigwrite(ofd, Cs, bcnt)` 就是这个形状）。
    pub fn append(&mut self, bytes: &[u8]) -> Result<(), SinkFailure> {
        use std::io::Write as _;
        self.0
            .write_all(bytes)
            .map_err(|failure| SinkFailure::Write(errno_of(&failure)))
    }

    /// 写完收尾。宿主这条腿看不见 `close` 的失败（`std::fs::File` 由 drop 关闭且不交回号），
    /// 与 `finish` 同一条已登记的差异（`SPLIT-CLOSE-ERRLEG`）。
    pub fn seal(self) -> Result<(), SinkFailure> {
        Ok(())
    }

    pub fn finish(mut self, bytes: &[u8]) -> Result<(), SinkFailure> {
        use std::io::Write as _;
        // 宿主这条腿看不见 `close` 的失败：`std::fs::File` 由 drop 关闭且不交回任何号，
        // 而 C 是在开下一枚分片之前 `close(ofd)` 并 `err(1, "%s", fname)`
        // （`minix3/usr.bin/split/split.c:319-322`）。真机那条腿按 `close_outcome` 报。
        self.0
            .write_all(bytes)
            .map_err(|failure| SinkFailure::Write(errno_of(&failure)))
    }
}

#[cfg(all(not(test), target_os = "none"))]
impl WriteSink {
    /// 一篇写不完就接着写，只有 `write` 交回 0 才算真没进展——这是 C 的
    /// `bigwrite()`（`minix3/usr.bin/split/split.c:337-351`）的循环形状。错误腿带
    /// 平台号，因为调用方要的就是 `err(1, "write")` 那一句。描述符在返回前释放；
    /// `close` 的失败本层不报（`SPLIT-CLOSE-ERRLEG` 已登记）。
    /// 往同一枚已打开的输出文件里再写一段，形状照 C 的 `bigwrite()`。
    pub fn append(&mut self, bytes: &[u8]) -> Result<(), SinkFailure> {
        write_loop(self.fd, bytes).map_err(SinkFailure::Write)
    }

    /// 写完收尾并交回 `close` 的号：C 在 `newfile()` 里 `close(ofd) != 0` 就
    /// `err(1, "%s", fname)`（`split.c:319-322`），此时 `fname` 还是刚写完那一枚。
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

/// 一枚输出文件的两种失败：写不进去，与写完关掉时失败。标签不同是 C 定的——
/// `bigwrite()` 对不上数是 `err(1, "write")`（`minix3/usr.bin/split/split.c:204`、`:216`、
/// `:225`），`newfile()` 里 `close(ofd) != 0` 是 `err(1, "%s", fname)`（`:319-322`，此时
/// 后缀还没往前推进，`fname` 还是**上一枚**分片的名字）。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SinkFailure {
    Write(minix_sys::Errno),
    Close(minix_sys::Errno),
}

/// 两腿的取舍写成一张明表，是因为它得能被门钉住：宿主看不见 `close` 的失败，真机才两条都有。
/// 规则是「写失败优先」——C 在 `bigwrite` 失手处当场 `err` 退出，根本走不到下一次 `close`。
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

/// 把 `bytes` 写完为止：`Ok(0)` 意味着描述符再也吃不进字节，此时没有平台号可交回，
/// 才落到 `EIO`。
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

/// 一枚输入操作数的两条失败腿。C 把它们打在两处：`file(argv[0], "r")` 里 `fopen`
/// 失手是 `err(1, "%s", *argv)`（`minix3/usr.bin/split/split.c:136-138`），读循环里
/// `read` 失手是 `err(1, "read")`（`:185-190`、`:252-257`）——同一枚操作数、两句不同
/// 的话。合成一条就把「档不存在」与「开得了但读不动」混成一谈，而后者在宿主上恰好
/// 可测：一枚目录开得动、读不动（`EISDIR`）。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum InputFailure {
    Open(minix_sys::Errno),
    Read(minix_sys::Errno),
}

/// 读一枚操作数：交回**到的字节**与**让它停下来的那一臂**（若有）。
///
/// 到的字节要一起交回，因为 C 是边读边切边写——`read` 失手时前面的分片已经在盘上了，
/// 调用方于是先把那些分片切完写完，再补一句 `err(1, "read")` 并退 1。
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

/// 输入自带的字节数：`-n` 那一支在读之前先问内核要这个数。
///
/// C 的形状是 `fstat(ifd, &sb)` 失手即 `err(1, "stat")`，随后 `sb.st_size` 既判块数上限
/// 又算块宽（`minix3/usr.bin/split/split.c:293-300`）。管道与字符设备的 `st_size` 是 0，
/// 所以「切 3 片」在那里当场报错——不是读不到数据，是**问不到长度**。`path` 为 `None`
/// 或 `"-"` 时问的是标准输入。
///
/// 宿主这条腿按路径问（标准输入走 `/dev/stdin`，它与 C 的 `fstat(0)` 同数：命名片给长度、
/// 管道与空设备给 0），因为 std 不给非 root 的进程一个不带 `unsafe` 的「对描述符取状态」通道。
/// 真机那条腿就是 `fstat`/`stat` 本身。同一件事，两种承载。
#[cfg(any(test, not(target_os = "none")))]
pub fn input_size(path: Option<&str>) -> Result<u64, minix_sys::Errno> {
    std::fs::metadata(path.unwrap_or("/dev/stdin"))
        .map(|status| status.len())
        .map_err(|failure| errno_of(&failure))
}

/// 真机那条腿：标准输入用 `fstat(0)`，命名档用 `stat(path)`——都是 C 在那个位置做的事
/// （`split.c:293` 拿的是已经打开的那个描述符，命名档的描述符就是 `split.c:137` 开的）。
#[cfg(all(not(test), target_os = "none"))]
pub fn input_size(path: Option<&str>) -> Result<u64, minix_sys::Errno> {
    // `Stat` 的对齐填充位是私有的，crate 外只能按本仓既有的零值形状起身
    // （同 `commands/bin/fileops/src/bin/test.rs:71`）。
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

/// 分块读的双生缝：每次最多读 `chunk_len` 字节，读到的原样交给 `take`，读到尽头或读失手就停。
///
/// 这是 `split -b` 那一支要的读法：C 的 `split1` 一圈一圈 `read(ifd, bfr, MAXBSIZE)`，
/// 跨读留下的只有「当前分片已写多少字节」这一个计数器，不留数据
/// （`minix3/usr.bin/split/split.c:175-236`）。交回 `Some(InputFailure)` 时，
/// 已经交出去的那几块**不收回**——C 是边读边写，读失手之前落盘的分片照旧在盘上。
/// 开档失手与读失手分两臂交回，因为 C 的句式不同：前者 `err(1, "%s", *argv)` 报操作数，
/// 后者 `err(1, "read")` 报这个词（`:137-138` 与 `:188-190`）。
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

/// `stream_input` 宿主腿的两个读源：标准输入与一枚按名开好的档。它们只差 `Read` 是谁，
/// 读循环只此一份，这样门能直接喂给 `drive_reads` 而去开真档。
///
/// 这三件（读源、它的 `Read` 实现、`drive_reads`）都点名 `std`，所以必须与宿主腿同一个
/// `cfg` 门：本文件是每个 bin 用 `#[path]` 收进去的，裸机形态下 `no_std` 的 bin 一旦看见
/// 无门的 `std` 就编不过（判例：批次三百五十八抽出这一层时漏了门，三架构 bin 编译门
/// 自 `2026-10-18` 起红，直到本轮现跑 `cargo build --bins --target x86_64-unknown-none`）。
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

/// 从任意 `Read` 上按 `chunk_len` 一块一块取，交给 `take`；读到尽头就收尾，
/// 中途失手则**先把已经攒下的那段交出去**再把错误报回去——这条顺序契约正是
/// C 的 `split1` 边读边写的形状（`minix3/usr.bin/split/split.c:184-204`），
/// 也是真机上没法造出「读两次成功、第三次失手」的输入时唯一能钉住它的办法。
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

/// 真机那条腿走 `minix_sys::read`，其余形状与宿主腿一字同意：交出去的块不收回，
/// 开档臂与读臂各归各的号。标准输入（含裸 `-`）不 `open` 也不 `close`。
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
/// 错误腿同 `create_sink`：带的是 `errno`，不是单位。
#[cfg(any(test, not(target_os = "none")))]
pub fn open_input(path: &str) -> Result<ReadSource, minix_sys::Errno> {
    std::fs::File::open(path)
        .map(ReadSource)
        .map_err(|e| errno_of(&e))
}

#[cfg(all(not(test), target_os = "none"))]
pub fn open_input(path: &str) -> Result<ReadSource, minix_sys::Errno> {
    // `O_RDONLY` 是零，就是 `open(path, 0, 0)` 已经传的那组形参。
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
/// 错误腿带的是**那个号**：C 那一支是 `err(2, "%s", outpath)`，印的就是 `strerror(errno)`。
/// 真机臂直接拿 `access(2)` 的号；宿主这一臂没有 `access(2)` 的 std 包装，所以用
/// 「以写方式打开一下」的同义判据（不开 `O_CREAT`/`O_TRUNC`，不动内容也不动元数据，
/// 失败时 `raw_os_error()` 就是 `EACCES`/`ENOENT` 这些真号）。之前那一版只看
/// owner-write 位，根本给不出号。
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
/// 错腿同样带号，因为那一支是 `err(2, "%s", outfile)`。`F_OK` 是 `0`
/// （`minix3/sys/sys/unistd.h:168`）。
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
/// created for itself). 失败时交回号，因为 C 那支是
/// `err(2, "cannot chmod %s: output left in %s", outpath, outfile)`（`sort.c:373-374`）。
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
/// which is why C unlinks the destination first (`sort.c:375-378`). 同样带号：
/// `err(2, "cannot link %s: output left in %s", outpath, outfile)`（`:379-380`）。
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

    /// 这枚门钉的是「号不能被压平」：拿一枚只读句柄包成写腿，宿主的 `write` 当场
    /// `EBADF`（9 号，Linux 与 Minix 同值——`minix3/sys/include/errno.h` 的
    /// `EBADF 9`）。若实现把失败折成单位或折成 `EIO`，这里两条断言立刻分家。
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
    use super::close_outcome;
    use super::SinkFailure::{Close, Write};

    /// 写失败赢：C 在 `bigwrite` 失手处当场退出，压根到不了下一次 `close`
    /// （`minix3/usr.bin/split/split.c:204` 与 `:319-322` 的先后）。
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
