//! System call protocol: the thin layer above send-and-receive.
//!
//! The communication primitives from the [`crate::ipc`] module move raw
//! messages. This module implements the three conventions that turn raw
//! message passing into usable system calls (C: `minix3/minix/lib/libc/sys/syscall.c`,
//! `loadname.c`, and `minix3/minix/lib/libsys/kernel_call.c`):
//!
//! 1. The request protocol: put the call number into the message type field,
//!    send the message and wait for the reply, translate a failed round trip
//!    into the message type, and translate a negative reply into a typed
//!    error.
//! 2. The path packing rule: short path names travel inside the message,
//!    long ones travel by pointer, with the length always recorded.
//! 3. The kernel-call retry rule: when the kernel answers "not ready", wait a
//!    growing number of clock ticks and try again instead of failing.
//!
//! All three are pure logic over the [`crate::ipc::IpcTransport`] trait, so
//! unit tests drive them with a scripted transport and never need a kernel.

use crate::ipc::{IpcTransport, TrapStatus};
use minix_types::{Endpoint, Errno, Message, MessKrnLsysSysTimes};

/// Maximum path name length that still fits inside a message.
///
/// C: `M_PATH_STRING_MAX 40` (`minix3/minix/include/minix/ipc.h:14`). The
/// message layout `mess_lc_vfs_path` (`ipc.h:761-767`) carries a 40-byte
/// inline buffer alongside the pointer and length fields.
pub const MAX_INLINE_PATH_BYTES: usize = 40;

/// Packed path name for a file-carrying message.
///
/// C: the contract of `_loadname` (`minix3/minix/lib/libc/sys/loadname.c:7-19`):
/// the length field always holds the string length including the terminating
/// zero byte, the pointer field always holds the caller's string address, and
/// the inline buffer holds a copy only when the whole string (terminator
/// included) fits into [`MAX_INLINE_PATH_BYTES`] bytes. The server reads the
/// inline copy for short names and follows the pointer for long ones.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PackedPathName {
    /// String length including the terminating zero byte (C: the `len` field).
    pub length_including_nul: usize,
    /// Inline copy, present exactly when the string fits (C: the `buf` field).
    pub inline_copy: Option<[u8; MAX_INLINE_PATH_BYTES]>,
}

/// Packs a zero-terminated path name following the `_loadname` rule.
///
/// The input must already include the terminating zero byte, like a C string
/// in memory. An empty input has no terminator and is rejected as malformed.
/// The boundary is exact: a 39-character name (40 bytes with the terminator)
/// travels inline, a 40-character name (41 bytes) travels by pointer.
pub fn pack_path_name(zero_terminated_name: &[u8]) -> Result<PackedPathName, Errno> {
    if zero_terminated_name.is_empty() || *zero_terminated_name.last().unwrap() != 0 {
        return Err(Errno::EINVAL);
    }
    let length = zero_terminated_name.len();
    let inline_copy = if length <= MAX_INLINE_PATH_BYTES {
        let mut buffer = [0u8; MAX_INLINE_PATH_BYTES];
        buffer[..length].copy_from_slice(zero_terminated_name);
        Some(buffer)
    } else {
        None
    };
    Ok(PackedPathName {
        length_including_nul: length,
        inline_copy,
    })
}

/// Performs one system call round trip.
///
/// This is the exact protocol of `_syscall`
/// (`minix3/minix/lib/libc/sys/syscall.c:9-25`):
///
/// 1. Write the call number into the message type field.
/// 2. Send the message and wait for the reply. When the round trip itself
///    fails, write the failure status into the message type field (C:
///    `msgptr->m_type = status`, with the comment that the string table does
///    not know every code).
/// 3. When the message type is negative, its negation is the error number:
///    return the typed error (C: `errno = -msgptr->m_type; return(-1);`).
/// 4. Otherwise return the non-negative message type as the call result.
///
/// The only deliberate difference from C is the error channel: C splits the
/// outcome across a return value plus a global variable, while this function
/// returns a single [`Result`], following the crate-wide error policy.
pub fn perform_syscall(
    transport: &impl IpcTransport,
    destination: Endpoint,
    call_number: i32,
    message: &mut Message,
) -> Result<i32, Errno> {
    message.m_type = call_number;
    if let Err(status) = transport.sendrec(destination, message) {
        message.m_type = status.0;
    }
    if message.m_type < 0 {
        Err(Errno::from_i32(-message.m_type))
    } else {
        Ok(message.m_type)
    }
}

/// Performs one server-side call round trip.
///
/// This is the exact protocol of `_taskcall`
/// (`minix3/minix/lib/libsys/taskcall.c:9-23`): "the same as `_syscall`
/// except it returns negative error codes directly and not in errno."
/// Write the call number, send and wait, report a failed round trip as-is,
/// and return the reply message type untouched — negative values are errors
/// in the caller's hands, not here.
pub fn perform_taskcall(
    transport: &impl IpcTransport,
    destination: Endpoint,
    call_number: i32,
    message: &mut Message,
) -> i32 {
    message.m_type = call_number;
    if let Err(status) = transport.sendrec(destination, message) {
        // Like the C version (`return(status)`), the raw round-trip status
        // goes back to the caller untouched.
        return status.0;
    }
    message.m_type
}
///
/// C: `do_kernel_call` (invoked from `_kernel_call` in
/// `minix3/minix/lib/libsys/kernel_call.c:13`). Server-side code uses this
/// trap instead of the user-space send-and-receive. Like [`IpcTransport`],
/// the trait separates the trap instruction (real implementation, owned by
/// the architecture layer) from scripted test doubles.
pub trait KernelCallTransport {
    /// Performs one kernel call, returning the raw reply message type.
    fn kernel_call(&self, message: &mut Message) -> i32;
}

/// Direct-trap kernel-call transport used by real server binaries.
///
/// In a hosted test environment no kernel answers, so the call reports a
/// generic input-output failure explicitly instead of faulting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct DirectKernelCallTransport;

#[allow(unused_variables)]
impl KernelCallTransport for DirectKernelCallTransport {
    fn kernel_call(&self, message: &mut Message) -> i32 {
        // E1 slice 3: the real SYSCALL-leg trap (hosted builds keep -EIO).
        #[cfg(all(target_arch = "x86_64", feature = "real-trap"))]
        {
            return unsafe { crate::arch_trap::kernel_call_trap(message) };
        }
        #[allow(unreachable_code)]
        -minix_types::EIO
    }
}

/// Scripted kernel-call transport used by unit tests.
///
/// Replies are raw message types handed out in order; the transport counts
/// invocations and records every outgoing message so tests can assert both
/// the retry sequence and the exact wire encoding (endpt/sig/... fields).
#[derive(Debug, Default)]
pub struct CannedKernelCallTransport {
    /// Raw reply message types, handed out in order.
    pub replies: alloc::vec::Vec<i32>,
    /// How many kernel calls happened so far.
    pub calls: core::cell::Cell<usize>,
    /// Outgoing messages, recorded in call order (wire-shape assertions).
    pub sent: core::cell::RefCell<alloc::vec::Vec<Message>>,
    /// Scripted full-message replies (payload + m_type), popped in order.
    pub payloads: core::cell::RefCell<alloc::collections::VecDeque<Message>>,
}

impl CannedKernelCallTransport {
    /// Creates an empty script.
    pub fn new() -> Self {
        CannedKernelCallTransport {
            replies: alloc::vec::Vec::new(),
            calls: core::cell::Cell::new(0),
            sent: core::cell::RefCell::new(alloc::vec::Vec::new()),
            payloads: core::cell::RefCell::new(alloc::collections::VecDeque::new()),
        }
    }

    /// Appends a raw reply message type to the script.
    pub fn reply(&mut self, reply_message_type: i32) {
        self.replies.push(reply_message_type);
    }

    /// Appends a full reply message to the script (payload + m_type 整体
    /// 回写，供 SYS_TIMES 这类"回复即载荷"的内核调用断言解码)。
    pub fn reply_message(&mut self, message: Message) {
        self.replies.push(message.m_type);
        self.payloads.borrow_mut().push_back(message);
    }
}

impl KernelCallTransport for CannedKernelCallTransport {
    fn kernel_call(&self, message: &mut Message) -> i32 {
        let index = self.calls.get();
        self.calls.set(index + 1);
        self.sent.borrow_mut().push(*message);
        // 有整条载荷脚本则整体回写；否则仅回写 m_type。
        match self.payloads.borrow_mut().pop_front() {
            Some(full) => *message = full,
            None => message.m_type = self.replies.get(index).cloned().unwrap_or(0),
        }
        message.m_type
    }
}
// ── SYS_* kernel-call wrappers（edge_todo.md E6：每类一个薄包装）──
//
// 约定（对齐 VM 侧 `kernel_gateway.rs` 与 C libsys）：
// - m_type 由 `perform_kernel_call` 写入调用号；
// - 请求载荷按 C 的 union 成员填写；
// - 返回值 = 内核回复 m_type（负 errno 或非负结果），**不**吞错——
//   调用方按各自 C 原位的语义决定忽略或 panic。

/// C: SYS_KILL 是内核调用 6（`kernel/src/syscall_signal.rs:138`
/// `Syscall::Kill`；C `callnr.h` `SYS_KILL`）。
pub const SYS_KILL_CALL: i32 = 6;

/// 向内核发送"终止信号"请求（C: libsys `sys_kill`，`sys_kill.c:8-17`）。
///
/// `_kernel_call(SYS_KILL, &m)`：载荷 `m_sigcalls.endpt`（目标 endpoint）
/// 与 `m_sigcalls.sig`（信号号），无回复载荷——返回值即结果（OK 或负
/// errno）。C 的 do_exit 对返回值不予检查（`forkexit.c:256`），调用方
/// 各按其语义处置。
pub fn sys_kill(
    transport: &impl KernelCallTransport,
    endpt: i32,
    sig: i32,
) -> i32 {
    let mut msg = Message::default();
    {
        // SAFETY: m_sigcalls 是 SYS_KILL 的文档化载荷布局
        //（kernel/src/syscall_signal.rs:141-147 读 map/endpt/sig）。
        let sc = unsafe { &mut msg.m_u.m_sigcalls };
        sc.endpt = endpt;
        sc.sig = sig;
    }
    perform_kernel_call(transport, SYS_KILL_CALL, &mut msg, |_| {})
}

