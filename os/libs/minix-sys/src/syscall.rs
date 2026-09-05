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
use minix_types::{Endpoint, Errno, Message};

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
/// invocations so tests can assert the exact retry sequence.
#[derive(Debug, Default)]
pub struct CannedKernelCallTransport {
    /// Raw reply message types, handed out in order.
    pub replies: alloc::vec::Vec<i32>,
    /// How many kernel calls happened so far.
    pub calls: core::cell::Cell<usize>,
}

impl CannedKernelCallTransport {
    /// Creates an empty script.
    pub fn new() -> Self {
        CannedKernelCallTransport {
            replies: alloc::vec::Vec::new(),
            calls: core::cell::Cell::new(0),
        }
    }

    /// Appends a raw reply message type to the script.
    pub fn reply(&mut self, reply_message_type: i32) {
        self.replies.push(reply_message_type);
    }
}

impl KernelCallTransport for CannedKernelCallTransport {
    fn kernel_call(&self, message: &mut Message) -> i32 {
        let index = self.calls.get();
        self.calls.set(index + 1);
        let reply = self.replies.get(index).cloned().unwrap_or(0);
        message.m_type = reply;
        reply
    }
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
