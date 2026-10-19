//! Minix-RS tail — last lines of a stream.
//!
//! Ground truth: `minix3/usr.bin/tail/tail.c` (NetBSD), `tail [-n count |
//! -count] [file ...]` with a default of ten lines. The ring holds as many
//! borrowed lines as the count asks for and no more than the input offered, so
//! any count is honoured the way C's backward walk through the mapped file
//! honours one. With no operand tail reads standard input; with
//! operands it prints the last `count` lines of each file, prefixing a
//! `==> name <==` banner when there is more than one file (this NetBSD `tail`
//! has no `-q`/`-v` to change that) with a blank line between files, and an
//! unopenable/unstatable file warns and is skipped (exit status stays
//! nonzero). Operands open with `fopen`, so `-` is a literal file name.
//!
//! Follow mode is modelled on the shape the C actually takes on Minix. The
//! kqueue arm of `forward()`'s loop is excluded by `#if !defined(__minix)`
//! (`forward.c:207-246`), so what is left is: copy from the handle's position to
//! the end, `fflush`, `sleep(1)` (`:191-200`, `:253`), and — only for `-F` on a
//! named operand (`:255`) — compare `st_ino`/`st_dev`/`st_rdev`/`st_nlink` and
//! reopen when they differ (`:257-266`). Because the position belongs to the open
//! handle, a file that shrinks does not rewind it: repositioning to the end lives
//! only in the excluded kqueue branch (`:238-243`), and the reference run of the
//! C confirms the surviving shape is a mid-line fragment, not a re-sent line.
//!
//! Two legs never reach that loop: a follow flag with more than one operand exits
//! before anything is opened (`tail.c:135-137`), and `-r` with a follow flag is a
//! usage error (`:143-145`). A follow flag on a pipe standard input is dropped
//! outright (`:197-201`, "POSIX.2 requires this").
//!
//! Byte mode `-c N`, block mode `-b N`, reverse mode `-r` and the forward
//! offsets `-n +N` / `-c +N` / `-b +N` are all modelled — C
//! `tail.c`/`forward.c`/`reverse.c`: `-r` prints lines bottom-up (`REVERSE`
//! reverses every line, `RLINES` = an explicit `-n` reverses the last `count`),
//! `-r` combined with `-c`/`-b` is the `RBYTES` byte-window line reverse
//! (`reverse_bytes`), and a `+N` value takes the forward arm of the `ARG` macro
//! so `tail -n +20` starts at line 20 rather than taking the last twenty lines.
//!
//! One declared simplification, registered rather than silently accepted: when a
//! read fails this shell reports the operand as failed and prints nothing, while
//! C's forward arms (`FBYTES`/`FLINES`) have already copied the bytes they read
//! before the failure (`forward.c:112-134`). The difference shows only on a read
//! that fails mid-stream, which neither the host gates nor the current machine
//! gates can produce deterministically.

#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

extern crate alloc;
use alloc::format;
use alloc::vec;

use alloc::string::String;
use alloc::vec::Vec;
#[path = "../bin_support.rs"]
mod support;

use minix_textfilter::window::{
    Follow, LineWindow, StatIdentity, TailError, TailStyle, TailWindow, parse_tail_args,
    reopen_needed,
};

// ---------------------------------------------------------------------------
// 跟随环路的双生缝
//
// 为什么写在壳这一层而不是 `bin_support`：这些缝只服务「拿着一个开好的句柄一直读」
// 这一种形状，本 crate 里今天只有 `tail` 用它。`bin_support` 是三十来个二进制各自
// `#[path]` 包进去的共享面，为一个用户加接口会把改动摊到全族头上；等第二个跟随者
// 出现再上提。
// ---------------------------------------------------------------------------

/// C `forward(FILE *fp, ...)` 里的那个 `fp`。跟随期间它必须一直开着：「读到哪儿」是
/// 句柄的性质而不是文件大小的算术（`forward.c:191-195` 每轮从当前位置抄到 EOF，
/// 而缩小时不回退位置——重定位只存在于被排除的 kqueue 支 `:238-243`）。
#[cfg(any(test, not(target_os = "none")))]
pub struct FollowSource {
    /// 句柄统一套一层 `ManuallyDrop`，由下面的 `Drop` 决定关不关：C 的 `fclose`
    /// 只作用于操作数（`tail.c:183`），标准输入那条腿不关。
    file: std::mem::ManuallyDrop<std::fs::File>,
    is_stdin: bool,
}

