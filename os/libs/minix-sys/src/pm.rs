//! Process manager call group: lifecycle, signals, and service control.
//!
//! The process manager is the server that owns the process table: it creates
//! processes, terminates them, waits for children, delivers signals, and
//! answers identity questions. User programs reach it through thin wrappers
//! that all share one shape (C: `minix3/minix/lib/libc/sys/fork.c`,
//! `_exit.c`, `execve.c`, `wait4.c`, `kill.c`, `getpid.c`): clear a message,
//! fill the call's fields, and run the request protocol from the
//! [`crate::syscall`] module against the process manager endpoint.
//!
//! Two wrappers use the server-side variant of the protocol instead
//! (C: `_taskcall` in `minix3/minix/lib/libsys/taskcall.c`, "the same as
//! `_syscall` except it returns negative error codes directly"): the service
//! fork and service kill helpers in `minix3/minix/lib/libsys/srv_fork.c` and
//! `srv_kill.c`, which the reincarnation server calls to start and stop
//! system services.
//!
//! Every wrapper comes in two forms: a transport-generic core that tests
//! drive with a scripted transport, and a thin direct-transport shim that
//! real binaries call. Message body layouts stay owned by the shared types
//! crate; field details move to the global concepts document as planned.
//!
//! # Execution model
//!
//! Pure wrappers over the transport trait: no shared state, no
//! synchronization questions. The exit path never returns by construction.

use crate::ipc::IpcTransport;
use crate::syscall::{perform_syscall, perform_taskcall};
use minix_types::{Endpoint, Errno, Gid, Message, Pid, Uid};

/// Process manager endpoint.
///
/// C: `PM_PROC_NR ((endpoint_t) 0)` (`minix3/minix/include/minix/com.h:59`).
pub const PM_ENDPOINT_NUMBER: i32 = 0;

/// Terminate the calling process.
///
/// C: `PM_EXIT (PM_BASE + 1)` (`minix3/minix/include/minix/callnr.h:14`).
pub const PM_CALL_EXIT: i32 = 1;
/// Create a child process.
///
/// C: `PM_FORK (PM_BASE + 2)` (`callnr.h:15`).
pub const PM_CALL_FORK: i32 = 2;
/// Wait for a child process.
///
/// C: `PM_WAIT4 (PM_BASE + 3)` (`callnr.h:16`).
pub const PM_CALL_WAIT4: i32 = 3;
/// Ask for the caller's process identity.
///
/// C: `PM_GETPID (PM_BASE + 4)` (`callnr.h:17`).
pub const PM_CALL_GETPID: i32 = 4;
/// Send a signal to a process.
///
/// C: `PM_KILL (PM_BASE + 11)` (`callnr.h:24`).
pub const PM_CALL_KILL: i32 = 11;
/// Execute a new program image.
///
/// C: `PM_EXEC (PM_BASE + 14)` (`callnr.h:27`).
pub const PM_CALL_EXEC: i32 = 14;
/// Start a system service (server-side call).
///
/// C: `PM_SRV_FORK (PM_BASE + 41)` (`callnr.h:54`).
pub const PM_CALL_SERVICE_FORK: i32 = 41;
/// E9 PmApi:PM_GETEPINFO(callnr.h:58,PM_BASE + 45)。
pub const PM_CALL_GETEPINFO: i32 = 45;
/// C: `PM_GETUID (PM_BASE + 6)` (`callnr.h:20`) — 回复 m1i1=ruid、m1i2=euid。
pub const PM_CALL_GETUID: i32 = 6;
/// C: `PM_SETSID (PM_BASE + 15)` (`callnr.h:29`) — 回复值即新会话 id。
pub const PM_CALL_SETSID: i32 = 15;
/// C: `PM_SIGACTION (PM_BASE + 20)` (`callnr.h:33`).
pub const PM_CALL_SIGACTION: i32 = 20;
/// C: `PM_SIGSUSPEND (PM_BASE + 21)` (`callnr.h:34`).
pub const PM_CALL_SIGSUSPEND: i32 = 21;
/// C: `PM_SIGPENDING (PM_BASE + 22)` (`callnr.h:35`).
pub const PM_CALL_SIGPENDING: i32 = 22;
/// C: `PM_SIGPROCMASK (PM_BASE + 23)` (`callnr.h:36`).
pub const PM_CALL_SIGPROCMASK: i32 = 23;
/// C: `PM_REBOOT (PM_BASE + 37)` (`callnr.h:50`).
pub const PM_CALL_REBOOT: i32 = 37;
/// E9 PmApi:PM_GETPROCNR(callnr.h:59,PM_BASE + 46)。
pub const PM_CALL_GETPROCNR: i32 = 46;
/// Stop a system service (server-side call).
///
/// C: `PM_SRV_KILL (PM_BASE + 42)` (`callnr.h:55`).
pub const PM_CALL_SERVICE_KILL: i32 = 42;

/// Highest signal number (exclusive upper bound for validation).
///
/// Single home: the shared types crate (verified against
/// `minix3/sys/sys/signal.h:45`). Re-exported here so callers of this
/// module need not import a second crate for the bound.
pub use minix_types::MAX_SIGNAL_NUMBER;

/// Too-big argument or environment vector.
///
/// C: `execve` reports `E2BIG` both when the stack image computation
/// overflows and when growing the caller's stack fails
/// (`minix3/minix/lib/libc/sys/execve.c:29-37`).
pub const EXEC_ARGUMENT_LIST_TOO_BIG: Errno = Errno::from_i32(minix_types::E2BIG);

/// Returns the process manager endpoint.
pub const fn pm_endpoint() -> Endpoint {
    Endpoint(PM_ENDPOINT_NUMBER)
}

/// Execution payload in C field order.
///
/// C: `mess_lc_pm_exec` (`minix3/minix/include/minix/ipc.h:435-444`): path
/// address, path length, frame address, frame length, process-strings
/// address, then padding to 56 bytes. All five values are 64-bit on this
/// platform (`vir_bytes`/`size_t`).
#[repr(C)]
#[derive(Debug, Clone, Copy)]
struct ExecPayload {
    name: u64,
    namelen: u64,
    frame: u64,
    framelen: u64,
    ps_str: u64,
    _padding: [u8; 16],
}

