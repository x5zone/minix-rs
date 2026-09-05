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
/// Stop a system service (server-side call).
///
/// C: `PM_SRV_KILL (PM_BASE + 42)` (`callnr.h:55`).
pub const PM_CALL_SERVICE_KILL: i32 = 42;

/// Highest signal number (exclusive upper bound for validation).
///
/// C: `_NSIG 64` (`minix3/sys/sys/signal.h:45`).
pub const MAX_SIGNAL_NUMBER: i32 = 64;

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

/// Creates a zeroed message, mirroring the `memset(&m, 0, sizeof(m))` that
/// opens every C wrapper in this group.
fn cleared_message() -> Message {
    Message::zeroed()
}

/// Copies a packed payload into the message body.
///
/// Payload layouts belong to the global concepts document; until it lands,
/// each call group packs its own fields through this helper. The copy keeps
/// the exact C field order and sizes, so the bytes on the wire match the C
/// library bit for bit.
fn write_payload(message: &mut Message, packed: &[u8]) {
    debug_assert!(packed.len() <= minix_types::MESSAGE_PAYLOAD_SIZE);
    // SAFETY: the raw payload is 56 writable bytes; the caller guarantees
    // the packed slice fits (debug-checked above).
    unsafe {
        message.m_u.raw[..packed.len()].copy_from_slice(packed);
    }
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
    let mut message = cleared_message();
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
    let mut message = cleared_message();
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
    let mut message = cleared_message();
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
    let mut message = cleared_message();
    perform_syscall(transport, pm_endpoint(), PM_CALL_GETPID, &mut message)
}

/// Sends a signal to a process.
///
/// C: `kill` (`minix3/minix/lib/libc/sys/kill.c:12-22`): clear a message,
/// store the target identifier and signal number, and run the protocol.
pub fn kill_via(transport: &impl IpcTransport, target: Pid, signal: i32) -> Result<(), Errno> {
    let mut message = cleared_message();
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
pub fn raise_via(transport: &impl IpcTransport, signal: i32) -> Result<(), Errno> {
    if !(0..MAX_SIGNAL_NUMBER).contains(&signal) {
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
    let mut message = cleared_message();
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
    write_payload(&mut message, bytes);
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
    let mut message = cleared_message();
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
    write_payload(&mut message, bytes);
    let reply = perform_taskcall(transport, pm_endpoint(), PM_CALL_SERVICE_FORK, &mut message);
    if reply < 0 {
        Err(Errno::from_i32(-reply))
    } else {
        Ok(reply)
    }
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
    let mut message = cleared_message();
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
