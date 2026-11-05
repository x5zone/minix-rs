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

use crate::ipc::IpcTransport;
use minix_types::{Endpoint, Errno, MessKrnLsysSysTimes, Message};

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
/// This follows the protocol of `_syscall`
/// (`minix3/minix/lib/libc/sys/syscall.c:9-25`):
///
/// 1. Write the call number into the message type field.
/// 2. Send the message and wait for the reply.
/// 3. When the reply's message type is negative, its negation is the error
///    number: return the typed error (C: `errno = -msgptr->m_type;
///    return(-1);`).
/// 4. Otherwise return the non-negative message type as the call result.
///
/// Two deliberate differences from C, both on the error channel:
///
/// - A failed round trip short-circuits to `Err(Errno)` directly. C has a
///   single channel (the message), so `syscall.c:14-17` writes the trap
///   status into `m_type` and re-reads it as a negative number; here the
///   transport already reports the failure through `Err(TrapStatus)`, and
///   the status is the positive errno (the same sign the kernel writes into
///   the reply register on the real trap path), so the detour through the
///   message would read a positive value as success.
/// - C splits the outcome across a return value plus a global `errno`;
///   this function returns a single [`Result`], following the crate-wide
///   error policy.
pub fn perform_syscall(
    transport: &impl IpcTransport,
    destination: Endpoint,
    call_number: i32,
    message: &mut Message,
) -> Result<i32, Errno> {
    message.m_type = call_number;
    // A failed round trip is the `Err` lane: the transport reports the
    // positive errno, and it becomes the typed error without touching the
    // message (see the module-level sign contract on [`TrapStatus`]).
    if let Err(status) = transport.sendrec(destination, message) {
        return Err(Errno::from_i32(status.0));
    }
    if message.m_type < 0 {
        Err(Errno::from_i32(-message.m_type))
    } else {
        Ok(message.m_type)
    }
}

/// Performs one server-side call round trip.
///
/// This follows the protocol of `_taskcall`
/// (`minix3/minix/lib/libsys/taskcall.c:9-23`): "the same as `_syscall`
/// except it returns negative error codes directly and not in errno."
/// Write the call number, send and wait, and hand the reply message type
/// back untouched — negative values are the server's error codes, in the
/// caller's hands, not here.
///
/// The round trip itself failing is the `Err` lane: the transport reports
/// the positive errno and it becomes the typed error. C cannot separate the
/// two failure kinds (one integer channel), which is why the C helper
/// returns the raw trap status as-is; the two-lane `Result` keeps the reply
/// contract on `Ok` and moves transport failures to `Err`.
pub fn perform_taskcall(
    transport: &impl IpcTransport,
    destination: Endpoint,
    call_number: i32,
    message: &mut Message,
) -> Result<i32, Errno> {
    message.m_type = call_number;
    if let Err(status) = transport.sendrec(destination, message) {
        return Err(Errno::from_i32(status.0));
    }
    Ok(message.m_type)
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
        // Barrier on the SYSCALL leg (the kernel copies the whole message from
        // caller memory — the counterpart of system.c copy_msg_from_user),
        // while LLVM sees only an integer-pointer operand: the store from
        // `message.m_type = call_number` is not treated as an asm-visible
        // memory write (an eager reply would overwrite the whole buffer, so the
        // store is dead) and may be eliminated or kept in a register before
        // svc/ecall/syscall. Real aarch64 hardware showed two symptoms of a
        // destroyed m_type slot (the kernel saw the stale value 0x62C, or 0):
        // this barrier is a **contract-level defense** (the leg semantics
        // require the store to be materialized first), complementary to the
        // root cause of the clobbered d9 FP constant (lazy-FPU rotation). It
        // also covers the three IPC legs (send/sendrec/sendnb), which carry the
        // same barrier; this leg had been missing it, and all three
        // architectures are now brought in line.
        #[cfg(all(
            any(
                target_arch = "x86_64",
                target_arch = "aarch64",
                target_arch = "riscv64"
            ),
            kernel_trap
        ))]
        unsafe {
            crate::ipc::commit_message_to_memory(message as *const Message);
        }
        // E1 slice 3: the real SYSCALL-leg trap (hosted builds keep -EIO).
        #[cfg(all(target_arch = "x86_64", kernel_trap))]
        {
            return unsafe { crate::arch_trap::kernel_call_trap(message) };
        }
        // riscv64/aarch64 legs: the same message leg through the
        // arch boundary (trap number 0, message pointer in a0/x0, reply
        // code in a0/x0) — see arch_trap::KERNEL_CALL_TRAP_NR.
        #[cfg(all(any(target_arch = "riscv64", target_arch = "aarch64"), kernel_trap))]
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

    /// Appends a full reply message to the script — the payload and the
    /// message type are written back as one, so calls whose reply IS the
    /// payload (SYS_TIMES and friends) can assert the decoded fields.
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
        // A scripted full-message payload overwrites the whole message;
        // without one, only the message type lands.
        match self.payloads.borrow_mut().pop_front() {
            Some(full) => *message = full,
            None => message.m_type = self.replies.get(index).cloned().unwrap_or(0),
        }
        message.m_type
    }
}
// ── SYS_* kernel-call wrappers (one thin wrapper per call, edge E6) ────
//
// Conventions (matching the VM-side `kernel_gateway.rs` and C libsys):
// - `perform_kernel_call` writes the call number into the message type;
// - request payloads fill the C union members;
// - the return value is the raw reply message type (a negative errno or a
//   non-negative result) and is never swallowed — each caller decides, per
//   its C call site, whether to ignore or panic on it.

/// C: SYS_KILL is kernel call 6 (`kernel/src/syscall_signal.rs:138`
/// `Syscall::Kill`; C `callnr.h` `SYS_KILL`).
pub const SYS_KILL_CALL: i32 = 6;

/// Send a terminate-signal request to the kernel (C: libsys `sys_kill`,
/// `sys_kill.c:3-13`).
///
/// `_kernel_call(SYS_KILL, &m)`: the payload is `m_sigcalls.endpt` (target
/// endpoint) and `m_sigcalls.sig` (signal number), with no reply payload — the
/// return value is the result (OK or a negative errno). C's do_exit does not
/// check the return value (`forkexit.c:256`); each caller decides what its own
/// semantics require.
pub fn sys_kill(transport: &impl KernelCallTransport, endpt: i32, sig: i32) -> i32 {
    let mut msg = Message::default();
    {
        // SAFETY: m_sigcalls is the documented payload layout of SYS_KILL
        // (kernel/src/syscall_signal.rs:141-147 reads map/endpt/sig).
        let sc = unsafe { &mut msg.m_u.m_sigcalls };
        sc.endpt = endpt;
        sc.sig = sig;
    }
    perform_kernel_call(transport, SYS_KILL_CALL, &mut msg, |_| {})
}

/// C `sys_statectl` (libsys sys_statectl.c:3-11): `_kernel_call(SYS_STATECTL,
/// &m)` with payload `m_lsys_krn_sys_statectl.{request,address,length}` — the
/// state-control surface (clear/set IPC filters, state-table registration).
/// The return value is the kernel result (OK or a negative errno); C's birth
/// protocol asserts `r == OK` for the first segment (sef_init.c:60), and
/// callers handle it in their own fail-closed shapes. Request numbers use the
/// `minix_types::SYS_STATE_CLEAR_IPC_FILTERS`-style com.h:442-446 constants.
pub fn sys_statectl(
    transport: &impl KernelCallTransport,
    request: i32,
    address: u64,
    length: i32,
) -> i32 {
    let mut msg = Message::default();
    {
        // SAFETY: m_lsys_krn_sys_statectl is the documented payload layout of
        // SYS_STATECTL (kernel/src/syscall_process.rs:104 reads
        // request/address/length).
        let sc = unsafe { &mut msg.m_u.m_lsys_krn_sys_statectl };
        sc.request = request;
        sc.address = address;
        sc.length = length;
    }
    perform_kernel_call(transport, minix_types::SYS_STATECTL, &mut msg, |_| {})
}

/// C: SYS_CLEAR is kernel call 2 (`kernel/src/syscall.rs` `Syscall::Clear`;
/// C `callnr.h` `SYS_CLEAR`).
pub const SYS_CLEAR_CALL: i32 = 2;
/// C: SYS_ABORT is kernel call 27 (`kernel/src/syscall.rs` `Syscall::Abort`;
/// C `callnr.h` `SYS_ABORT`).
pub const SYS_ABORT_CALL: i32 = 27;

/// C: SYS_TIMES is kernel call 25 (`kernel/src/syscall_clock.rs:90`
/// `dispatch_times`; C `callnr.h` `SYS_TIMES`).
pub const SYS_TIMES_CALL: i32 = 25;

/// C: SYS_VIRCOPY is kernel call 15 (`kernel/src/syscall.rs` `Syscall::Vircopy`).
pub const SYS_VIRCOPY_CALL: i32 = 15;

/// C: `SELF` (endpoint.h:56, `_ENDPOINT_SLOT_TOP - 3`) — the sentinel used when
/// the source or destination endpoint is the caller itself; the kernel
/// substitutes the caller's real endpoint during dispatch. The authoritative
/// value derives from `minix_types::Endpoint::SELF`; the historical wrong value
/// -2 is exactly the SYSTEM task endpoint (com.h:50), and once the kernel
/// comparison side was made authoritative (syscall_copy.rs:126), passing -2
/// never hits the SELF substitution and the on-target wire layer then fails to
/// resolve concrete endpoints.
pub const SELF: i32 = Endpoint::SELF.0;

/// Ask the kernel to abort the system (C: libsys `sys_abort`, `sys_abort.c:8-13`).
///
/// `_kernel_call(SYS_ABORT, &m)`: the payload is m1i1 = `how` (the RB_* bits of
/// `sys/reboot.h`), with no reply payload — on success the machine stops
/// outright and the call does not return. C's PM-side REBOOT handling
/// (`main.c:304-312`) does not check the return value: after the abort request
/// is out, PM waits in its main loop for the HARD_STOP notification, so a
/// failure merely keeps the loop going.
pub fn sys_abort(transport: &impl KernelCallTransport, how: i32) -> i32 {
    let mut msg = Message::default();
    {
        // SAFETY: m1i1 is the documented payload layout of SYS_ABORT
        // (kernel/src/syscall.rs dispatch_abort reads m1.m1i1 = how).
        let m1 = unsafe { &mut msg.m_u.m_m1 };
        m1.m1i1 = how;
    }
    perform_kernel_call(transport, SYS_ABORT_CALL, &mut msg, |_| {})
}

/// Read a process's CPU times (C: libsys `sys_times`, `sys_times.c:3-23`).
///
/// `_kernel_call(SYS_TIMES, &m)`: the request payload is
/// m_lsys_krn_sys_times.endpt; the reply payload is m_krn_lsys_sys_times (the
/// four values user/system/real/boot plus boottime). `endpt == SELF`
/// (endpoint.h:56 sentinel, authoritative derivation in
/// `minix_types::Endpoint::SELF`) is replaced by the kernel with the caller
/// itself (do_times.c:33-34).
pub fn sys_times(
    transport: &impl KernelCallTransport,
    endpt: i32,
) -> Result<MessKrnLsysSysTimes, i32> {
    let mut msg = Message {
        m_type: SYS_TIMES_CALL,
        ..Default::default()
    };
    {
        // SAFETY: m_lsys_krn_sys_times is the documented payload layout of
        // SYS_TIMES (kernel/src/syscall_clock.rs dispatch_times reads req.endpt).
        let req = unsafe { &mut msg.m_u.m_lsys_krn_sys_times };
        req.endpt = endpt;
    }
    let r = perform_kernel_call(transport, SYS_TIMES_CALL, &mut msg, |_| {});
    if r < 0 {
        return Err(r);
    }
    // SAFETY: the kernel overwrites the reply payload with m_krn_lsys_sys_times
    // (end of dispatch_times).
    Ok(unsafe { msg.m_u.m_krn_lsys_sys_times })
}

/// C: SYS_SIGSEND is kernel call 9 (`kernel/src/syscall.rs` `Syscall::Sigsend`).
pub const SYS_SIGSEND_CALL: i32 = 9;