#[cfg(any(test, not(target_os = "none")))]
impl Drop for FollowSource {
    fn drop(&mut self) {
        if !self.is_stdin {
            unsafe { std::mem::ManuallyDrop::drop(&mut self.file) };
        }
    }
}

#[cfg(all(not(test), target_os = "none"))]
pub struct FollowSource {
    fd: minix_sys::Fd,
    is_stdin: bool,
}

// 宿主错误号 → C `errno` 的换算住在 `support::errno_of`（与 `read_file`、`open_input`
// 同一口径），本文件不再另写一份：C 的 `ierr()` 走的是带号的 `xerr`/`vwarn`
// （`misc.c:53-57`、`:65-76`），两份换算迟早会漂移。

impl FollowSource {
    /// C `fopen(fname, "r")`（`tail.c:167`）。
    #[cfg(any(test, not(target_os = "none")))]
    fn open(path: &str) -> Result<Self, minix_sys::Errno> {
        std::fs::File::open(path)
            .map(|file| FollowSource {
                file: std::mem::ManuallyDrop::new(file),
                is_stdin: false,
            })
            .map_err(|e| support::errno_of(&e))
    }

    #[cfg(all(not(test), target_os = "none"))]
    fn open(path: &str) -> Result<Self, minix_sys::Errno> {
        // `O_RDONLY` 是零，就是 `bin_support` 里那条读缝用的同一组形参。
        minix_sys::open(path, 0, 0).map(|fd| FollowSource {
            fd,
            is_stdin: false,
        })
    }

    /// 无操作数那一腿的 stdin。C 在 `tail.c:186` 把 `fname` 写成字面量 `"stdin"`，
    /// 诊断行因此也念得出这个名字。
    #[cfg(any(test, not(target_os = "none")))]
    fn stdin() -> Self {
        use std::os::fd::FromRawFd as _;
        FollowSource {
            file: std::mem::ManuallyDrop::new(unsafe {
                std::fs::File::from_raw_fd(support::STDIN)
            }),
            is_stdin: true,
        }
    }

    #[cfg(all(not(test), target_os = "none"))]
    fn stdin() -> Self {
        FollowSource {
            fd: support::STDIN,
            is_stdin: true,
        }
    }

    /// C `fstat(fileno(fp), &sb)`（`tail.c:168`、`:188`）——操作数打不开**或**取不到
    /// 身份都是 `ierr(); continue`，所以这一口不是可选项。
    #[cfg(any(test, not(target_os = "none")))]
    fn identity(&self) -> Result<StatIdentity, minix_sys::Errno> {
        self.file
            .metadata()
            .map(host_identity)
            .map_err(|e| support::errno_of(&e))
    }

    #[cfg(all(not(test), target_os = "none"))]
    fn identity(&self) -> Result<StatIdentity, minix_sys::Errno> {
        // `struct stat` 的每个字段都是整数或整数的 timespec，全零是合法实例——与本
        // crate `probe_path`、`fileops` 的 `ls`/`test` 用同一种构造。
        let mut stat: minix_sys::Stat = unsafe { core::mem::zeroed() };
        minix_sys::fstat(self.fd, &mut stat)?;
        Ok(target_identity(&stat))
    }

    /// C 的定位探测：`lseek(fileno(stdin), 0, SEEK_CUR) == -1 && errno == ESPIPE`
    /// （`tail.c:197-201`）。只有这一个号会让 C 放弃跟随，别的错误（比如 `EBADF`）
    /// 仍然往下走，所以这里也不能「凡是错就算管道」。
    #[cfg(any(test, not(target_os = "none")))]
    fn is_pipe_by_position(&mut self) -> bool {
        use std::io::Seek as _;
        // `SeekFrom::Current(0)` 就是 C 那句 `lseek(fd, 0, SEEK_CUR)`：只问位置、
        // 不动它，所以探测本身不会影响后面那拍从哪儿读。
        match self.file.seek(std::io::SeekFrom::Current(0)) {
            Err(error) => error.raw_os_error() == Some(minix_sys::ESPIPE),
            _ => false,
        }
    }