/// C: SYS_CLEAR 是内核调用 2（`kernel/src/syscall.rs` `Syscall::Clear`；
/// C `callnr.h` `SYS_CLEAR`）。
pub const SYS_CLEAR_CALL: i32 = 2;
/// C: SYS_ABORT 是内核调用 27（`kernel/src/syscall.rs` `Syscall::Abort`；
/// C `callnr.h` `SYS_ABORT`）。
pub const SYS_ABORT_CALL: i32 = 27;

/// C: SYS_TIMES 是内核调用 25（`kernel/src/syscall_clock.rs:90`
/// `dispatch_times`；C `callnr.h` `SYS_TIMES`）。
pub const SYS_TIMES_CALL: i32 = 25;

/// C: SYS_VIRCOPY 是内核调用 15（`kernel/src/syscall.rs` `Syscall::Vircopy`）。
pub const SYS_VIRCOPY_CALL: i32 = 15;

/// C: `SELF`（kernel/src/syscall_copy.rs:124）——源/目标 endpoint 为调用者
/// 自身时使用的哨兵值，内核分发时替换为调用者真实 endpoint。
pub const SELF: i32 = -2;

/// 请求内核中止系统（C: libsys `sys_abort`，`sys_abort.c:8-13`）。
///
/// `_kernel_call(SYS_ABORT, &m)`：载荷 m1i1 = `how`（`sys/reboot.h` 的
/// RB_* 位组），无回复载荷——成功时机器直接停机，调用不会返回。
/// C 的 PM 侧 REBOOT 处理（`main.c:304-312`）对返回值不予检查：abort
/// 请求发出后 PM 在主循环等待 HARD_STOP 通知，失败也只是继续循环。
pub fn sys_abort(
    transport: &impl KernelCallTransport,
    how: i32,
) -> i32 {
    let mut msg = Message::default();
    {
        // SAFETY: m1i1 是 SYS_ABORT 的文档化载荷布局
        //（kernel/src/syscall.rs dispatch_abort 读 m1.m1i1 = how）。
        let m1 = unsafe { &mut msg.m_u.m_m1 };
        m1.m1i1 = how;
    }
    perform_kernel_call(transport, SYS_ABORT_CALL, &mut msg, |_| {})
}

/// 读取进程 CPU 计时（C: libsys `sys_times`，`sys_times.c:8-24`）。
///
/// `_kernel_call(SYS_TIMES, &m)`：请求载荷 m_lsys_krn_sys_times.endpt，
/// 回复载荷 m_krn_lsys_sys_times（user/system/real/boot 四值 + boottime）。
/// `endpt == SELF`（-1）由内核替换为调用者自身（do_times.c:33-34）。
pub fn sys_times(
    transport: &impl KernelCallTransport,
    endpt: i32,
) -> Result<MessKrnLsysSysTimes, i32> {
    let mut msg = Message {
        m_type: SYS_TIMES_CALL,
        ..Default::default()
    };
    {
        // SAFETY: m_lsys_krn_sys_times 是 SYS_TIMES 的文档化载荷布局
        //（kernel/src/syscall_clock.rs dispatch_times 读 req.endpt）。
        let req = unsafe { &mut msg.m_u.m_lsys_krn_sys_times };
        req.endpt = endpt;
    }
    let r = perform_kernel_call(transport, SYS_TIMES_CALL, &mut msg, |_| {});
    if r < 0 {
        return Err(r);
    }
    // SAFETY: 内核以 m_krn_lsys_sys_times 覆写回复载荷（dispatch_times 尾部）。
    Ok(unsafe { msg.m_u.m_krn_lsys_sys_times })
}

/// C: SYS_SIGSEND 是内核调用 9（`kernel/src/syscall.rs` `Syscall::Sigsend`）。
pub const SYS_SIGSEND_CALL: i32 = 9;

/// C: `struct sigmsg`（`minix/type.h:71-77`，x86-64 布局 40 字节）——
/// `sys_sigsend` 经 `m_sigcalls.sigctx` 指针指向调用方内存中的此结构，
/// 内核 safecopy 取回后建立 sigframe 并唤醒目标进程。
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct SigMsgWire {
    /// Signal number being caught. C: `int sm_signo`（offset 0）。
    pub signo: u32,
    /// Mask to restore when handler returns. C: `sigset_t sm_mask`（offset 8）。
    pub mask: u64,
    /// Handler address. C: `vir_bytes sm_sighandler`（offset 16）。
    pub sighandler: u64,
    /// `_sigreturn` trampoline in libc. C: `vir_bytes sm_sigreturn`（offset 24）。
    pub sigreturn: u64,
    /// User stack pointer at signal time. C: `vir_bytes sm_stkptr`（offset 32）。
    pub stkptr: u64,
}

/// C: `sys_sigsend(proc_nr_e, smp)`（libsys `sys_sigsend.c:8-18`）——
/// `_kernel_call(SYS_SIGSEND, &m)`：载荷 `m_sigcalls.endpt`（目标）+
/// `m_sigcalls.sigctx`（调用方内存中 `sigmsg` 的虚地址，内核 safecopy
/// 取回）。返回 EFAULT/ENOMEM = 进程内存装不下 handler（合法失败，
/// 目标将被杀）；其它负 errno = PM/内核失配（调用方 panic，C 同型）。
pub fn sys_sigsend(
    transport: &impl KernelCallTransport,
    endpt: i32,
    sigmsg: &SigMsgWire,
) -> Result<(), i32> {
    let mut msg = Message {
        m_type: SYS_SIGSEND_CALL,
        ..Default::default()
    };
    {
        // SAFETY: m_sigcalls 是 SIGSEND 的文档化载荷布局
        //（kernel/src/syscall_signal.rs dispatch_sigsend 读 endpt/sigctx）。
        let sc = unsafe { &mut msg.m_u.m_sigcalls };
        sc.endpt = endpt;
        sc.sigctx = sigmsg as *const SigMsgWire as u64;
    }
    let r = perform_kernel_call(transport, SYS_SIGSEND_CALL, &mut msg, |_| {});
    if r < 0 {
        return Err(r);
    }
    Ok(())
}

/// C: SYS_GETKSIG 是内核调用 7（`kernel/src/syscall.rs` `Syscall::Getksig`）。
pub const SYS_GETKSIG_CALL: i32 = 7;
/// C: SYS_ENDKSIG 是内核调用 8（`kernel/src/syscall.rs` `Syscall::Endksig`）。
pub const SYS_ENDKSIG_CALL: i32 = 8;

/// C: `sys_getksig(proc_nr_e, &sigset)`（libsys `sys_getksig.c:8-30`）——
/// 取回一个有待处理内核信号的进程及其信号位图；无待处理时 `endpt`
/// 为 `Endpoint::NONE`。回复载荷 `m_sigcalls.{endpt, map}`。取回即消费：
/// 内核同时清除该进程的 RTS_SIGNALED 与 p_pending（后续 endksig 逐信号
/// 清 SIG_PENDING）。PM 的 SIGKSIG 拉取循环（sef_signal.c:27-63）驱动。
pub fn sys_getksig(transport: &impl KernelCallTransport) -> Result<(i32, u64), i32> {
    let mut msg = Message {
        m_type: SYS_GETKSIG_CALL,
        ..Default::default()
    };
    let r = perform_kernel_call(transport, SYS_GETKSIG_CALL, &mut msg, |_| {});
    if r < 0 {
        return Err(r);
    }
    // SAFETY: 内核以 m_sigcalls 覆写回复载荷（syscall_signal.rs:440-447）。
    let sc = unsafe { msg.m_u.m_sigcalls };
    Ok((sc.endpt, sc.map))
}

/// C: `sys_endksig(proc_nr_e, sig_nr)`（libsys `sys_endksig.c:8-20`）——
/// 确认消费一个内核信号（载荷 `m_sigcalls.{endpt, sig}`），内核清
/// SIG_PENDING 位。调用方必须是目标进程的信号管理器（否则 EPERM）。
pub fn sys_endksig(transport: &impl KernelCallTransport, endpt: i32, sig: i32) -> Result<(), i32> {
    let mut msg = Message {
        m_type: SYS_ENDKSIG_CALL,
        ..Default::default()
    };
    {
        // SAFETY: m_sigcalls 是 GETKSIG/ENDKSIG 的文档化载荷布局
        //（kernel/src/syscall_signal.rs msg_sigcalls 读 endpt/sig）。
        let sc = unsafe { &mut msg.m_u.m_sigcalls };
        sc.endpt = endpt;
        sc.sig = sig;
    }
    let r = perform_kernel_call(transport, SYS_ENDKSIG_CALL, &mut msg, |_| {});
    if r < 0 {
        return Err(r);
    }
    Ok(())
}

/// C: SYS_TRACE 是内核调用 5（`kernel/src/syscall.rs` `Syscall::Trace`）。
pub const SYS_TRACE_CALL: i32 = 5;