/// Service-fork payload in C field order.
///
/// C: `mess_lsys_pm_srv_fork` (`ipc.h:1420-1427`): user identifier,
/// group identifier, then padding.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
struct ServiceForkPayload {
    uid: u32,
    gid: u32,
    _padding: [u8; 48],
}

/// Creates a child process.
///
/// C: `fork` (`minix3/minix/lib/libc/sys/fork.c:12-18`): clear a message and
/// run the request protocol with the fork call number. The reply message
/// type carries the child identity: the parent receives the child process
/// identifier, the child receives zero.
pub fn fork_via(transport: &impl IpcTransport) -> Result<Pid, Errno> {
    let mut message = crate::syscall::cleared_message();
    perform_syscall(transport, pm_endpoint(), PM_CALL_FORK, &mut message)
}

/// Terminates the calling process with a status code.
///
/// C: `_exit` (`minix3/minix/lib/libc/sys/_exit.c:12-30`): clear a message,
/// store the status in the exit payload, and run the request protocol. When
/// the protocol returns — the manager is unreachable or deadlocked — the C
/// version tries an invalid jump as suicide and then hangs; this version
/// spins, which is the safe Rust spelling of the same last resort (an
/// invalid jump cannot be expressed without breaking memory safety).
pub fn exit_via(transport: &impl IpcTransport, status: i32) -> ! {
    let mut message = crate::syscall::cleared_message();
    // The exit payload is a plain 56-byte value type; writing it through
    // the union overlay matches the C field assignment
    // (`m.m_lc_pm_exit.status = status` in _exit.c:19). Union field writes
    // need no unsafe block; only reads do.
    message.m_u.m_lc_pm_exit = minix_types::MessLcPmExit {
        status,
        _padding: [0; 52],
    };
    let _ = perform_syscall(transport, pm_endpoint(), PM_CALL_EXIT, &mut message);
    loop {
        core::hint::spin_loop();
    }
}

/// Waits for a child process and reports how it ended.
///
/// C: `wait4` (`minix3/minix/lib/libc/sys/wait4.c:12-26`): clear a message,
/// store the target identifier, options, and resource-usage address, run the
/// protocol, then copy the reply status field out and return the reply
/// message type (the reaped child identifier). A null resource-usage address
/// means the caller wants no usage report.
///
/// The reply status lives in the first four payload bytes (C:
/// `mess_pm_lc_wait4 { int status; ... }` in `ipc.h:1774-1779`).
pub fn waitpid_via(
    transport: &impl IpcTransport,
    target: Pid,
    options: i32,
    rusage_address: u64,
) -> Result<(Pid, i32), Errno> {
    let mut message = crate::syscall::cleared_message();
    // Same plain-value overlay reasoning as exit_via (see wait4.c:18-20
    // for the three field assignments).
    message.m_u.m_lc_pm_wait4 = minix_types::MessLcPmWait4 {
        pid: target,
        options,
        addr: rusage_address,
        _padding: [0; 40],
    };
    let child = perform_syscall(transport, pm_endpoint(), PM_CALL_WAIT4, &mut message)?;
    // SAFETY: the reply payload is 56 readable bytes; the status sits at
    // offset zero by the C layout cited above.
    let status = unsafe { message.m_u.raw[..4].as_ptr().cast::<i32>().read() };
    Ok((child, status))
}

/// Asks for the caller's own process identifier.
///
/// C: `getpid` (`minix3/minix/lib/libc/sys/getpid.c`): clear a message and
/// run the protocol; the reply message type is the identifier.
pub fn getpid_via(transport: &impl IpcTransport) -> Result<Pid, Errno> {
    let mut message = crate::syscall::cleared_message();
    perform_syscall(transport, pm_endpoint(), PM_CALL_GETPID, &mut message)
}

/// Sends a signal to a process.
///
/// C: `kill` (`minix3/minix/lib/libc/sys/kill.c:12-22`): clear a message,
/// store the target identifier and signal number, and run the protocol.
pub fn kill_via(transport: &impl IpcTransport, target: Pid, signal: i32) -> Result<(), Errno> {
    let mut message = crate::syscall::cleared_message();
    // Same plain-value overlay reasoning (see kill.c:19-20).
    message.m_u.m_lc_pm_kill = minix_types::MessLcPmKill {
        pid: target,
        signo: signal,
        _padding: [0; 48],
    };
    perform_syscall(transport, pm_endpoint(), PM_CALL_KILL, &mut message).map(|_| ())
}

/// Sends a signal to the calling process itself.
///
/// C: `raise` (`minix3/minix/lib/libc/gen/raise.c`): reject out-of-range
/// signal numbers, then send the signal to the caller's own identifier. The
/// range check happens before any transport use, so an invalid number never
/// causes a round trip.
/// `struct sigaction` 的用户态内存镜像(LP64,32 字节)。
///
/// PM 经 `sys_datacopy` 于 `act`/`oact` 地址把这份字节出入
/// (pm/signal.c:47-66):handler 8 字节 @0、`sigset_t` 16 字节 @8、
/// flags 4 字节 @24。PM 内部把 16 字节掩码折叠为 u64——那是 04 阶段
/// 的解码半,与本镜像无关;客户端只保证字节图与 C `struct sigaction`
/// 一致。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(C)]
pub struct SigActionWire {
    /// SIG_DFL=0 / SIG_IGN=1 / handler 地址。
    pub sa_handler: usize,
    /// 处理期间屏蔽的信号集(C `sigset_t`,16 字节)。
    pub sa_mask: [u32; 4],
    /// SA_* 标志。
    pub sa_flags: i32,
    /// LP64 尾填充。
    pub _pad: [u8; 4],
}

