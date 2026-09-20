//! Minix-RS System Call Library.
//!
//! System call wrappers for user-space programs.
//!
//! The communication primitives ([`ipc`]) and the system call protocol
//! ([`syscall`]) are fully implemented over an explicit transport trait.
//! Service-group wrappers below (processes, files, memory mapping) belong to
//! later stage documents and still report explicit failures until their own
//! documents land.
//!
//! # Transport model
//!
//! The zero-argument functions in this file delegate to the direct-trap
//! transport: the only transport a real binary has. In a hosted test
//! environment no kernel answers, so they return explicit errors instead of
//! faulting. Test code that needs scripted replies uses [`ipc::CannedTransport`]
//! with [`syscall::perform_syscall`] directly.
//!
//! # Module ownership (E-MINSYS-SCOPE plan A)
//!
//! This crate is 14-stage-runtime's implementation crate; the domain-external
//! client modules below belong to other stages and are feature-gated (all
//! default-on, so existing dependents compile unchanged). When another stage
//! evolves its client protocol, its feature boundary contains the breakage.
//!
//! | Module           | Feature   | Owning stage doc                          | Related edge entry |
//! |------------------|-----------|--------------------------------------------|--------------------|
//! | `ipc` `syscall` `pm` `vfs` `vm` `misc` `stack` `arch_trap` `rs` `grant` | (core, always on) | 14-stage-runtime (docs 04-05, 08-12) | — |
//! | `ds`             | `ds`      | 07-stage-ds (libsys ds.c)                  | E-DSWIRE           |
//! | `rmib`           | `rmib`    | 10-stage-mib (22-mib-rmib-client.md)       | E-RMIBWIRE         |
//! | `devman_client`  | `devman`  | 11-stage-devman (10-libdevman-client.md)   | E-DMWIRE           |
//! | `usb_model`      | `usb`     | 11-stage-devman (11-usb-device-model.md)   | E-DMWIRE           |
//! | `inputdriver`    | `input`   | 12-stage-input (12-libinputdriver.md)      | E-INWIRE           |
//! | `socket`         | `socket`  | 17-stage-net (23-libc-socket.md)           | — (policy only)    |

#![no_std]

extern crate alloc;

use ipc::IpcTransport;
use minix_types::{Endpoint, IpcError, Message};

pub use minix_types::{Gid, Pid, Uid};
// 用户态 `struct stat` ABI 走显式路径再导出——minix-types 根上
// `ipc::fs_driver::Stat`（VTreeFS 序列化载荷）同名，glob 会歧义。
pub use minix_types::types::stat::Stat;

/// The errno constant table at the crate root — the position `<errno.h>`
/// holds for C programs (C: `minix3/sys/sys/errno.h`; command binaries
/// `use minix_sys::EEXIST` the way C code includes the header, per the
/// 18-stage dependency rule that commands build on `minix-sys` +
/// `minix-rt` alone). The names come from the single authority in
/// `minix-types` (`types/errno.rs`); this crate adds no constants and
/// never forks a value.
pub use minix_types::types::errno::*;

/// The termios table at the crate root — the position `<termios.h>` holds
/// for C programs (C: `minix3/sys/sys/termios.h`; command binaries like
/// `stty` `use minix_sys::ECHO` the way C code includes the header, per
/// the same 18-stage layering rule that keeps `minix-types` off command
/// imports — edge E-CMDSYSFACE precedent, defect batch X-1). The names
/// come from the single authority in `minix-types` (`types/termios.rs`);
/// this crate adds no constants and never forks a value.
pub use minix_types::types::termios::*;

/// Inter-process communication primitives (document 04).
pub mod ipc;
/// System call protocol above send-and-receive (document 05).
pub mod syscall;
/// Process manager call group (document 08).
pub mod pm;
/// File system call group (document 09).
pub mod vfs;
/// Working-directory traversal behind `getcwd` (C
/// `minix3/minix/lib/libc/sys/__getcwd.c`).
pub mod getcwd;
/// Virtual memory call group (document 10).
pub mod vm;
/// Miscellaneous calls: sleeping, server control, clock reads (document 11).
pub mod misc;
/// Initial-stack frame construction (E-BOOTFRAME — `minix_stack_params`/
/// `minix_stack_fill`, libc stack_utils.c).
pub mod stack;
pub mod tty;
/// User-side trap bodies (E1 slice 3 — the `int 0x21`/`syscall`
/// instruction sequences behind the direct transports).
pub mod arch_trap;
/// Reincarnation server queries: lookup and endpoint questions (document 12).
pub mod rs;
/// User-space socket call policy: call list, flag handling, fallback rule
/// (17-stage-net/23-libc-socket.md). Traps stay with the callers; the legacy
/// device fallback is documented but never taken ([ARCH] N-2).
#[cfg(feature = "socket")]
pub mod socket;
/// User-space grant table (C libsys safecopies.c) — the transport half for
/// every granting client (E-DSWIRE: DS first; devman/RS/VM clients follow).
pub mod grant;
/// Data Store client (C libsys ds.c, E-DSWIRE transport half).
#[cfg(feature = "ds")]
pub mod ds;