/// C: `struct sigmsg` (`minix/type.h:71-77`, 40 bytes on x86-64) —
/// `sys_sigsend` points `m_sigcalls.sigctx` at this structure in the caller's
/// memory; the kernel safecopies it out, builds the sigframe and wakes the
/// target process.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct SigMsgWire {
    /// Signal number being caught. C: `int sm_signo` (offset 0).
    pub signo: u32,
    /// Mask to restore when handler returns. C: `sigset_t sm_mask` (offset 8).
    pub mask: u64,
    /// Handler address. C: `vir_bytes sm_sighandler` (offset 16).
    pub sighandler: u64,
    /// `_sigreturn` trampoline in libc. C: `vir_bytes sm_sigreturn` (offset 24).
    pub sigreturn: u64,
    /// User stack pointer at signal time. C: `vir_bytes sm_stkptr` (offset 32).
    pub stkptr: u64,
}

/// C: `sys_sigsend(proc_nr_e, smp)` (libsys `sys_sigsend.c:8-18`) —
/// `_kernel_call(SYS_SIGSEND, &m)` with payload `m_sigcalls.endpt` (target) +
/// `m_sigcalls.sigctx` (virtual address of the `sigmsg` in the caller's memory,
/// safecopied by the kernel). EFAULT/ENOMEM means the process's memory cannot
/// hold the handler (a legitimate failure; the target will be killed); any
/// other negative errno means a PM/kernel mismatch (the caller panics, as in
/// C).
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
        // SAFETY: m_sigcalls is the documented payload layout of SIGSEND
        // (kernel/src/syscall_signal.rs dispatch_sigsend reads endpt/sigctx).
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

/// C: SYS_GETKSIG is kernel call 7 (`kernel/src/syscall.rs` `Syscall::Getksig`).
pub const SYS_GETKSIG_CALL: i32 = 7;
/// C: SYS_ENDKSIG is kernel call 8 (`kernel/src/syscall.rs` `Syscall::Endksig`).
pub const SYS_ENDKSIG_CALL: i32 = 8;

/// C: `sys_getksig(proc_nr_e, &sigset)` (libsys `sys_getsig.c:6-17`) — fetch a
/// process with a pending kernel signal together with its signal bitmap; when
/// nothing is pending, `endpt` is `Endpoint::NONE`. The reply payload is
/// `m_sigcalls.{endpt, map}`. Fetching consumes: the kernel also clears that
/// process's RTS_SIGNALED and p_pending (the later endksig clears SIG_PENDING
/// per signal). PM's SIGKSIG pull loop (sef_signal.c:27-63) drives it. The
/// bitmap travels in C's wire form (`sigset_t` = 4×u32, 128 bits): PM consumes
/// it bit by bit with [`minix_types::sigset_contains`].
pub fn sys_getksig(
    transport: &impl KernelCallTransport,
) -> Result<(i32, minix_types::SigSetBits), i32> {
    let mut msg = Message {
        m_type: SYS_GETKSIG_CALL,
        ..Default::default()
    };
    let r = perform_kernel_call(transport, SYS_GETKSIG_CALL, &mut msg, |_| {});
    if r < 0 {
        return Err(r);
    }
    // SAFETY: the kernel overwrites the reply payload with m_sigcalls
    // (syscall_signal.rs:440-447).
    let sc = unsafe { msg.m_u.m_sigcalls };
    Ok((sc.endpt, sc.map))
}

/// C: `sys_endksig(proc_nr_e, sig_nr)` (libsys `sys_endsig.c:6-15`) — confirm
/// consumption of one kernel signal (payload `m_sigcalls.{endpt, sig}`); the
/// kernel clears the SIG_PENDING bit. The caller must be the target process's
/// signal manager (otherwise EPERM).
pub fn sys_endksig(transport: &impl KernelCallTransport, endpt: i32, sig: i32) -> Result<(), i32> {
    let mut msg = Message {
        m_type: SYS_ENDKSIG_CALL,
        ..Default::default()
    };
    {
        // SAFETY: m_sigcalls is the documented payload layout of GETKSIG/ENDKSIG
        // (kernel/src/syscall_signal.rs msg_sigcalls reads endpt/sig).
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

/// C: SYS_TRACE is kernel call 5 (`kernel/src/syscall.rs` `Syscall::Trace`).
pub const SYS_TRACE_CALL: i32 = 5;

/// C: `sys_trace(req, proc_ep, addr, datap)` (libsys `sys_trace.c:3-18`) —
/// `_kernel_call(SYS_TRACE, &m)` with payload `mess_lsys_krn_sys_trace`
/// (request@0/endpt@4/address@8/data@16; this layout is **different** from
/// `m_m1` — `m1.m1i1` lands on request rather than endpt, see the IMPORTANT
/// note in minix-types; the kernel's `misc.rs msg_trace` reads this layout).
/// The kernel writes a read value back into the data field (`misc.rs
/// write_trace_reply_data`, same offset 16 as the request data). PM's ptrace
/// pass-through (trace.c:244-248) and trace_stop's T_STOP (trace.c:263) use
/// this channel.
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
        // SAFETY: m_lsys_krn_sys_trace is the documented payload layout of
        // SYS_TRACE (kernel/src/misc.rs msg_trace reads request/endpt/address/data).
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
    // SAFETY: the kernel writes the read value back into data
    // (write_trace_reply_data, same offset 16; m_lsys_krn_sys_trace and
    // m_krn_lsis_sys_trace occupy the same memory).
    *data = unsafe { msg.m_u.m_lsys_krn_sys_trace }.data;
    Ok(())
}

/// Virtual-address copy (C: the libsys `sys_vircopy`/`sys_datacopy` family,
/// `kernel/src/syscall_copy.rs dispatch_vircopy` = `Syscall::Vircopy = 15`).
///
/// Payload `mess_lsys_krn_sys_copy`: from src_endpt to the virtual address at
/// dst_endpt (`SELF` is replaced by the kernel with the caller's endpoint; the
/// sentinel authority is [`SELF`]). PM's rusage delivery (tell_parent) and the
/// exec frame copy share this channel.
///
/// C: SYS_RUNCTL is kernel call 46 (`kernel/src/syscall.rs` `Syscall::Runctl`).
pub const SYS_RUNCTL_CALL: i32 = 46;
/// Runctl action: stop the process. C: `RC_STOP` (do_runctl.c).
pub const RC_STOP: i32 = 0;
/// Runctl action: resume the process. C: `RC_RESUME` (do_runctl.c).
pub const RC_RESUME: i32 = 1;
/// Runctl flag: delay the stop while sending. C: `RC_DELAY` (do_runctl.c).
pub const RC_DELAY: i32 = 1;

/// Process run control (C: libsys `sys_runctl`, `sys_runctl.c:3-15`).
///
/// `_kernel_call(SYS_RUNCTL, &m)`: payload m1i1 = RC_ENDPT, m1i2 = RC_ACTION,
/// m1i3 = RC_FLAGS. Return value = the kernel reply (OK / EBUSY / negative
/// errno).
pub fn sys_runctl(
    transport: &impl KernelCallTransport,
    endpt: i32,
    action: i32,
    flags: i32,
) -> i32 {
    let mut msg = Message::default();
    {
        // SAFETY: m1 is the documented payload layout of SYS_RUNCTL
        // (kernel/src/syscall_process.rs dispatch_runctl reads m1i1/m1i2/m1i3).
        let m1 = unsafe { &mut msg.m_u.m_m1 };
        m1.m1i1 = endpt;
        m1.m1i2 = action;
        m1.m1i3 = flags;
    }
    perform_kernel_call(transport, SYS_RUNCTL_CALL, &mut msg, |_| {})
}

/// Resume a stopped process (C: `sys_resume`, `syslib.h:48` — the RC_RESUME action).
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
        // SAFETY: m_lsys_krn_sys_copy is the payload layout shared by
        // VIRCOPY/PHYSCOPY (kernel/src/syscall_copy.rs:244-260 reads
        // src/dst/nr_bytes).
        let cp = unsafe { &mut msg.m_u.m_lsys_krn_sys_copy };
        cp.src_endpt = src_endpt;
        cp.src_addr = src_addr;
        cp.dst_endpt = dst_endpt;
        cp.dst_addr = dst_addr;
        cp.nr_bytes = nr_bytes;
    }
    perform_kernel_call(transport, SYS_VIRCOPY_CALL, &mut msg, |_| {})
}

/// VUMAP input vector element: the caller-side mirror of `struct vumap_vir`
/// (C `minix3/minix/include/minix/type.h:40-46`: a grant/addr union plus size).
///
/// The union is 8 bytes on 64-bit (`u_addr` sets the width); this mirror stores
/// the whole 8 bytes (grant in the low 32 bits, the same reading as the kernel's
/// `VumapVir::vv_grant()`).
#[repr(C)]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct VumapVir {
    /// Grant id (low 32 bits) or virtual address (C `vv_u`).
    pub u: u64,
    /// Byte length of this element (C `vv_size`).
    pub size: u64,
}

/// VUMAP output vector element: the caller-side mirror of `struct vumap_phys`
/// (C `type.h:50-53`: `vp_addr`/`vp_size`).
///
/// The kernel side has a private isomorphic type (`VumapPhys` in
/// `os/kernel/src/syscall_copy.rs`); this is the library-visible one, two 64-bit
/// fields (16 bytes), matching the kernel's element-by-element copy shape when
/// it writes the vector out.
#[repr(C)]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct VumapPhys {
    /// Physical address (C `vp_addr`).
    pub addr: u64,
    /// Byte count of this segment (C `vp_size`).
    pub size: u64,
}

/// VUMAP access bit: read (C `VUA_READ`, type.h).
pub const VUA_READ: i32 = 0x01;
/// VUMAP access bit: write (C `VUA_WRITE`, type.h).
pub const VUA_WRITE: i32 = 0x02;

/// SYS_VUMAP: map one grant into a physical vector (C: libsys `sys_vumap`,
/// `sys_vumap.c:7-40`; kernel `do_vumap.c`).
///
/// The input vector is fixed at a single element `{grant, size}` — the only
/// shape used by the sole caller in the C tree (vbox's `store_pages`), so it is
/// wrapped as two scalars. `offset` is the starting offset inside the vector
/// (C `offset`), and `access` takes the [`VUA_READ`]/[`VUA_WRITE`] bits; the
/// capacity of `out` is C's `pmax` input value, and success returns the number
/// of elements actually filled (C's output value `pcount`).
///
/// Both copies — the kernel reading the input vector and writing the physical
/// vector — use the caller's (this process's) virtual addresses as source and
/// destination: C passes `(vir_bytes) vvec` and `(vir_bytes) pvec`
/// (`sys_vumap.c:30,35`), and the kernel side does a `data_copy` against the
/// caller (`syscall_copy.rs:1141-1157`, `:1270-1285`).
pub fn sys_vumap(
    transport: &impl KernelCallTransport,
    endpt: Endpoint,
    grant: i32,
    size: u64,
    offset: u64,
    access: i32,
    out: &mut [VumapPhys],
) -> Result<usize, i32> {
    let vvec = [VumapVir {
        // The low 32 bits are the grant; the upper bits are zero, since the
        // kernel reads this union as i32 (`vv_grant()`).
        u: u64::from(grant as u32),
        size,
    }];
    let mut msg = Message::default();
    {
        // SAFETY: m_lsys_krn_sys_vumap is the documented request layout of
        // SYS_VUMAP (kernel/src/syscall_copy.rs:1063-1071 reads
        // endpt/vaddr/vcount/offset/access/paddr/pmax).
        let v = unsafe { &mut msg.m_u.m_lsys_krn_sys_vumap };
        v.endpt = endpt.0;
        v.vaddr = vvec.as_ptr() as u64;
        v.vcount = 1;
        v.paddr = out.as_mut_ptr() as u64;
        v.pmax = out.len() as i32;
        v.access = access;
        v.offset = offset;
    }
    let reply = perform_kernel_call(transport, minix_types::SYS_VUMAP, &mut msg, |_| {});
    if reply < 0 {
        return Err(reply);
    }
    // SAFETY: the kernel fills pcount through m_krn_lsys_sys_vumap
    // (around kernel/src/syscall_copy.rs:1288; C ipc.h mess_krn_lsys_sys_vumap).
    let pcount = unsafe { msg.m_u.m_krn_lsys_sys_vumap }.pcount;
    Ok(pcount.max(0) as usize)
}

