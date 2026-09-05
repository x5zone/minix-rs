//! Reincarnation server queries: name lookup and endpoint questions.
//!
//! Programs rarely hard-code where servers live: the reincarnation server
//! owns the registry mapping service names to endpoints, and three
//! question shapes cover the remaining "who is who" needs (C:
//! `minix3/minix/lib/libc/sys/minix_rs.c` plus `minix3/minix/lib/libsys/`
//! `getepinfo.c`, `getprocnr.c`, `getsysinfo.c`):
//!
//! 1. Name lookup: hand over a service name, get back its endpoint.
//! 2. Endpoint questions: hand over an endpoint, get back the process,
//!    user, and group identities behind it.
//! 3. System information routing: pick the right server and call number
//!    from the target endpoint, then ask for a numbered information item.
//!
//! Every wrapper comes in two forms: a transport-generic core that tests
//! drive with a scripted transport, and a thin direct-transport shim that
//! real binaries call. Notably, lookup performs no client-side caching —
//! like the C version, every call is a fresh round trip, so a restarted
//! service is never shadowed by a stale answer.
//!
//! # Execution model
//!
//! Pure wrappers over the transport trait: no shared state, no
//! synchronization questions.

use crate::ipc::IpcTransport;
use crate::syscall::{perform_syscall, perform_taskcall};
use minix_types::{Endpoint, Errno, Gid, Message, Pid, Uid};

/// Reincarnation server endpoint.
///
/// C: `RS_PROC_NR ((endpoint_t) 2)` (`minix3/minix/include/minix/com.h:61`).
pub const RS_ENDPOINT_NUMBER: i32 = 2;

/// Look up a service name. C: `RS_LOOKUP (RS_RQ_BASE + 8)` (`com.h:474`),
/// with the request base at `0x700` (`com.h:463`).
pub const RS_CALL_LOOKUP: i32 = 0x708;
/// Ask a server for a numbered information item.
/// C: `RS_GETSYSINFO (RS_RQ_BASE + 9)`.
pub const RS_CALL_GET_SYSTEM_INFO: i32 = 0x709;

/// Ask the process manager for endpoint identities.
/// C: `PM_GETEPINFO (PM_BASE + 45)` (`callnr.h:58`).
pub const PM_CALL_GET_ENDPOINT_INFO: i32 = 45;
/// Ask the process manager for the endpoint behind a process identifier.
/// C: `PM_GETPROCNR (PM_BASE + 46)` (`callnr.h:59`).
pub const PM_CALL_GET_PROCESS_NUMBER: i32 = 46;
/// Ask the process manager for a numbered information item.
/// C: `PM_GETSYSINFO (PM_BASE + 47)` (`callnr.h:60`).
pub const PM_CALL_GET_SYSTEM_INFO: i32 = 47;
/// Ask the file system for a numbered information item.
/// C: `VFS_GETSYSINFO (VFS_BASE + 48)` (`callnr.h:120`).
pub const VFS_CALL_GET_SYSTEM_INFO: i32 = 0x130;
/// Ask the data store for a numbered information item.
/// C: `DS_GETSYSINFO (DS_RQ_BASE + 7)`.
pub const DS_CALL_GET_SYSTEM_INFO: i32 = 0x907;

/// Data store endpoint (one routing target of the info query).
///
/// C: `DS_PROC_NR ((endpoint_t) 6)` (`com.h:65`).
pub const DS_ENDPOINT_NUMBER: i32 = 6;

/// Returns the reincarnation server endpoint.
pub const fn rs_endpoint() -> Endpoint {
    Endpoint(RS_ENDPOINT_NUMBER)
}

/// Creates a zeroed message, mirroring the `memset(&m, 0, sizeof(m))` that
/// opens every C wrapper in this group.
fn cleared_message() -> Message {
    Message::zeroed()
}

/// Copies a packed payload into the message body (same helper shape as the
/// earlier groups; layouts centralize in the global concepts document as
/// planned).
fn write_payload(message: &mut Message, packed: &[u8]) {
    debug_assert!(packed.len() <= minix_types::MESSAGE_PAYLOAD_SIZE);
    // SAFETY: the raw payload is 56 writable bytes; the caller guarantees
    // the packed slice fits (debug-checked above).
    unsafe {
        message.m_u.raw[..packed.len()].copy_from_slice(packed);
    }
}