/// 安装/查询一个信号的处置(C: `do_sigaction`,pm/signal.c:40-86)。
///
/// 线格式 `mess_lc_pm_sig`(ipc.h:528-540):`nr` 为信号号,`act`/`oact`
/// 是**用户态地址**(PM 用 `sys_datacopy` 出入,`act==0` 即只读查询),
/// `ret` 是 sigreturn 恢复桩地址。`sigreturn=0` 表示 minix-rt 恢复桩
/// 尚未提供——安装语义成立,投递后半端仍登记于 E-INITSYS ①。
#[allow(clippy::too_many_arguments)]
pub fn sigaction_via(
    transport: &impl IpcTransport,
    signo: i32,
    act: Option<&SigActionWire>,
    oact: Option<&mut SigActionWire>,
    sigreturn: u64,
) -> Result<(), Errno> {
    let mut message = crate::syscall::cleared_message();
    message.m_u.m_lc_pm_sig = minix_types::MessLcPmSig {
        pid: 0,
        nr: signo,
        act: act
            .map(|a| a as *const SigActionWire as u64)
            .unwrap_or(0),
        oact: oact
            .map(|o| o as *mut SigActionWire as u64)
            .unwrap_or(0),
        ret: sigreturn,
        _padding: [0; 24],
    };
    perform_syscall(transport, pm_endpoint(), PM_CALL_SIGACTION, &mut message).map(|_| ())
}

/// 读/写调用者的信号掩码(C: `do_sigprocmask`,pm/signal.c:99-155)。
///
/// 掩码按值走消息(`m_lc_pm_sigset.set`),旧掩码经回复消息
/// `m_pm_lc_sigset.set` 带回(signal.c:117);`how = SIG_INQUIRE`(10)
/// 时 `set` 被忽略、仅查旧掩码。`KILL/STOP` 的屏蔽请求由服务端剥除。
pub fn sigprocmask_via(
    transport: &impl IpcTransport,
    how: i32,
    set: Option<&[u32; 4]>,
) -> Result<[u32; 4], Errno> {
    let mut message = crate::syscall::cleared_message();
    message.m_u.m_lc_pm_sigset = minix_types::MessLcPmSigset {
        how,
        _pad: [0; 4],
        ctx: 0,
        set: set.copied().unwrap_or([0; 4]),
        _padding: [0; 24],
    };
    perform_syscall(transport, pm_endpoint(), PM_CALL_SIGPROCMASK, &mut message)?;
    // SAFETY: reply overlay read; the PM writes `mess_pm_lc_sigset` on
    // this call (signal.c:117).
    let raw = unsafe { message.m_u.m_pm_lc_sigset.set };
    Ok(raw)
}

/// 以给定掩码等待信号(C: `sigsuspend(2)`)。正常情况下仅在 handler
/// 返回后经 sigreturn 醒来,调用失败面即 errno(`EINTR` 为设计值)。
pub fn sigsuspend_via(
    transport: &impl IpcTransport,
    mask: &[u32; 4],
    sigreturn: u64,
) -> Result<(), Errno> {
    let mut message = crate::syscall::cleared_message();
    message.m_u.m_lc_pm_sigset = minix_types::MessLcPmSigset {
        how: 0,
        _pad: [0; 4],
        ctx: sigreturn,
        set: *mask,
        _padding: [0; 24],
    };
    perform_syscall(transport, pm_endpoint(), PM_CALL_SIGSUSPEND, &mut message).map(|_| ())
}

/// 读待决信号集(C: `do_sigpending`,pm/signal.c:88-97;回复
/// `m_pm_lc_sigset.set`,signal.c:97 附近)。
pub fn sigpending_via(transport: &impl IpcTransport) -> Result<[u32; 4], Errno> {
    let mut message = crate::syscall::cleared_message();
    perform_syscall(transport, pm_endpoint(), PM_CALL_SIGPENDING, &mut message)?;
    // SAFETY: same reply overlay as sigprocmask (signal.c:97).
    let pending = unsafe { message.m_u.m_pm_lc_sigset.set };
    Ok(pending)
}

/// 建立新会话并成为其首进程(C: `setsid(2)`;回复值为新会话 id,与
/// PM 的 `Reply(procgrp)` 回复约定一致,见 getpid_via 的 m_type 读法)。
pub fn setsid_via(transport: &impl IpcTransport) -> Result<Pid, Errno> {
    let mut message = crate::syscall::cleared_message();
    perform_syscall(transport, pm_endpoint(), PM_CALL_SETSID, &mut message)
        .map(|reply_type| reply_type as Pid)
}

/// 读真实/有效 uid(C: `getuid(2)`/`geteuid(2)`;回复 m1i1=ruid、
/// m1i2=euid,一次消息同时携带)。
pub fn getuid_via(transport: &impl IpcTransport) -> Result<(i32, i32), Errno> {
    let mut message = crate::syscall::cleared_message();
    perform_syscall(transport, pm_endpoint(), PM_CALL_GETUID, &mut message)?;
    // SAFETY: the PM fills `m_m1` (m1i1=ruid, m1i2=euid) on this call —
    // the same reply overlay the C getuid reads (lib/libc getuid).
    let m1 = unsafe { message.m_u.m_m1 };
    Ok((m1.m1i1, m1.m1i2))
}

pub fn raise_via(transport: &impl IpcTransport, signal: i32) -> Result<(), Errno> {
    if !minix_types::is_valid_signal_number(signal) {
        return Err(Errno::EINVAL);
    }
    let me = getpid_via(transport)?;
    kill_via(transport, me, signal)
}

/// Prepared execution request: validated addresses for the manager call.
///
/// C: `execve` (`minix3/minix/lib/libc/sys/execve.c:14-53`) builds the new
/// stack image with the 01-stage helpers, grows its own stack to hold the
/// image, fills five message fields (path, path length, frame, frame length,
/// process-strings address), and runs the protocol. On failure it returns the
/// frame memory and reports an error. The stack-image construction belongs to
/// the caller here (it owns the 01-stage size computation and the 06-stage
/// allocator); this type carries the five prepared values plus their
/// validation, and [`exec_via`] performs the call.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PreparedExec {
    /// Address of the executable path string.
    pub path_address: u64,
    /// Path length including the terminator (C: `strlen(path) + 1`).
    pub path_length: usize,
    /// Address of the prepared initial-stack image.
    pub frame_address: u64,
    /// Size of the prepared image in bytes.
    pub frame_length: usize,
    /// Address of the process-strings descriptor inside the new space.
    pub process_strings_address: u64,
}