    #[cfg(all(not(test), target_os = "none"))]
    fn is_pipe_by_position(&mut self) -> bool {
        // `SEEK_CUR` 是 1（`minix3/sys/sys/unistd.h:175`）。
        match minix_sys::lseek(self.fd, 0, 1) {
            Err(error) => error.to_i32() == minix_sys::ESPIPE,
            _ => false,
        }
    }

    /// 初始那一整块：把当前位置之后的字节收齐（C 的定位与首轮抄字节都在 `forward()`
    /// 的 switch 与第一个 `getc` 环路里，`forward.c:101-178`、`:191-195`）。
    /// 收到 EOF 或出错为止，句柄不关。
    #[cfg(any(test, not(target_os = "none")))]
    fn collect(&mut self) -> Result<Vec<u8>, minix_sys::Errno> {
        use std::io::Read as _;
        let mut data = Vec::new();
        self.file
            .read_to_end(&mut data)
            .map_err(|e| support::errno_of(&e))?;
        Ok(data)
    }

    #[cfg(all(not(test), target_os = "none"))]
    fn collect(&mut self) -> Result<Vec<u8>, minix_sys::Errno> {
        let mut data = Vec::new();
        let mut chunk = [0u8; 4096];
        loop {
            match minix_sys::read(self.fd, &mut chunk) {
                Ok(0) => break,
                Ok(count) => data.extend_from_slice(&chunk[..count]),
                Err(error) => return Err(error),
            }
        }
        Ok(data)
    }

    /// 跟随环路的每一拍：边读边写（C 的 `putchar` 就在 `getc` 里同步做，
    /// `forward.c:192-195`），所以读失败之前抄到的字节已经出去了。
    #[cfg(any(test, not(target_os = "none")))]
    fn pump(&mut self) -> Result<(), minix_sys::Errno> {
        use std::io::Read as _;
        let mut chunk = [0u8; 4096];
        loop {
            match self.file.read(&mut chunk) {
                Ok(0) => return Ok(()),
                Ok(count) => support::emit(&chunk[..count]),
                Err(error) => return Err(support::errno_of(&error)),
            }
        }
    }

    #[cfg(all(not(test), target_os = "none"))]
    fn pump(&mut self) -> Result<(), minix_sys::Errno> {
        let mut chunk = [0u8; 4096];
        loop {
            match minix_sys::read(self.fd, &mut chunk) {
                Ok(0) => return Ok(()),
                Ok(count) => support::emit(&chunk[..count]),
                Err(error) => return Err(error),
            }
        }
    }

    /// C `freopen(fname, "r", fp)`（`forward.c:261`）：按名字重新打开同一个操作数，
    /// 读位置回到新档的开头。目标侧没有 `freopen`，语义是「放掉旧的再按名字开」，
    /// 而标准输入永不走到这里（`probes_identity` 那一层就挡住了）。
    #[cfg(any(test, not(target_os = "none")))]
    fn reopen(&mut self, path: &str) -> Result<(), minix_sys::Errno> {
        if self.is_stdin {
            return Err(minix_sys::Errno::EBADF);
        }
        let fresh = std::fs::File::open(path).map_err(|e| support::errno_of(&e))?;
        // C 的 `freopen` 是把那个流换掉，旧句柄不留着；本对象自己管那份释放
        // （`Drop` 只在没重开过的手上跑）。
        unsafe { std::mem::ManuallyDrop::drop(&mut self.file) };
        self.file = std::mem::ManuallyDrop::new(fresh);
        Ok(())
    }

    #[cfg(all(not(test), target_os = "none"))]
    fn reopen(&mut self, path: &str) -> Result<(), minix_sys::Errno> {
        if self.is_stdin {
            return Err(minix_sys::Errno::EBADF);
        }
        let fresh = minix_sys::open(path, 0, 0)?;
        let _ = minix_sys::close(self.fd);
        self.fd = fresh;
        Ok(())
    }
}

/// 真机侧的释放对位 C `fclose(fp)`（`tail.c:183`）：操作数读完就关，标准输入不关
///（C 的那条腿也不 `fclose(stdin)`）。宿主侧 `File` 的析构已经是这个语义，
/// `ManuallyDrop` 那一臂天生不会去关 fd 0。
#[cfg(all(not(test), target_os = "none"))]
impl Drop for FollowSource {
    fn drop(&mut self) {
        if !self.is_stdin {
            let _ = minix_sys::close(self.fd);
        }
    }
}