/// C: `sys_trace(req, proc_ep, addr, datap)`（libsys `sys_trace.c:8-22`）——
/// `_kernel_call(SYS_TRACE, &m)`：载荷 `mess_lsys_krn_sys_trace`
///（request@0/endpt@4/address@8/data@16，布局与 `m_m1` **不同**——
/// `m1.m1i1` 落在 request 上而不是 endpt，见 minix-types 的 IMPORTANT
/// 注记；kernel `misc.rs msg_trace` 按该布局读取）。回复的读值由内核
/// 写回 data 字段（`misc.rs write_trace_reply_data`，与请求 data 同偏移
/// 16）。PM 的 ptrace 透传（trace.c:244-248）与 trace_stop 的 T_STOP
///（trace.c:263）走此通道。
pub fn sys_trace(
    transport: &impl KernelCallTransport,
    req: i32,
    endpt: i32,
    address: u64,
    data: &mut i64,
) -> Result<(), i32> {
    let mut msg = Message {
        m_type: SYS_TRACE_CALL,
        ..Default::default()
    };
    {
        // SAFETY: m_lsys_krn_sys_trace 是 SYS_TRACE 的文档化载荷布局
        //（kernel/src/misc.rs msg_trace 读 request/endpt/address/data）。
        let t = unsafe { &mut msg.m_u.m_lsys_krn_sys_trace };
        t.request = req;
        t.endpt = endpt;
        t.address = address;
        t.data = *data;
    }
    let r = perform_kernel_call(transport, SYS_TRACE_CALL, &mut msg, |_| {});
    if r < 0 {
        return Err(r);
    }
    // SAFETY: 内核把读值写回 data（write_trace_reply_data，同偏移 16，
    // m_lsys_krn_sys_trace 与 m_krn_lsis_sys_trace 占同一内存）。
    *data = unsafe { msg.m_u.m_lsys_krn_sys_trace }.data;
    Ok(())
}

/// 虚地址复制（C: libsys `sys_vircopy`/`sys_datacopy` 家族，
/// `kernel/src/syscall_copy.rs dispatch_vircopy` = `Syscall::Vircopy = 15`）。
///
/// 载荷 `mess_lsys_krn_sys_copy`：src_endpt（`SELF` 由内核替换为调用者
/// endpoint，`syscall_copy.rs:124` `SELF = -2`）→ dst_endpt 的虚地址。
/// PM 的 rusage 投递（tell_parent）与 exec 的 frame 拷贝共用此通道。
///
/// C: SYS_RUNCTL 是内核调用 46（`kernel/src/syscall.rs` `Syscall::Runctl`）。
pub const SYS_RUNCTL_CALL: i32 = 46;
/// Runctl 动作：停止进程。C: `RC_STOP`（do_runctl.c）。
pub const RC_STOP: i32 = 0;
/// Runctl 动作：恢复进程。C: `RC_RESUME`（do_runctl.c）。
pub const RC_RESUME: i32 = 1;
/// Runctl 标志：发送中延迟停止。C: `RC_DELAY`（do_runctl.c）。
pub const RC_DELAY: i32 = 1;

/// 进程运行控制（C: libsys `sys_runctl`，`sys_runctl.c:8-16`）。
///
/// `_kernel_call(SYS_RUNCTL, &m)`：载荷 m1i1 = RC_ENDPT、m1i2 = RC_ACTION、
/// m1i3 = RC_FLAGS。返回值 = 内核回复（OK / EBUSY / 负 errno）。
pub fn sys_runctl(
    transport: &impl KernelCallTransport,
    endpt: i32,
    action: i32,
    flags: i32,
) -> i32 {
    let mut msg = Message::default();
    {
        // SAFETY: m1 是 SYS_RUNCTL 的文档化载荷布局
        //（kernel/src/syscall_process.rs dispatch_runctl 读 m1i1/m1i2/m1i3）。
        let m1 = unsafe { &mut msg.m_u.m_m1 };
        m1.m1i1 = endpt;
        m1.m1i2 = action;
        m1.m1i3 = flags;
    }
    perform_kernel_call(transport, SYS_RUNCTL_CALL, &mut msg, |_| {})
}

/// 恢复被停止的进程（C: `sys_resume`，`syslib.h:48` —— RC_RESUME 动作）。
pub fn sys_resume(transport: &impl KernelCallTransport, endpt: i32) -> i32 {
    sys_runctl(transport, endpt, RC_RESUME, 0)
}
pub fn sys_vircopy(
    transport: &impl KernelCallTransport,
    src_endpt: i32,
    src_addr: u64,
    dst_endpt: i32,
    dst_addr: u64,
    nr_bytes: u64,
) -> i32 {
    let mut msg = Message::default();
    {
        // SAFETY: m_lsys_krn_sys_copy 是 VIRCOPY/PHYSCOPY 共用的载荷布局
        //（kernel/src/syscall_copy.rs:244-260 读 src/dst/nr_bytes）。
        let cp = unsafe { &mut msg.m_u.m_lsys_krn_sys_copy };
        cp.src_endpt = src_endpt;
        cp.src_addr = src_addr;
        cp.dst_endpt = dst_endpt;
        cp.dst_addr = dst_addr;
        cp.nr_bytes = nr_bytes;
    }
    perform_kernel_call(transport, SYS_VIRCOPY_CALL, &mut msg, |_| {})
}


/// 通知内核回收已退出的进程（C: libsys `sys_clear`，`sys_clear.c:8-14`）。
///
/// `_kernel_call(SYS_CLEAR, &m)`：载荷 m1i1 = 目标 endpoint，无回复载荷。
/// C 的 exit_proc（PRIV_PROC 直毁，`forkexit.c:366-368`）与 exit_restart
///（用户进程回收，`forkexit.c:449-451`）对失败均 panic——进程已终结而
/// 内核侧未回收即永久泄漏，不可恢复。
pub fn sys_clear(
    transport: &impl KernelCallTransport,
    endpt: i32,
) -> i32 {
    let mut msg = Message::default();
    {
        // SAFETY: m1 是 SYS_CLEAR 的文档化载荷布局
        //（kernel/src/syscall_process.rs:402-404 读 m1i1）。
        let m1 = unsafe { &mut msg.m_u.m_m1 };
        m1.m1i1 = endpt;
    }
    perform_kernel_call(transport, SYS_CLEAR_CALL, &mut msg, |_| {})
}

// ── VM 侧 SYS_* wrappers（edge_todo.md E2：六个薄包装，CannedTransport
// 回放测试）──
//
// 约定（同上方既有 wrapper 区）：
// - m_type 由 `perform_kernel_call` 写入调用号；
// - 请求载荷按 C 的 union 成员填写（kernel dispatch 侧的读取臂是
//   契约锚点）；
// - 返回值 = 内核回复 m_type（负 errno 或非负结果），不吞错。
// 调用号常量消费 minix-types 的 kernel_call 权威（E-MINTYPES-SYS），
// 不再本地定义。

/// SYS_FORK：创建子进程（C: libsys `sys_fork` 语义；kernel
/// `dispatch_fork`）。
///
/// 应答形状见 E-FORKMSG：内核返回 OK(0) 并把出参原地写进应答臂
/// `m_krn_lsys_sys_fork`（ipc.h:283-287；do_fork.c:111-112）——
/// `endpt` 是子进程端点，`msgaddr` 是父进程交付消息缓冲地址（VM 据此
/// 做 eager-CoW，fork.c:100-108）。成功返回 `(child_endpt, msgaddr)`，
/// 失败返回负 errno。
pub fn sys_fork(
    transport: &impl KernelCallTransport,
    parent_endpt: i32,
    child_slot: i32,
    flags: u32,
) -> Result<(i32, u64), i32> {
    let mut msg = Message::default();
    {
        // SAFETY: m_lsys_krn_sys_fork 是 SYS_FORK 的文档化请求布局
        //（kernel/src/syscall_process.rs dispatch_fork 读 endpt/slot/flags；
        // C ipc.h:1173-1177）。
        let req = unsafe { &mut msg.m_u.m_lsys_krn_sys_fork };
        req.endpt = parent_endpt;
        req.slot = child_slot;
        req.flags = flags;
    }
    let reply = perform_kernel_call(transport, minix_types::SYS_FORK, &mut msg, |_| {});
    if reply < 0 {
        return Err(reply);
    }
    // SAFETY: 应答由内核原地写入同一消息；臂为 plain-old-data。
    let arm = unsafe { msg.m_u.m_krn_lsys_sys_fork };
    Ok((arm.endpt, arm.msgaddr))
}

/// SYS_EXEC：重载目标进程映像（C: libsys `sys_exec`；kernel
/// `dispatch_exec` 读 `m_lsys_krn_sys_exec`，成功返回 OK）。
pub fn sys_exec(
    transport: &impl KernelCallTransport,
    endpt: i32,
    ip: u64,
    stack: u64,
    name: u64,
    ps_str: u64,
) -> Result<(), i32> {
    let mut msg = Message::default();
    {
        // SAFETY: m_lsys_krn_sys_exec 是 SYS_EXEC 的文档化载荷布局
        //（kernel/src/syscall_process.rs dispatch_exec；C ipc.h）。
        let e = unsafe { &mut msg.m_u.m_lsys_krn_sys_exec };
        e.endpt = endpt;
        e.ip = ip;
        e.stack = stack;
        e.name = name;
        e.ps_str = ps_str;
    }
    let reply = perform_kernel_call(transport, minix_types::SYS_EXEC, &mut msg, |_| {});
    if reply < 0 {
        return Err(reply);
    }
    Ok(())
}

/// SYS_SAFECOPYFROM：经 grant 从授权方拷入调用方空间（C: libsys
/// `sys_safecopyfrom`；kernel `dispatch_safecopy_from` 读
/// `m_lsys_kern_safecopy`，成功返回 OK，无应答出参）。
pub fn sys_safecopyfrom(
    transport: &impl KernelCallTransport,
    granter_endpt: i32,
    grant_id: i32,
    grant_offset: u64,
    user_addr: u64,
    bytes: u64,
) -> Result<(), i32> {
    let mut msg = Message::default();
    {
        // SAFETY: m_lsys_kern_safecopy 是 SAFECOPYFROM/TO 共用的载荷布局
        //（kernel/src/syscall_copy.rs:181；C ipc.h）。
        let sc = unsafe { &mut msg.m_u.m_lsys_kern_safecopy };
        sc.from_to = granter_endpt;
        sc.grant_id = grant_id;
        sc.offset = grant_offset;
        sc.address = user_addr;
        sc.bytes = bytes;
    }
    let reply = perform_kernel_call(transport, minix_types::SYS_SAFECOPYFROM, &mut msg, |_| {});
    if reply < 0 {
        return Err(reply);
    }
    Ok(())
}