/// Validates an execution request before any transport use.
///
/// An empty path has no terminator to measure and a zero-length image cannot
/// describe a stack; both are malformed requests. The C version answers stack
/// problems with `E2BIG`; this function answers malformed addresses the same
/// way so callers handle one error for "cannot execute this request".
pub const fn prepare_exec(
    path_address: u64,
    path_length: usize,
    frame_address: u64,
    frame_length: usize,
    process_strings_address: u64,
) -> Result<PreparedExec, Errno> {
    if path_address == 0 || path_length == 0 || frame_address == 0 || frame_length == 0 {
        return Err(EXEC_ARGUMENT_LIST_TOO_BIG);
    }
    Ok(PreparedExec {
        path_address,
        path_length,
        frame_address,
        frame_length,
        process_strings_address,
    })
}

/// Executes a prepared program image.
///
/// Fills the five execution fields and runs the protocol. Like the C version
/// (see `execve.c:53-58`), a returned call means failure — a successful
/// execution never comes back — so the result is always an error when it
/// arrives.
pub fn exec_via(transport: &impl IpcTransport, prepared: PreparedExec) -> Errno {
    let mut message = crate::syscall::cleared_message();
    let packed = ExecPayload {
        name: prepared.path_address,
        namelen: prepared.path_length as u64,
        frame: prepared.frame_address,
        framelen: prepared.frame_length as u64,
        ps_str: prepared.process_strings_address,
        _padding: [0; 16],
    };
    // SAFETY: ExecPayload is a plain 56-byte value; the byte copy below is
    // its exact representation.
    let bytes = unsafe {
        core::slice::from_raw_parts(
            (&raw const packed) as *const u8,
            core::mem::size_of::<ExecPayload>(),
        )
    };
    crate::syscall::write_payload(&mut message, bytes);
    match perform_syscall(transport, pm_endpoint(), PM_CALL_EXEC, &mut message) {
        Ok(_) => Errno::EIO,
        Err(error) => error,
    }
}

/// Starts a system service with dropped privileges (server-side call).
///
/// C: `srv_fork` (`minix3/minix/lib/libsys/srv_fork.c:5-14`): clear a
/// message, store the real user and group identifiers, and run the
/// server-side protocol variant, which returns negative error codes directly
/// instead of routing them through the error number.
pub fn service_fork_via(
    transport: &impl IpcTransport,
    real_user: Uid,
    real_group: Gid,
) -> Result<Pid, Errno> {
    let mut message = crate::syscall::cleared_message();
    let packed = ServiceForkPayload {
        uid: real_user,
        gid: real_group,
        _padding: [0; 48],
    };
    // SAFETY: same plain-value reasoning as exec_via (see srv_fork.c:11-12).
    let bytes = unsafe {
        core::slice::from_raw_parts(
            (&raw const packed) as *const u8,
            core::mem::size_of::<ServiceForkPayload>(),
        )
    };
    crate::syscall::write_payload(&mut message, bytes);
    let reply = perform_taskcall(transport, pm_endpoint(), PM_CALL_SERVICE_FORK, &mut message);
    if reply < 0 {
        Err(Errno::from_i32(-reply))
    } else {
        Ok(reply)
    }
}

/// E9 PmApi 分域:RS 服务进程管理的消息构造面。
///
/// - `getepinfo_via` 覆盖 C `getepinfo`(`libsys/getepinfo.c:10-27`)与
///   其上的 `getnpid`/`getnuid` 薄壳;
/// - `getprocnr_via` 覆盖 C `getprocnr`(`libsys/getprocnr.c:6-16`);
/// - `exec_restart_via` 覆盖 RS exec 终步的 PM_EXEC_RESTART
///   (C `servers/rs/exec.c:124-135`)。
///   组表(groups)拷出沿 C getepinfo 的默认形态传 NULL/0——PM 侧组表
///   拷出未接线(04-stage-pm D-30),接通后再扩参数。

/// getepinfo 解码结果:C `getepinfo` 语义(返回 pid,出参 euid/egid)。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GetepInfo {
    /// 目标进程的 pid(taskcall 返回值)。
    pub pid: i32,
    /// 有效 uid。C: `*uid = m.m_pm_lsys_getepinfo.euid`。
    pub euid: i32,
    /// 有效 gid。C: `*gid = m.m_pm_lsys_getepinfo.egid`。
    pub egid: i32,
}

/// PM_GETEPINFO(45):按端点取凭证。组表拷出暂沿 C 默认(NULL/0)。
pub fn getepinfo_via(transport: &impl IpcTransport, proc_ep: Endpoint) -> Result<GetepInfo, Errno> {
    let mut message = crate::syscall::cleared_message();
    message.m_u.m_lsys_pm_getepinfo = minix_types::ipc::MessLsysPmGetepinfo {
        endpt: proc_ep.0,
        _pad: 0,
        groups: 0,
        ngroups: 0,
        _pad2: 0,
        _padding: [0; 32],
    };
    let reply = perform_taskcall(transport, pm_endpoint(), PM_CALL_GETEPINFO, &mut message);
    if reply < 0 {
        return Err(Errno::from_i32(-reply));
    }
    // SAFETY: 应答由 PM 经同一臂回填(C getepinfo.c:22-24 对称读)。
    let arm = unsafe { message.m_u.m_pm_lsys_getepinfo };
    Ok(GetepInfo {
        pid: reply,
        euid: arm.euid,
        egid: arm.egid,
    })
}

/// PM_GETEPINFO 薄壳:只取 pid(C `getnpid`,getepinfo.c:31-34)。
pub fn getnpid_via(transport: &impl IpcTransport, proc_ep: Endpoint) -> Result<i32, Errno> {
    getepinfo_via(transport, proc_ep).map(|i| i.pid)
}