/// C `forward()` 手上那三样：开着的句柄、它的身份（`sbp`）、以及操作数名字
/// （全局 `fname`）。标准输入没有名字，所以这里是 `Option<&str>`——C 用
/// `fp != stdin` 表达同一件事（`forward.c:255`），而决定半的
/// [`Follow::probes_identity`] 就是那道判据的模型。
struct FollowTarget<'a> {
    source: FollowSource,
    opened: StatIdentity,
    path: Option<&'a str>,
}

impl<'a> FollowTarget<'a> {
    fn is_stdin(&self) -> bool {
        self.path.is_none()
    }

    /// 诊断行里念的那个名字：C 的 `ierr()` 用的是全局 `fname`，无操作数时
    /// `tail.c:186` 把它写成字面量 `"stdin"`。
    fn label(&self) -> &'a str {
        self.path.unwrap_or("stdin")
    }
}

/// 宿主 `struct stat` 的四个被比字段（`std::os::unix::fs::MetadataExt`）。
#[cfg(any(test, not(target_os = "none")))]
fn host_identity(metadata: std::fs::Metadata) -> StatIdentity {
    use std::os::unix::fs::MetadataExt as _;
    StatIdentity {
        ino: metadata.ino(),
        dev: metadata.dev(),
        rdev: metadata.rdev(),
        nlink: metadata.nlink(),
    }
}

/// 真机侧 `Stat` 的同四个字段。
#[cfg(all(not(test), target_os = "none"))]
fn target_identity(stat: &minix_sys::Stat) -> StatIdentity {
    StatIdentity {
        ino: stat.st_ino,
        dev: stat.st_dev,
        rdev: stat.st_rdev,
        nlink: u64::from(stat.st_nlink),
    }
}

/// C `stat(fname, &statbuf)`（`forward.c:256`）：按操作数名字查身份，与手上这个
/// 句柄的身份相比。打不开的名字（换档后被删掉）在 C 里被 `!= -1` 那道判据挡住，
/// 这一拍什么都不做，下一拍继续睡。
#[cfg(any(test, not(target_os = "none")))]
fn stat_by_path(path: &str) -> Result<StatIdentity, minix_sys::Errno> {
    std::fs::metadata(path)
        .map(host_identity)
        .map_err(|e| support::errno_of(&e))
}

#[cfg(all(not(test), target_os = "none"))]
fn stat_by_path(path: &str) -> Result<StatIdentity, minix_sys::Errno> {
    let mut stat: minix_sys::Stat = unsafe { core::mem::zeroed() };
    minix_sys::stat(path, &mut stat)?;
    Ok(target_identity(&stat))
}

/// C 的 `(void) sleep(1)`（`forward.c:253`）：返回值被丢掉，被信号提前打断也就打断。
#[cfg(any(test, not(target_os = "none")))]
fn sleep_one_tick() {
    std::thread::sleep(std::time::Duration::from_secs(1));
}

/// 真机侧的等待通道是 `minix_sys::nanosleep`，它的实现就是 VFS 的空集 `select`
/// 带超时（`misc::nanosleep_via` → `select_empty_via`，与 C `nanosleep.c` 同源）。
/// 返回值同样丢掉，对位 C 的 `(void)`。
#[cfg(all(not(test), target_os = "none"))]
fn sleep_one_tick() {
    let _ = minix_sys::nanosleep(1, 0);
}

/// C `(void) fflush(stdout)`（`forward.c:200`）：一拍的数据要在睡下去之前交出去。
/// 宿主的标准输出带行缓冲，没有这一口就会被扣住半行；真机侧 `emit` 直接 `write(2)`，
/// 没有用户态缓冲可刷。
#[cfg(any(test, not(target_os = "none")))]
fn flush_output() {
    use std::io::Write as _;
    let _ = std::io::stdout().flush();
}

#[cfg(all(not(test), target_os = "none"))]
fn flush_output() {}