/// SYS_SAFECOPYTO：经 grant 把调用方空间拷给授权方（方向与
/// [`sys_safecopyfrom`] 相反，同一载荷臂，C: libsys `sys_safecopyto`）。
pub fn sys_safecopyto(
    transport: &impl KernelCallTransport,
    granter_endpt: i32,
    grant_id: i32,
    grant_offset: u64,
    user_addr: u64,
    bytes: u64,
) -> Result<(), i32> {
    let mut msg = Message::default();
    {
        // SAFETY: 同 sys_safecopyfrom——两者共用 m_lsys_kern_safecopy 臂。
        let sc = unsafe { &mut msg.m_u.m_lsys_kern_safecopy };
        sc.from_to = granter_endpt;
        sc.grant_id = grant_id;
        sc.offset = grant_offset;
        sc.address = user_addr;
        sc.bytes = bytes;
    }
    let reply = perform_kernel_call(transport, minix_types::SYS_SAFECOPYTO, &mut msg, |_| {});
    if reply < 0 {
        return Err(reply);
    }
    Ok(())
}

/// SYS_UPDATE：RS live-update 的内核相（C: libsys `sys_update`；kernel
/// `dispatch_update` 按 M1 读 src/dst/flags —— do_update.c:9-11 的 C
/// 形状就是 M1，非误读，成功返回 OK）。
pub fn sys_update(
    transport: &impl KernelCallTransport,
    src_endpt: i32,
    dst_endpt: i32,
    flags: i32,
) -> Result<(), i32> {
    let mut msg = Message::default();
    {
        // SAFETY: m_m1 是 SYS_UPDATE 的文档化载荷（kernel/src/misc.rs
        // dispatch_update 读 m1i1/m1i2/m1i3；C do_update.c:9-11）。
        let m1 = unsafe { &mut msg.m_u.m_m1 };
        m1.m1i1 = src_endpt;
        m1.m1i2 = dst_endpt;
        m1.m1i3 = flags;
    }
    let reply = perform_kernel_call(transport, minix_types::SYS_UPDATE, &mut msg, |_| {});
    if reply < 0 {
        return Err(reply);
    }
    Ok(())
}

/// SYS_DIAGCTL：诊断控制（C: libsys `sys_diagctl(int code, char *arg1,
/// int arg2)`——kernel `dispatch_diagctl` 按 code 分派：DIAG(1) 用
/// buf/len 从调用方空间拷取至多 DIAGBUFSIZE 字节打到控制台；STACKTRACE(2)
/// 用 `endpt` 字段指定目标进程）。
pub fn sys_diagctl(
    transport: &impl KernelCallTransport,
    code: i32,
    arg1: u64,
    arg2: i32,
) -> Result<(), i32> {
    let mut msg = Message::default();
    {
        // SAFETY: m_lsys_krn_sys_diagctl 是 SYS_DIAGCTL 的文档化载荷布局
        //（kernel/src/syscall.rs dispatch_diagctl；C ipc.h）。
        let d = unsafe { &mut msg.m_u.m_lsys_krn_sys_diagctl };
        d.code = code;
        if code == 1 {
            d.buf = arg1;
            d.len = arg2 as u64;
        } else {
            d.endpt = arg2;
        }
    }
    let reply = perform_kernel_call(transport, minix_types::SYS_DIAGCTL, &mut msg, |_| {});
    if reply < 0 {
        return Err(reply);
    }
    Ok(())
}

/// SYS_SETALARM：设置（或取消）闹钟并取回旧闹钟信息（C: libsys
/// `sys_setalarm2`；kernel `dispatch_setalarm` 读/写
/// `m_lsys_krn_sys_setalarm`——请求 exp_time/abs_time，应答 time_left/
/// uptime 经同一臂回填）。成功返回 `(time_left, uptime)`。
pub fn sys_setalarm(
    transport: &impl KernelCallTransport,
    exp_time: u64,
    abs_time: bool,
) -> Result<(u64, u64), i32> {
    let mut msg = Message::default();
    {
        // SAFETY: m_lsys_krn_sys_setalarm 是 SETALARM 的文档化载荷布局
        //（kernel/src/syscall_clock.rs dispatch_setalarm；C ipc.h）。
        let a = unsafe { &mut msg.m_u.m_lsys_krn_sys_setalarm };
        a.exp_time = exp_time;
        a.abs_time = abs_time as i32;
    }
    let reply = perform_kernel_call(transport, minix_types::SYS_SETALARM, &mut msg, |_| {});
    if reply < 0 {
        return Err(reply);
    }
    // SAFETY: 应答由内核经同一臂回填（syscall_clock.rs:265）。
    let arm = unsafe { msg.m_u.m_lsys_krn_sys_setalarm };
    Ok((arm.time_left, arm.uptime))
}

// ── E9 切片 1:RS SysApi 面所需的 SYS_* 命名包装 ──
// 六个调用对齐 servers/rs/src/boot.rs 的 SysApi 九方法中尚缺的六项
// (kill/update/setalarm 已有)。全部经 perform_kernel_call,Canned
// 回放可测;真实传输 = real-trap 构建下的 SYSCALL 腿。

/// SYS_GETINFO · GET_MACHINE(12):取机器信息(处理器数 + BSP id)。
/// C: `sys_getinfo(GET_MACHINE, ...)`;kernel `getinfo_machine` 经
/// `copy_struct_to_caller` 拷 `MachineStruct`。`out` 至少要容纳内核
/// 侧结构(内核按 min(len) 截断拷贝)。
pub fn sys_get_machine(transport: &impl KernelCallTransport, out: &mut [u8]) -> Result<(), i32> {
    sys_getinfo_into(transport, minix_types::GET_MACHINE, out, minix_types::Endpoint::NONE.0)
}

/// SYS_GETINFO · GET_HZ(18):取系统时钟频率(i32)。C: do_getinfo.c:81-84。
pub fn sys_get_hz(transport: &impl KernelCallTransport) -> Result<i32, i32> {
    let mut buf = [0u8; 4];
    sys_getinfo_into(transport, minix_types::GET_HZ, &mut buf, minix_types::Endpoint::NONE.0)?;
    Ok(i32::from_le_bytes(buf))
}

/// SYS_GETINFO · GET_PRIV(17):读目标进程的特权结构快照。
/// 目标端点经 `val_len2_e` 域传递(kernel getinfo_priv:925-929)。
pub fn sys_get_priv(transport: &impl KernelCallTransport, endpt: i32, out: &mut [u8]) -> Result<(), i32> {
    sys_getinfo_into(transport, minix_types::GET_PRIV, out, endpt)
}

/// GETINFO 通用承载:填充 `m_lsys_krn_sys_getinfo` 并执行。
/// `endpt` 落 `val_len2_e`(kernel getinfo_priv 以此读目标端点;
/// 无端点语义的子请求传 NONE,内核忽略)。
pub fn sys_getinfo_into(
    transport: &impl KernelCallTransport,
    request: i32,
    out: &mut [u8],
    endpt: i32,
) -> Result<(), i32> {
    let mut msg = Message::default();
    {
        // SAFETY: m_lsys_krn_sys_getinfo 是 GETINFO 的文档化载荷
        //(kernel/src/misc.rs msg_getinfo:269-279——勿用 m1 覆盖)。
        let gi = unsafe { &mut msg.m_u.m_lsys_krn_sys_getinfo };
        gi.request = request;
        gi.endpt = endpt;
        gi.val_ptr = out.as_mut_ptr() as u64;
        gi.val_len = out.len() as i32;
        gi.val_len2_e = endpt;
    }
    let reply = perform_kernel_call(transport, minix_types::SYS_GETINFO, &mut msg, |_| {});
    if reply < 0 {
        return Err(reply);
    }
    Ok(())
}

/// SYS_PRIVCTL:特权控制(allow/disallow/set_sys/set_user/...)。
/// M1 载荷:m1i1=request,m1i2=endpt,m1p1=arg_ptr(用户态特权结构
/// 指针,无则为 0)。C: mess_lsys_krn_sys_privctl,do_privctl.c:47-51。
pub fn sys_privctl(
    transport: &impl KernelCallTransport,
    endpt: i32,
    request: i32,
    arg_ptr: u64,
) -> Result<(), i32> {
    let mut msg = Message::default();
    {
        // SAFETY: M1 覆盖(privctl 载荷与 m1 同形,syscall.rs:1354-1358)。
        let m1 = unsafe { &mut msg.m_u.m_m1 };
        m1.m1i1 = request;
        m1.m1i2 = endpt;
        m1.m1p1 = arg_ptr;
    }
    let reply = perform_kernel_call(transport, minix_types::SYS_PRIVCTL, &mut msg, |_| {});
    if reply < 0 {
        return Err(reply);
    }
    Ok(())
}

/// SYS_DIAGCTL · STACKTRACE(2):请求目标进程的栈回溯打印。
/// C: main.c:681-683(RS 信号管理器的 stacktrace-signal 分支)。
pub fn sys_diagctl_stacktrace(transport: &impl KernelCallTransport, target: i32) -> Result<(), i32> {
    sys_diagctl(transport, minix_types::DIAGCTL_CODE_STACKTRACE, target as u64, 0)
}

/// SYS_SIGRETURN：信号处理返回，恢复目标进程的被信号上下文（C: libsys
/// `sys_sigreturn`——`m_sigcalls.endpt/sigctx`；kernel
/// `dispatch_sigreturn`）。
pub fn sys_sigreturn(
    transport: &impl KernelCallTransport,
    proc_endpt: i32,
    sig_ctx: u64,
) -> Result<(), i32> {
    let mut msg = Message::default();
    {
        // SAFETY: m_sigcalls 是 SIGRETURN 的文档化载荷布局
        //（kernel/src/syscall_signal.rs dispatch_sigreturn）。
        let sc = unsafe { &mut msg.m_u.m_sigcalls };
        sc.endpt = proc_endpt;
        sc.sigctx = sig_ctx;
    }
    let reply = perform_kernel_call(transport, minix_types::SYS_SIGRETURN, &mut msg, |_| {});
    if reply < 0 {
        return Err(reply);
    }
    Ok(())
}