/// devman client library: driver-side registration + bind handling
/// (11-stage-devman/10-libdevman-client.md).
#[cfg(feature = "devman")]
pub mod devman_client;
/// Input-driver client library: driver-side announce, event filing, and
/// server-message handling (12-stage-input/12-libinputdriver.md).
/// Transport (label lookup, publish, blocking send) stays out until the
/// IPC transport lands — same boundary as `devman_client`.
#[cfg(feature = "input")]
pub mod inputdriver;
/// Remote MIB client: pure bookkeeping for mounted subtrees
/// (10-stage-mib/22-mib-rmib-client.md). Message sending and grant
/// handling stay out until the IPC transport lands.
#[cfg(feature = "rmib")]
pub mod rmib;
/// USB device modeling over the devman client
/// (11-stage-devman/11-usb-device-model.md).
#[cfg(feature = "usb")]
pub mod usb_model;

pub mod wait;

/// File descriptor.
pub type Fd = i32;

// ── IPC system calls ──
//
// Each function delegates to the direct-trap transport (see `ipc`). The
// transport reports an explicit failure status where no kernel answers;
// the mapping to the public error type lives in one place
// (`ipc::trap_status_to_ipc_error`).

/// Sends a message (blocking).
pub fn send(dest: Endpoint, msg: &Message) -> Result<(), IpcError> {
    ipc::DirectTrapTransport
        .send(dest, msg)
        .map_err(ipc::trap_status_to_ipc_error)
}

/// Receives a message (blocking).
pub fn receive(src: Endpoint, msg: &mut Message) -> Result<(), IpcError> {
    ipc::DirectTrapTransport
        .receive(src, msg)
        .map(|_| ())
        .map_err(ipc::trap_status_to_ipc_error)
}

/// Sends and receives (synchronous call).
pub fn sendrec(dest: Endpoint, msg: &mut Message) -> Result<(), IpcError> {
    // The C library implements send-and-receive as one trap (not as
    // send-then-receive); keep the single-trap shape here as well.
    ipc::DirectTrapTransport
        .sendrec(dest, msg)
        .map_err(ipc::trap_status_to_ipc_error)
}

/// Sends a notification.
pub fn notify(dest: Endpoint, type_: minix_types::NotifyType) -> Result<(), IpcError> {
    let _ = type_;
    ipc::DirectTrapTransport
        .notify(dest)
        .map_err(ipc::trap_status_to_ipc_error)
}

// ── Process system calls ──
//
// Each function delegates to the matching `pm` wrapper over the
// direct-trap transport (see `pm`). Execution preparation (`exec`) takes a
// caller-prepared request: the stack image is built with the 01-stage size
// computation plus the 06-stage allocator, which live in `minix-rt` (this
// crate cannot depend on it without a dependency cycle).

/// Creates a child process.
pub fn fork() -> Result<Pid, Errno> {
    pm::fork_via(&ipc::DirectTrapTransport)
}

/// Executes a prepared program image.
///
/// The request is prepared by the caller (see `pm::prepare_exec`); a
/// successful execution never returns, so the result is always the failure
/// that came back.
pub fn exec(prepared: pm::PreparedExec) -> Errno {
    pm::exec_via(&ipc::DirectTrapTransport, prepared)
}

/// Process exit.
pub fn exit(status: i32) -> ! {
    pm::exit_via(&ipc::DirectTrapTransport, status)
}