fn run() -> ! {
    let argv: Vec<String> = support::args();
    // C 的每一句诊断都以 `getprogname()` 开头（`misc.c:66-91` 的 vwarn/vwarnx），
    // 所以下面不再写死「tail」。
    let prog = support::prog_name(&argv);
    let args: Vec<&str> = argv.iter().map(String::as_str).collect();
    // argv folding lives in the deciding half (`parse_tail_args`): BSD
    // `obsolete()` + `getopt("Fb:c:fn:r")`, with attached-or-next counts and
    // option clustering. The old inline whole-token matcher leaked attached
    // values (`tail -n10 f`, `tail -c100 f`) and unknown flags (`tail -x f`)
    // into the operand list, silently treating them as file names.
    let parsed = match parse_tail_args(&args[1..]) {
        Ok(parsed) => parsed,
        Err(e) => arg_error(e, prog),
    };
    let style = parsed.style;
    let follow = parsed.follow;
    let operands: Vec<String> = parsed.operands;

    // C `for each file: fopen+fstat (ierr on failure), banner when argc>1,
    // forward()`. No operands read stdin. The banner comes after the successful
    // open because C `continue`s past a file it cannot open without printing one.
    let show_banner = operands.len() > 1;
    let mut first = true;
    let mut status = 0;

    if operands.is_empty() {
        // C `tail.c:185-207`：`fname = "stdin"`、fstat 不成就 `ierr(); exit(1)`，
        // 然后探一次能不能定位——管道就把 `fflag` 清零（POSIX.2）。
        let source = FollowSource::stdin();
        let opened = match source.identity() {
            Ok(opened) => opened,
            Err(code) => {
                support::warn_line(prog, "stdin", code);
                support::terminate(1);
            }
        };
        let mut target = FollowTarget {
            source,
            opened,
            path: None,
        };
        let follow = follow.kept_for_stdin(target.source.is_pipe_by_position());
        let data = match target.source.collect() {
            Ok(data) => data,
            Err(code) => {
                support::warn_line(prog, target.label(), code);
                support::terminate(1);
            }
        };
        tail_stream(&data, style);
        if follow.active() && !follow_rounds(&mut target, follow, prog) {
            support::terminate(1);
        }
        support::terminate(0);
    }
    for operand in &operands {
        // C `tail.c:167-171`：`fopen` 失败**或** `fstat` 失败都是 `ierr(); continue`，
        // 那句诊断是 `tail: <名>: <句子>`（`misc.c:53-57` 走的是带 `errno` 的
        // `xerr`/`vwarn`，不是 `warnx`）。
        let source = match FollowSource::open(operand) {
            Ok(source) => source,
            Err(code) => {
                support::warn_line(prog, operand, code);
                status = 1;
                continue;
            }
        };
        let opened = match source.identity() {
            Ok(opened) => opened,
            Err(code) => {
                support::warn_line(prog, operand, code);
                status = 1;
                continue;
            }
        };
        let mut target = FollowTarget {
            source,
            opened,
            path: Some(operand),
        };
        let data = match target.source.collect() {
            Ok(data) => data,
            Err(code) => {
                support::warn_line(prog, operand, code);
                status = 1;
                continue;
            }
        };
        if show_banner {
            let sep = if first { "" } else { "\n" };
            support::emit(format!("{sep}==> {operand} <==\n").as_bytes());
        }
        first = false;
        tail_stream(&data, style);
        if follow.active() {
            // C 的 `forward()` 在跟随时永不返回，能回到这里只有 `ierr()` 那一条腿；
            // 后面的操作数早在 `tail.c:135-137` 被单文件门挡住了，所以收尾直接退。
            if !follow_rounds(&mut target, follow, prog) {
                status = 1;
            }
            break;
        }
    }
    support::terminate(status)
}