/// Name-lookup request in C field order.
///
/// C: `mess_rs_req` (`minix3/minix/include/minix/ipc.h:1887-1896`) carries
/// the name length, the endpoint answer slot, a scratch address, the name
/// pointer, and a subtype. On 64-bit addresses the two pointers widen to
/// eight bytes each; the integer lanes keep their order and the total stays
/// 56 bytes.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
struct LookupPayload {
    length: i32,
    name_length: i32,
    endpoint: i32,
    _padding_before_address: [u8; 4],
    address: u64,
    name: u64,
    subtype: i32,
    _padding: [u8; 20],
}

/// Looks up a service name and returns its endpoint.
///
/// C: `minix_rs_lookup` (`minix3/minix/lib/libc/sys/minix_rs.c:23-40`):
/// measure the name including its terminator, clear a message, store the
/// name pointer and length, run the protocol, and read the endpoint answer
/// back from the reply. A failed call reports an error; there is no
/// client-side cache in either language, so restarts are always observed.
pub fn lookup_via(
    transport: &impl IpcTransport,
    name_address: u64,
    name_length_including_nul: usize,
) -> Result<Endpoint, Errno> {
    let mut message = cleared_message();
    let packed = LookupPayload {
        length: 0,
        name_length: name_length_including_nul as i32,
        endpoint: 0,
        _padding_before_address: [0; 4],
        address: 0,
        name: name_address,
        subtype: 0,
        _padding: [0; 20],
    };
    // SAFETY: plain 56-byte value; exact byte representation below.
    let bytes = unsafe {
        core::slice::from_raw_parts(
            (&raw const packed) as *const u8,
            core::mem::size_of::<LookupPayload>(),
        )
    };
    write_payload(&mut message, bytes);
    perform_syscall(transport, rs_endpoint(), RS_CALL_LOOKUP, &mut message)?;
    // The answer endpoint sits at the reply's endpoint lane (byte offset 8,
    // after the two length lanes).
    // SAFETY: the reply payload is 56 readable bytes.
    let endpoint = unsafe { message.m_u.raw[8..12].as_ptr().cast::<i32>().read() };
    Ok(Endpoint(endpoint))
}

/// Endpoint identities behind one endpoint.
///
/// The reply packs four identifiers plus the group count (C:
/// `mess_pm_lsys_getepinfo` in `ipc.h:1788-1797`: user, effective user,
/// group, effective group, group count). All four travel even when the
/// caller wants only one; the narrower helpers below select.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EndpointIdentities {
    /// User identifier.
    pub user: Uid,
    /// Effective user identifier.
    pub effective_user: Uid,
    /// Group identifier.
    pub group: Gid,
    /// Effective group identifier.
    pub effective_group: Gid,
}

/// Asks who stands behind an endpoint.
///
/// C: `getepinfo` (`minix3/minix/lib/libsys/getepinfo.c`): clear a message,
/// store the endpoint with empty group fields, run the server-side protocol,
/// and read the four identifiers back. The narrower C helpers (`getnpid`,
/// `getnuid`, `getngid`) select one field each; here the full tuple returns
/// and the caller ignores what it does not need.
pub fn endpoint_identities_via(
    transport: &impl IpcTransport,
    endpoint: Endpoint,
) -> Result<(Pid, EndpointIdentities), Errno> {
    let mut message = cleared_message();
    // SAFETY: endpoint at bytes 0..4, group fields zeroed (empty request).
    unsafe {
        message.m_u.raw[..4].copy_from_slice(&endpoint.0.to_ne_bytes());
    }
    let reply = perform_taskcall(
        transport,
        crate::pm::pm_endpoint(),
        PM_CALL_GET_ENDPOINT_INFO,
        &mut message,
    );
    if reply < 0 {
        return Err(Errno::from_i32(-reply));
    }
    // SAFETY: the reply packs user, effective user, group, effective group
    // at bytes 0, 4, 8, 12 by the C layout cited above.
    let raw = unsafe { &message.m_u.raw };
    let user = u32::from_ne_bytes(raw[0..4].try_into().unwrap());
    let effective_user = u32::from_ne_bytes(raw[4..8].try_into().unwrap());
    let group = u32::from_ne_bytes(raw[8..12].try_into().unwrap());
    let effective_group = u32::from_ne_bytes(raw[12..16].try_into().unwrap());
    Ok((
        reply,
        EndpointIdentities { user, effective_user, group, effective_group },
    ))
}