/// Returns from a signal handler into the interrupted context (C:
/// `sigreturn`, `minix3/minix/lib/libc/sys/sigreturn.c:18-36`).
///
/// `ctx` is the sigcontext address exactly as the signal-delivery frame
/// carries it (C: `scp`); no frame layout knowledge lives on this side —
/// the kernel reads registers and mask out of that memory itself
/// (`do_sigreturn`, pm/signal.c:189).
///
/// Before the call, every signal is blocked (C sigreturn.c:27-29): the
/// window between the handler returning and the context being restored is
/// the one moment a re-entering signal could corrupt the handoff. A
/// successful round trip never comes back here — the kernel swaps the
/// calling context out from under the reply — so any reachable return
/// means the restoration did not happen; like [`exit`], the function then
/// parks instead of unwinding into a signal frame that is no longer valid.
pub fn sigreturn(ctx: u64) -> ! {
    let _ = pm::sigprocmask_via(&ipc::DirectTrapTransport, pm::SIG_SETMASK, Some(&[u32::MAX; 4]));
    let _ = pm::sigreturn_via(&ipc::DirectTrapTransport, ctx);
    loop {
        core::hint::spin_loop();
    }
}

/// Waits for a child process, reporting how it ended in `status`.
pub fn waitpid(pid: Pid, status: &mut i32, options: i32) -> Result<Pid, Errno> {
    let (child, child_status) =
        pm::waitpid_via(&ipc::DirectTrapTransport, pid, options, 0)?;
    *status = child_status;
    Ok(child)
}

/// Sends a signal.
pub fn kill(pid: Pid, sig: i32) -> Result<(), Errno> {
    pm::kill_via(&ipc::DirectTrapTransport, pid, sig)
}

// ── File system calls ──
//
// Each function delegates to the matching `vfs` wrapper over the
// direct-trap transport (see `vfs`). Buffer addresses come from the caller:
// user programs pass their own buffers, whose addresses the server reads
// through granted memory.

/// Opens a file.
///
/// Opens a path, dispatching on the create flag (see `vfs::dispatch_open`):
/// the create and open-existing paths carry their own wire layouts, and the
/// inline-capable open-existing path answers with the new descriptor.
pub fn open(path: &str, flags: i32, mode: u32) -> Result<Fd, Errno> {
    vfs::open_via(
        &ipc::DirectTrapTransport,
        path.as_ptr() as u64,
        path.len().saturating_add(1),
        flags,
        mode,
    )
}

/// Creates a pipe, returning its `(read end, write end)` descriptor pair.
///
/// C: `pipe(fild[2])` is `pipe2(fild, 0)`
/// (`minix3/minix/lib/libc/sys/pipe.c:30-33`).
pub fn pipe() -> Result<(Fd, Fd), Errno> {
    vfs::pipe2_via(&ipc::DirectTrapTransport, 0)
}

/// Creates a pipe with `O_*` flags (e.g. `O_CLOEXEC`, `O_NONBLOCK`).
///
/// C: `pipe2(fild[2], flags)` (`minix3/minix/lib/libc/sys/pipe.c:14-27`);
/// the request goes to VFS as `VFS_PIPE2` and the flags are checked there.
pub fn pipe2(flags: i32) -> Result<(Fd, Fd), Errno> {
    vfs::pipe2_via(&ipc::DirectTrapTransport, flags)
}

// 文件操作族（NL4 高优先批）：路径参数一律 `&str`，上线时按 C 惯例
// 携带 NUL —— 地址 = 首字节指针，长度 = `len + 1`（strlen+1 对位的
// wire 语义，见 `vfs::path_request_via` 等处的 C 包装器锚点）。

/// Deletes a directory entry (C: `unlink`).
pub fn unlink(path: &str) -> Result<(), Errno> {
    vfs::unlink_via(
        &ipc::DirectTrapTransport,
        path.as_ptr() as u64,
        path.len().saturating_add(1),
    )
}

/// Removes an empty directory (C: `rmdir`).
pub fn rmdir(path: &str) -> Result<(), Errno> {
    vfs::rmdir_via(
        &ipc::DirectTrapTransport,
        path.as_ptr() as u64,
        path.len().saturating_add(1),
    )
}

/// Changes the working directory (C: `chdir`).
pub fn chdir(path: &str) -> Result<(), Errno> {
    vfs::chdir_via(
        &ipc::DirectTrapTransport,
        path.as_ptr() as u64,
        path.len().saturating_add(1),
    )
}

/// Changes the process root directory (C: `chroot`).
pub fn chroot(path: &str) -> Result<(), Errno> {
    vfs::chroot_via(
        &ipc::DirectTrapTransport,
        path.as_ptr() as u64,
        path.len().saturating_add(1),
    )
}

/// Creates a directory; the umask narrows the permission bits
/// server-side (C: `mkdir`).
pub fn mkdir(path: &str, mode: u32) -> Result<(), Errno> {
    vfs::mkdir_via(
        &ipc::DirectTrapTransport,
        path.as_ptr() as u64,
        path.len().saturating_add(1),
        mode,
    )
}