/// C `forward.c:191-274` 的 `for(;;)` 在 Minix 上剩下的那一支（kqueue 那半被
/// `#if !defined(__minix)` 排除在 `:207-246`）：睡一拍 →仅 `-F` 的真文件按名字查
/// 身份、认出换档就重开→把当前位置之后的字节边读边写。
///
/// 首拍的输出不在这里：调用方已经把整块读到并打了区域，位置也就到了 EOF；C 的环路
/// 是「先抄后睡」，所以这里从睡起算与真源同一序。
///
/// 返回 `false` 对位 C 的 `ierr(); return;` 与 `ierr(); goto out;`（`:196-199`、
/// `:262-265`）：非致命，但退出码已经变成 1。
fn follow_rounds(target: &mut FollowTarget, follow: Follow, prog: &str) -> bool {
    loop {
        sleep_one_tick();
        if follow.probes_identity(target.is_stdin()) {
            // 能走到这里 `path` 必是 `Some`：`probes_identity` 对标准输入恒为 false。
            // 不 `unwrap`，那个不可达状态不值得一个panic。
            if let Some(name) = target.path {
                match stat_by_path(name) {
                    // C 的 `stat(fname, &statbuf) != -1`：名字认不出来（换档后
                    // 被删掉）就这一拍什么都不做，下一拍继续睡——不报错也不退出。
                    Err(_) => {}
                    Ok(current) => {
                        if reopen_needed(&target.opened, &current) {
                            if let Err(code) = target.source.reopen(name) {
                                support::warn_line(prog, name, code);
                                return false;
                            }
                            // C 的 `*sbp = statbuf`：新档的身份接手，下一拍比的是它。
                            target.opened = current;
                        }
                    }
                }
            }
        }
        let pumped = target.source.pump();
        if let Err(code) = pumped {
            let label = target.label();
            support::warn_line(prog, label, code);
            return false;
        }
        flush_output();
    }
}

/// Emit the tail of one open descriptor for the resolved [`TailStyle`]. The
/// forward shapes print raw bytes from an offset (C `forward()`'s `getc` loop
/// copies what is left of the file, newlines and all, so no line is invented
/// and a trailing unterminated line stays unterminated); the reverse shapes
/// print whole lines bottom-up (C `reverse()`).
fn tail_stream(input: &[u8], style: TailStyle) {
    // C's zero arms come back with nothing written and a successful exit:
    // `forward.c` seeks to the end for `RLINES`/`RBYTES` with `off == 0`, and
    // `reverse.c` returns before touching the input for any other style with
    // `off == 0`. Only `REVERSE` (bare `-r`) prints the whole file.
    if style.selects_nothing() {
        return;
    }
    match style {
        TailStyle::SkipBytes { off } => support::emit(bytes_from(input, off)),
        TailStyle::SkipLines { off } => support::emit(bytes_after_lines(input, off)),
        TailStyle::LastBytes { off } => support::emit(last_bytes_of(input, off)),
        TailStyle::LastLines { off } => {
            let mut window = match TailWindow::new(off) {
                Ok(window) => window,
                Err(_) => support::terminate(1),
            };
            for line in support::lines_of(input) {
                window.push(line);
            }
            // Size the output by what the input actually offered, never by the
            // requested count: `tail -n 1000000` on ten lines holds ten.
            let want = (window.seen() as usize).min(off);
            let mut survivors: Vec<&str> = vec![""; want];
            let drained = window.drain(&mut survivors);
            for line in &survivors[..drained] {
                emit_line(line);
            }
        }
        TailStyle::ReverseAll => {
            for line in reversed_lines(&support::lines_of(input), None) {
                emit_line(line);
            }
        }
        // `off == 0` prints nothing, which is C's early return at
        // `reverse.c:79-80` (`style != REVERSE && off == 0`).
        TailStyle::ReverseLines { off } => {
            for line in reversed_lines(&support::lines_of(input), Some(off)) {
                emit_line(line);
            }
        }
        TailStyle::ReverseBytes { off } => {
            for line in reverse_bytes(input, off) {
                emit_line(line);
            }
        }
    }
}

fn emit_line(line: &str) {
    let mut out = line.as_bytes().to_vec();
    out.push(b'\n');
    support::emit(&out);
}

/// The line order C `reverse()` prints: `RLINES` (`Some(off)`) takes the last
/// `off` lines then reverses them (`reverse.c:138-141` stops the backward walk
/// after `off` lines); `REVERSE` (`None`) reverses every line. `Some(0)` selects
/// no line at all, matching C's early return.
fn reversed_lines<'a>(lines: &[&'a str], take: Option<usize>) -> Vec<&'a str> {
    let selected: &[&'a str] = match take {
        Some(off) => &lines[lines.len().saturating_sub(off)..],
        None => lines,
    };
    selected.iter().rev().copied().collect()
}