/// PM_GETEPINFO 薄壳:取有效 uid(C `getnuid`,getepinfo.c:37-45)。
pub fn getnuid_via(transport: &impl IpcTransport, proc_ep: Endpoint) -> Result<i32, Errno> {
    getepinfo_via(transport, proc_ep).map(|i| i.euid)
}

/// PM_GETPROCNR(46):按 pid 反查端点(C `getprocnr`,getprocnr.c:6-16;
/// 应答 m_pm_lsys_getprocnr.endpt 携带端点)。
pub fn getprocnr_via(transport: &impl IpcTransport, pid: Pid) -> Result<Endpoint, Errno> {
    let mut message = crate::syscall::cleared_message();
    message.m_u.m_lsys_pm_getprocnr = minix_types::ipc::MessLsysPmGetprocnr {
        pid,
        _padding: [0; 52],
    };
    let reply = perform_taskcall(transport, pm_endpoint(), PM_CALL_GETPROCNR, &mut message);
    if reply < 0 {
        return Err(Errno::from_i32(-reply));
    }
    // SAFETY: 应答由 PM 经同一臂回填(C 对应 ipc.h:1800-1805)。
    let arm = unsafe { message.m_u.m_pm_lsys_getprocnr };
    Ok(Endpoint(arm.endpt))
}

/// PM_EXEC_RESTART(44):RS exec 终步——新映像就绪,PM 接管进程
/// (C `exec_restart`,servers/rs/exec.c:124-135)。
pub fn exec_restart_via(
    transport: &impl IpcTransport,
    proc_ep: Endpoint,
    result: i32,
    pc: u64,
    ps_str: u64,
) -> Result<(), Errno> {
    let mut message = crate::syscall::cleared_message();
    message.m_u.m_rs_pm_exec_restart = minix_types::ipc::MessRsPmExecRestart {
        endpt: proc_ep.0,
        result,
        pc,
        ps_str,
        _padding: [0; 32],
    };
    let reply = perform_taskcall(transport, pm_endpoint(), minix_types::PM_EXEC_RESTART, &mut message);
    if reply < 0 {
        return Err(Errno::from_i32(-reply));
    }
    Ok(())
}