/// Sets a path's permission bits (C: `chmod`).
pub fn chmod(path: &str, mode: u32) -> Result<(), Errno> {
    vfs::chmod_via(
        &ipc::DirectTrapTransport,
        path.as_ptr() as u64,
        path.len().saturating_add(1),
        mode,
    )
}

/// Checks real-uid access bits (`R_OK|W_OK|X_OK` or `F_OK`) without
/// opening the file (C: `access`).
pub fn access(path: &str, amode: i32) -> Result<(), Errno> {
    vfs::access_via(
        &ipc::DirectTrapTransport,
        path.as_ptr() as u64,
        path.len().saturating_add(1),
        amode,
    )
}

/// Creates a hard link from `old` to `new` (C: `link`).
pub fn link(old: &str, new: &str) -> Result<(), Errno> {
    vfs::link_via(
        &ipc::DirectTrapTransport,
        old.as_ptr() as u64,
        old.len().saturating_add(1),
        new.as_ptr() as u64,
        new.len().saturating_add(1),
    )
}

/// Creates a symbolic link at `linkpath` pointing at `target` (C:
/// `symlink`).
pub fn symlink(target: &str, linkpath: &str) -> Result<(), Errno> {
    vfs::symlink_via(
        &ipc::DirectTrapTransport,
        target.as_ptr() as u64,
        target.len().saturating_add(1),
        linkpath.as_ptr() as u64,
        linkpath.len().saturating_add(1),
    )
}

/// Moves `old` to `new` (C: `rename`).
pub fn rename(old: &str, new: &str) -> Result<(), Errno> {
    vfs::rename_via(
        &ipc::DirectTrapTransport,
        old.as_ptr() as u64,
        old.len().saturating_add(1),
        new.as_ptr() as u64,
        new.len().saturating_add(1),
    )
}

/// Sets a path's owner and group (C: `chown`).
pub fn chown(path: &str, owner: u32, group: u32) -> Result<(), Errno> {
    vfs::chown_via(
        &ipc::DirectTrapTransport,
        path.as_ptr() as u64,
        path.len().saturating_add(1),
        owner,
        group,
    )
}

/// Sets a path's length (C: `truncate`).
pub fn truncate(path: &str, length: i64) -> Result<(), Errno> {
    vfs::truncate_via(
        &ipc::DirectTrapTransport,
        path.as_ptr() as u64,
        path.len().saturating_add(1),
        length,
    )
}

/// Sets an open descriptor's length (C: `ftruncate`).
pub fn ftruncate(fd: Fd, length: i64) -> Result<(), Errno> {
    vfs::ftruncate_via(&ipc::DirectTrapTransport, fd, length)
}

/// Changes the working directory to an open descriptor's (C: `fchdir`).
pub fn fchdir(fd: Fd) -> Result<(), Errno> {
    vfs::fchdir_via(&ipc::DirectTrapTransport, fd)
}

/// Sets the file-creation mask and returns the previous one (C: `umask`).
pub fn umask(mask: u32) -> Result<u32, Errno> {
    vfs::umask_via(&ipc::DirectTrapTransport, mask)
}

/// Reads a symlink target into the caller's buffer, returning the byte
/// count written (C: `readlink`; the NUL terminator is *not* added, as in
/// C).
pub fn readlink(path: &str, buf: &mut [u8]) -> Result<usize, Errno> {
    vfs::readlink_via(
        &ipc::DirectTrapTransport,
        path.as_ptr() as u64,
        path.len().saturating_add(1),
        buf.as_mut_ptr() as u64,
        buf.len(),
    )
}

/// Creates a special file node; `mode` carries the file-type bits
/// (C: `mknod`).
pub fn mknod(path: &str, mode: u32, device: u64) -> Result<(), Errno> {
    vfs::mknod_via(
        &ipc::DirectTrapTransport,
        path.as_ptr() as u64,
        path.len().saturating_add(1),
        mode,
        device,
    )
}

/// Creates a FIFO (C: `mkfifo` = `mknod(name, mode | S_IFIFO, 0)`,
/// `minix3/minix/lib/libc/sys/mkfifo.c:8-11`).
pub fn mkfifo(path: &str, mode: u32) -> Result<(), Errno> {
    mknod(path, mode | vfs::S_IFIFO, 0)
}

// ── Process identity and scheduling ──