/// Notify the kernel that an exited process may be reclaimed (C: libsys
/// `sys_clear`, `sys_clear.c:8-14`).
///
/// `_kernel_call(SYS_CLEAR, &m)`: payload m1i1 = target endpoint, no reply
/// payload. C's exit_proc (direct PRIV_PROC destruction, `forkexit.c:366-368`)
/// and exit_restart (user process reclamation, `forkexit.c:449-451`) both panic
/// on failure — a process already terminated but not reclaimed by the kernel is
/// a permanent leak, unrecoverable.
pub fn sys_clear(transport: &impl KernelCallTransport, endpt: i32) -> i32 {
    let mut msg = Message::default();
    {
        // SAFETY: m1 is the documented payload layout of SYS_CLEAR
        // (kernel/src/syscall_process.rs:402-404 reads m1i1).
        let m1 = unsafe { &mut msg.m_u.m_m1 };
        m1.m1i1 = endpt;
    }
    perform_kernel_call(transport, SYS_CLEAR_CALL, &mut msg, |_| {})
}

// ── VM-side SYS_* wrappers (six thin wrappers, replayed with CannedTransport) ──
//
// Conventions (as in the existing wrapper section above):
// - `perform_kernel_call` writes the call number into m_type;
// - request payloads fill C's union members (the reading arm on the kernel
//   dispatch side is the contract anchor);
// - the return value = the kernel reply m_type (a negative errno or a
//   non-negative result), never swallowed.
// Call-number constants consume minix-types' kernel_call authority
// (E-MINTYPES-SYS) instead of being defined locally.

/// SYS_FORK: create a child process (C: libsys `sys_fork` semantics; kernel
/// `dispatch_fork`).
///
/// Reply shape: the kernel returns OK(0) and writes the out-parameters into the
/// reply arm `m_krn_lsys_sys_fork` in place (ipc.h:283-287; do_fork.c:111-112) —
/// `endpt` is the child endpoint and `msgaddr` is the buffer address where the
/// parent receives the message (VM uses it for eager-CoW, fork.c:100-108).
/// Success returns `(child_endpt, msgaddr)`; failure returns a negative errno.
pub fn sys_fork(
    transport: &impl KernelCallTransport,
    parent_endpt: i32,
    child_slot: i32,
    flags: u32,
) -> Result<(i32, u64), i32> {
    let mut msg = Message::default();
    {
        // SAFETY: m_lsys_krn_sys_fork is the documented request layout of
        // SYS_FORK (kernel/src/syscall_process.rs dispatch_fork reads
        // endpt/slot/flags; C ipc.h:1173-1177).
        let req = unsafe { &mut msg.m_u.m_lsys_krn_sys_fork };
        req.endpt = parent_endpt;
        req.slot = child_slot;
        req.flags = flags;
    }
    let reply = perform_kernel_call(transport, minix_types::SYS_FORK, &mut msg, |_| {});
    if reply < 0 {
        return Err(reply);
    }
    // SAFETY: the kernel writes the reply in place into the same message; the
    // arm is plain old data.
    let arm = unsafe { msg.m_u.m_krn_lsys_sys_fork };
    Ok((arm.endpt, arm.msgaddr))
}

/// SYS_EXEC: reload the target process image (C: libsys `sys_exec`; kernel
/// `dispatch_exec` reads `m_lsys_krn_sys_exec` and returns OK on success).
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
        // SAFETY: m_lsys_krn_sys_exec is the documented payload layout of
        // SYS_EXEC (kernel/src/syscall_process.rs dispatch_exec; C ipc.h).
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

/// SYS_SAFECOPYFROM: copy in from the granter through a grant into the caller's
/// space (C: libsys `sys_safecopyfrom`; kernel `dispatch_safecopy_from` reads
/// `m_lsys_kern_safecopy`, returning OK on success with no reply out-parameter).
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
        // SAFETY: m_lsys_kern_safecopy is the payload layout shared by
        // SAFECOPYFROM/TO (kernel/src/syscall_copy.rs:181; C ipc.h).
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

/// SYS_SAFECOPYTO: copy the caller's space to the granter through a grant (the
/// direction opposite to [`sys_safecopyfrom`], same payload arm, C: libsys
/// `sys_safecopyto`).
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
        // SAFETY: same as sys_safecopyfrom — both share the m_lsys_kern_safecopy arm.
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