/// Asks for the endpoint behind a process identifier.
///
/// C: `getprocnr` (`minix3/minix/lib/libsys/getprocnr.c`): clear a message,
/// store the identifier, run the server-side protocol, and read the endpoint
/// back from the reply's first four bytes (C: `mess_pm_lsys_getprocnr`
/// holds one endpoint).
pub fn process_number_via(
    transport: &impl IpcTransport,
    pid: Pid,
) -> Result<Endpoint, Errno> {
    let mut message = cleared_message();
    // SAFETY: identifier at bytes 0..4.
    unsafe {
        message.m_u.raw[..4].copy_from_slice(&pid.to_ne_bytes());
    }
    let reply = perform_taskcall(
        transport,
        crate::pm::pm_endpoint(),
        PM_CALL_GET_PROCESS_NUMBER,
        &mut message,
    );
    if reply < 0 {
        return Err(Errno::from_i32(-reply));
    }
    // SAFETY: the reply carries the endpoint at byte zero.
    let endpoint = unsafe { message.m_u.raw[..4].as_ptr().cast::<i32>().read() };
    Ok(Endpoint(endpoint))
}

/// Routes a system-information query to the owning server.
///
/// C: `getsysinfo` (`minix3/minix/lib/libsys/getsysinfo.c:8-32`): switch on
/// the target endpoint — process manager, file system, reincarnation
/// server, data store — picking each server's own information call number;
/// any other endpoint is not a valid target. The what/where/size triple
/// travels unchanged; the server writes the answer through the address.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SystemInfoTarget {
    /// Owning server endpoint.
    pub server: Endpoint,
    /// That server's information call number.
    pub call_number: i32,
}

/// Selects the query route, or reports that no server owns the target.
///
/// Unknown endpoints map to "not implemented" (C: `return ENOSYS` in the
/// default branch at `getsysinfo.c:23-24`).
pub const fn route_system_info(target: Endpoint) -> Result<SystemInfoTarget, Errno> {
    match target.0 {
        0 => Ok(SystemInfoTarget { server: target, call_number: PM_CALL_GET_SYSTEM_INFO }),
        1 => Ok(SystemInfoTarget { server: target, call_number: VFS_CALL_GET_SYSTEM_INFO }),
        2 => Ok(SystemInfoTarget { server: target, call_number: RS_CALL_GET_SYSTEM_INFO }),
        6 => Ok(SystemInfoTarget { server: target, call_number: DS_CALL_GET_SYSTEM_INFO }),
        _ => Err(Errno::ENOSYS),
    }
}