/// Returns the process id of the caller (C: `getpid`,
/// `minix3/minix/lib/libc/sys/getpid.c:12-19`; the module-only
/// `pm::getpid_via` topled per the C-44 leftover list).
pub fn getpid() -> Result<Pid, Errno> {
    pm::getpid_via(&ipc::DirectTrapTransport)
}

/// Returns the parent process id of the caller (C: `getppid`,
/// `minix3/minix/lib/libc/sys/getppid.c:12-19`).
pub fn getppid() -> Result<Pid, Errno> {
    pm::getppid_via(&ipc::DirectTrapTransport)
}

/// Returns the real user id of the caller (C: `getuid`; the same
/// `PM_GETUID` round trip as [`geteuid`], which reads the other lane —
/// see `pm::getuid_via`).
pub fn getuid() -> Result<i32, Errno> {
    Ok(pm::getuid_via(&ipc::DirectTrapTransport)?.0)
}

/// Returns the effective user id of the caller (C: `geteuid`).
pub fn geteuid() -> Result<i32, Errno> {
    Ok(pm::getuid_via(&ipc::DirectTrapTransport)?.1)
}

/// Sets the real user id of the caller (C: `setuid`, over
/// `PM_SETUID` — see `pm::setuid_via`).
pub fn setuid(uid: u32) -> Result<(), Errno> {
    pm::setuid_via(&ipc::DirectTrapTransport, uid)
}

/// Detaches the caller from its controlling terminal into a new session
/// and returns the new session id (C: `setsid`).
pub fn setsid() -> Result<Pid, Errno> {
    pm::setsid_via(&ipc::DirectTrapTransport)
}

/// Reads the scheduling nice value of `(which, who)` (C: `getpriority`,
/// `minix3/minix/lib/libc/sys/priority.c:16-31`; the `PRIO_MIN`
/// correction of the raw PM reply happens inside `pm::getpriority_via`).
pub fn getpriority(which: i32, who: i32) -> Result<i32, Errno> {
    pm::getpriority_via(&ipc::DirectTrapTransport, which, who)
}

/// Sets the scheduling nice value of `(which, who)` (C: `setpriority`).
pub fn setpriority(which: i32, who: i32, prio: i32) -> Result<(), Errno> {
    pm::setpriority_via(&ipc::DirectTrapTransport, which, who, prio)
}

// ── Signals (mask face; handler installation stays on the `pm` face) ──

/// Reads the pending signal set of the caller (C: `sigpending`; the
/// four 32-bit words carry 128 signal slots, matching the C
/// `sigset_t` bit layout).
pub fn sigpending() -> Result<[u32; 4], Errno> {
    pm::sigpending_via(&ipc::DirectTrapTransport)
}

/// Examines or changes the signal mask and returns the previous mask
/// (C: `sigprocmask`; `how` takes `SIG_BLOCK`/`SIG_UNBLOCK`/
/// `SIG_SETMASK`, `set = None` inquires without writing).
pub fn sigprocmask(how: i32, set: Option<&[u32; 4]>) -> Result<[u32; 4], Errno> {
    pm::sigprocmask_via(&ipc::DirectTrapTransport, how, set)
}

/// Sends `signal` to the caller itself (C: `raise` — implemented as
/// `kill(getpid(), sig)` exactly like the C composition; invalid
/// signal numbers are rejected without a round trip).
pub fn raise(signal: i32) -> Result<(), Errno> {
    pm::raise_via(&ipc::DirectTrapTransport, signal)
}

// ── Descriptor and offset face ──

/// Duplicates `fd` onto `fd2`, closing any previous `fd2` (C: `dup2`).
pub fn dup2(fd: Fd, fd2: Fd) -> Result<Fd, Errno> {
    vfs::dup2_via(&ipc::DirectTrapTransport, fd, fd2)
}

/// Repositions `fd` and returns the new offset (C: `lseek`; `whence`
/// is the server-interpreted `SEEK_SET`/`SEEK_CUR`/`SEEK_END`).
pub fn lseek(fd: Fd, offset: i64, whence: i32) -> Result<i64, Errno> {
    vfs::lseek_via(&ipc::DirectTrapTransport, fd, offset, whence)
}

// ── Time face ──

/// Sleeps for the requested duration (C: `nanosleep`; the C
/// `remain` output is not carried — measure with wall-clock snapshots
/// as [`misc::remaining_sleep`] prescribes). A sleep interrupted by a
/// delivered signal answers `EINTR`.
pub fn nanosleep(seconds: i64, nanoseconds: i64) -> Result<(), Errno> {
    misc::nanosleep_via(
        &ipc::DirectTrapTransport,
        Some(misc::SleepRequest {
            seconds,
            nanoseconds,
        }),
    )
}