/// SYS_UPDATE: the kernel phase of RS live update (C: libsys `sys_update`;
/// kernel `dispatch_update` reads src/dst/flags from m_m1 — the C shape at
/// do_update.c:9-11 really is m_m1, not a misread; returns OK on success).
pub fn sys_update(
    transport: &impl KernelCallTransport,
    src_endpt: i32,
    dst_endpt: i32,
    flags: i32,
) -> Result<(), i32> {
    let mut msg = Message::default();
    {
        // SAFETY: m_m1 is the documented payload of SYS_UPDATE (kernel/src/misc.rs
        // dispatch_update reads m1i1/m1i2/m1i3; C do_update.c:9-11).
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

/// SYS_DIAGCTL: diagnostic control (C: libsys `sys_diagctl(int code, char *arg1,
/// int arg2)` — kernel `dispatch_diagctl` dispatches by code: DIAG(1) copies up
/// to DIAGBUFSIZE bytes from the caller's space via buf/len and prints them to
/// the console; STACKTRACE(2) uses the `endpt` field to name the target
/// process).
pub fn sys_diagctl(
    transport: &impl KernelCallTransport,
    code: i32,
    arg1: u64,
    arg2: i32,
) -> Result<(), i32> {
    let mut msg = Message::default();
    {
        // SAFETY: m_lsys_krn_sys_diagctl is the documented payload layout of
        // SYS_DIAGCTL (kernel/src/syscall.rs dispatch_diagctl; C ipc.h).
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

/// Convenience wrapper for the SYS_DIAGCTL code-1 string — the shared sink of
/// the server diagnostic-output seam ([ARCH: A-6]). C's server `printf`/
/// `rs_verbose` family (the diagnostic face of rs's `const.h:6-7`
/// `DEBUG_DEFAULT`/`PRIV_DEBUG_DEFAULT` etc.) reaches the kernel console through
/// this on the Rust side (kernel `dispatch_diagctl` code 1: copy up to
/// DIAGBUFSIZE bytes from the caller's space via buf/len and print them to the
/// console). The text must stay alive during the sendrec — the kernel safecopies
/// that pointer during the synchronous kernel call, and the `&str` borrow covers
/// exactly that span; chunking, prefixes and flush pacing are the caller's
/// business.
pub fn sys_diagctl_write(transport: &impl KernelCallTransport, text: &str) -> Result<(), i32> {
    sys_diagctl(transport, 1, text.as_ptr() as u64, text.len() as i32)
}

/// I/O direction and width bits of a SYS_DEVIO request byte (C: com.h:283/:287
/// `_DIO_INPUT=0x001`/`_DIO_OUTPUT=0x002`/`_DIO_BYTE=0x010`; the kernel's
/// `IoDirection`/`IoSize` `from_request_mask` decodes the same values).
pub const _DIO_INPUT: i32 = 0x001;
/// C: `com.h:283` `_DIO_OUTPUT`.
pub const _DIO_OUTPUT: i32 = 0x002;
/// C: `com.h:287` `_DIO_BYTE`.
pub const _DIO_BYTE: i32 = 0x010;
/// C: `com.h:285` `_DIO_WORD` (16-bit port width).
pub const _DIO_WORD: i32 = 0x020;
/// C: `com.h:286` `_DIO_LONG` (32-bit port width).
pub const _DIO_LONG: i32 = 0x030;

/// SYS_DEVIO byte output: write one byte to a hardware port (C: libsys syslib.h:217
/// `sys_outb(p,v) = sys_out(p, v, _DIO_BYTE)`). Port reachability is decided by
/// the kernel `do_devio` CHECK_IO_PORT gate — it passes the caller when the
/// privilege lacks CHECK_IO_PORT (C's system.conf tty service `io ALL` is this
/// shape: drivers never hold IOPL, and all port I/O goes through the kernel).
pub fn sys_outb(transport: &impl KernelCallTransport, port: u16, value: u8) -> Result<(), i32> {
    let mut msg = Message::default();
    {
        // SAFETY: m_lsys_krn_sys_devio is the documented payload layout of
        // SYS_DEVIO (kernel/src/syscall_device.rs dispatch_devio:
        // request/port/value; C ipc.h mess_lsys_krn_sys_devio).
        let devio = unsafe { &mut msg.m_u.m_lsys_krn_sys_devio };
        devio.request = _DIO_OUTPUT | _DIO_BYTE;
        devio.port = i32::from(port);
        devio.value = u32::from(value);
    }
    let reply = perform_kernel_call(transport, minix_types::SYS_DEVIO, &mut msg, |_| {});
    if reply < 0 {
        return Err(reply);
    }
    Ok(())
}

/// SYS_DEVIO byte input: read one byte from a hardware port (C: the libsys
/// syslib.h `sys_inb(p, v*)` sibling wrapper; the reply payload is
/// `mess_krn_lsys_sys_devio`, value@0 — the kernel `dispatch_devio` Input arm
/// fills the same slot).
pub fn sys_inb(transport: &impl KernelCallTransport, port: u16) -> Result<u8, i32> {
    let mut msg = Message::default();
    {
        // SAFETY: m_lsys_krn_sys_devio is the documented payload layout of
        // SYS_DEVIO (kernel/src/syscall_device.rs dispatch_devio; C ipc.h
        // mess_lsys_krn_sys_devio).
        let devio = unsafe { &mut msg.m_u.m_lsys_krn_sys_devio };
        devio.request = _DIO_INPUT | _DIO_BYTE;
        devio.port = i32::from(port);
    }
    let reply = perform_kernel_call(transport, minix_types::SYS_DEVIO, &mut msg, |_| {});
    if reply < 0 {
        return Err(reply);
    }
    // SAFETY: the reply value sits in MessKrnLsysSysDevio.value (offset 0) — the
    // kernel dispatch_devio Input arm writes back the same variant (see the
    // counterpart comment around :506).
    Ok(unsafe { msg.m_u.m_krn_lsys_sys_devio.value } as u8)
}

/// SYS_DEVIO word output: write a 16-bit word to a hardware port (C: libsys
/// syslib.h:218 `sys_outw(p,v) = sys_out(p, v, _DIO_WORD)`). The kernel
/// `dispatch_devio` `IoSize::Word` arm is implemented (`port_io.outw`); this
/// wrapper adds the word width beyond bytes.
pub fn sys_outw(transport: &impl KernelCallTransport, port: u16, value: u16) -> Result<(), i32> {
    let mut msg = Message::default();
    {
        // SAFETY: m_lsys_krn_sys_devio is the documented payload layout of
        // SYS_DEVIO (kernel/src/syscall_device.rs dispatch_devio; C ipc.h).
        let devio = unsafe { &mut msg.m_u.m_lsys_krn_sys_devio };
        devio.request = _DIO_OUTPUT | _DIO_WORD;
        devio.port = i32::from(port);
        devio.value = u32::from(value);
    }
    let reply = perform_kernel_call(transport, minix_types::SYS_DEVIO, &mut msg, |_| {});
    if reply < 0 {
        return Err(reply);
    }
    Ok(())
}

/// SYS_DEVIO word input: read a 16-bit word from a hardware port (C: libsys
/// syslib.h:224 `sys_inw(p,v) = sys_in(p, v, _DIO_WORD)`). The reply value is in
/// `mess_krn_lsys_sys_devio.value` (offset 0); the kernel `dispatch_devio` Input
/// arm zero-extends `port_io.inw(port)` and writes it back to the same slot, and
/// the low 16 bits are taken here.
pub fn sys_inw(transport: &impl KernelCallTransport, port: u16) -> Result<u16, i32> {
    let mut msg = Message::default();
    {
        // SAFETY: m_lsys_krn_sys_devio is the documented payload layout of SYS_DEVIO.
        let devio = unsafe { &mut msg.m_u.m_lsys_krn_sys_devio };
        devio.request = _DIO_INPUT | _DIO_WORD;
        devio.port = i32::from(port);
    }
    let reply = perform_kernel_call(transport, minix_types::SYS_DEVIO, &mut msg, |_| {});
    if reply < 0 {
        return Err(reply);
    }
    // SAFETY: the reply value is in MessKrnLsysSysDevio.value (offset 0), the
    // same slot as the byte arm.
    Ok(unsafe { msg.m_u.m_krn_lsys_sys_devio.value } as u16)
}

/// SYS_DEVIO doubleword output: write a 32-bit word to a hardware port (C:
/// libsys syslib.h:219 `sys_outl(p,v) = sys_out(p, v, _DIO_LONG)`). The kernel
/// `IoSize::Long` arm is implemented.
pub fn sys_outl(transport: &impl KernelCallTransport, port: u16, value: u32) -> Result<(), i32> {
    let mut msg = Message::default();
    {
        // SAFETY: m_lsys_krn_sys_devio is the documented payload layout of SYS_DEVIO.
        let devio = unsafe { &mut msg.m_u.m_lsys_krn_sys_devio };
        devio.request = _DIO_OUTPUT | _DIO_LONG;
        devio.port = i32::from(port);
        devio.value = value;
    }
    let reply = perform_kernel_call(transport, minix_types::SYS_DEVIO, &mut msg, |_| {});
    if reply < 0 {
        return Err(reply);
    }
    Ok(())
}

/// SYS_DEVIO doubleword input: read a 32-bit word from a hardware port (C:
/// libsys syslib.h:225 `sys_inl(p,v) = sys_in(p, v, _DIO_LONG)`). The kernel
/// `IoSize::Long` arm fills all 32 bits of `port_io.inl(port)` into the value
/// slot.
pub fn sys_inl(transport: &impl KernelCallTransport, port: u16) -> Result<u32, i32> {
    let mut msg = Message::default();
    {
        // SAFETY: m_lsys_krn_sys_devio is the documented payload layout of SYS_DEVIO.
        let devio = unsafe { &mut msg.m_u.m_lsys_krn_sys_devio };
        devio.request = _DIO_INPUT | _DIO_LONG;
        devio.port = i32::from(port);
    }
    let reply = perform_kernel_call(transport, minix_types::SYS_DEVIO, &mut msg, |_| {});
    if reply < 0 {
        return Err(reply);
    }
    // SAFETY: the reply value is in MessKrnLsysSysDevio.value (offset 0).
    Ok(unsafe { msg.m_u.m_krn_lsys_sys_devio.value })
}

/// SYS_SETALARM: set (or cancel) an alarm and fetch the previous alarm
/// information (C: libsys `sys_setalarm2`; kernel `dispatch_setalarm` reads and
/// writes `m_lsys_krn_sys_setalarm` — request exp_time/abs_time, reply
/// time_left/uptime through the same arm). Success returns `(time_left, uptime)`.
pub fn sys_setalarm(
    transport: &impl KernelCallTransport,
    exp_time: u64,
    abs_time: bool,
) -> Result<(u64, u64), i32> {
    let mut msg = Message::default();
    {
        // SAFETY: m_lsys_krn_sys_setalarm is the documented payload layout of
        // SETALARM (kernel/src/syscall_clock.rs dispatch_setalarm; C ipc.h).
        let a = unsafe { &mut msg.m_u.m_lsys_krn_sys_setalarm };
        a.exp_time = exp_time;
        a.abs_time = abs_time as i32;
    }
    let reply = perform_kernel_call(transport, minix_types::SYS_SETALARM, &mut msg, |_| {});
    if reply < 0 {
        return Err(reply);
    }
    // SAFETY: the kernel fills the reply through the same arm (syscall_clock.rs:265).
    let arm = unsafe { msg.m_u.m_lsys_krn_sys_setalarm };
    Ok((arm.time_left, arm.uptime))
}

// ── SYS_* named wrappers needed by the RS SysApi face ──
// Six calls align with the six of the nine SysApi methods in
// servers/rs/src/boot.rs that are still missing (kill/update/setalarm already
// exist). All go through perform_kernel_call and are replay-testable with
// Canned; the real transport is the SYSCALL leg under a real-trap build.

/// SYS_GETINFO · GET_MACHINE(12): fetch machine information (processor count +
/// BSP id). C: `sys_getinfo(GET_MACHINE, ...)`; kernel `getinfo_machine` copies
/// `MachineStruct` via `copy_struct_to_caller`. `out` must hold at least the
/// kernel-side structure (the kernel truncates the copy to min(len)).
pub fn sys_get_machine(transport: &impl KernelCallTransport, out: &mut [u8]) -> Result<(), i32> {
    sys_getinfo_into(
        transport,
        minix_types::GET_MACHINE,
        out,
        minix_types::Endpoint::NONE.0,
    )
}

/// SYS_GETINFO · GET_HZ(18): read the system clock rate (i32). C: do_getinfo.c:81-84.
pub fn sys_get_hz(transport: &impl KernelCallTransport) -> Result<i32, i32> {
    let mut buf = [0u8; 4];
    sys_getinfo_into(
        transport,
        minix_types::GET_HZ,
        &mut buf,
        minix_types::Endpoint::NONE.0,
    )?;
    Ok(i32::from_le_bytes(buf))
}

/// SYS_GETINFO · GET_PRIV(17): read a snapshot of the target process's
/// privilege structure. The target endpoint travels in the `val_len2_e` field
/// (kernel getinfo_priv:925-929).
pub fn sys_get_priv(
    transport: &impl KernelCallTransport,
    endpt: i32,
    out: &mut [u8],
) -> Result<(), i32> {
    sys_getinfo_into(transport, minix_types::GET_PRIV, out, endpt)
}

/// SYS_GETINFO · GET_MONPARAMS(4): fetch the boot monitor parameter buffer.
/// C: `sys_getmonparams(v,vl)` = `sys_getinfo(GET_MONPARAMS, v,vl, 0,0)`
/// (the syslib.h:187 macro). The kernel always copies `sizeof(kinfo.param_buf)`
/// = 1024 bytes (do_getinfo.c:143-146, param.h:28, zero-padded at the tail) —
/// the caller's buffer must be sized per MULTIBOOT_PARAM_BUF_SIZE, and a short
/// buffer gets E2BIG.
pub fn sys_getmonparams(transport: &impl KernelCallTransport, out: &mut [u8]) -> Result<(), i32> {
    sys_getinfo_into(
        transport,
        minix_types::GET_MONPARAMS,
        out,
        minix_types::Endpoint::NONE.0,
    )
}

/// SYS_GETINFO · GET_IMAGE(1): fetch the kernel boot image table.
/// C: `sys_getimage(dst)` = `sys_getinfo(GET_IMAGE, dst, 0,0,0)`
/// (the syslib.h:184 macro). The payload is `struct boot_image[NR_BOOT_PROCS]`
/// (do_getinfo.c:86-90; wire-layout authority = minix-types `BootImageStruct`,
/// 40 bytes × 17) — the caller's buffer must hold the whole table.
pub fn sys_getimage(transport: &impl KernelCallTransport, out: &mut [u8]) -> Result<(), i32> {
    sys_getinfo_into(
        transport,
        minix_types::GET_IMAGE,
        out,
        minix_types::Endpoint::NONE.0,
    )
}

/// Generic GETINFO carrier: fill `m_lsys_krn_sys_getinfo` and execute.
/// `endpt` lands in `val_len2_e` (the kernel getinfo_priv reads the target
/// endpoint from there; sub-requests with no endpoint semantics pass NONE and
/// the kernel ignores it).
pub fn sys_getinfo_into(
    transport: &impl KernelCallTransport,
    request: i32,
    out: &mut [u8],
    endpt: i32,
) -> Result<(), i32> {
    let mut msg = Message::default();
    {
        // SAFETY: m_lsys_krn_sys_getinfo is the documented payload of GETINFO
        // (kernel/src/misc.rs msg_getinfo:269-279 — do not overwrite with m1).
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

/// The caller's own identity (the reply of SYS_GETINFO · GET_WHOAMI).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WhoAmI {
    /// Caller endpoint. C: the `who_ep` out-parameter.
    pub endpoint: Endpoint,
    /// Privilege flags. C: the `priv_flags` out-parameter.
    pub priv_flags: i32,
    /// Init flags. C: the `init_flags` out-parameter.
    pub init_flags: i32,
    /// Process name (44 bytes on the wire; the kernel writes the NUL).
    /// C: the `who_name` out-parameter.
    pub name: [u8; 44],
}

/// SYS_GETINFO · GET_WHOAMI(19): fetch the caller's own identity. C: `sys_whoami`
/// (libsys, sys_getinfo.c:29-55) — asked at birth by sef_startup (sef.c:76-87),
/// and the sole source of C driver/server `sef_self_endpoint`. The kernel answers
/// from the caller's process context and writes the reply message overlay
/// directly (kernel/src/misc.rs getinfo_whoami, C do_getinfo.c:132-141) without
/// a structure copy.
///
/// Shape difference from C: the `len<2` gate (C sys_getinfo.c:40-41) protects a
/// name buffer supplied by the caller; on the Rust side the name returns as part
/// of the structure at its 44-byte wire width, with no caller-supplied buffer,
/// so that guard has no counterpart and is absent. Failure semantics match C: a
/// negative kernel value is returned as `Err` unchanged.
pub fn sys_whoami(transport: &impl KernelCallTransport) -> Result<WhoAmI, i32> {
    let mut msg = Message::default();
    {
        // SAFETY: m_lsys_krn_sys_getinfo is the documented payload of GETINFO
        // (kernel/src/misc.rs msg_getinfo — do not overwrite with m1).
        let gi = unsafe { &mut msg.m_u.m_lsys_krn_sys_getinfo };
        // C sys_getinfo.c:38 — a GET_WHOAMI request carries only the request
        // lane; endpt/val_* are all left unset.
        gi.request = minix_types::GET_WHOAMI;
    }
    let reply = perform_kernel_call(transport, minix_types::SYS_GETINFO, &mut msg, |_| {});
    if reply < 0 {
        return Err(reply);
    }
    // SAFETY: the kernel writes the reply through the GET_WHOAMI arm directly
    // into the same overlay (kernel/src/misc.rs getinfo_whoami).
    let w = unsafe { msg.m_u.m_krn_lsys_sys_getwhoami };
    Ok(WhoAmI {
        endpoint: Endpoint(w.endpt),
        priv_flags: w.privflags,
        init_flags: w.initflags,
        name: w.name,
    })
}

/// SYS_PRIVCTL: privilege control (allow/disallow/set_sys/set_user/...).
/// m_m1 payload: m1i1=request, m1i2=endpt, m1p1=arg_ptr (pointer to the
/// user-space privilege structure, 0 when absent). C:
/// mess_lsys_krn_sys_privctl, do_privctl.c:47-51.
pub fn sys_privctl(
    transport: &impl KernelCallTransport,
    endpt: i32,
    request: i32,
    arg_ptr: u64,
) -> Result<(), i32> {
    let mut msg = Message::default();
    {
        // SAFETY: m_m1 covers it (the privctl payload has the same shape as m1,
        // syscall.rs:1354-1358).
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

/// SYS_DIAGCTL · STACKTRACE(2): request a stack-trace dump of the target
/// process. C: main.c:681-683 (the stacktrace-signal branch of the RS signal
/// manager).
///
/// C: `syslib.h:167` defines it as `sys_diagctl(DIAGCTL_CODE_STACKTRACE, NULL, ep)`
/// — the endpoint goes in **arg2** (`endpt = (endpoint_t)arg2` at
/// `sys_diagctl.c:16-17`). An earlier version wrongly put the target in arg1
/// (the buf slot) with arg2 always 0, so the kernel always received `endpt` 0 —
/// corrected per C when wiring the PM stacktrace branch.
pub fn sys_diagctl_stacktrace(
    transport: &impl KernelCallTransport,
    target: i32,
) -> Result<(), i32> {
    sys_diagctl(transport, minix_types::DIAGCTL_CODE_STACKTRACE, 0, target)
}

/// SYS_DATACOPY named alias: C syslib.h:129 defines it as a macro over
/// sys_vircopy (identical parameters) — Rust likewise carries it with
/// [`sys_vircopy`], offering the C name only so that ipc-server's
/// semop/semctl/shmctl copy-boundary verbs are grep-reachable.
/// Creates a zeroed message — the single crate-wide helper
/// (`cleared_message`, mirroring the `memset(&m, 0, sizeof(m))` that opens
/// every C wrapper). Previously duplicated per group module; consolidated
/// here (E-MINTYPES-RUNTIME step 2, single-point helper).
pub(crate) fn cleared_message() -> crate::Message {
    crate::Message::zeroed()
}

/// Copies a packed payload into the message body (single crate-wide
/// helper; payloads must fit the 56-byte union).
pub(crate) fn write_payload(message: &mut crate::Message, packed: &[u8]) {
    debug_assert!(packed.len() <= minix_types::MESSAGE_PAYLOAD_SIZE);
    // SAFETY: the raw payload is 56 writable bytes; the caller guarantees
    // the packed slice fits (debug-checked above).
    unsafe {
        message.m_u.raw[..packed.len()].copy_from_slice(packed);
    }
}

pub fn sys_datacopy(
    transport: &impl KernelCallTransport,
    src_endpt: i32,
    src_addr: u64,
    dst_endpt: i32,
    dst_addr: u64,
    bytes: u64,
) -> Result<(), i32> {
    // SAFETY/semantics: the C macro passes straight through — sys_vircopy
    // returns a negative errno or OK.
    let r = sys_vircopy(transport, src_endpt, src_addr, dst_endpt, dst_addr, bytes);
    if r < 0 {
        return Err(r);
    }
    Ok(())
}

/// Whole-block fill on the kernel side (C libsys `sys_memset`, sys_memset.c:3-16).
///
/// `bytes == 0` short-circuits to `OK` (same semantics as C :9-11); otherwise
/// SYS_MEMSET (com.h:221) has the kernel write `pattern` into
/// `base..base+bytes` of `process` — the clearmem callback at the end of exec
/// (exec.c:382 via libexec_clear_sys_memset) uses it to clear segment gaps,
/// without going through this process's bounce buffer.
pub fn sys_memset(
    transport: &impl KernelCallTransport,
    process: i32,
    pattern: u64,
    base: u64,
    bytes: u64,
) -> Result<(), i32> {
    if bytes == 0 {
        return Ok(());
    }
    let mut msg = cleared_message();
    {
        // SAFETY: m_lsys_krn_sys_memset is the documented payload of MEMSET
        // (the same union arm at kernel syscall_copy.rs msg_memset:187-191).
        let m = unsafe { &mut msg.m_u.m_lsys_krn_sys_memset };
        m.base = base;
        m.count = bytes;
        m.pattern = pattern;
        m.process = process;
    }
    let reply = perform_kernel_call(transport, minix_types::SYS_MEMSET, &mut msg, |_| {});
    if reply < 0 {
        return Err(reply);
    }
    Ok(())
}

/// clock_time client.
///
/// C: libsys `clock_time` (clock_time.c:12-38) reads boottime/realtime/hz from
/// kerninfo's kclockinfo and computes `sec = boottime + realtime / hz`. This
/// rewrite carries the same-source data through the SYS_TIMES + GET_HZ kernel
/// calls (the user mapping of the kerninfo page lands with the E1 power-up
/// family, see the E-KERNINFO note) — same ClockState source, different
/// transport, equivalent semantics.
pub fn clock_time_via(transport: &impl KernelCallTransport) -> Result<u64, i32> {
    let times = sys_times(transport, minix_types::Endpoint::SELF.0)?;
    let hz = sys_get_hz(transport)?;
    if hz <= 0 {
        return Err(-minix_types::EINVAL);
    }
    Ok(times.boot_time + times.real_ticks / hz as u64)
}

/// SYS_SCHEDCTL(54): scheduling control — the KERNEL branch of the RS SchedApi
/// (C `sched_start.c:46-88`: `sys_schedctl(SCHEDCTL_FLAG_KERNEL, endpt,
/// priority, quantum, cpu)`; kernel `dispatch_schedctl`; semantics: -1 = keep the
/// current value).
pub fn sys_schedctl(
    transport: &impl KernelCallTransport,
    endpoint: i32,
    priority: i32,
    quantum: i32,
    cpu: i32,
) -> Result<(), i32> {
    let mut msg = Message::default();
    {
        // SAFETY: m_lsys_krn_schedctl is the documented payload of SCHEDCTL
        // (kernel/src/syscall_process.rs msg_schedctl:114-117).
        let sc = unsafe { &mut msg.m_u.m_lsys_krn_schedctl };
        sc.flags = minix_types::SCHEDCTL_FLAG_KERNEL;
        sc.endpoint = endpoint;
        sc.priority = priority;
        sc.quantum = quantum;
        sc.cpu = cpu;
    }
    let reply = perform_kernel_call(transport, minix_types::SYS_SCHEDCTL, &mut msg, |_| {});
    if reply < 0 {
        return Err(reply);
    }
    Ok(())
}

/// SYS_SIGRETURN: signal-handler return, restoring the target process's
/// signaled context (C: libsys `sys_sigreturn` — `m_sigcalls.endpt/sigctx`;
/// kernel `dispatch_sigreturn`).
pub fn sys_sigreturn(
    transport: &impl KernelCallTransport,
    proc_endpt: i32,
    sig_ctx: u64,
) -> Result<(), i32> {
    let mut msg = Message::default();
    {
        // SAFETY: m_sigcalls is the documented payload layout of SIGRETURN
        // (kernel/src/syscall_signal.rs dispatch_sigreturn).
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

/// SYS_SPROF: statistical profiling control (C: libsys `sys_sprof` — the
/// action/freq/intr_type/endpt/ctl_ptr/mem_ptr of `m_lsys_krn_sys_sprof`;
/// kernel `dispatch_profile`).
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
        // SAFETY: m_lsys_krn_sys_sprof is the documented payload layout of
        // SPROF (kernel/src/misc.rs dispatch_profile; C ipc.h).
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

/// SYS_SETTIME: set the real-time clock or adjust the gradual drift (C: libsys
/// `sys_settime` — the now/clock_id/sec/nsec of `m_lsys_krn_sys_settime`;
/// kernel `dispatch_settime`, EINVAL for anything other than CLOCK_REALTIME).
pub fn sys_settime(
    transport: &impl KernelCallTransport,
    now: i32,
    clock_id: i32,
    sec: u64,
    nsec: i64,
) -> Result<(), i32> {
    let mut msg = Message::default();
    {
        // SAFETY: m_lsys_krn_sys_settime is the documented payload layout of
        // SETTIME (kernel/src/syscall_clock.rs dispatch_settime).
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

/// SYS_STIME: set the system boot time (C: libsys `sys_stime` —
/// `m_lsys_krn_sys_stime.boot_time`; kernel `dispatch_stime`).
pub fn sys_stime(transport: &impl KernelCallTransport, boottime: u64) -> Result<(), i32> {
    let mut msg = Message::default();
    {
        // SAFETY: m_lsys_krn_sys_stime is the documented payload layout of
        // STIME (kernel/src/syscall_clock.rs dispatch_stime).
        let t = unsafe { &mut msg.m_u.m_lsys_krn_sys_stime };
        t.boot_time = boottime;
    }
    let reply = perform_kernel_call(transport, minix_types::SYS_STIME, &mut msg, |_| {});
    if reply < 0 {
        return Err(reply);
    }
    Ok(())
}

/// SYS_VTIMER: set/fetch a process's virtual or profile timer (C: libsys
/// `sys_vtimer` — kernel `dispatch_vtimer` reads which/set/value/endpt from m_m2,
/// and the old value is written back through `m_m2.m2l1`).
///
/// When `new_val` is `Some` a new value is set; the reply carries the old value.
pub fn sys_vtimer(
    transport: &impl KernelCallTransport,
    proc_endpt: i32,
    which: i32,
    new_val: Option<u64>,
) -> Result<Option<u64>, i32> {
    let mut msg = Message::default();
    {
        // SAFETY: the kernel dispatch_vtimer reads VT_WHICH/SET/VALUE/ENDPT from
        // m_m2 (syscall_clock.rs:437-441; C do_vtimer.c:33-71).
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
    // SAFETY: the kernel writes the old value back into m_m2.m2l1
    // (syscall_clock.rs:505-508).
    let old = unsafe { msg.m_u.m_m2.m2l1 } as u64;
    Ok(Some(old))
}

/// SYS_GETMCONTEXT: fetch the target process's machine context (the
/// endpt/ctx_ptr of `m_lsys_krn_sys_mcontext`; kernel `dispatch_getmcontext`).
pub fn sys_getmcontext(
    transport: &impl KernelCallTransport,
    proc_endpt: i32,
    ctx_ptr: u64,
) -> Result<(), i32> {
    let mut msg = Message::default();
    {
        // SAFETY: m_lsys_krn_sys_mcontext is the payload layout shared by
        // GET/SETMCONTEXT (kernel/src/syscall.rs dispatch_getmcontext).
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

/// SYS_SETMCONTEXT: set the target process's machine context (the same payload
/// arm as [`sys_getmcontext`], kernel `dispatch_setmcontext`).
pub fn sys_setmcontext(
    transport: &impl KernelCallTransport,
    proc_endpt: i32,
    ctx_ptr: u64,
) -> Result<(), i32> {
    let mut msg = Message::default();
    {
        // SAFETY: same as sys_getmcontext — GET/SET share m_lsys_krn_sys_mcontext.
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ipc::TrapStatus;

    #[test]
    fn test_sys_kill_encodes_sigcalls_wire() {
        // C: libsys sys_kill.c:3-13 — m_sigcalls.endpt/sig, SYS_KILL = 5;
        // the reply is OK (the kernel do_kill has no reply payload).
        let mut canned = CannedKernelCallTransport::new();
        canned.reply(0);

        let r = sys_kill(&canned, 7, 9); // endpoint 7, SIGKILL

        assert_eq!(r, 0);
        // The call number must match `Syscall::Kill = 6` in
        // kernel/src/syscall.rs (the kernel enum; the wrong number 5 is
        // SYS_TRACE).
        assert_eq!(SYS_KILL_CALL, 6);
        let sent = canned.sent.borrow();
        assert_eq!(sent.len(), 1);
        assert_eq!(sent[0].m_type, SYS_KILL_CALL);
        assert_eq!(unsafe { sent[0].m_u.m_sigcalls }.endpt, 7);
        assert_eq!(unsafe { sent[0].m_u.m_sigcalls }.sig, 9);
    }

    #[test]
    fn test_sys_statectl_encodes_request_address_length() {
        // The payload shape of C sys_statectl.c:3-11: request/address/length go
        // into m_lsys_krn_sys_statectl, and the call number is
        // minix_types::SYS_STATECTL (com.h:263, KERNEL_CALL+55 = 0x637; a wrong
        // number would land in another kernel call).
        let canned = CannedKernelCallTransport::default();
        let r = sys_statectl(&canned, minix_types::SYS_STATE_CLEAR_IPC_FILTERS, 0, 0);
        assert_eq!(r, 0);
        let sent = canned.sent.borrow();
        assert_eq!(sent.len(), 1);
        assert_eq!(sent[0].m_type, minix_types::SYS_STATECTL);
        let sc = unsafe { sent[0].m_u.m_lsys_krn_sys_statectl };
        assert_eq!(sc.request, minix_types::SYS_STATE_CLEAR_IPC_FILTERS);
        assert_eq!(sc.address, 0);
        assert_eq!(sc.length, 0);
    }

    #[test]
    fn test_sys_clear_encodes_m1_endpoint() {
        // C: libsys sys_clear.c:8-14 — m1i1 = target endpoint, SYS_CLEAR = 2;
        // no reply payload, OK means reclamation is done.
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
    fn test_sys_whoami_requests_get_whoami_and_decodes_reply_overlay() {
        // C: libsys sys_getinfo.c:29-55 — the request carries only
        // request=GET_WHOAMI; the reply writes m_krn_lsys_sys_getwhoami directly
        // (endpt/privflags/initflags/name).
        let mut canned = CannedKernelCallTransport::new();
        let mut reply = Message::default();
        {
            // SAFETY: the test fills the documented GET_WHOAMI reply layout.
            let w = unsafe { &mut reply.m_u.m_krn_lsys_sys_getwhoami };
            w.endpt = 0x5F5C; // any slot-derived endpoint (just not a sentinel value)
            w.privflags = 0x0000_0040;
            w.initflags = 0x0000_0003;
            let mut name = [0u8; 44];
            name[..11].copy_from_slice(b"drv.chr.tty");
            w.name = name;
        }
        canned.reply_message(reply);

        let who = sys_whoami(&canned).expect("kernel answered OK");

        assert_eq!(who.endpoint, Endpoint(0x5F5C));
        assert_eq!(who.priv_flags, 0x40);
        assert_eq!(who.init_flags, 3);
        assert_eq!(&who.name[..12], b"drv.chr.tty\0");
        // Request leg: m_type = SYS_GETINFO, the overlay carries only request=GET_WHOAMI.
        let sent = canned.sent.borrow();
        assert_eq!(sent.len(), 1);
        assert_eq!(sent[0].m_type, minix_types::SYS_GETINFO);
        assert_eq!(
            unsafe { sent[0].m_u.m_lsys_krn_sys_getinfo }.request,
            minix_types::GET_WHOAMI
        );
    }

    #[test]
    fn test_sys_whoami_propagates_kernel_error() {
        // C sys_getinfo.c:43-44 — a negative _kernel_call value is returned as-is.
        let mut canned = CannedKernelCallTransport::new();
        canned.reply(-1);

        assert_eq!(sys_whoami(&canned), Err(-1));
    }

    #[test]
    fn test_sys_times_encodes_endpt_and_decodes_reply() {
        // C: libsys sys_times.c:3-23 — the request m_lsys_krn_sys_times.endpt;
        // the reply m_krn_lsys_sys_times (the four values user/system etc.).
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
        // dst_addr, nr_bytes}; SELF is replaced by the kernel with the caller
        // endpoint.
        // src uses the `SELF` constant (the same shape as the ipc-server/rs/ds/pm
        // consumption sites).
        let mut canned = CannedKernelCallTransport::new();
        canned.reply(0);

        let r = sys_vircopy(&canned, SELF, 0x1000, 9, 0x7000, 144);

        assert_eq!(r, 0);
        let sent = canned.sent.borrow();
        assert_eq!(sent[0].m_type, SYS_VIRCOPY_CALL);
        let cp = unsafe { &sent[0].m_u.m_lsys_krn_sys_copy };
        assert_eq!(cp.src_endpt, SELF);
        // Sentinel absolute-value pin: -2 is the SYSTEM task endpoint (com.h:50);
        // if the constant falls back to the historical wrong value, SELF
        // substitution on the real wire layer silently stops working.
        assert_eq!(SELF, 31742); // endpoint.h:56
        assert_eq!(cp.src_addr, 0x1000);
        assert_eq!(cp.dst_endpt, 9);
        assert_eq!(cp.dst_addr, 0x7000);
        assert_eq!(cp.nr_bytes, 144);
    }

    #[test]
    fn test_sys_vumap_encodes_request_and_decodes_pcount() {
        // C: libsys sys_vumap.c:14-38 — the request arm fills endpt/vaddr/vcount/
        // offset/access/paddr/pmax, and the reply fills
        // m_krn_lsys_sys_vumap.pcount. The single-element input vector is the
        // shape of the tree's only caller (vbox store_pages).
        let mut canned = CannedKernelCallTransport::new();
        let mut reply = Message::default();
        reply.m_type = 0;
        // SAFETY(test): build the reply arm as the kernel's pcount fill.
        let arm = unsafe { &mut reply.m_u.m_krn_lsys_sys_vumap };
        arm.pcount = 2;
        canned.reply_message(reply);

        let mut out = [VumapPhys::default(); 8];
        let r = sys_vumap(&canned, Endpoint(7), 42, 0x2000, 0x100, VUA_READ, &mut out);

        assert_eq!(r, Ok(2));
        let sent = canned.sent.borrow();
        assert_eq!(sent[0].m_type, minix_types::SYS_VUMAP);
        let v = unsafe { &sent[0].m_u.m_lsys_krn_sys_vumap };
        assert_eq!(v.endpt, 7);
        assert_eq!(v.vcount, 1);
        assert_eq!(v.offset, 0x100);
        assert_eq!(v.access, VUA_READ);
        assert_eq!(v.pmax, 8);
        // vaddr/paddr are caller virtual addresses (C passes vir_bytes the same
        // way); only nonzero-ness is checked.
        assert_ne!(v.vaddr, 0);
        assert_ne!(v.paddr, 0);
        // C layout of the two vector elements (type.h:40-53): 16 bytes each on 64-bit.
        assert_eq!(core::mem::size_of::<VumapVir>(), 16);
        assert_eq!(core::mem::size_of::<VumapPhys>(), 16);
        assert_eq!(core::mem::offset_of!(VumapVir, size), 8);
        assert_eq!(core::mem::offset_of!(VumapPhys, size), 8);

        // A negative errno is not swallowed: returned as-is.
        let mut denied = CannedKernelCallTransport::new();
        denied.reply(-minix_types::EFAULT);
        let mut out = [VumapPhys::default(); 4];
        assert_eq!(
            sys_vumap(&denied, Endpoint(7), 42, 0x1000, 0, VUA_WRITE, &mut out),
            Err(-minix_types::EFAULT)
        );
    }

    #[test]
    fn test_sys_trace_encodes_lsys_layout_and_decodes_read_value() {
        // C: libsys sys_trace.c:3-18 — payload mess_lsys_krn_sys_trace
        // (request@0/endpt@4/address@8/data@16, **different from m_m1**:
        // m1.m1i1 is request rather than endpt; the kernel misc.rs msg_trace
        // reads this layout); the reply's read value is written back into the
        // data field at the same offset (write_trace_reply_data).
        let mut canned = CannedKernelCallTransport::new();
        let mut reply = Message::default();
        reply.m_type = 0;
        unsafe { reply.m_u.m_lsys_krn_sys_trace.data = 0x55 };
        canned.reply_message(reply);

        let mut data: i64 = 0; // READ-class commands ignore the input value
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
        // C: do_getksig.c:31-32 — the reply m_sigcalls.{endpt, map}; an endpt of
        // NONE means nothing more is pending on the kernel side (the termination
        // condition of PM's pull loop).
        use minix_types::MessSigcalls;
        let mut canned = CannedKernelCallTransport::new();
        let mut reply = Message::default();
        reply.m_type = 0;
        unsafe {
            // Signal 15 → bit 14 of word 0 (matching C `__sigmask(15)`).
            reply.m_u.m_sigcalls = MessSigcalls {
                map: [1u32 << 14, 0, 0, 0],
                endpt: 42,
                sig: 0,
                sigctx: 0,
                _padding: [0u8; 24],
            };
        }
        canned.reply_message(reply);

        let (endpt, map) = sys_getksig(&canned).expect("OK reply decodes");

        assert_eq!(endpt, 42);
        assert!(minix_types::sigset_contains(map, 15));
        assert!(!minix_types::sigset_contains(map, 14));
        assert_eq!(SYS_GETKSIG_CALL, 7, "kernel Syscall::Getksig = 7");
        let sent = canned.sent.borrow();
        assert_eq!(sent[0].m_type, SYS_GETKSIG_CALL);
    }

    #[test]
    fn test_sys_endksig_encodes_endpt_and_sig() {
        // C: do_endksig.c — payload m_sigcalls.{endpt, sig}; the caller must be
        // the target process's signal manager (otherwise the kernel answers
        // EPERM, passed through as a negative errno).
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
        // C: the return value = the kernel reply (a negative errno passes through
        // as-is; do_trace does not fold it).
        let mut canned = CannedKernelCallTransport::new();
        canned.reply(-1); // EPERM

        let mut data: i64 = 0;
        assert_eq!(sys_trace(&canned, 8 /* T_EXIT */, 7, 0, &mut data), Err(-1));
        assert_eq!(canned.calls.get(), 1);
    }

    #[test]
    fn test_sys_runctl_resume_encodes_m1() {
        // C: syslib.h:48 — sys_resume = sys_runctl(ep, RC_RESUME, 0);
        // payload m1i1 = RC_ENDPT, m1i2 = RC_RESUME, m1i3 = RC_FLAGS(0).
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
        // C: main.c:309 — the return value is ignored; on failure PM keeps its
        // main loop running.
        let mut canned = CannedKernelCallTransport::new();
        canned.reply(-5);

        assert_eq!(sys_abort(&canned, 0x0008), -5);
        assert_eq!(canned.calls.get(), 1);
    }

    #[test]
    fn test_sys_kill_negative_errno_passthrough() {
        // C: the return value = the kernel reply (a negative errno passes through
        // as-is; each caller decides).
        let mut canned = CannedKernelCallTransport::new();
        canned.reply(-1); // EPERM

        assert_eq!(sys_kill(&canned, 7, 9), -1);
        assert_eq!(canned.calls.get(), 1);
    }
    use super::DirectKernelCallTransport;
    use super::*;
    use crate::ipc::{CALL_SENDREC, CannedTransport};

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
        //[SD19-gate] Test fixture: CannedTransport answers only the first scripted call; the call count is asserted by the counter below
        #[allow(clippy::let_underscore_must_use)]
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
    fn test_transport_failure_short_circuits_as_typed_error() {
        // Both producers report the positive errno in `Err(TrapStatus)` —
        // the hosted fallback (`TrapStatus(EIO)`) and the real-trap reply
        // register alike. The status short-circuits into the typed error;
        // writing it into the message type field instead made every hosted
        // round-trip failure read back as a positive "success"
        // (edge E-SYSCALL-SIGN).
        let mut transport = CannedTransport::new();
        transport.reply_sendrec(Err(TrapStatus(minix_types::EIO)));
        let mut message = test_message(0);
        let result = perform_syscall(&transport, Endpoint(1), 5, &mut message);
        assert_eq!(result, Err(Errno::from_i32(minix_types::EIO)));
        // The message keeps the call number: the failure never touched it.
        assert_eq!(message.m_type, 5);
        assert_eq!(transport.sendrec_calls.get(), 1);
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
    fn test_taskcall_returns_raw_reply_untouched() {
        let mut transport = CannedTransport::new();
        transport.reply_sendrec(Ok(test_message(-5)));
        let mut message = test_message(0);
        assert_eq!(
            perform_taskcall(&transport, Endpoint(0), 41, &mut message),
            Ok(-5)
        );
    }

    #[test]
    fn test_taskcall_transport_failure_is_typed_error() {
        // The positive-errno transport failure becomes the Err lane; the raw
        // integer channel only ever carries server replies now.
        let mut transport = CannedTransport::new();
        transport.reply_sendrec(Err(TrapStatus(minix_types::EIO)));
        let mut message = test_message(0);
        assert_eq!(
            perform_taskcall(&transport, Endpoint(0), 41, &mut message),
            Err(Errno::from_i32(minix_types::EIO))
        );
    }

    // ── E2: CannedTransport replay tests for the six VM-side SYS_* wrappers ──

    /// SYS_FORK replay: field by field on the request arm + out-parameter
    /// decode on the reply arm (the E-FORKMSG shape).
    #[test]
    fn test_sys_fork_wire_roundtrip() {
        let mut canned = CannedKernelCallTransport::new();
        let mut reply = Message::default();
        reply.m_type = 0;
        // SAFETY(test): build the reply arm mirroring the kernel's in-place write
        // at do_fork.c:111-112.
        let arm = unsafe { &mut reply.m_u.m_krn_lsys_sys_fork };
        arm.endpt = 32771;
        arm.msgaddr = 0x7000;
        canned.reply_message(reply);

        let r = sys_fork(&canned, 10, 3, 0);
        assert_eq!(r, Ok((32771, 0x7000)));
        let sent = canned.sent.borrow();
        assert_eq!(sent.len(), 1);
        assert_eq!(sent[0].m_type, minix_types::SYS_FORK); // 0x600
        // SAFETY(test): read the request arm back to check each field.
        let req = unsafe { sent[0].m_u.m_lsys_krn_sys_fork };
        assert_eq!((req.endpt, req.slot, req.flags), (10, 3, 0));
    }

    /// SYS_FORK error pass-through (a negative errno is not folded).
    #[test]
    fn test_sys_fork_negative_errno_passthrough() {
        let mut canned = CannedKernelCallTransport::new();
        canned.reply(-14); // EFAULT
        assert_eq!(sys_fork(&canned, 10, 3, 0), Err(-14));
        assert_eq!(canned.calls.get(), 1);
    }

    /// SYS_EXEC replay: five payload fields + OK.
    #[test]
    fn test_sys_exec_wire_roundtrip() {
        let mut canned = CannedKernelCallTransport::new();
        canned.reply(0);
        let r = sys_exec(&canned, 8, 0x1000, 0x9000, 0x8000, 0x7f00);
        assert_eq!(r, Ok(()));
        let sent = canned.sent.borrow();
        assert_eq!(sent[0].m_type, minix_types::SYS_EXEC); // 0x601
        // SAFETY(test): read the request arm back to check each field.
        let e = unsafe { sent[0].m_u.m_lsys_krn_sys_exec };
        assert_eq!(
            (e.endpt, e.ip, e.stack, e.name, e.ps_str),
            (8, 0x1000, 0x9000, 0x8000, 0x7f00)
        );
    }

    /// SYS_SAFECOPYFROM replay: the grant quintuple + OK (no reply out-parameter).
    #[test]
    fn test_sys_safecopyfrom_wire_roundtrip() {
        let mut canned = CannedKernelCallTransport::new();
        canned.reply(0);
        let r = sys_safecopyfrom(&canned, 4, 12, 0x40, 0x20_0000, 256);
        assert_eq!(r, Ok(()));
        let sent = canned.sent.borrow();
        assert_eq!(sent[0].m_type, minix_types::SYS_SAFECOPYFROM); // 0x61f
        // SAFETY(test): read the request arm back to check each field.
        let sc = unsafe { sent[0].m_u.m_lsys_kern_safecopy };
        assert_eq!(
            (sc.from_to, sc.grant_id, sc.offset, sc.address, sc.bytes),
            (4, 12, 0x40, 0x20_0000, 256)
        );
    }

    /// SYS_SAFECOPYTO replay: same arm, opposite call number.
    #[test]
    fn test_sys_safecopyto_wire_roundtrip() {
        let mut canned = CannedKernelCallTransport::new();
        canned.reply(0);
        let r = sys_safecopyto(&canned, 4, 12, 0x40, 0x20_0000, 256);
        assert_eq!(r, Ok(()));
        let sent = canned.sent.borrow();
        assert_eq!(sent[0].m_type, minix_types::SYS_SAFECOPYTO); // 0x620
    }

    /// SYS_UPDATE replay: the three m_m1 fields (the real shape of C do_update.c:9-11).
    #[test]
    fn test_sys_update_wire_roundtrip() {
        let mut canned = CannedKernelCallTransport::new();
        canned.reply(0);
        let r = sys_update(&canned, 2, 9, 1);
        assert_eq!(r, Ok(()));
        let sent = canned.sent.borrow();
        assert_eq!(sent[0].m_type, minix_types::SYS_UPDATE); // 0x634
        // SAFETY(test): read the m_m1 arm back to check each field.
        let m1 = unsafe { sent[0].m_u.m_m1 };
        assert_eq!((m1.m1i1, m1.m1i2, m1.m1i3), (2, 9, 1));
    }

    /// SYS_DIAGCTL replay: code/buf/len + OK.
    #[test]
    fn test_sys_diagctl_wire_roundtrip() {
        let mut canned = CannedKernelCallTransport::new();
        canned.reply(0);
        let r = sys_diagctl(&canned, 1, 0x4000, 32);
        assert_eq!(r, Ok(()));
        let sent = canned.sent.borrow();
        assert_eq!(sent[0].m_type, minix_types::SYS_DIAGCTL); // 0x62c
        // SAFETY(test): read the payload arm back to check each field.
        let d = unsafe { sent[0].m_u.m_lsys_krn_sys_diagctl };
        assert_eq!((d.code, d.buf, d.len), (1, 0x4000, 32));
    }

    /// SYS_DIAGCTL STACKTRACE shape: arg2 = the target endpoint lands in the endpt field.
    #[test]
    fn test_sys_diagctl_stacktrace_encodes_endpt() {
        let mut canned = CannedKernelCallTransport::new();
        canned.reply(0);
        let r = sys_diagctl(&canned, 2, 0, 9); // DIAGCTL_CODE_STACKTRACE, endpt 9
        assert_eq!(r, Ok(()));
        let sent = canned.sent.borrow();
        // SAFETY(test): read the payload arm back to check the endpt field.
        let d = unsafe { sent[0].m_u.m_lsys_krn_sys_diagctl };
        assert_eq!((d.code, d.endpt), (2, 9));
    }

    /// SYS_DEVIO outb replay: request word `_DIO_OUTPUT|_DIO_BYTE` + port/value.
    #[test]
    fn test_sys_outb_encodes_devio_wire() {
        let mut canned = CannedKernelCallTransport::new();
        canned.reply(0);

        let r = sys_outb(&canned, 0x3F8, 0x61);

        assert_eq!(r, Ok(()));
        let sent = canned.sent.borrow();
        assert_eq!(sent[0].m_type, minix_types::SYS_DEVIO);
        // SAFETY(test): read the payload arm back to check request/port/value.
        let d = unsafe { sent[0].m_u.m_lsys_krn_sys_devio };
        assert_eq!(d.request, _DIO_OUTPUT | _DIO_BYTE);
        assert_eq!(d.port, 0x3F8);
        assert_eq!(d.value, 0x61);
    }

    /// SYS_DEVIO inb replay: request word `_DIO_INPUT|_DIO_BYTE`, reply value
    /// taken from `mess_krn_lsys_sys_devio` value@0 (the kernel dispatch_devio
    /// Input arm fills the same slot); an errno reply takes the Err lane.
    #[test]
    fn test_sys_inb_decodes_reply_value() {
        let mut canned = CannedKernelCallTransport::new();
        let mut reply = Message::default();
        reply.m_type = 0;
        // SAFETY(test): build the reply arm mirroring the kernel fill
        // (syscall_device.rs Input arm).
        let arm = unsafe { &mut reply.m_u.m_krn_lsys_sys_devio };
        arm.value = 0x20;
        canned.reply_message(reply);

        let r = sys_inb(&canned, 0x3FD); // COM1 LSR
        assert_eq!(r, Ok(0x20));
        let sent = canned.sent.borrow();
        assert_eq!(sent[0].m_type, minix_types::SYS_DEVIO);
        // SAFETY(test): read the request arm back to check direction/width/port.
        let d = unsafe { sent[0].m_u.m_lsys_krn_sys_devio };
        assert_eq!(d.request, _DIO_INPUT | _DIO_BYTE);
        assert_eq!(d.port, 0x3FD);

        // errno reply (EPERM=-1): a whitelist rejection (C do_devio.c:55 EPERM).
        let mut denied = CannedKernelCallTransport::new();
        denied.reply(-minix_types::EPERM);
        assert_eq!(sys_inb(&denied, 0x3FD), Err(-minix_types::EPERM));
    }

    /// SYS_DEVIO outw replay: request word `_DIO_OUTPUT|_DIO_WORD` + port/value.
    /// Counterpart of C syslib.h:218 `sys_outw(p,v)=sys_out(p,v,_DIO_WORD)`.
    #[test]
    fn test_sys_outw_encodes_devio_word_wire() {
        let mut canned = CannedKernelCallTransport::new();
        canned.reply(0);

        let r = sys_outw(&canned, 0x1F0, 0xABCD);

        assert_eq!(r, Ok(()));
        let sent = canned.sent.borrow();
        assert_eq!(sent[0].m_type, minix_types::SYS_DEVIO);
        // SAFETY(test): read the payload arm back to check direction/width/port/value.
        let d = unsafe { sent[0].m_u.m_lsys_krn_sys_devio };
        assert_eq!(d.request, _DIO_OUTPUT | _DIO_WORD);
        assert_eq!(d.port, 0x1F0);
        assert_eq!(d.value, 0xABCD);
    }

    /// SYS_DEVIO inw replay: request word `_DIO_INPUT|_DIO_WORD`, reply value
    /// taken from the low 16 bits (the kernel `IoSize::Word` arm zero-extends
    /// `inw` into the value slot); an errno takes the Err lane.
    #[test]
    fn test_sys_inw_decodes_reply_word() {
        let mut canned = CannedKernelCallTransport::new();
        let mut reply = Message::default();
        reply.m_type = 0;
        // SAFETY(test): build the reply arm mirroring the kernel fill
        // (syscall_device.rs Input Word arm).
        let arm = unsafe { &mut reply.m_u.m_krn_lsys_sys_devio };
        arm.value = 0x1234;
        canned.reply_message(reply);

        let r = sys_inw(&canned, 0x1F0); // IDE data register
        assert_eq!(r, Ok(0x1234));
        let sent = canned.sent.borrow();
        // SAFETY(test): read the request arm back to check direction/width/port.
        let d = unsafe { sent[0].m_u.m_lsys_krn_sys_devio };
        assert_eq!(d.request, _DIO_INPUT | _DIO_WORD);
        assert_eq!(d.port, 0x1F0);

        let mut denied = CannedKernelCallTransport::new();
        denied.reply(-minix_types::EPERM);
        assert_eq!(sys_inw(&denied, 0x1F0), Err(-minix_types::EPERM));
    }

    /// SYS_DEVIO outl/inl replay: the request word carries `_DIO_LONG`, the
    /// reply value is all 32 bits. Counterpart of C syslib.h:219/:225
    /// sys_outl/sys_inl.
    #[test]
    fn test_sys_outl_inl_long_wire_roundtrip() {
        let mut out = CannedKernelCallTransport::new();
        out.reply(0);
        assert_eq!(sys_outl(&out, 0x020, 0xDEAD_BEEF), Ok(()));
        {
            let sent = out.sent.borrow();
            // SAFETY(test): read the request arm back to check the width bits and the value.
            let d = unsafe { sent[0].m_u.m_lsys_krn_sys_devio };
            assert_eq!(d.request, _DIO_OUTPUT | _DIO_LONG);
            assert_eq!(d.value, 0xDEAD_BEEF);
        }

        let mut canned = CannedKernelCallTransport::new();
        let mut reply = Message::default();
        reply.m_type = 0;
        // SAFETY(test): build the reply arm mirroring the kernel Long fill.
        let arm = unsafe { &mut reply.m_u.m_krn_lsys_sys_devio };
        arm.value = 0xFFFF_0000;
        canned.reply_message(reply);
        assert_eq!(sys_inl(&canned, 0x020), Ok(0xFFFF_0000));
        let sent = canned.sent.borrow();
        // SAFETY(test): read the request arm back to check direction/width.
        let d = unsafe { sent[0].m_u.m_lsys_krn_sys_devio };
        assert_eq!(d.request, _DIO_INPUT | _DIO_LONG);
    }

    /// SYS_SETALARM replay: request exp/abs + reply arm time_left/uptime.
    #[test]
    fn test_sys_setalarm_wire_roundtrip() {
        let mut canned = CannedKernelCallTransport::new();
        let mut reply = Message::default();
        reply.m_type = 0;
        // SAFETY(test): build the reply arm mirroring the kernel fill
        // (syscall_clock.rs:265).
        let arm = unsafe { &mut reply.m_u.m_lsys_krn_sys_setalarm };
        arm.time_left = 500;
        arm.uptime = 9000;
        canned.reply_message(reply);

        let r = sys_setalarm(&canned, 100, false);
        assert_eq!(r, Ok((500, 9000)));
        let sent = canned.sent.borrow();
        assert_eq!(sent[0].m_type, minix_types::SYS_SETALARM); // 0x618
        // SAFETY(test): read the request arm back.
        let a = unsafe { sent[0].m_u.m_lsys_krn_sys_setalarm };
        assert_eq!((a.exp_time, a.abs_time), (100, 0));
    }

    /// SYS_SIGRETURN replay: endpt/sigctx.
    #[test]
    fn test_sys_sigreturn_wire_roundtrip() {
        let mut canned = CannedKernelCallTransport::new();
        canned.reply(0);
        let r = sys_sigreturn(&canned, 7, 0x30_0000);
        assert_eq!(r, Ok(()));
        let sent = canned.sent.borrow();
        assert_eq!(sent[0].m_type, minix_types::SYS_SIGRETURN); // 0x60a
        // SAFETY(test): read the payload arm back.
        let sc = unsafe { sent[0].m_u.m_sigcalls };
        assert_eq!((sc.endpt, sc.sigctx), (7, 0x30_0000));
    }

    /// SYS_SPROF replay: six fields.
    #[test]
    fn test_sys_sprof_wire_roundtrip() {
        let mut canned = CannedKernelCallTransport::new();
        canned.reply(0);
        let r = sys_sprof(
            &canned, 0, /* start */
            4096, 1, /* NMI */
            5, 0x5000, 0x6000,
        );
        assert_eq!(r, Ok(()));
        let sent = canned.sent.borrow();
        assert_eq!(sent[0].m_type, minix_types::SYS_SPROF); // 0x624
        // SAFETY(test): read the payload arm back.
        let s = unsafe { sent[0].m_u.m_lsys_krn_sys_sprof };
        assert_eq!(
            (s.action, s.freq, s.intr_type, s.endpt, s.ctl_ptr, s.mem_ptr),
            (0, 4096, 1, 5, 0x5000, 0x6000)
        );
    }

    /// SYS_SETTIME replay: four fields.
    #[test]
    fn test_sys_settime_wire_roundtrip() {
        let mut canned = CannedKernelCallTransport::new();
        canned.reply(0);
        let r = sys_settime(
            &canned,
            1, /* now */
            0, /* REALTIME */
            1_700_000_000,
            500,
        );
        assert_eq!(r, Ok(()));
        let sent = canned.sent.borrow();
        assert_eq!(sent[0].m_type, minix_types::SYS_SETTIME); // 0x628
        // SAFETY(test): read the payload arm back.
        let t = unsafe { sent[0].m_u.m_lsys_krn_sys_settime };
        assert_eq!(
            (t.now, t.clock_id, t.sec, t.nsec),
            (1, 0, 1_700_000_000, 500)
        );
    }

    /// SYS_STIME replay: boot_time.
    #[test]
    fn test_sys_stime_wire_roundtrip() {
        let mut canned = CannedKernelCallTransport::new();
        canned.reply(0);
        let r = sys_stime(&canned, 1_700_000_000);
        assert_eq!(r, Ok(()));
        let sent = canned.sent.borrow();
        assert_eq!(sent[0].m_type, minix_types::SYS_STIME); // 0x627
        // SAFETY(test): read the payload arm back.
        let t = unsafe { sent[0].m_u.m_lsys_krn_sys_stime };
        assert_eq!(t.boot_time, 1_700_000_000);
    }

    /// SYS_VTIMER replay: the m_m2 shape (which/set/value/endpt) + old-value read-back.
    #[test]
    fn test_sys_vtimer_wire_roundtrip() {
        let mut canned = CannedKernelCallTransport::new();
        let mut reply = Message::default();
        reply.m_type = 0;
        // SAFETY(test): build the old-value write-back (the kernel writes m_m2.m2l1).
        reply.m_u.m_m2.m2l1 = 1234;
        canned.reply_message(reply);

        let r = sys_vtimer(&canned, 8, 1 /* VT_VIRTUAL */, Some(5000));
        assert_eq!(r, Ok(Some(1234)));
        let sent = canned.sent.borrow();
        assert_eq!(sent[0].m_type, minix_types::SYS_VTIMER); // 0x62d
        // SAFETY(test): read the m_m2 request fields back.
        let m2 = unsafe { sent[0].m_u.m_m2 };
        assert_eq!((m2.m2i1, m2.m2i2, m2.m2l1, m2.m2l2), (1, 1, 5000, 8));
    }

    /// SYS_GETMCONTEXT replay: endpt/ctx_ptr (GET/SET share the arm, different
    /// call numbers).
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

/// GETINFO carrier: request field, buffer pointer, length and endpoint
/// field each land in m_lsys_krn_sys_getinfo, and the kernel replies OK.
#[test]
fn test_sys_getinfo_into_wire() {
    let mut canned = CannedKernelCallTransport::new();
    canned.reply(0);
    let mut out = [0u8; 16];
    let r = sys_getinfo_into(
        &canned,
        minix_types::GET_MACHINE,
        &mut out,
        minix_types::Endpoint::NONE.0,
    );
    assert!(r.is_ok());
    // Assertion on the outgoing message (CannedKernelCallTransport records sent).
    let sent = canned.sent.borrow();
    assert_eq!(sent.len(), 1);
    // SAFETY: read the m_lsys_krn_sys_getinfo arm back in the assertion.
    let gi = unsafe { sent[0].m_u.m_lsys_krn_sys_getinfo };
    assert_eq!(gi.request, minix_types::GET_MACHINE);
    assert_eq!(gi.val_ptr, out.as_ptr() as u64);
    assert_eq!(gi.val_len, 16);
}

/// GET_HZ goes through a 4-byte buffer and decodes back to i32 little-endian.
#[test]
fn test_sys_get_hz_roundtrip() {
    let mut canned = CannedKernelCallTransport::new();
    canned.reply(0);
    let hz = sys_get_hz(&canned).unwrap();
    assert_eq!(hz, 0, "hosted canned reply zeros the buffer");
    let sent = canned.sent.borrow();
    // SAFETY: read the request field back in the assertion.
    let gi = unsafe { sent[0].m_u.m_lsys_krn_sys_getinfo };
    assert_eq!(gi.request, minix_types::GET_HZ);
    assert_eq!(gi.val_len, 4);
}

/// GET_PRIV's target endpoint lands in the val_len2_e field
/// (the kernel getinfo_priv:925-929 resolves the target from there).
#[test]
fn test_sys_get_priv_endpt_in_val_len2_e() {
    let mut canned = CannedKernelCallTransport::new();
    canned.reply(0);
    let mut out = [0u8; 64];
    sys_get_priv(&canned, 9, &mut out).unwrap();
    let sent = canned.sent.borrow();
    // SAFETY: read the getinfo arm back in the assertion.
    let gi = unsafe { sent[0].m_u.m_lsys_krn_sys_getinfo };
    assert_eq!(gi.request, minix_types::GET_PRIV);
    assert_eq!(gi.val_len2_e, 9);
}

/// GET_MONPARAMS request shape: request=4, the buffer length passed through
/// as-is (the (v,vl) two-parameter shape of the C syslib.h:187 macro).
#[test]
fn test_sys_getmonparams_wire() {
    let mut canned = CannedKernelCallTransport::new();
    canned.reply(0);
    let mut out = [0u8; 1024]; // sized to MULTIBOOT_PARAM_BUF_SIZE
    sys_getmonparams(&canned, &mut out).unwrap();
    let sent = canned.sent.borrow();
    // SAFETY: read the getinfo arm back in the assertion.
    let gi = unsafe { sent[0].m_u.m_lsys_krn_sys_getinfo };
    assert_eq!(gi.request, minix_types::GET_MONPARAMS);
    assert_eq!(gi.val_ptr, out.as_ptr() as u64);
    assert_eq!(gi.val_len, 1024);
}

/// GET_IMAGE request shape: request=1, buffer sized to the whole table
/// (NR_BOOT_PROCS × 40 = 680 bytes, C syslib.h:184 macro).
#[test]
fn test_sys_getimage_wire() {
    let mut canned = CannedKernelCallTransport::new();
    canned.reply(0);
    let mut out = [0u8; minix_types::NR_BOOT_PROCS * 40];
    sys_getimage(&canned, &mut out).unwrap();
    let sent = canned.sent.borrow();
    // SAFETY: read the getinfo arm back in the assertion.
    let gi = unsafe { sent[0].m_u.m_lsys_krn_sys_getinfo };
    assert_eq!(gi.request, minix_types::GET_IMAGE);
    assert_eq!(gi.val_len, (minix_types::NR_BOOT_PROCS * 40) as i32);
}

/// The PRIVCTL m_m1 payload (request/endpt/arg_ptr).
#[test]
fn test_sys_privctl_m1_wire() {
    let mut canned = CannedKernelCallTransport::new();
    canned.reply(0);
    sys_privctl(&canned, 9, 1 /* SYS_PRIV_ALLOW */, 0).unwrap();
    let sent = canned.sent.borrow();
    assert_eq!(sent.len(), 1);
    // SAFETY: read the m_m1 arm back in the assertion.
    let m1 = unsafe { sent[0].m_u.m_m1 };
    assert_eq!(m1.m1i1, 1);
    assert_eq!(m1.m1i2, 9);
    assert_eq!(m1.m1p1, 0);
}

/// TIMES reply-arm fill (RS get_ticks consumes boot_ticks).
#[test]
fn test_sys_times_reply_arm() {
    let mut canned = CannedKernelCallTransport::new();
    // Kernel-shaped reply: OK(0) + the five fields filled in the same arm.
    let mut reply = Message::default();
    reply.m_type = 0;
    // SAFETY: construct the reply arm.
    let arm = unsafe { &mut reply.m_u.m_krn_lsys_sys_times };
    arm.boot_ticks = 777;
    canned.reply_message(reply);
    let times = sys_times(&canned, minix_types::Endpoint::SELF.0).unwrap();
    assert_eq!(times.boot_ticks, 777);
    // The request endpt field = SELF.
    let sent = canned.sent.borrow();
    // SAFETY: read the times request arm back in the assertion.
    let req = unsafe { sent[0].m_u.m_lsys_krn_sys_times };
    assert_eq!(req.endpt, minix_types::Endpoint::SELF.0);
}

/// STACKTRACE diagnostic code (2) + target endpoint.
/// The `endpt` itself must be checked — an earlier version asserted only
/// code, so a target placed in the wrong argument slot (arg1 instead of C's
/// arg2) slipped through and the kernel always received endpt=0.
#[test]
fn test_sys_diagctl_stacktrace_code_and_target() {
    let mut canned = CannedKernelCallTransport::new();
    canned.reply(0);
    sys_diagctl_stacktrace(&canned, 9).unwrap();
    let sent = canned.sent.borrow();
    // SAFETY: read the diagctl arm back in the assertion.
    let d = unsafe { sent[0].m_u.m_lsys_krn_sys_diagctl };
    assert_eq!(d.code, minix_types::DIAGCTL_CODE_STACKTRACE);
    assert_eq!(
        d.endpt, 9,
        "target 必须落进 endpt 字段（C: sys_diagctl.c:17）"
    );
}

/// clock_time_via combines SYS_TIMES + GET_HZ —
/// sec = boot_time + real_ticks / hz (C clock_time.c:30-33).
#[test]
fn test_clock_time_via_composite() {
    let mut canned = CannedKernelCallTransport::new();
    // First call: the SYS_TIMES reply-arm fill.
    let mut times_reply = Message::default();
    times_reply.m_type = 0;
    // SAFETY: construct the reply arm.
    let arm = unsafe { &mut times_reply.m_u.m_krn_lsys_sys_times };
    arm.real_ticks = 2500;
    arm.boot_time = 1_000_000;
    canned.reply_message(times_reply);
    // Second call: the GET_HZ reply (4 bytes LE).
    let mut hz_reply = Message::default();
    hz_reply.m_type = 0;
    // SAFETY: the buffer the getinfo arm's val_ptr points at is written by
    // the real path; the Canned shape only needs an OK reply — the hz
    // assertion is covered by the sys_get_hz test, and hz=0 here triggers the
    // EINVAL branch, which pins the defense.
    canned.reply(0);
    // The hz read comes from the user buffer (all zero on the Canned path) → hz=0 → the EINVAL defense.
    let r = clock_time_via(&canned);
    assert_eq!(r, Err(-minix_types::EINVAL));
}

/// SYS_SCHEDCTL wire face for the KERNEL branch: flags set to
/// SCHEDCTL_FLAG_KERNEL; -1 for priority/quantum/cpu = keep the current value
/// (C sched_start.c:46-88; the kernel dispatch_schedctl Option conversion
/// starting at :651 uses -1 as the sentinel).
#[test]
fn test_sys_schedctl_wire() {
    let mut canned = CannedKernelCallTransport::new();
    canned.reply(0);
    sys_schedctl(&canned, 9, 12, 200, -1).unwrap();
    let sent = canned.sent.borrow();
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0].m_type, minix_types::SYS_SCHEDCTL);
    // SAFETY: read the schedctl arm back in the assertion.
    let sc = unsafe { sent[0].m_u.m_lsys_krn_schedctl };
    assert_eq!(sc.flags, minix_types::SCHEDCTL_FLAG_KERNEL);
    assert_eq!(sc.endpoint, 9);
    assert_eq!(sc.priority, 12);
    assert_eq!(sc.quantum, 200);
    assert_eq!(sc.cpu, -1);
}

/// The wrapper passes a negative errno through when the kernel rejects
/// (illegal flag combination).
#[test]
fn test_sys_schedctl_propagates_kernel_error() {
    let mut canned = CannedKernelCallTransport::new();
    canned.reply(-minix_types::EINVAL);
    let r = sys_schedctl(&canned, 9, 12, 200, -1);
    assert_eq!(r, Err(-minix_types::EINVAL));
}