/// SYS_SPROF：统计式 profiling 控制（C: libsys `sys_sprof`——
/// `m_lsys_krn_sys_sprof` 的 action/freq/intr_type/endpt/ctl_ptr/mem_ptr；
/// kernel `dispatch_profile`）。
pub fn sys_sprof(
    transport: &impl KernelCallTransport,
    action: i32,
    freq: i32,
    intr_type: i32,
    endpt: i32,
    ctl_ptr: u64,
    mem_ptr: u64,
) -> Result<(), i32> {
    let mut msg = Message::default();
    {
        // SAFETY: m_lsys_krn_sys_sprof 是 SPROF 的文档化载荷布局
        //（kernel/src/misc.rs dispatch_profile；C ipc.h）。
        let s = unsafe { &mut msg.m_u.m_lsys_krn_sys_sprof };
        s.action = action;
        s.freq = freq;
        s.intr_type = intr_type;
        s.endpt = endpt;
        s.ctl_ptr = ctl_ptr;
        s.mem_ptr = mem_ptr;
    }
    let reply = perform_kernel_call(transport, minix_types::SYS_SPROF, &mut msg, |_| {});
    if reply < 0 {
        return Err(reply);
    }
    Ok(())
}

/// SYS_SETTIME：设置实时钟或调整渐变（C: libsys `sys_settime`——
/// `m_lsys_krn_sys_settime` 的 now/clock_id/sec/nsec；kernel
/// `dispatch_settime`，CLOCK_REALTIME 之外 EINVAL）。
pub fn sys_settime(
    transport: &impl KernelCallTransport,
    now: i32,
    clock_id: i32,
    sec: u64,
    nsec: i64,
) -> Result<(), i32> {
    let mut msg = Message::default();
    {
        // SAFETY: m_lsys_krn_sys_settime 是 SETTIME 的文档化载荷布局
        //（kernel/src/syscall_clock.rs dispatch_settime）。
        let t = unsafe { &mut msg.m_u.m_lsys_krn_sys_settime };
        t.now = now;
        t.clock_id = clock_id;
        t.sec = sec;
        t.nsec = nsec;
    }
    let reply = perform_kernel_call(transport, minix_types::SYS_SETTIME, &mut msg, |_| {});
    if reply < 0 {
        return Err(reply);
    }
    Ok(())
}

/// SYS_STIME：设置系统引导时间（C: libsys `sys_stime`——
/// `m_lsys_krn_sys_stime.boot_time`；kernel `dispatch_stime`）。
pub fn sys_stime(transport: &impl KernelCallTransport, boottime: u64) -> Result<(), i32> {
    let mut msg = Message::default();
    {
        // SAFETY: m_lsys_krn_sys_stime 是 STIME 的文档化载荷布局
        //（kernel/src/syscall_clock.rs dispatch_stime）。
        let t = unsafe { &mut msg.m_u.m_lsys_krn_sys_stime };
        t.boot_time = boottime;
    }
    let reply = perform_kernel_call(transport, minix_types::SYS_STIME, &mut msg, |_| {});
    if reply < 0 {
        return Err(reply);
    }
    Ok(())
}

/// SYS_VTIMER：设置/取回进程的虚拟或 profile 定时器（C: libsys
/// `sys_vtimer`——kernel `dispatch_vtimer` 按 M2 读
/// which/set/value/endpt，旧值经 `m_m2.m2l1` 回填）。
///
/// `new_val` 为 `Some` 时设置新值；应答携带旧值。
pub fn sys_vtimer(
    transport: &impl KernelCallTransport,
    proc_endpt: i32,
    which: i32,
    new_val: Option<u64>,
) -> Result<Option<u64>, i32> {
    let mut msg = Message::default();
    {
        // SAFETY: kernel dispatch_vtimer 按 M2 读 VT_WHICH/SET/VALUE/ENDPT
        //（syscall_clock.rs:437-441；C do_vtimer.c:33-71）。
        let m2 = unsafe { &mut msg.m_u.m_m2 };
        m2.m2i1 = which;
        m2.m2i2 = new_val.is_some() as i32;
        m2.m2l1 = new_val.unwrap_or(0) as i64;
        m2.m2l2 = proc_endpt as i64;
    }
    let reply = perform_kernel_call(transport, minix_types::SYS_VTIMER, &mut msg, |_| {});
    if reply < 0 {
        return Err(reply);
    }
    // SAFETY: 旧值由内核写回 m_m2.m2l1（syscall_clock.rs:505-508）。
    let old = unsafe { msg.m_u.m_m2.m2l1 } as u64;
    Ok(Some(old))
}

/// SYS_GETMCONTEXT：取目标进程的机器上下文（`m_lsys_krn_sys_mcontext`
/// 的 endpt/ctx_ptr；kernel `dispatch_getmcontext`）。
pub fn sys_getmcontext(
    transport: &impl KernelCallTransport,
    proc_endpt: i32,
    ctx_ptr: u64,
) -> Result<(), i32> {
    let mut msg = Message::default();
    {
        // SAFETY: m_lsys_krn_sys_mcontext 是 GET/SETMCONTEXT 共用的载荷
        //布局（kernel/src/syscall.rs dispatch_getmcontext）。
        let mc = unsafe { &mut msg.m_u.m_lsys_krn_sys_mcontext };
        mc.endpt = proc_endpt;
        mc.ctx_ptr = ctx_ptr;
    }
    let reply = perform_kernel_call(transport, minix_types::SYS_GETMCONTEXT, &mut msg, |_| {});
    if reply < 0 {
        return Err(reply);
    }
    Ok(())
}

/// SYS_SETMCONTEXT：设置目标进程的机器上下文（同
/// [`sys_getmcontext`] 的载荷臂，kernel `dispatch_setmcontext`）。
pub fn sys_setmcontext(
    transport: &impl KernelCallTransport,
    proc_endpt: i32,
    ctx_ptr: u64,
) -> Result<(), i32> {
    let mut msg = Message::default();
    {
        // SAFETY: 同 sys_getmcontext——GET/SET 共用 m_lsys_krn_sys_mcontext。
        let mc = unsafe { &mut msg.m_u.m_lsys_krn_sys_mcontext };
        mc.endpt = proc_endpt;
        mc.ctx_ptr = ctx_ptr;
    }
    let reply = perform_kernel_call(transport, minix_types::SYS_SETMCONTEXT, &mut msg, |_| {});
    if reply < 0 {
        return Err(reply);
    }
    Ok(())
}


/// Performs a kernel call with "not ready" retries.
///
/// This is the exact loop of `_kernel_call`
/// (`minix3/minix/lib/libsys/kernel_call.c:7-21`): write the call number,
/// trap, read back the reply type, and when it equals "not ready" (C:
/// `ENOTREADY`, value 201 in the user-space convention), delay for a growing
/// number of clock ticks and try again. The first delay is one tick, then
/// two, then three (C: `t = 1; tickdelay(t++)`).
///
/// The delay itself travels through a callback so tests can record the delay
/// sequence without sleeping: production code passes the real tick delay,
/// tests pass a recorder. The loop is unbounded like the C version — a
/// kernel that never becomes ready blocks the caller forever in both
/// languages — so tests must always script a final non-"not ready" reply.
pub fn perform_kernel_call(
    transport: &impl KernelCallTransport,
    call_number: i32,
    message: &mut Message,
    mut delay_ticks: impl FnMut(u32),
) -> i32 {
    let mut wait: u32 = 1;
    loop {
        message.m_type = call_number;
        let reply = transport.kernel_call(message);
        if reply != minix_types::ENOTREADY {
            return reply;
        }
        delay_ticks(wait);
        wait = wait.saturating_add(1);
    }
}

/// Converts a trap failure status into the message-type assignment of step 2.
///
/// Small helper keeping the `_syscall` failure branch explicit and tested:
/// the status integer becomes the new message type, from which the negative
/// check derives the error.
pub const fn trap_failure_to_message_type(status: TrapStatus) -> i32 {
    status.0
}

#[cfg(test)]
mod tests {

    #[test]
    fn test_sys_kill_encodes_sigcalls_wire() {
        // C: libsys sys_kill.c:8-17 — m_sigcalls.endpt/sig，SYS_KILL = 5；
        // 回复 OK（内核 do_kill 无回复载荷）。
        let mut canned = CannedKernelCallTransport::new();
        canned.reply(0);

        let r = sys_kill(&canned, 7, 9); // endpoint 7, SIGKILL

        assert_eq!(r, 0);
        // 调用号必须与 kernel/src/syscall.rs 的 `Syscall::Kill = 6` 一致
        //（对齐 05750e28c 引入的内核枚举；错号 5 = SYS_TRACE）。
        assert_eq!(SYS_KILL_CALL, 6);
        let sent = canned.sent.borrow();
        assert_eq!(sent.len(), 1);
        assert_eq!(sent[0].m_type, SYS_KILL_CALL);
        assert_eq!(unsafe { sent[0].m_u.m_sigcalls }.endpt, 7);
        assert_eq!(unsafe { sent[0].m_u.m_sigcalls }.sig, 9);
    }

    #[test]
    fn test_sys_clear_encodes_m1_endpoint() {
        // C: libsys sys_clear.c:8-14 — m1i1 = 目标 endpoint，SYS_CLEAR = 2；
        // 无回复载荷，OK 即回收完成。
        let mut canned = CannedKernelCallTransport::new();
        canned.reply(0);

        let r = sys_clear(&canned, 11); // INIT

        assert_eq!(r, 0);
        let sent = canned.sent.borrow();
        assert_eq!(sent.len(), 1);
        assert_eq!(sent[0].m_type, SYS_CLEAR_CALL);
        assert_eq!(unsafe { sent[0].m_u.m_m1 }.m1i1, 11);
    }