/// Asks a server for a numbered information item.
///
/// Fills the what/where/size triple and runs the server-side protocol
/// against the routed server. The triple mirrors the C field order
/// (what, where, size).
pub fn system_info_via(
    transport: &impl IpcTransport,
    target: Endpoint,
    what: i32,
    where_address: u64,
    size: usize,
) -> Result<(), Errno> {
    let route = route_system_info(target)?;
    let mut message = cleared_message();
    // SAFETY: three plain lanes at bytes 0..16 in C field order.
    unsafe {
        message.m_u.raw[..4].copy_from_slice(&what.to_ne_bytes());
        message.m_u.raw[8..16].copy_from_slice(&where_address.to_ne_bytes());
        message.m_u.raw[16..24].copy_from_slice(&(size as u64).to_ne_bytes());
    }
    let reply = perform_taskcall(transport, route.server, route.call_number, &mut message);
    if reply < 0 {
        Err(Errno::from_i32(-reply))
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reply_with_type(message_type: i32) -> Message {
        let mut message = Message::zeroed();
        message.m_type = message_type;
        message
    }

    #[test]
    fn test_call_numbers_match_com_header() {
        assert_eq!(RS_ENDPOINT_NUMBER, 2);
        assert_eq!(RS_CALL_LOOKUP, 0x708);
        assert_eq!(RS_CALL_GET_SYSTEM_INFO, 0x709);
        assert_eq!(PM_CALL_GET_ENDPOINT_INFO, 45);
        assert_eq!(PM_CALL_GET_PROCESS_NUMBER, 46);
        assert_eq!(PM_CALL_GET_SYSTEM_INFO, 47);
        assert_eq!(VFS_CALL_GET_SYSTEM_INFO, 0x130);
        assert_eq!(DS_CALL_GET_SYSTEM_INFO, 0x907);
        assert_eq!(DS_ENDPOINT_NUMBER, 6);
    }

    #[test]
    fn test_lookup_payload_matches_c_field_order() {
        assert_eq!(core::mem::size_of::<LookupPayload>(), 56);
        let packed = LookupPayload {
            length: 0,
            name_length: 5,
            endpoint: 0,
            _padding_before_address: [0; 4],
            address: 0,
            name: 0x7000,
            subtype: 0,
            _padding: [0; 20],
        };
        // SAFETY: plain value read-back of the struct just built above.
        // C order: length @0, name length @4, endpoint @8, address @16,
        // name @24, subtype @32.
        let bytes = unsafe {
            core::slice::from_raw_parts(
                (&raw const packed) as *const u8,
                core::mem::size_of::<LookupPayload>(),
            )
        };
        assert_eq!(i32::from_ne_bytes(bytes[0..4].try_into().unwrap()), 0);
        assert_eq!(i32::from_ne_bytes(bytes[4..8].try_into().unwrap()), 5);
        assert_eq!(u64::from_ne_bytes(bytes[24..32].try_into().unwrap()), 0x7000);
        assert_eq!(i32::from_ne_bytes(bytes[32..36].try_into().unwrap()), 0);
    }

    #[test]
    fn test_lookup_returns_answer_endpoint() {
        let mut transport = crate::ipc::CannedTransport::new();
        let mut reply = reply_with_type(0);
        // SAFETY: test-only payload setup through the documented overlay.
        unsafe {
            reply.m_u.raw[8..12].copy_from_slice(&3i32.to_ne_bytes());
        }
        transport.reply_sendrec(Ok(reply));
        assert_eq!(lookup_via(&transport, 0x7000, 5), Ok(Endpoint(3)));
    }

    #[test]
    fn test_lookup_error_propagates() {
        let mut transport = crate::ipc::CannedTransport::new();
        transport.reply_sendrec(Ok(reply_with_type(-2)));
        assert_eq!(
            lookup_via(&transport, 0x7000, 5),
            Err(Errno::from_i32(2))
        );
    }

    #[test]
    fn test_endpoint_identities_read_four_fields() {
        let mut transport = crate::ipc::CannedTransport::new();
        let mut reply = reply_with_type(11);
        // SAFETY: test-only payload setup through the documented overlay.
        unsafe {
            reply.m_u.raw[0..4].copy_from_slice(&100u32.to_ne_bytes());
            reply.m_u.raw[4..8].copy_from_slice(&101u32.to_ne_bytes());
            reply.m_u.raw[8..12].copy_from_slice(&200u32.to_ne_bytes());
            reply.m_u.raw[12..16].copy_from_slice(&201u32.to_ne_bytes());
        }
        transport.reply_sendrec(Ok(reply));
        let (pid, identities) = endpoint_identities_via(&transport, Endpoint(4)).unwrap();
        assert_eq!(pid, 11);
        assert_eq!(
            identities,
            EndpointIdentities {
                user: 100,
                effective_user: 101,
                group: 200,
                effective_group: 201,
            }
        );
    }

    #[test]
    fn test_process_number_returns_endpoint() {
        let mut transport = crate::ipc::CannedTransport::new();
        let mut reply = reply_with_type(0);
        // SAFETY: test-only payload setup through the documented overlay.
        unsafe {
            reply.m_u.raw[..4].copy_from_slice(&4i32.to_ne_bytes());
        }
        transport.reply_sendrec(Ok(reply));
        assert_eq!(process_number_via(&transport, 42), Ok(Endpoint(4)));
    }

    #[test]
    fn test_route_covers_four_servers_and_rejects_rest() {
        assert_eq!(
            route_system_info(Endpoint(0)),
            Ok(SystemInfoTarget { server: Endpoint(0), call_number: PM_CALL_GET_SYSTEM_INFO })
        );
        assert_eq!(
            route_system_info(Endpoint(1)),
            Ok(SystemInfoTarget { server: Endpoint(1), call_number: VFS_CALL_GET_SYSTEM_INFO })
        );
        assert_eq!(
            route_system_info(Endpoint(2)),
            Ok(SystemInfoTarget { server: Endpoint(2), call_number: RS_CALL_GET_SYSTEM_INFO })
        );
        assert_eq!(
            route_system_info(Endpoint(6)),
            Ok(SystemInfoTarget { server: Endpoint(6), call_number: DS_CALL_GET_SYSTEM_INFO })
        );
        assert_eq!(route_system_info(Endpoint(9)), Err(Errno::ENOSYS));
    }

    #[test]
    fn test_system_info_runs_routed_call() {
        let mut transport = crate::ipc::CannedTransport::new();
        transport.reply_sendrec(Ok(reply_with_type(0)));
        assert_eq!(system_info_via(&transport, Endpoint(2), 3, 0x8000, 64), Ok(()));
        assert_eq!(transport.sendrec_calls.get(), 1);
    }

    #[test]
    fn test_system_info_rejects_before_transport() {
        let mut transport = crate::ipc::CannedTransport::new();
        assert_eq!(
            system_info_via(&transport, Endpoint(9), 3, 0x8000, 64),
            Err(Errno::ENOSYS)
        );
        assert_eq!(transport.sendrec_calls.get(), 0);
    }
}