/// C `tail -r` with `-c`/`-b` (`RBYTES`, `reverse.c` `r_reg`): the lines of the
/// trailing `off`-byte window, printed bottom-up. The window can begin mid-line;
/// r_reg still emits that leading fragment (its leftover `WR`), which is
/// precisely what a byte-window line split yields, so the reverse is exact.
fn reverse_bytes(input: &[u8], off: usize) -> Vec<&str> {
    let window = last_bytes_of(input, off);
    let lines = support::lines_of(window);
    reversed_lines(&lines, None)
}

/// The last `off` bytes of `input`, or all of it when shorter — C `tail -c N`
/// prints the final N bytes of each file.
fn last_bytes_of(input: &[u8], off: usize) -> &[u8] {
    &input[input.len().saturating_sub(off)..]
}

/// C `forward()` + `FBYTES` (`forward.c:100-113`): print from byte `off + 1` on.
/// C clamps the seek to the file size for regular files and stops at EOF for
/// pipes, either way an offset past the end prints nothing.
fn bytes_from(input: &[u8], off: usize) -> &[u8] {
    &input[off.min(input.len())..]
}

/// C `forward()` + `FLINES` (`forward.c:115-127`): consume input through the
/// `off`-th newline, then print everything after it. `off == 0` prints the whole
/// input (that is `tail -n +1`), and an input with fewer than `off` newlines
/// leaves C at EOF, which prints nothing.
fn bytes_after_lines(input: &[u8], off: usize) -> &[u8] {
    if off == 0 {
        return input;
    }
    let mut seen = 0usize;
    for (index, byte) in input.iter().enumerate() {
        if *byte == b'\n' {
            seen += 1;
            if seen == off {
                return &input[index + 1..];
            }
        }
    }
    &[]
}

/// Map a [`TailError`] from `parse_tail_args` to its diagnostic and exit status,
/// then terminate.
fn arg_error(e: TailError, prog: &str) -> ! {
    match e {
        TailError::Usage => usage(prog),
        TailError::Getopt(report) => {
            // libc 的那一句（`getopt.c:101-104` 或 `:132-136`）在前，usage 行在后。
            // 句子单源住在 `GetoptReport`，本族其它命令拿到的是同一份文本。字母位走
            // `line_bytes`：`getopt.c:79` 的 `optopt = *place++` 一次进一个字节、`:103`
            // 与 `:134` 的 `%c` 原样写出，所以 `-é`（`c3 a9`）报的是 `c3`；取法在
            // `src/window.rs` 的环里已经是 `bytes[pos] as char`，两边同形才成立。
            support::warn(&report.line_bytes(prog));
            usage(prog)
        }
        TailError::FollowMultiFile => {
            // C `tail.c:135-137` 的 `xerrx(1, ...)`：`xerrx` 走 `vwarnx`
            // （`misc.c:78-89`），所以那一句后面**不缀 `errno` 句子**。
            support::warn(
                format!("{prog}: -f and -F options only appropriate for a single file\n")
                    .as_bytes(),
            );
            support::terminate(1);
        }
        TailError::IllegalCount(v) => {
            // C `xerrx(1, "illegal offset -- %s", optarg)` (`tail.c:90`).
            support::warn(format!("{prog}: illegal offset -- {v}\n").as_bytes());
            support::terminate(1);
        }
        TailError::IllegalOption(v) => {
            // C `obsolete()`'s fatal arm: `xerrx(1, "illegal option -- %s", *argv)`.
            support::warn(format!("{prog}: illegal option -- {v}\n").as_bytes());
            support::terminate(1);
        }
    }
}

/// C `tail.c:299-305`：那一行是大写 `Usage:`、带 `getprogname()`，而且选项列表里
/// 本来就有 `-f | -F`（本壳之前那行小写且只列 `-r` 的写法是自创的）。
fn usage(prog: &str) -> ! {
    support::warn(
        format!("Usage: {prog} [-f | -F | -r] [-b # | -c # | -n #] [file ...]\n").as_bytes(),
    );
    support::terminate(1);
}

#[cfg(all(not(test), target_os = "none"))]
#[unsafe(no_mangle)]
extern "Rust" fn main() -> i32 {
    run()
}

#[cfg(any(test, not(target_os = "none")))]
fn main() {
    run()
}

#[cfg(test)]
mod tests {
    use super::{bytes_after_lines, bytes_from, last_bytes_of, reverse_bytes, reversed_lines};