/// Flushes every mounted file system's dirty state to its device
/// (C: `sync`, `minix3/minix/lib/libc/sys/sync.c:12-17` — the C
/// wrapper discards the return value, so this cannot fail).
pub fn sync() {
    let _ = vfs::sync_via(&ipc::DirectTrapTransport);
}

/// A `(seconds, nanoseconds)` pair in the C `struct timespec` role;
/// the nanosecond lane may carry [`vfs::UTIME_NOW`] or
/// [`vfs::UTIME_OMIT`].
pub type TimeSpec = (i64, i64);

/// The C default when the `utimes`/`futimens` `tv` argument is NULL:
/// both stamps become the current time.
const NOW_TIMESPEC: [TimeSpec; 2] = [(0, vfs::UTIME_NOW), (0, vfs::UTIME_NOW)];

/// Sets a path's access and modification times (C: `utimensat`,
/// `minix3/minix/lib/libc/sys/utimensat.c`). `dirfd` must be
/// [`vfs::AT_FDCWD`] for relative paths; `times = None` means
/// "set both to the current time" (the C default array).
pub fn utimensat(
    dirfd: i32,
    path: &str,
    times: Option<[TimeSpec; 2]>,
    flags: i32,
) -> Result<(), Errno> {
    let [atime, mtime] = times.unwrap_or(NOW_TIMESPEC);
    vfs::utimensat_via(
        &ipc::DirectTrapTransport,
        dirfd,
        path.as_ptr() as u64,
        path.len().saturating_add(1),
        atime.0,
        atime.1,
        mtime.0,
        mtime.1,
        flags,
    )
}

/// Sets an open file's access and modification times (C: `futimens`,
/// `minix3/minix/lib/libc/sys/futimens.c`; `times = None` means "now"
/// per the C default array).
pub fn futimens(fd: Fd, times: Option<[TimeSpec; 2]>) -> Result<(), Errno> {
    let [atime, mtime] = times.unwrap_or(NOW_TIMESPEC);
    vfs::futimens_via(
        &ipc::DirectTrapTransport,
        fd,
        atime.0,
        atime.1,
        mtime.0,
        mtime.1,
    )
}

/// Sets a path's timestamps from microsecond pairs (C: `utimes`,
/// `minix3/minix/lib/libc/sys/utimes.c:9-35` — the C conversion is
/// `usec * 1000`, and `tv = None` answers "set both to now").
pub fn utimes(path: &str, times: Option<[(i64, i64); 2]>) -> Result<(), Errno> {
    let converted = times.map(|[a, b]| [(a.0, a.1.saturating_mul(1000)), (b.0, b.1.saturating_mul(1000))]);
    utimensat(vfs::AT_FDCWD, path, converted, 0)
}

/// Sets an open file's timestamps from microsecond pairs (C:
/// `futimes`, `minix3/minix/lib/libc/sys/futimes.c` — same
/// microsecond-to-nanosecond conversion as [`utimes`]).
pub fn futimes(fd: Fd, times: Option<[(i64, i64); 2]>) -> Result<(), Errno> {
    let converted = times.map(|[a, b]| [(a.0, a.1.saturating_mul(1000)), (b.0, b.1.saturating_mul(1000))]);
    futimens(fd, converted)
}

/// Sets a path's timestamps by name (C: `utimens`,
/// `minix3/lib/libc/gen/utimens.c` — composition
/// `utimensat(AT_FDCWD, path, times, 0)`).
pub fn utimens(path: &str, times: Option<[TimeSpec; 2]>) -> Result<(), Errno> {
    utimensat(vfs::AT_FDCWD, path, times, 0)
}

// ── Terminal face ──

/// Reports whether `fd` refers to a terminal (C: `isatty`,
/// `minix3/lib/libc/gen/isatty.c:44-48` — pure composition: a
/// successful `tcgetattr` means tty, every failure means not).
pub fn isatty(fd: Fd) -> bool {
    let mut termios = minix_types::types::termios::Termios::default();
    tcgetattr(fd, &mut termios).is_ok()
}


/// Terminal ioctl request: get the `struct termios`
/// (`ttycom.h:88`, `_IOR('t', 19, struct termios)` — `IOC_OUT` 0x4000_0000,
/// 44-byte argument in the length field, group `'t'`, number 19).
pub const TIOCGETA: u64 = 0x4000_0000 | (44 << 16) | (0x74 << 8) | 19;

