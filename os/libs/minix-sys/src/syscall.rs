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

impl KernelCallTransport for DirectKernelCallTransport {
    fn kernel_call(&self, _message: &mut Message) -> i32 {
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

/// C: SYS_KILL 是内核调用 5（`kernel/src/syscall_signal.rs:138`
/// `Syscall::Kill`；C `callnr.h` `SYS_KILL`）。
pub const SYS_KILL_CALL: i32 = 5;

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
    let mut msg = Message::default();
    msg.m_type = SYS_TIMES_CALL;
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
        use minix_types::MessKrnLsysSysTimes;
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
    fn test_sys_times_negative_errno_is_err() {
        let mut canned = CannedKernelCallTransport::new();
        canned.reply(-3); // ESRCH

        assert_eq!(sys_times(&canned, 7).unwrap_err(), -3);
    }

    #[test]
    fn test_sys_abort_encodes_how() {
        // C: libsys sys_abort.c — m1i1 = how（RB_* 位组），SYS_ABORT = 27。
        let mut canned = CannedKernelCallTransport::new();
        canned.reply(0);

        let r = sys_abort(&canned, 0x0800 | 0x0008); // RB_POWERDOWN

        assert_eq!(r, 0);
        let sent = canned.sent.borrow();
        assert_eq!(sent.len(), 1);
        assert_eq!(sent[0].m_type, SYS_ABORT_CALL);
        assert_eq!(unsafe { sent[0].m_u.m_m1 }.m1i1, 0x0808);
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
}