    #[test]
    fn last_bytes_truncates_to_count() {
        assert_eq!(last_bytes_of(b"abcdef", 3), b"def");
    }

    #[test]
    fn last_bytes_short_input_returns_all() {
        assert_eq!(last_bytes_of(b"ab", 10), b"ab");
    }

    #[test]
    fn last_bytes_of_empty_is_empty() {
        assert_eq!(last_bytes_of(b"", 5), b"");
    }

    #[test]
    fn bytes_from_skips_the_offset() {
        // `-c +3` is `off = 2` (`ARG` gives back one unit), i.e. byte three on.
        assert_eq!(bytes_from(b"abcdef", 2), b"cdef");
    }

    #[test]
    fn bytes_from_zero_is_whole_input() {
        // `-c +1` folds to `off = 0`: C breaks out of the `FBYTES` case without
        // seeking, so the whole file prints.
        assert_eq!(bytes_from(b"abcdef", 0), b"abcdef");
    }

    #[test]
    fn bytes_from_past_end_prints_nothing() {
        // C clamps the seek to `st_size` (`forward.c`), and a pipe stops at EOF —
        // either way nothing is left to print.
        assert_eq!(bytes_from(b"abc", 9), b"");
    }

    #[test]
    fn bytes_after_lines_lands_on_the_nth_line() {
        let input = b"one\ntwo\nthree\nfour\n";
        // `-n +1` → off 0 → whole file; `-n +2` → off 1 → past the first newline.
        assert_eq!(bytes_after_lines(input, 0), input);
        assert_eq!(bytes_after_lines(input, 1), b"two\nthree\nfour\n");
        assert_eq!(bytes_after_lines(input, 3), b"four\n");
    }

    #[test]
    fn bytes_after_lines_keeps_a_final_unterminated_line() {
        // C copies the remaining bytes character by character, so a last line
        // without a newline keeps its shape instead of gaining one.
        let input = b"a\nb world";
        assert_eq!(bytes_after_lines(input, 1), b"b world");
    }

    #[test]
    fn bytes_after_lines_short_input_prints_nothing() {
        // Fewer newlines than `off`: C's `getc` loop reaches EOF and prints
        // nothing.
        assert_eq!(bytes_after_lines(b"only\n", 5), b"");
    }

    #[test]
    fn reversed_lines_all_reverses_every_line() {
        let lines = ["1", "2", "3"];
        assert_eq!(reversed_lines(&lines, None), ["3", "2", "1"]);
    }

    #[test]
    fn reversed_lines_last_n_reversed() {
        // -r -n 2 on [1,2,3,4] -> last 2 (3,4) reversed -> 4,3.
        let lines = ["1", "2", "3", "4"];
        assert_eq!(reversed_lines(&lines, Some(2)), ["4", "3"]);
    }

    #[test]
    fn reversed_lines_count_exceeding_input_is_all() {
        let lines = ["a", "b"];
        assert_eq!(reversed_lines(&lines, Some(5)), ["b", "a"]);
    }

    #[test]
    fn reversed_lines_zero_takes_nothing() {
        // C `reverse.c:79-80`: a non-`REVERSE` style with `off == 0` returns
        // before printing, so `tail -r -n +1` prints nothing rather than the
        // whole file.
        let lines = ["a", "b"];
        assert!(reversed_lines(&lines, Some(0)).is_empty());
    }

    #[test]
    fn reverse_bytes_reverses_lines_of_trailing_window() {
        // -r -c on "aaa\nbbb\nccc\n": last 8 bytes = "bbb\nccc\n" -> ccc,bbb.
        let input = b"aaa\nbbb\nccc\n";
        assert_eq!(reverse_bytes(input, 8), ["ccc", "bbb"]);
        // window starts mid-line (last 7 = "bb\nccc\n"); r_reg emits the leading
        // fragment too, so "bb" survives as its own line.
        assert_eq!(reverse_bytes(input, 7), ["ccc", "bb"]);
        // byte offset past the input reverses the whole file's lines.
        assert_eq!(reverse_bytes(input, 99), ["ccc", "bbb", "aaa"]);
        // -r -c +1 folds to RBYTES with off 0, whose empty window holds no line.
        assert_eq!(reverse_bytes(input, 0), Vec::<&str>::new());
    }
}