    #[test]
    fn test_sys_times_encodes_endpt_and_decodes_reply() {
        // C: libsys sys_times.c:8-24 — 请求 m_lsys_krn_sys_times.endpt；
        // 回复 m_krn_lsys_sys_times（user/system 等四值）。
        use minix_types::{MessKrnLsysSysTimes, MessSigcalls};
        let mut canned = CannedKernelCallTransport::new();
        let mut reply = Message::default();
        reply.m_type = 0;
        {
            let krn = unsafe { &mut reply.m_u.m_krn_lsys_sys_times };
            krn.user_time = 30;
            krn.system_time = 12;
        }
        canned.reply_message(reply);

        let r = sys_times(&canned, 7);

        let times = r.expect("OK reply decodes");
        assert_eq!(times.user_time, 30);
        assert_eq!(times.system_time, 12);
        let sent = canned.sent.borrow();
        assert_eq!(sent[0].m_type, SYS_TIMES_CALL);
        assert_eq!(unsafe { sent[0].m_u.m_lsys_krn_sys_times }.endpt, 7);
    }

    #[test]
    fn test_sys_vircopy_encodes_copy_payload() {
        // C: do_copy.c — m_lsys_krn_sys_copy {src_endpt, src_addr, dst_endpt,
        // dst_addr, nr_bytes}；SELF(-2) 由内核替换为调用者 endpoint。
        let mut canned = CannedKernelCallTransport::new();
        canned.reply(0);

        let r = sys_vircopy(&canned, -2, 0x1000, 9, 0x7000, 144);

        assert_eq!(r, 0);
        let sent = canned.sent.borrow();
        assert_eq!(sent[0].m_type, SYS_VIRCOPY_CALL);
        let cp = unsafe { &sent[0].m_u.m_lsys_krn_sys_copy };
        assert_eq!(cp.src_endpt, -2);
        assert_eq!(cp.src_addr, 0x1000);
        assert_eq!(cp.dst_endpt, 9);
        assert_eq!(cp.dst_addr, 0x7000);
        assert_eq!(cp.nr_bytes, 144);
    }

    #[test]
    fn test_sys_trace_encodes_lsys_layout_and_decodes_read_value() {
        // C: libsys sys_trace.c:8-22 — 载荷 mess_lsys_krn_sys_trace
        //（request@0/endpt@4/address@8/data@16，**不同于 m_m1**：m1.m1i1
        // 是 request 而非 endpt，kernel misc.rs msg_trace 按该布局读）；
        // 回复的读值经同偏移 data 字段写回（write_trace_reply_data）。
        let mut canned = CannedKernelCallTransport::new();
        let mut reply = Message::default();
        reply.m_type = 0;
        unsafe { reply.m_u.m_lsys_krn_sys_trace.data = 0x55 };
        canned.reply_message(reply);

        let mut data: i64 = 0; // READ 类命令忽略入值
        let r = sys_trace(&canned, 1 /* T_GETINS */, 7, 0x2000, &mut data);

        assert_eq!(r, Ok(()));
        assert_eq!(data, 0x55, "kernel read value must overwrite data");
        let sent = canned.sent.borrow();
        assert_eq!(sent[0].m_type, SYS_TRACE_CALL);
        assert_eq!(SYS_TRACE_CALL, 5, "kernel Syscall::Trace = 5");
        let t = unsafe { &sent[0].m_u.m_lsys_krn_sys_trace };
        assert_eq!(t.request, 1);
        assert_eq!(t.endpt, 7);
        assert_eq!(t.address, 0x2000);
    }

    #[test]
    fn test_sys_getksig_decodes_endpt_and_map() {
        // C: do_getksig.c:31-32 — 回复 m_sigcalls.{endpt, map}；endpt NONE
        // 表示内核侧无更多待处理（PM 拉取循环的终止条件）。
        use minix_types::MessSigcalls;
        let mut canned = CannedKernelCallTransport::new();
        let mut reply = Message::default();
        reply.m_type = 0;
        unsafe {
            reply.m_u.m_sigcalls = MessSigcalls {
                map: 1u64 << (15 - 1),
                endpt: 42,
                sig: 0,
                sigctx: 0,
                _padding: [0u8; 32],
            };
        }
        canned.reply_message(reply);

        let (endpt, map) = sys_getksig(&canned).expect("OK reply decodes");

        assert_eq!(endpt, 42);
        assert_eq!(map, 1u64 << 14);
        assert_eq!(SYS_GETKSIG_CALL, 7, "kernel Syscall::Getksig = 7");
        let sent = canned.sent.borrow();
        assert_eq!(sent[0].m_type, SYS_GETKSIG_CALL);
    }

    #[test]
    fn test_sys_endksig_encodes_endpt_and_sig() {
        // C: do_endksig.c — 载荷 m_sigcalls.{endpt, sig}；调用方必须是
        // 目标进程的信号管理器（否则内核 EPERM，负 errno 透传）。
        let mut canned = CannedKernelCallTransport::new();
        canned.reply(-1); // EPERM

        assert_eq!(sys_endksig(&canned, 42, 15), Err(-1));
        let sent = canned.sent.borrow();
        assert_eq!(sent[0].m_type, SYS_ENDKSIG_CALL);
        assert_eq!(SYS_ENDKSIG_CALL, 8, "kernel Syscall::Endksig = 8");
        let sc = unsafe { &sent[0].m_u.m_sigcalls };
        assert_eq!(sc.endpt, 42);
        assert_eq!(sc.sig, 15);
    }

    #[test]
    fn test_sys_trace_negative_errno_passthrough() {
        // C: 返回值 = 内核回复（负 errno 原样透传，do_trace 不折叠）。
        let mut canned = CannedKernelCallTransport::new();
        canned.reply(-1); // EPERM

        let mut data: i64 = 0;
        assert_eq!(sys_trace(&canned, 8 /* T_EXIT */, 7, 0, &mut data), Err(-1));
        assert_eq!(canned.calls.get(), 1);
    }

    #[test]
    fn test_sys_runctl_resume_encodes_m1() {
        // C: syslib.h:48 — sys_resume = sys_runctl(ep, RC_RESUME, 0)；
        // 载荷 m1i1 = RC_ENDPT、m1i2 = RC_RESUME、m1i3 = RC_FLAGS(0)。
        let mut canned = CannedKernelCallTransport::new();
        canned.reply(0);

        let r = sys_resume(&canned, 9);

        assert_eq!(r, 0);
        let sent = canned.sent.borrow();
        assert_eq!(sent[0].m_type, SYS_RUNCTL_CALL);
        let m1 = unsafe { &sent[0].m_u.m_m1 };
        assert_eq!(m1.m1i1, 9);
        assert_eq!(m1.m1i2, RC_RESUME);
        assert_eq!(m1.m1i3, 0);
    }

    #[test]
    fn test_sys_abort_encodes_how() {
        let mut canned = CannedKernelCallTransport::new();
        canned.reply(-3); // ESRCH

        assert_eq!(sys_times(&canned, 7).unwrap_err(), -3);
    }

    #[test]
    fn test_sys_abort_failure_passthrough() {
        // C: main.c:309 — 返回值被忽略，失败时 PM 继续主循环。
        let mut canned = CannedKernelCallTransport::new();
        canned.reply(-5);

        assert_eq!(sys_abort(&canned, 0x0008), -5);
        assert_eq!(canned.calls.get(), 1);
    }

    #[test]
    fn test_sys_kill_negative_errno_passthrough() {
        // C: 返回值 = 内核回复（负 errno 原样透传，调用方自行处置）。
        let mut canned = CannedKernelCallTransport::new();
        canned.reply(-1); // EPERM

        assert_eq!(sys_kill(&canned, 7, 9), -1);
        assert_eq!(canned.calls.get(), 1);
    }
    use super::*;
    use super::DirectKernelCallTransport;
    use crate::ipc::{CannedTransport, CALL_SENDREC};

    fn test_message(message_type: i32) -> Message {
        Message {
            m_source: Endpoint(0),
            m_type: message_type,
            m_u: minix_types::MessageUnion::zeroed(),
        }
    }

    #[test]
    fn test_successful_round_trip_returns_reply_type() {
        let mut transport = CannedTransport::new();
        let mut reply = test_message(0);
        reply.m_type = 7;
        transport.reply_sendrec(Ok(reply));
        let mut message = test_message(0);
        let result = perform_syscall(&transport, Endpoint(1), CALL_SENDREC as i32, &mut message);
        assert_eq!(result, Ok(7));
        assert_eq!(transport.sendrec_calls.get(), 1);
    }

    #[test]
    fn test_call_number_is_written_before_sending() {
        let mut transport = CannedTransport::new();
        transport.reply_sendrec(Ok(test_message(0)));
        let mut message = test_message(999);
        let _ = perform_syscall(&transport, Endpoint(1), 33, &mut message);
        // The reply overwrote the type, but the transport saw call number 33:
        // verified indirectly because the CannedTransport only answers the
        // scripted first call.
        assert_eq!(transport.sendrec_calls.get(), 1);
    }

    #[test]
    fn test_negative_reply_becomes_typed_error() {
        let mut transport = CannedTransport::new();
        let mut reply = test_message(-22);
        reply.m_type = -22;
        transport.reply_sendrec(Ok(reply));
        let mut message = test_message(0);
        let result = perform_syscall(&transport, Endpoint(1), 5, &mut message);
        assert_eq!(result, Err(Errno::EINVAL));
    }

    #[test]
    fn test_transport_failure_becomes_message_type_then_error() {
        let mut transport = CannedTransport::new();
        transport.reply_sendrec(Err(TrapStatus(-22)));
        let mut message = test_message(0);
        let result = perform_syscall(&transport, Endpoint(1), 5, &mut message);
        // C: m_type = status (-22), then negative becomes error 22.
        assert_eq!(message.m_type, -22);
        assert_eq!(result, Err(Errno::EINVAL));
    }