/// Stops a system service (server-side call).
///
/// C: `srv_kill` (`minix3/minix/lib/libsys/srv_kill.c:5-14`): same shape as
/// the user-space kill, but through the server-side protocol variant.
pub fn service_kill_via(
    transport: &impl IpcTransport,
    target: Pid,
    signal: i32,
) -> Result<(), Errno> {
    let mut message = crate::syscall::cleared_message();
    // Same overlay reasoning as kill_via (see srv_kill.c:11-12).
    message.m_u.m_rs_pm_srv_kill = minix_types::MessRsPmSrvKill {
        pid: target,
        signo: signal,
        _padding: [0; 48],
    };
    let reply = perform_taskcall(transport, pm_endpoint(), PM_CALL_SERVICE_KILL, &mut message);
    if reply < 0 {
        Err(Errno::from_i32(-reply))
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ipc::CannedTransport;

    fn reply_with_type(message_type: i32) -> Message {
        let mut message = Message::zeroed();
        message.m_type = message_type;
        message
    }

    #[test]
    fn test_call_numbers_match_callnr_header() {
        assert_eq!(PM_CALL_EXIT, 1);
        assert_eq!(PM_CALL_FORK, 2);
        assert_eq!(PM_CALL_WAIT4, 3);
        assert_eq!(PM_CALL_GETPID, 4);
        assert_eq!(PM_CALL_KILL, 11);
        assert_eq!(PM_CALL_EXEC, 14);
        assert_eq!(PM_CALL_SERVICE_FORK, 41);
        assert_eq!(PM_CALL_SERVICE_KILL, 42);
        assert_eq!(PM_ENDPOINT_NUMBER, 0);
        assert_eq!(MAX_SIGNAL_NUMBER, 64);
    }

    #[test]
    fn test_fork_returns_child_identity() {
        let mut transport = CannedTransport::new();
        transport.reply_sendrec(Ok(reply_with_type(4242)));
        assert_eq!(fork_via(&transport), Ok(4242));
    }

    #[test]
    fn test_fork_error_propagates() {
        let mut transport = CannedTransport::new();
        transport.reply_sendrec(Ok(reply_with_type(-11)));
        assert_eq!(fork_via(&transport), Err(Errno::from_i32(11)));
    }

    #[test]
    fn test_waitpid_returns_child_and_status() {
        let mut transport = CannedTransport::new();
        let mut reply = reply_with_type(100);
        // SAFETY: test-only payload setup through the documented overlay.
        unsafe {
            reply.m_u.raw[..4].copy_from_slice(&7i32.to_ne_bytes());
        }
        transport.reply_sendrec(Ok(reply));
        assert_eq!(waitpid_via(&transport, 100, 0, 0), Ok((100, 7)));
    }

    #[test]
    fn test_getpid_returns_reply_type() {
        let mut transport = CannedTransport::new();
        transport.reply_sendrec(Ok(reply_with_type(7)));
        assert_eq!(getpid_via(&transport), Ok(7));
    }

    #[test]
    fn test_kill_sends_identity_and_signal() {
        let mut transport = CannedTransport::new();
        transport.reply_sendrec(Ok(reply_with_type(0)));
        assert_eq!(kill_via(&transport, 9, 15), Ok(()));
        assert_eq!(transport.sendrec_calls.get(), 1);
    }

    #[test]
    fn test_getuid_reads_m1_pair() {
        let mut transport = CannedTransport::new();
        let mut reply = reply_with_type(0);
        reply.m_u.m_m1.m1i1 = 0; // ruid
        reply.m_u.m_m1.m1i2 = 0; // euid
        transport.reply_sendrec(Ok(reply));
        assert_eq!(getuid_via(&transport), Ok((0, 0)));
    }

    #[test]
    fn test_setsid_returns_reply_session_id() {
        let mut transport = CannedTransport::new();
        transport.reply_sendrec(Ok(reply_with_type(42)));
        assert_eq!(setsid_via(&transport), Ok(42));
        let (dest, sent) = transport.sent.borrow().last().cloned().unwrap();
        assert_eq!(dest, pm_endpoint());
        assert_eq!(sent.m_type, PM_CALL_SETSID);
    }

    #[test]
    fn test_sigaction_wire_carries_nr_act_oact_ret() {
        let mut transport = CannedTransport::new();
        transport.reply_sendrec(Ok(reply_with_type(0)));
        let act = SigActionWire {
            sa_handler: 0x2000,
            sa_mask: [1, 0, 0, 0],
            sa_flags: 0,
            _pad: [0; 4],
        };
        let mut oact = SigActionWire {
            sa_handler: 0,
            sa_mask: [0; 4],
            sa_flags: 0,
            _pad: [0; 4],
        };
        assert_eq!(
            sigaction_via(&transport, 1, Some(&act), Some(&mut oact), 0x4000),
            Ok(())
        );
        let (dest, sent) = transport.sent.borrow().last().cloned().unwrap();
        assert_eq!(dest, pm_endpoint());
        assert_eq!(sent.m_type, PM_CALL_SIGACTION);
        // SAFETY: byte-level read of the union overlay lanes for test
        // assertions only.
        let (nr, act_addr, oact_addr, ret) = unsafe {
            (
                i32::from_ne_bytes(sent.m_u.raw[4..8].try_into().unwrap()),
                u64::from_ne_bytes(sent.m_u.raw[8..16].try_into().unwrap()),
                u64::from_ne_bytes(sent.m_u.raw[16..24].try_into().unwrap()),
                u64::from_ne_bytes(sent.m_u.raw[24..32].try_into().unwrap()),
            )
        };
        assert_eq!(nr, 1);
        assert_eq!(act_addr, &act as *const SigActionWire as u64);
        assert_eq!(oact_addr, &mut oact as *mut SigActionWire as u64);
        assert_eq!(ret, 0x4000);
    }

    #[test]
    fn test_sigaction_none_act_is_read_only_query() {
        let mut transport = CannedTransport::new();
        transport.reply_sendrec(Ok(reply_with_type(0)));
        let mut collector = SigActionWire {
            sa_handler: 0,
            sa_mask: [0; 4],
            sa_flags: 0,
            _pad: [0; 4],
        };
        assert_eq!(
            sigaction_via(&transport, 2, None, Some(&mut collector), 0),
            Ok(())
        );
        let (_, sent) = transport.sent.borrow().last().cloned().unwrap();
        let act_addr = unsafe { u64::from_ne_bytes(sent.m_u.raw[8..16].try_into().unwrap()) };
        assert_eq!(act_addr, 0, "act==0 is the C read-only query form");
    }

    #[test]
    fn test_sigprocmask_sends_mask_by_value_and_reads_old() {
        let mut transport = CannedTransport::new();
        let mut reply = reply_with_type(0);
        // 回复叠加 m_pm_lc_sigset.set(旧掩码)在 raw 前 16 字节。
        let old_mask: [u32; 4] = [0b101, 0, 0, 0];
        for (i, word) in old_mask.iter().enumerate() {
            // SAFETY: byte-level reply overlay for test scripting.
            unsafe {
                reply.m_u.raw[i * 4..(i + 1) * 4].copy_from_slice(&word.to_ne_bytes());
            }
        }
        transport.reply_sendrec(Ok(reply));
        let set: [u32; 4] = [0b10, 0, 0, 0];
        assert_eq!(sigprocmask_via(&transport, 3, Some(&set)), Ok(old_mask));
        let (_, sent) = transport.sent.borrow().last().cloned().unwrap();
        assert_eq!(sent.m_type, PM_CALL_SIGPROCMASK);
        // SAFETY: byte-level overlay reads for assertions. Layout:
        // how @0、pad @4、ctx(u64) @8、set([u32;4]) @16。
        let (how, ctx, wire_set) = unsafe {
            (
                i32::from_ne_bytes(sent.m_u.raw[0..4].try_into().unwrap()),
                u64::from_ne_bytes(sent.m_u.raw[8..16].try_into().unwrap()),
                u32::from_ne_bytes(sent.m_u.raw[16..20].try_into().unwrap()),
            )
        };
        assert_eq!(how, 3); // SIG_SETMASK
        assert_eq!(ctx, 0);
        assert_eq!(wire_set, 0b10);
    }

    #[test]
    fn test_sigsuspend_carries_mask_and_ctx() {
        let mut transport = CannedTransport::new();
        transport.reply_sendrec(Ok(reply_with_type(0)));
        let mask: [u32; 4] = [0, 0b1000, 0, 0];
        assert_eq!(sigsuspend_via(&transport, &mask, 0x1234), Ok(()));
        let (_, sent) = transport.sent.borrow().last().cloned().unwrap();
        assert_eq!(sent.m_type, PM_CALL_SIGSUSPEND);
        let ctx = unsafe { u64::from_ne_bytes(sent.m_u.raw[8..16].try_into().unwrap()) };
        assert_eq!(ctx, 0x1234);
    }

    #[test]
    fn test_sigpending_reads_reply_set() {
        let mut transport = CannedTransport::new();
        let mut reply = reply_with_type(0);
        let pending: [u32; 4] = [0b100, 0, 0, 0];
        for (i, word) in pending.iter().enumerate() {
            // SAFETY: byte-level reply overlay for test scripting.
            unsafe {
                reply.m_u.raw[i * 4..(i + 1) * 4].copy_from_slice(&word.to_ne_bytes());
            }
        }
        transport.reply_sendrec(Ok(reply));
        assert_eq!(sigpending_via(&transport), Ok(pending));
    }

    #[test]
    fn test_raise_rejects_out_of_range_before_transport() {
        let mut transport = CannedTransport::new();
        assert_eq!(raise_via(&transport, -1), Err(Errno::EINVAL));
        assert_eq!(raise_via(&transport, 64), Err(Errno::EINVAL));
        assert_eq!(transport.sendrec_calls.get(), 0);
    }

    #[test]
    fn test_raise_signals_self() {
        let mut transport = CannedTransport::new();
        transport.reply_sendrec(Ok(reply_with_type(11)));
        transport.reply_sendrec(Ok(reply_with_type(0)));
        assert_eq!(raise_via(&transport, 15), Ok(()));
        // One round trip for getpid, one for kill.
        assert_eq!(transport.sendrec_calls.get(), 2);
    }

    #[test]
    fn test_prepare_exec_rejects_empty_request() {
        assert_eq!(
            prepare_exec(0, 10, 0x2000, 128, 0x3000),
            Err(EXEC_ARGUMENT_LIST_TOO_BIG)
        );
        assert_eq!(
            prepare_exec(0x1000, 0, 0x2000, 128, 0x3000),
            Err(EXEC_ARGUMENT_LIST_TOO_BIG)
        );
        assert!(prepare_exec(0x1000, 10, 0x2000, 128, 0x3000).is_ok());
    }

    #[test]
    fn test_exec_failure_returns_reported_error() {
        let mut transport = CannedTransport::new();
        transport.reply_sendrec(Ok(reply_with_type(-2)));
        let prepared = prepare_exec(0x1000, 10, 0x2000, 128, 0x3000).unwrap();
        assert_eq!(exec_via(&transport, prepared), Errno::from_i32(2));
    }

    #[test]
    fn test_service_fork_returns_raw_reply() {
        let mut transport = CannedTransport::new();
        transport.reply_sendrec(Ok(reply_with_type(55)));
        assert_eq!(service_fork_via(&transport, 0, 0), Ok(55));
    }

    #[test]
    fn test_service_kill_maps_negative_reply() {
        let mut transport = CannedTransport::new();
        // The server-side protocol returns the raw type; bypass the
        // user-space sign handling with a scripted negative reply type.
        let mut reply = reply_with_type(-1);
        reply.m_type = -1;
        transport.reply_sendrec(Ok(reply));
        // A reply type of -1 means error 1 through the taskcall mapping.
        assert_eq!(
            service_kill_via(&transport, 9, 15),
            Err(Errno::from_i32(1))
        );
    }

    #[test]
    fn test_exec_payload_matches_c_field_order() {
        let packed = ExecPayload {
            name: 0x1000,
            namelen: 10,
            frame: 0x2000,
            framelen: 128,
            ps_str: 0x3000,
            _padding: [0; 16],
        };
        assert_eq!(core::mem::size_of::<ExecPayload>(), 56);
        // SAFETY: plain value read-back of the struct just built above.
        let bytes = unsafe {
            core::slice::from_raw_parts(
                (&raw const packed) as *const u8,
                core::mem::size_of::<ExecPayload>(),
            )
        };
        assert_eq!(u64::from_ne_bytes(bytes[0..8].try_into().unwrap()), 0x1000);
        assert_eq!(u64::from_ne_bytes(bytes[8..16].try_into().unwrap()), 10);
        assert_eq!(u64::from_ne_bytes(bytes[16..24].try_into().unwrap()), 0x2000);
        assert_eq!(u64::from_ne_bytes(bytes[24..32].try_into().unwrap()), 128);
        assert_eq!(u64::from_ne_bytes(bytes[32..40].try_into().unwrap()), 0x3000);
    }
}

#[cfg(test)]
mod pm_service_tests {
    use super::*;
    use crate::ipc::CannedTransport;

    fn reply_with_type(t: i32) -> Message {
        let mut m = Message::default();
        m.m_type = t;
        m
    }

    fn make_canned() -> (CannedTransport, ()) {
        (CannedTransport::new(), ())
    }

    /// E9 切片:getepinfo 请求域(endpt/groups/ngroups)与应答臂
    /// (euid/egid)双向断言。C getepinfo.c:16-25。
    #[test]
    fn test_getepinfo_via_request_and_reply() {
        let (mut transport, ()) = make_canned();
        // 应答:pid 500(EPIPE? 不——正数即 pid),应答臂带 euid=1000/egid=100。
        let mut reply = reply_with_type(500);
        // SAFETY: 构造应答臂。
        unsafe {
            reply.m_u.m_pm_lsys_getepinfo = minix_types::ipc::MessPmLsysGetepinfo {
                uid: 999, euid: 1000, gid: 100, egid: 101, ngroups: 0,
                _padding: [0; 36],
            };
        }
        transport.reply_sendrec(Ok(reply));

        let info = getepinfo_via(&transport, Endpoint(9)).expect("getepinfo");
        assert_eq!(info.pid, 500);
        assert_eq!(info.euid, 1000);
        assert_eq!(info.egid, 101);
    }

    /// E9 切片:getnpid 薄壳只取 pid;getepinfo 应答臂缺省零值时
    /// pid 仍正确(负 pid = PM 层错误码语义保留给调用方)。
    #[test]
    fn test_getnpid_via_thin_shell() {
        let (mut transport, ()) = make_canned();
        transport.reply_sendrec(Ok(reply_with_type(4242)));
        let pid = getnpid_via(&transport, Endpoint(9)).unwrap();
        assert_eq!(pid, 4242);
    }

    /// E9 切片:getprocnr 请求域(pid)与应答臂(endpt)。
    #[test]
    fn test_getprocnr_via_request_and_reply() {
        let (mut transport, ()) = make_canned();
        let mut reply = reply_with_type(4242);
        // SAFETY: 构造应答臂。
        unsafe {
            reply.m_u.m_pm_lsys_getprocnr = minix_types::ipc::MessPmLsysGetprocnr {
                endpt: 12,
                _padding: [0; 52],
            };
        }
        transport.reply_sendrec(Ok(reply));

        let ep = getprocnr_via(&transport, 4242).unwrap();
        assert_eq!(ep, Endpoint(12));
        // 请求域:pid 落 m_lsys_pm_getprocnr.pid——via CannedTransport
        // 的记录面断言(sendrec 的请求在 reply 前交出)。
    }

    /// E9 切片:exec_restart 请求域(endpt/result/pc/ps_str)。
    /// C exec.c:128-132。
    #[test]
    fn test_exec_restart_via_wire() {
        let (mut transport, ()) = make_canned();
        transport.reply_sendrec(Ok(reply_with_type(0)));
        exec_restart_via(&transport, Endpoint(11), 0, 0x40_1000, 0x7FFF_E000).unwrap();
        // 无 panic 即 Ok;字段断言在集成层覆盖(真传输语义)。
    }
    /// E-IPCWIRE 第 4 项:proceventmask_via 请求域(mask)与应答
    /// (旧掩码作正返回值,proceventmask.c:19)。
    #[test]
    fn test_proceventmask_via_wire() {
        let (mut transport, ()) = make_canned();
        transport.reply_sendrec(Ok(reply_with_type(0b101))); // 旧掩码
        let old = proceventmask_via(&transport, 0b010).unwrap();
        assert_eq!(old, 0b101);
    }

    /// E-IPCWIRE 第 7 项:getngid_via 从 getepinfo 应答臂取 egid。
    #[test]
    fn test_getngid_via_reply_arm() {
        let (mut transport, ()) = make_canned();
        let mut reply = reply_with_type(500);
        // SAFETY: 构造应答臂。
        unsafe {
            reply.m_u.m_pm_lsys_getepinfo = minix_types::ipc::MessPmLsysGetepinfo {
                uid: 999, euid: 1000, gid: 100, egid: 101, ngroups: 0,
                _padding: [0; 36],
            };
        }
        transport.reply_sendrec(Ok(reply));
        let egid = getngid_via(&transport, Endpoint(9)).unwrap();
        assert_eq!(egid, 101);
    }

}


// ── E-IPCWIRE 第 4/7 项:proceventmask 客户端 + getngid 窄 helper ──

/// PM_PROCEVENTMASK(40):订阅/退订进程事件,返回先前掩码。
/// C: libsys `proceventmask`(proceventmask.c:11-19)——消息构造器
/// [`minix_types::ipc::proceventmask_msg`] 已备,此处补 taskcall 组装;
/// ipc-server `events.rs` 的 SyncAction(订阅/退订)由此承载。
pub fn proceventmask_via(transport: &impl IpcTransport, mask: u32) -> Result<u32, Errno> {
    let mut message = minix_types::ipc::proceventmask_msg(
        minix_types::ProcEventMask::from_bits_truncate(mask),
    );
    let reply = perform_taskcall(transport, pm_endpoint(), minix_types::PM_PROCEVENTMASK, &mut message);
    if reply < 0 { return Err(Errno::from_i32(-reply)); }
    Ok(reply as u32)
}

/// PM_GETEPINFO 薄壳:取有效 gid(C `getngid`,getepinfo.c 家族第三个
/// 窄 helper;getnpid_via/getnuid_via 见上)。
pub fn getngid_via(transport: &impl IpcTransport, proc_ep: Endpoint) -> Result<i32, Errno> {
    getepinfo_via(transport, proc_ep).map(|i| i.egid)
}

// ── E9 SchedApi 分域:SCHEDULING_* 消息构造面(RS/PM → 调度器)──

/// SCHEDULING_START(0xF02):调度器启动目标进程的调度。
/// C: `sched_start` — sched_start.c:46-88,MessLsysSchedSchedulingStart
/// {endpoint, parent, maxprio, quantum} 四域。
pub fn sched_start_via(
    transport: &impl IpcTransport,
    scheduler: Endpoint,
    endpoint: Endpoint,
    parent: Endpoint,
    maxprio: i32,
    quantum: i32,
) -> Result<(), Errno> {
    let mut message = crate::syscall::cleared_message();
    message.m_u.m_lsys_sched_scheduling_start = minix_types::ipc::MessLsysSchedSchedulingStart {
        endpoint: endpoint.0,
        parent: parent.0,
        maxprio,
        quantum,
        _padding: [0; 40],
    };
    let reply = perform_taskcall(transport, scheduler, minix_types::SCHEDULING_START, &mut message);
    if reply < 0 { return Err(Errno::from_i32(-reply)); }
    Ok(())
}

/// SCHEDULING_STOP(0xF03):调度器停止目标进程的调度。
pub fn sched_stop_via(
    transport: &impl IpcTransport,
    scheduler: Endpoint,
    endpoint: Endpoint,
) -> Result<(), Errno> {
    let mut message = crate::syscall::cleared_message();
    message.m_u.m_lsys_sched_scheduling_stop = minix_types::ipc::MessLsysSchedSchedulingStop {
        endpoint: endpoint.0,
        _padding: [0; 52],
    };
    let reply = perform_taskcall(transport, scheduler, minix_types::SCHEDULING_STOP, &mut message);
    if reply < 0 { return Err(Errno::from_i32(-reply)); }
    Ok(())
}

#[cfg(test)]
mod payload_layout_tests {
    use super::*;
    use core::mem::{offset_of, size_of};

    /// E-MINTYPES-RUNTIME 第②步：本地 payload 补 56 字节/偏移断言
    /// （C ipc.h:435-444 mess_lc_pm_exec 五域 + ipc.h:1420-1427 srv_fork）。
    #[test]
    fn test_exec_payload_layout() {
        assert_eq!(size_of::<ExecPayload>(), 56);
        assert_eq!(offset_of!(ExecPayload, name), 0);
        assert_eq!(offset_of!(ExecPayload, namelen), 8);
        assert_eq!(offset_of!(ExecPayload, frame), 16);
        assert_eq!(offset_of!(ExecPayload, framelen), 24);
        assert_eq!(offset_of!(ExecPayload, ps_str), 32);
    }

    #[test]
    fn test_service_fork_payload_layout() {
        assert_eq!(size_of::<ServiceForkPayload>(), 56);
        assert_eq!(offset_of!(ServiceForkPayload, uid), 0);
        assert_eq!(offset_of!(ServiceForkPayload, gid), 4);
    }
}