/// Terminal ioctl request: set the `struct termios` immediately
/// (`ttycom.h:89`, `_IOW('t', 20, struct termios)` — `IOC_IN` 0x8000_0000).
pub const TIOCSETA: u64 = 0x8000_0000 | (44 << 16) | (0x74 << 8) | 20;

/// Fetches the terminal attributes of `fd` (`termios.h` `tcgetattr`
/// face over `TIOCGETA`).
pub fn tcgetattr(fd: Fd, termios: &mut minix_types::types::termios::Termios) -> Result<(), Errno> {
    let mut bytes = [0u8; 44];
    let result = vfs::ioctl_via(
        &ipc::DirectTrapTransport,
        fd,
        TIOCGETA,
        bytes.as_mut_ptr() as u64,
    );
    match result {
        Ok(_) => match minix_types::types::termios::Termios::from_bytes(&bytes) {
            Some(parsed) => {
                *termios = parsed;
                Ok(())
            }
            None => Err(Errno::EINVAL),
        },
        Err(e) => Err(e),
    }
}

/// Applies the terminal attributes to `fd` immediately (`termios.h`
/// `tcsetattr(fd, TCSANOW, …)` face over `TIOCSETA`).
pub fn tcsetattr(
    fd: Fd,
    termios: &minix_types::types::termios::Termios,
) -> Result<(), Errno> {
    let mut bytes = [0u8; 44];
    if termios.to_bytes(&mut bytes).is_none() {
        return Err(Errno::EINVAL);
    }
    vfs::ioctl_via(
        &ipc::DirectTrapTransport,
        fd,
        TIOCSETA,
        bytes.as_ptr() as u64,
    )
    .map(|_| ())
}

/// Closes a file.
pub fn close(fd: Fd) -> Result<(), Errno> {
    vfs::close_via(&ipc::DirectTrapTransport, fd)
}

/// Reads from a file into the caller's buffer.
pub fn read(fd: Fd, buf: &mut [u8]) -> Result<usize, Errno> {
    vfs::read_via(
        &ipc::DirectTrapTransport,
        fd,
        buf.as_mut_ptr() as u64,
        buf.len(),
    )
}

/// Writes the caller's buffer to a file.
pub fn write(fd: Fd, buf: &[u8]) -> Result<usize, Errno> {
    vfs::write_via(
        &ipc::DirectTrapTransport,
        fd,
        buf.as_ptr() as u64,
        buf.len(),
    )
}

/// Retrieves file status by path, following symlinks (C: `stat`).
///
/// The path length travels NUL-inclusive, matching the C convention; the
/// server fills `buf` through a grant, so the reply is just the verdict.
pub fn stat(path: &str, buf: &mut Stat) -> Result<(), Errno> {
    vfs::stat_via(
        &ipc::DirectTrapTransport,
        path.as_ptr() as u64,
        path.len().saturating_add(1),
        buf as *mut Stat as u64,
    )
}

/// Retrieves file status by path without following the final symlink
/// (C: `lstat`).
pub fn lstat(path: &str, buf: &mut Stat) -> Result<(), Errno> {
    vfs::lstat_via(
        &ipc::DirectTrapTransport,
        path.as_ptr() as u64,
        path.len().saturating_add(1),
        buf as *mut Stat as u64,
    )
}

/// Retrieves file status for an open descriptor (C: `fstat`).
pub fn fstat(fd: Fd, buf: &mut Stat) -> Result<(), Errno> {
    vfs::fstat_via(&ipc::DirectTrapTransport, fd, buf as *mut Stat as u64)
}

/// Runs a device-specific control request (C: `ioctl`).
///
/// The argument is raw: the request number alone defines whether it is
/// interpreted, and as what shape.
pub fn ioctl(fd: Fd, request: u64, argument: u64) -> Result<i32, Errno> {
    vfs::ioctl_via(&ipc::DirectTrapTransport, fd, request, argument)
}

/// Runs a file control command with an integer argument (C: `fcntl`).
///
/// The reply is the command's result — a new descriptor for `F_DUPFD`, the
/// flag word for the get forms, zero otherwise. The pointer-argument
/// commands (`F_GETLK` family) have no root face yet; their consumers
/// appear with the record-locking callers.
pub fn fcntl(fd: Fd, command: i32, argument: i32) -> Result<i32, Errno> {
    vfs::fcntl_via(&ipc::DirectTrapTransport, fd, command, argument, 0)
}