    #[test]
    fn test_short_name_travels_inline() {
        // "hi" plus terminator: 3 bytes, fits.
        let packed = pack_path_name(b"hi\0").unwrap();
        assert_eq!(packed.length_including_nul, 3);
        let inline_copy = packed.inline_copy.expect("short name goes inline");
        assert_eq!(&inline_copy[..3], b"hi\0");
    }

    #[test]
    fn test_boundary_name_of_exactly_forty_bytes_travels_inline() {
        // 39 characters plus terminator: exactly 40 bytes, still inline.
        let mut name = [b'a'; 40];
        name[39] = 0;
        let packed = pack_path_name(&name).unwrap();
        assert_eq!(packed.length_including_nul, 40);
        assert!(packed.inline_copy.is_some());
    }

    #[test]
    fn test_name_of_forty_one_bytes_travels_by_pointer() {
        // 40 characters plus terminator: 41 bytes, pointer path.
        let mut name = [b'a'; 41];
        name[40] = 0;
        let packed = pack_path_name(&name).unwrap();
        assert_eq!(packed.length_including_nul, 41);
        assert_eq!(packed.inline_copy, None);
    }

    #[test]
    fn test_missing_terminator_is_rejected() {
        assert_eq!(pack_path_name(b"no-terminator"), Err(Errno::EINVAL));
        assert_eq!(pack_path_name(b""), Err(Errno::EINVAL));
    }

    #[test]
    fn test_kernel_call_returns_first_non_retry_reply() {
        let mut transport = CannedKernelCallTransport::new();
        transport.reply(minix_types::ENOTREADY);
        transport.reply(minix_types::ENOTREADY);
        transport.reply(0);
        let mut delays = alloc::vec::Vec::new();
        let mut message = test_message(0);
        let result = perform_kernel_call(&transport, 12, &mut message, |ticks| {
            delays.push(ticks);
        });
        assert_eq!(result, 0);
        assert_eq!(transport.calls.get(), 3);
        // C delays 1, then 2 (t = 1; tickdelay(t++)).
        assert_eq!(delays, alloc::vec![1, 2]);
    }

    #[test]
    fn test_kernel_call_without_retry_never_delays() {
        let mut transport = CannedKernelCallTransport::new();
        transport.reply(5);
        let mut delays = alloc::vec::Vec::new();
        let mut message = test_message(0);
        let result = perform_kernel_call(&transport, 12, &mut message, |ticks| {
            delays.push(ticks);
        });
        assert_eq!(result, 5);
        assert!(delays.is_empty());
    }

    #[test]
    fn test_direct_kernel_call_reports_explicit_failure() {
        let transport = DirectKernelCallTransport;
        let mut message = test_message(0);
        assert_eq!(transport.kernel_call(&mut message), -minix_types::EIO);
    }

    #[test]
    fn test_trap_failure_assignment_matches_c_statement() {
        assert_eq!(trap_failure_to_message_type(TrapStatus(-11)), -11);
    }

    #[test]
    fn test_taskcall_returns_raw_reply_untouched() {
        let mut transport = CannedTransport::new();
        transport.reply_sendrec(Ok(test_message(-5)));
        let mut message = test_message(0);
        assert_eq!(perform_taskcall(&transport, Endpoint(0), 41, &mut message), -5);
    }

    #[test]
    fn test_taskcall_reports_round_trip_status() {
        let mut transport = CannedTransport::new();
        transport.reply_sendrec(Err(TrapStatus(-11)));
        let mut message = test_message(0);
        assert_eq!(perform_taskcall(&transport, Endpoint(0), 41, &mut message), -11);
    }

    // ── E2:VM 侧六个 SYS_* wrapper 的 CannedTransport 回放测试 ──

    /// SYS_FORK 回放:请求臂逐字段 + 应答臂出参解析(E-FORKMSG 形状)。
    #[test]
    fn test_sys_fork_wire_roundtrip() {
        let mut canned = CannedKernelCallTransport::new();
        let mut reply = Message::default();
        reply.m_type = 0;
        // SAFETY(test):构造应答臂镜像内核 do_fork.c:111-112 的原地写。
        let arm = unsafe { &mut reply.m_u.m_krn_lsys_sys_fork };
        arm.endpt = 32771;
        arm.msgaddr = 0x7000;
        canned.reply_message(reply);

        let r = sys_fork(&canned, 10, 3, 0);
        assert_eq!(r, Ok((32771, 0x7000)));
        let sent = canned.sent.borrow();
        assert_eq!(sent.len(), 1);
        assert_eq!(sent[0].m_type, minix_types::SYS_FORK); // 0x600
        // SAFETY(test):读回请求臂核对逐字段。
        let req = unsafe { sent[0].m_u.m_lsys_krn_sys_fork };
        assert_eq!((req.endpt, req.slot, req.flags), (10, 3, 0));
    }

    /// SYS_FORK 错误直通(负 errno 不折叠)。
    #[test]
    fn test_sys_fork_negative_errno_passthrough() {
        let mut canned = CannedKernelCallTransport::new();
        canned.reply(-14); // EFAULT
        assert_eq!(sys_fork(&canned, 10, 3, 0), Err(-14));
        assert_eq!(canned.calls.get(), 1);
    }

    /// SYS_EXEC 回放:五个载荷字段 + OK。
    #[test]
    fn test_sys_exec_wire_roundtrip() {
        let mut canned = CannedKernelCallTransport::new();
        canned.reply(0);
        let r = sys_exec(&canned, 8, 0x1000, 0x9000, 0x8000, 0x7f00);
        assert_eq!(r, Ok(()));
        let sent = canned.sent.borrow();
        assert_eq!(sent[0].m_type, minix_types::SYS_EXEC); // 0x601
        // SAFETY(test):读回请求臂核对逐字段。
        let e = unsafe { sent[0].m_u.m_lsys_krn_sys_exec };
        assert_eq!((e.endpt, e.ip, e.stack, e.name, e.ps_str), (8, 0x1000, 0x9000, 0x8000, 0x7f00));
    }

    /// SYS_SAFECOPYFROM 回放:grant 五元组 + OK(无应答出参)。
    #[test]
    fn test_sys_safecopyfrom_wire_roundtrip() {
        let mut canned = CannedKernelCallTransport::new();
        canned.reply(0);
        let r = sys_safecopyfrom(&canned, 4, 12, 0x40, 0x20_0000, 256);
        assert_eq!(r, Ok(()));
        let sent = canned.sent.borrow();
        assert_eq!(sent[0].m_type, minix_types::SYS_SAFECOPYFROM); // 0x61f
        // SAFETY(test):读回请求臂核对逐字段。
        let sc = unsafe { sent[0].m_u.m_lsys_kern_safecopy };
        assert_eq!(
            (sc.from_to, sc.grant_id, sc.offset, sc.address, sc.bytes),
            (4, 12, 0x40, 0x20_0000, 256)
        );
    }

    /// SYS_SAFECOPYTO 回放:同臂反向调用号。
    #[test]
    fn test_sys_safecopyto_wire_roundtrip() {
        let mut canned = CannedKernelCallTransport::new();
        canned.reply(0);
        let r = sys_safecopyto(&canned, 4, 12, 0x40, 0x20_0000, 256);
        assert_eq!(r, Ok(()));
        let sent = canned.sent.borrow();
        assert_eq!(sent[0].m_type, minix_types::SYS_SAFECOPYTO); // 0x620
    }

    /// SYS_UPDATE 回放:M1 三字段(C do_update.c:9-11 的真实形状)。
    #[test]
    fn test_sys_update_wire_roundtrip() {
        let mut canned = CannedKernelCallTransport::new();
        canned.reply(0);
        let r = sys_update(&canned, 2, 9, 1);
        assert_eq!(r, Ok(()));
        let sent = canned.sent.borrow();
        assert_eq!(sent[0].m_type, minix_types::SYS_UPDATE); // 0x634
        // SAFETY(test):读回 M1 臂核对逐字段。
        let m1 = unsafe { sent[0].m_u.m_m1 };
        assert_eq!((m1.m1i1, m1.m1i2, m1.m1i3), (2, 9, 1));
    }

    /// SYS_DIAGCTL 回放:code/buf/len + OK。
    #[test]
    fn test_sys_diagctl_wire_roundtrip() {
        let mut canned = CannedKernelCallTransport::new();
        canned.reply(0);
        let r = sys_diagctl(&canned, 1, 0x4000, 32);
        assert_eq!(r, Ok(()));
        let sent = canned.sent.borrow();
        assert_eq!(sent[0].m_type, minix_types::SYS_DIAGCTL); // 0x62c
        // SAFETY(test):读回载荷臂核对逐字段。
        let d = unsafe { sent[0].m_u.m_lsys_krn_sys_diagctl };
        assert_eq!((d.code, d.buf, d.len), (1, 0x4000, 32));
    }

    /// SYS_DIAGCTL STACKTRACE 形状:arg2 = 目标端点进 endpt 字段。
    #[test]
    fn test_sys_diagctl_stacktrace_encodes_endpt() {
        let mut canned = CannedKernelCallTransport::new();
        canned.reply(0);
        let r = sys_diagctl(&canned, 2, 0, 9); // DIAGCTL_CODE_STACKTRACE, endpt 9
        assert_eq!(r, Ok(()));
        let sent = canned.sent.borrow();
        // SAFETY(test):读回载荷臂核对 endpt 字段。
        let d = unsafe { sent[0].m_u.m_lsys_krn_sys_diagctl };
        assert_eq!((d.code, d.endpt), (2, 9));
    }

    /// SYS_SETALARM 回放:请求 exp/abs + 应答臂 time_left/uptime。
    #[test]
    fn test_sys_setalarm_wire_roundtrip() {
        let mut canned = CannedKernelCallTransport::new();
        let mut reply = Message::default();
        reply.m_type = 0;
        // SAFETY(test):构造应答臂镜像内核回填(syscall_clock.rs:265)。
        let arm = unsafe { &mut reply.m_u.m_lsys_krn_sys_setalarm };
        arm.time_left = 500;
        arm.uptime = 9000;
        canned.reply_message(reply);

        let r = sys_setalarm(&canned, 100, false);
        assert_eq!(r, Ok((500, 9000)));
        let sent = canned.sent.borrow();
        assert_eq!(sent[0].m_type, minix_types::SYS_SETALARM); // 0x618
        // SAFETY(test):读回请求臂。
        let a = unsafe { sent[0].m_u.m_lsys_krn_sys_setalarm };
        assert_eq!((a.exp_time, a.abs_time), (100, 0));
    }

    /// SYS_SIGRETURN 回放:endpt/sigctx。
    #[test]
    fn test_sys_sigreturn_wire_roundtrip() {
        let mut canned = CannedKernelCallTransport::new();
        canned.reply(0);
        let r = sys_sigreturn(&canned, 7, 0x30_0000);
        assert_eq!(r, Ok(()));
        let sent = canned.sent.borrow();
        assert_eq!(sent[0].m_type, minix_types::SYS_SIGRETURN); // 0x60a
        // SAFETY(test):读回载荷臂。
        let sc = unsafe { sent[0].m_u.m_sigcalls };
        assert_eq!((sc.endpt, sc.sigctx), (7, 0x30_0000));
    }

    /// SYS_SPROF 回放:六字段。
    #[test]
    fn test_sys_sprof_wire_roundtrip() {
        let mut canned = CannedKernelCallTransport::new();
        canned.reply(0);
        let r = sys_sprof(&canned, 0 /* start */, 4096, 1 /* NMI */, 5, 0x5000, 0x6000);
        assert_eq!(r, Ok(()));
        let sent = canned.sent.borrow();
        assert_eq!(sent[0].m_type, minix_types::SYS_SPROF); // 0x624
        // SAFETY(test):读回载荷臂。
        let s = unsafe { sent[0].m_u.m_lsys_krn_sys_sprof };
        assert_eq!(
            (s.action, s.freq, s.intr_type, s.endpt, s.ctl_ptr, s.mem_ptr),
            (0, 4096, 1, 5, 0x5000, 0x6000)
        );
    }

    /// SYS_SETTIME 回放:四字段。
    #[test]
    fn test_sys_settime_wire_roundtrip() {
        let mut canned = CannedKernelCallTransport::new();
        canned.reply(0);
        let r = sys_settime(&canned, 1 /* now */, 0 /* REALTIME */, 1_700_000_000, 500);
        assert_eq!(r, Ok(()));
        let sent = canned.sent.borrow();
        assert_eq!(sent[0].m_type, minix_types::SYS_SETTIME); // 0x628
        // SAFETY(test):读回载荷臂。
        let t = unsafe { sent[0].m_u.m_lsys_krn_sys_settime };
        assert_eq!((t.now, t.clock_id, t.sec, t.nsec), (1, 0, 1_700_000_000, 500));
    }

    /// SYS_STIME 回放:boot_time。
    #[test]
    fn test_sys_stime_wire_roundtrip() {
        let mut canned = CannedKernelCallTransport::new();
        canned.reply(0);
        let r = sys_stime(&canned, 1_700_000_000);
        assert_eq!(r, Ok(()));
        let sent = canned.sent.borrow();
        assert_eq!(sent[0].m_type, minix_types::SYS_STIME); // 0x627
        // SAFETY(test):读回载荷臂。
        let t = unsafe { sent[0].m_u.m_lsys_krn_sys_stime };
        assert_eq!(t.boot_time, 1_700_000_000);
    }

    /// SYS_VTIMER 回放:M2 形状(which/set/value/endpt)+ 旧值回读。
    #[test]
    fn test_sys_vtimer_wire_roundtrip() {
        let mut canned = CannedKernelCallTransport::new();
        let mut reply = Message::default();
        reply.m_type = 0;
        // SAFETY(test):构造旧值回写(kernel 写 m_m2.m2l1)。
        reply.m_u.m_m2.m2l1 = 1234;
        canned.reply_message(reply);

        let r = sys_vtimer(&canned, 8, 1 /* VT_VIRTUAL */, Some(5000));
        assert_eq!(r, Ok(Some(1234)));
        let sent = canned.sent.borrow();
        assert_eq!(sent[0].m_type, minix_types::SYS_VTIMER); // 0x62d
        // SAFETY(test):读回 M2 请求字段。
        let m2 = unsafe { sent[0].m_u.m_m2 };
        assert_eq!((m2.m2i1, m2.m2i2, m2.m2l1, m2.m2l2), (1, 1, 5000, 8));
    }

    /// SYS_GETMCONTEXT 回放:endpt/ctx_ptr(GET/SET 同臂异调用号)。
    #[test]
    fn test_sys_mcontext_wire_roundtrip() {
        let mut canned = CannedKernelCallTransport::new();
        canned.reply(0);
        let r = sys_getmcontext(&canned, 6, 0x40_0000);
        assert_eq!(r, Ok(()));
        let mut canned = CannedKernelCallTransport::new();
        canned.reply(0);
        let r = sys_setmcontext(&canned, 6, 0x40_0000);
        assert_eq!(r, Ok(()));
    }
}

    /// E9 切片 1:GETINFO 承载——请求域/缓冲指针/长度/端点域逐项
    /// 落 m_lsys_krn_sys_getinfo,内核应答 OK。
    #[test]
    fn test_sys_getinfo_into_wire() {
        let mut canned = CannedKernelCallTransport::new();
        canned.reply(0);
        let mut out = [0u8; 16];
        let r = sys_getinfo_into(&canned, minix_types::GET_MACHINE, &mut out, minix_types::Endpoint::NONE.0);
        assert!(r.is_ok());
        // 出站消息断言(CannedKernelCallTransport 记录 sent)。
        let sent = canned.sent.borrow();
        assert_eq!(sent.len(), 1);
        // SAFETY: 断言读回 m_lsys_krn_sys_getinfo 臂。
        let gi = unsafe { sent[0].m_u.m_lsys_krn_sys_getinfo };
        assert_eq!(gi.request, minix_types::GET_MACHINE);
        assert_eq!(gi.val_ptr, out.as_ptr() as u64);
        assert_eq!(gi.val_len, 16);
    }

    /// E9 切片 1:GET_HZ 走 4 字节缓冲并以 LE 解回 i32。
    #[test]
    fn test_sys_get_hz_roundtrip() {
        let mut canned = CannedKernelCallTransport::new();
        canned.reply(0);
        let hz = sys_get_hz(&canned).unwrap();
        assert_eq!(hz, 0, "hosted canned reply zeros the buffer");
        let sent = canned.sent.borrow();
        // SAFETY: 断言读回 request 域。
        let gi = unsafe { sent[0].m_u.m_lsys_krn_sys_getinfo };
        assert_eq!(gi.request, minix_types::GET_HZ);
        assert_eq!(gi.val_len, 4);
    }

    /// E9 切片 1:GET_PRIV 的目标端点落 val_len2_e 域
    /// (kernel getinfo_priv:925-929 以此解析目标)。
    #[test]
    fn test_sys_get_priv_endpt_in_val_len2_e() {
        let mut canned = CannedKernelCallTransport::new();
        canned.reply(0);
        let mut out = [0u8; 64];
        sys_get_priv(&canned, 9, &mut out).unwrap();
        let sent = canned.sent.borrow();
        // SAFETY: 断言读回 getinfo 臂。
        let gi = unsafe { sent[0].m_u.m_lsys_krn_sys_getinfo };
        assert_eq!(gi.request, minix_types::GET_PRIV);
        assert_eq!(gi.val_len2_e, 9);
    }

    /// E9 切片 1:PRIVCTL 的 M1 载荷(request/endpt/arg_ptr)。
    #[test]
    fn test_sys_privctl_m1_wire() {
        let mut canned = CannedKernelCallTransport::new();
        canned.reply(0);
        sys_privctl(&canned, 9, 1 /* SYS_PRIV_ALLOW */, 0).unwrap();
        let sent = canned.sent.borrow();
        assert_eq!(sent.len(), 1);
        // SAFETY: 断言读回 M1 臂。
        let m1 = unsafe { sent[0].m_u.m_m1 };
        assert_eq!(m1.m1i1, 1);
        assert_eq!(m1.m1i2, 9);
        assert_eq!(m1.m1p1, 0);
    }

    /// E9 切片 1:TIMES 应答臂回填(RS get_ticks 消费 boot_ticks)。
    #[test]
    fn test_sys_times_reply_arm() {
        let mut canned = CannedKernelCallTransport::new();
        // 内核形状应答:OK(0) + 同臂回填五域。
        let mut reply = Message::default();
        reply.m_type = 0;
        // SAFETY: 构造应答臂。
        let arm = unsafe { &mut reply.m_u.m_krn_lsys_sys_times };
        arm.boot_ticks = 777;
        canned.reply_message(reply);
        let times = sys_times(&canned, minix_types::Endpoint::SELF.0).unwrap();
        assert_eq!(times.boot_ticks, 777);
        // 请求 endpt 域 = SELF。
        let sent = canned.sent.borrow();
        // SAFETY: 断言读回 times 请求臂。
        let req = unsafe { sent[0].m_u.m_lsys_krn_sys_times };
        assert_eq!(req.endpt, minix_types::Endpoint::SELF.0);
    }

    /// E9 切片 1:STACKTRACE 诊断码(2) + 目标端点。
    #[test]
    fn test_sys_diagctl_stacktrace_code_and_target() {
        let mut canned = CannedKernelCallTransport::new();
        canned.reply(0);
        sys_diagctl_stacktrace(&canned, 9).unwrap();
        let sent = canned.sent.borrow();
        // SAFETY: 断言读回 diagctl 臂。
        let d = unsafe { sent[0].m_u.m_lsys_krn_sys_diagctl };
        assert_eq!(d.code, minix_types::DIAGCTL_CODE_STACKTRACE);
    }