/// Reads directory entries into the caller's buffer (C: `getdents`).
///
/// The reply is the number of bytes the server wrote; zero marks the end
/// of the directory.
pub fn getdents(fd: Fd, buf: &mut [u8]) -> Result<usize, Errno> {
    vfs::getdents_via(
        &ipc::DirectTrapTransport,
        fd,
        buf.as_mut_ptr() as u64,
        buf.len(),
    )
}

/// Returns the working directory path in the caller's buffer.
///
/// C: `getcwd(buf, size)` — answered in user space by climbing parent
/// directories (mechanism and C source in the [`getcwd`] module header).
/// On success the buffer holds the NUL-terminated absolute path at its
/// front and the returned slice is the path without the terminator; a
/// buffer of one byte or less is `EINVAL`, a path that does not fit is
/// `ERANGE`, and the working directory is unchanged on every exit.
pub fn getcwd(buffer: &mut [u8]) -> Result<&[u8], Errno> {
    getcwd::getcwd_via(&ipc::DirectTrapTransport, buffer)
}

/// Memory mapping.
///
/// Maps memory for the caller itself (the third-party flag stays clear).
/// A failed call is an `Err` carrying the errno; C's mapped-failed sentinel
/// never crosses this interface.
pub fn mmap(
    addr: *mut u8,
    len: usize,
    prot: i32,
    flags: i32,
    fd: Fd,
    offset: i64,
) -> Result<*mut u8, Errno> {
    let request = vm::MapRequest {
        beneficiary: Endpoint(0),
        address: minix_types::VirBytes(addr as u64),
        length: minix_types::VirBytes(len as u64),
        protection: prot as u32,
        flags: flags as u32,
        file: fd,
        offset,
    };
    // The caller endpoint is unknown inside this shim; self-mapping is the
    // overwhelmingly common case and matches the C `mmap` wrapper, which
    // always passes SELF. Third-party mappings use `vm::mmap_via` directly
    // with an explicit beneficiary.
    let placed = vm::mmap_via(&ipc::DirectTrapTransport, Endpoint(0), request)?;
    Ok(placed.0 as *mut u8)
}

/// Error number — single shared ABI type.
///
/// Re-exported from `minix-types` (the Redox `redox_syscall::Error` pattern:
/// one shared error type across the ABI boundary). The local enum previously
/// duplicated `minix_types::Errno` with a different naming style (`Eperm`)
/// and **lacked the RS-required values** `ENOSYS`/`EDEADEPT`/`EDONTREPLY`/
/// `EGENERIC`/`ERESTART` (errno.h:78/211/199/200/196); two Errno types force
/// a conversion at every `KernelApi` boundary (todo §11 N4).
pub use minix_types::Errno;

/// Signal number constants (Minix3 `<sys/signal.h>` numbering, e.g.
/// SIGUSR1 = 30 — the value Linux x86 numbers SIGUSR1 differently).
/// Re-exported so command crates read signums from one authority
/// without depending on `minix-types` directly (edge E-INITSYS ④).
pub use minix_types::signal;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_errno_values() {
        assert_eq!(Errno::EPERM.to_i32(), 1);
        assert_eq!(Errno::EINVAL.to_i32(), 22);
        // The RS-required values the old enum lacked must be reachable from
        // the shared type (todo §11 N4).
        assert_eq!(Errno::ENOSYS.to_i32(), 78);
    }
}

/// `TIOCGETA`/`TIOCSETA` 请求号按 `ttycom.h:88-89` 的 `_IOR/_IOW('t', …,
/// struct termios)` 编码钉值：44 字节参数长度进第 16 到 27 位，组 `'t'`
/// 在第 8 到 15 位，方向位（`IOC_OUT`/`IOC_IN`）在最高两位。
#[test]
fn test_tty_ioctl_requests_match_ttycom() {
    assert_eq!(TIOCGETA, 0x4000_0000 | (44 << 16) | (0x74 << 8) | 19);
    assert_eq!(TIOCSETA, 0x8000_0000 | (44 << 16) | (0x74 << 8) | 20);
    // 长度域：IOCPARM_LEN 语义 ((x >> 16) & 0x1fff) == sizeof(termios)。
    assert_eq!((TIOCGETA >> 16) & 0x1fff, 44);
    // 组与序号。
    assert_eq!((TIOCGETA >> 8) & 0xff, b't' as u64);
    assert_eq!(TIOCGETA & 0xff, 19);
    assert_eq!(TIOCSETA & 0xff, 20);
}
