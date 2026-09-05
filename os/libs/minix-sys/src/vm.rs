//! Virtual memory call group: mapping, break management, and client queries.
//!
//! The memory server owns every address space: it maps files and anonymous
//! memory, moves the heap boundary, forks address spaces, answers physical
//! address questions, and remaps pages between processes. User programs and
//! servers reach it through thin wrappers that share the shape established
//! by the earlier groups (see [`crate::pm`]): clear a message, fill the
//! call's fields, and run a protocol against the memory endpoint. The C
//! sources are the user-side files `minix3/minix/lib/libc/sys/mmap.c`,
//! `brk.c`, `sbrk.c` and the client helpers `minix3/minix/lib/libsys/vm_*`
//! (the user-space subset; privilege and live-update helpers belong to the
//! integration stage, as the stage plan records).
//!
//! Three request shapes recur across the group:
//!
//! 1. The mapping request: address, length, protection, flags, file,
//!    offset, and the beneficiary, with the third-party flag added whenever
//!    the beneficiary is not the caller.
//! 2. The break request: one address, sent only when it differs from the
//!    cached boundary.
//! 3. The query shapes: endpoint plus address in, scalar answer out, with a
//!    reserved failure sentinel (zero address, all-ones reference count).
//!
//! Every wrapper comes in two forms: a transport-generic core that tests
//! drive with a scripted transport, and a thin direct-transport shim that
//! real binaries call.
//!
//! # Execution model
//!
//! Pure wrappers over the transport trait: no shared state, no
//! synchronization questions.

use crate::ipc::IpcTransport;
use crate::syscall::{perform_syscall, perform_taskcall};
use minix_types::{Endpoint, Errno, Message, PhysBytes, VirBytes};

/// Memory server endpoint.
///
/// C: `VM_PROC_NR ((endpoint_t) 8)` (`minix3/minix/include/minix/com.h:67`).
pub const VM_ENDPOINT_NUMBER: i32 = 8;

/// Terminate address-space bookkeeping. C: `VM_EXIT (VM_RQ_BASE+0)`.
pub const VM_CALL_EXIT: i32 = 0xC00;
/// Fork an address space. C: `VM_FORK (VM_RQ_BASE+1)`.
pub const VM_CALL_FORK: i32 = 0xC01;
/// Move the heap boundary. C: `VM_BRK (VM_RQ_BASE+2)`.
pub const VM_CALL_BREAK: i32 = 0xC02;
/// Announce an upcoming exit. C: `VM_WILLEXIT (VM_RQ_BASE+5)`.
pub const VM_CALL_WILL_EXIT: i32 = 0xC05;
/// Map memory. C: `VM_MMAP (VM_RQ_BASE+10)`.
pub const VM_CALL_MMAP: i32 = 0xC0A;
/// Map physical memory. C: `VM_MAP_PHYS (VM_RQ_BASE+15)`.
pub const VM_CALL_MAP_PHYS: i32 = 0xC0F;
/// Unmap physical memory. C: `VM_UNMAP_PHYS (VM_RQ_BASE+16)`.
pub const VM_CALL_UNMAP_PHYS: i32 = 0xC10;
/// Unmap memory. C: `VM_MUNMAP (VM_RQ_BASE+17)`.
pub const VM_CALL_MUNMAP: i32 = 0xC11;
/// Remap pages between processes. C: `VM_REMAP (VM_RQ_BASE+33)`.
pub const VM_CALL_REMAP: i32 = 0xC21;
/// Unmap shared memory. C: `VM_SHM_UNMAP (VM_RQ_BASE+34)`.
pub const VM_CALL_SHARED_UNMAP: i32 = 0xC22;
/// Translate to a physical address. C: `VM_GETPHYS (VM_RQ_BASE+35)`.
pub const VM_CALL_GET_PHYSICAL: i32 = 0xC23;
/// Read a page reference count. C: `VM_GETREF (VM_RQ_BASE+36)`.
pub const VM_CALL_GET_REFERENCE: i32 = 0xC24;
/// Query memory information. C: `VM_INFO (VM_RQ_BASE+40)`.
pub const VM_CALL_INFO: i32 = 0xC28;
/// Read-only remap. C: `VM_REMAP_RO (VM_RQ_BASE+44)`.
pub const VM_CALL_REMAP_READ_ONLY: i32 = 0xC2C;
/// Process control. C: `VM_PROCCTL (VM_RQ_BASE+45)`.
pub const VM_CALL_PROCESS_CONTROL: i32 = 0xC2D;

/// No access. C: `PROT_NONE 0x00` (`minix3/sys/sys/mman.h:62`).
pub const MAP_PROTECTION_NONE: u32 = 0x00;
/// Readable pages. C: `PROT_READ 0x01` (`mman.h:63`).
pub const MAP_PROTECTION_READ: u32 = 0x01;
/// Writable pages. C: `PROT_WRITE 0x02` (`mman.h:64`).
pub const MAP_PROTECTION_WRITE: u32 = 0x02;
/// Executable pages. C: `PROT_EXEC 0x04` (`mman.h:65`).
pub const MAP_PROTECTION_EXECUTE: u32 = 0x04;

/// Perform the mapping on behalf of another process.
///
/// C: `MAP_THIRDPARTY 0x800000` (`minix3/sys/sys/mman.h:124`): added to the
/// flags whenever the beneficiary differs from the caller (see
/// `mmap.c:36-38`).
pub const MAP_FLAG_THIRD_PARTY: u32 = 0x800000;

/// Returns the memory server endpoint.
pub const fn vm_endpoint() -> Endpoint {
    Endpoint(VM_ENDPOINT_NUMBER)
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

/// Mapping request in C field order.
///
/// C: `mess_mmap` (`minix3/minix/include/minix/ipc.h:1582-1593`) — offset,
/// address, length, protection, flags, file, beneficiary, return address —
/// plus padding. Note the leading position of the offset: like the seek
/// payload, the address is not first.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
struct MapPayload {
    offset: i64,
    address: u64,
    length: u64,
    protection: u32,
    flags: u32,
    file: i32,
    beneficiary: i32,
    return_address: u64,
    _padding: [u8; 8],
}

/// A validated mapping request: what to map, where, and for whom.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MapRequest {
    /// Beneficiary endpoint (the caller itself for ordinary mappings).
    pub beneficiary: Endpoint,
    /// Hint address (the server may choose another).
    pub address: VirBytes,
    /// Length in bytes.
    pub length: VirBytes,
    /// Protection bits (see `MAP_PROTECTION_*`).
    pub protection: u32,
    /// Flag bits (see `MAP_FLAG_*`; third-party added automatically).
    pub flags: u32,
    /// Backing file descriptor, if any.
    pub file: i32,
    /// File offset for file mappings.
    pub offset: i64,
}

impl MapRequest {
    /// Fills the third-party flag exactly like the C version: whenever the
    /// beneficiary is not the caller's own endpoint (see `mmap.c:36-38`).
    pub const fn effective_flags(self, caller: Endpoint) -> u32 {
        if self.beneficiary.0 != caller.0 {
            self.flags | MAP_FLAG_THIRD_PARTY
        } else {
            self.flags
        }
    }
}

/// Maps memory through the memory server.
///
/// C: `minix_mmap_for` (`minix3/minix/lib/libc/sys/mmap.c:21-47`): clear a
/// message, fill the seven mapping fields, add the third-party flag when
/// mapping for someone else, and run the protocol. A failed call reports the
/// mapped-failed sentinel in C; this version reports `None` instead of a
/// sentinel pointer, so callers cannot mistake failure for address minus one.
/// The reply carries the chosen address.
pub fn mmap_via(
    transport: &impl IpcTransport,
    caller: Endpoint,
    request: MapRequest,
) -> Result<Option<VirBytes>, Errno> {
    let mut message = cleared_message();
    let packed = MapPayload {
        offset: request.offset,
        address: request.address.0,
        length: request.length.0,
        protection: request.protection,
        flags: request.effective_flags(caller),
        file: request.file,
        beneficiary: request.beneficiary.0,
        return_address: 0,
        _padding: [0; 8],
    };
    // SAFETY: plain 56-byte value; exact byte representation below.
    let bytes = unsafe {
        core::slice::from_raw_parts(
            (&raw const packed) as *const u8,
            core::mem::size_of::<MapPayload>(),
        )
    };
    write_payload(&mut message, bytes);
    perform_syscall(transport, vm_endpoint(), VM_CALL_MMAP, &mut message)?;
    // SAFETY: the reply payload is 56 readable bytes; the chosen address
    // sits at the return-address lane (last eight bytes before padding).
    let chosen = unsafe { message.m_u.raw[40..48].as_ptr().cast::<u64>().read() };
    Ok(Some(VirBytes(chosen)))
}

/// Unmaps a memory range.
///
/// C: `munmap` (`mmap.c:76-85`): clear a message, store address and length
/// through the mapping payload's address/length lanes, and run the protocol.
pub fn munmap_via(
    transport: &impl IpcTransport,
    address: VirBytes,
    length: VirBytes,
) -> Result<(), Errno> {
    let mut message = cleared_message();
    let packed = MapPayload {
        offset: 0,
        address: address.0,
        length: length.0,
        protection: 0,
        flags: 0,
        file: 0,
        beneficiary: 0,
        return_address: 0,
        _padding: [0; 8],
    };
    // SAFETY: same plain-value reasoning as mmap_via.
    let bytes = unsafe {
        core::slice::from_raw_parts(
            (&raw const packed) as *const u8,
            core::mem::size_of::<MapPayload>(),
        )
    };
    write_payload(&mut message, bytes);
    perform_syscall(transport, vm_endpoint(), VM_CALL_MUNMAP, &mut message).map(|_| ())
}

/// Moves the heap boundary, skipping the round trip when nothing changed.
///
/// C: `brk` (`minix3/minix/lib/libc/sys/brk.c:22-34`): when the requested
/// address equals the cached boundary, return success without calling the
/// server; otherwise send the break request and adopt the new boundary only
/// on success. The returned value is the boundary the caller should cache.
pub fn break_via(
    transport: &impl IpcTransport,
    cached_break: VirBytes,
    requested_break: VirBytes,
) -> Result<VirBytes, Errno> {
    if requested_break == cached_break {
        return Ok(cached_break);
    }
    let mut message = cleared_message();
    // The break payload is one address at byte zero (see the shared break
    // payload type); the remaining bytes stay zeroed.
    // SAFETY: writing eight bytes into the 56-byte raw payload.
    unsafe {
        message.m_u.raw[..8].copy_from_slice(&requested_break.0.to_ne_bytes());
    }
    perform_syscall(transport, vm_endpoint(), VM_CALL_BREAK, &mut message)?;
    Ok(requested_break)
}

/// Forks an address space (server-side call).
///
/// C: `vm_fork` (`minix3/minix/lib/libsys/vm_fork.c:10-24`): store the
/// endpoint and slot, run the server-side protocol, and read the child
/// endpoint back from the reply. The child endpoint sits in the first four
/// reply bytes.
pub fn fork_address_space_via(
    transport: &impl IpcTransport,
    endpoint: Endpoint,
    slot: i32,
) -> Result<Endpoint, Errno> {
    let mut message = cleared_message();
    // SAFETY: two plain integers at bytes zero and four; exact bytes below.
    unsafe {
        message.m_u.raw[..4].copy_from_slice(&endpoint.0.to_ne_bytes());
        message.m_u.raw[4..8].copy_from_slice(&slot.to_ne_bytes());
    }
    let reply = perform_taskcall(transport, vm_endpoint(), VM_CALL_FORK, &mut message);
    if reply < 0 {
        return Err(Errno::from_i32(-reply));
    }
    // SAFETY: the reply payload is 56 readable bytes; the child endpoint
    // sits at byte zero.
    let child = unsafe { message.m_u.raw[..4].as_ptr().cast::<i32>().read() };
    Ok(Endpoint(child))
}

/// Ends address-space bookkeeping (server-side call).
///
/// C: `vm_exit` (sends the endpoint, returns the raw reply).
pub fn exit_address_space_via(transport: &impl IpcTransport, endpoint: Endpoint) -> i32 {
    let mut message = cleared_message();
    // SAFETY: one plain integer at byte zero.
    unsafe {
        message.m_u.raw[..4].copy_from_slice(&endpoint.0.to_ne_bytes());
    }
    perform_taskcall(transport, vm_endpoint(), VM_CALL_EXIT, &mut message)
}

/// Remaps pages from one process to another.
///
/// C: `vm_remap` (`mmap.c:88-108`): destination, source, both addresses, and
/// size travel in fixed lanes; the reply carries the destination address. A
/// failed call reports the mapped-failed sentinel in C and `None` here, like
/// [`mmap_via`].
pub fn remap_via(
    transport: &impl IpcTransport,
    call: i32,
    destination: Endpoint,
    source: Endpoint,
    destination_address: VirBytes,
    source_address: VirBytes,
    size: VirBytes,
) -> Result<Option<VirBytes>, Errno> {
    let mut message = cleared_message();
    // SAFETY: five plain 64-bit lanes at bytes 0..40; exact bytes below.
    unsafe {
        message.m_u.raw[0..8].copy_from_slice(&(destination.0 as u64).to_ne_bytes());
        message.m_u.raw[8..16].copy_from_slice(&(source.0 as u64).to_ne_bytes());
        message.m_u.raw[16..24].copy_from_slice(&destination_address.0.to_ne_bytes());
        message.m_u.raw[24..32].copy_from_slice(&source_address.0.to_ne_bytes());
        message.m_u.raw[32..40].copy_from_slice(&size.0.to_ne_bytes());
    }
    perform_syscall(transport, vm_endpoint(), call, &mut message)?;
    // SAFETY: the reply carries the destination address at byte zero.
    let placed = unsafe { message.m_u.raw[..8].as_ptr().cast::<u64>().read() };
    Ok(Some(VirBytes(placed)))
}

/// Translates a virtual address to a physical address.
///
/// C: `vm_getphys` (`mmap.c:143-156`): endpoint plus address in, physical
/// address out; failure reports zero, which is also a valid physical
/// address, so this version reports `None` on failure instead of overloading
/// zero.
pub fn physical_address_via(
    transport: &impl IpcTransport,
    endpoint: Endpoint,
    address: VirBytes,
) -> Result<Option<PhysBytes>, Errno> {
    let mut message = cleared_message();
    // SAFETY: endpoint at bytes 0..4, address at bytes 8..16.
    unsafe {
        message.m_u.raw[..4].copy_from_slice(&endpoint.0.to_ne_bytes());
        message.m_u.raw[8..16].copy_from_slice(&address.0.to_ne_bytes());
    }
    perform_syscall(transport, vm_endpoint(), VM_CALL_GET_PHYSICAL, &mut message)?;
    // SAFETY: the reply carries the physical address at byte zero.
    let physical = unsafe { message.m_u.raw[..8].as_ptr().cast::<u64>().read() };
    Ok(Some(PhysBytes(physical)))
}

/// Reads a page reference count.
///
/// C: `vm_getrefcount` (`mmap.c:158-171`): failure reports all-ones, which is
/// also a conceivable count, so this version reports `None` on failure.
pub fn reference_count_via(
    transport: &impl IpcTransport,
    endpoint: Endpoint,
    address: VirBytes,
) -> Result<Option<u8>, Errno> {
    let mut message = cleared_message();
    // SAFETY: same two-field layout as the physical query above.
    unsafe {
        message.m_u.raw[..4].copy_from_slice(&endpoint.0.to_ne_bytes());
        message.m_u.raw[8..16].copy_from_slice(&address.0.to_ne_bytes());
    }
    perform_syscall(transport, vm_endpoint(), VM_CALL_GET_REFERENCE, &mut message)?;
    // SAFETY: the reply carries the count in its first byte.
    let count = unsafe { message.m_u.raw[0] };
    Ok(Some(count))
}

/// Maps physical memory for a target process (server-side call).
///
/// C: `vm_map_phys`: target, physical address, and length travel to the
/// server; the reply carries the virtual address. The C version additionally
/// registers the region in its local bookkeeping; that registry belongs to
/// the server-integration stage, not to this client wrapper.
pub fn map_physical_via(
    transport: &impl IpcTransport,
    target: Endpoint,
    physical: PhysBytes,
    length: VirBytes,
) -> Result<VirBytes, Errno> {
    let mut message = cleared_message();
    // SAFETY: three plain lanes at bytes 0..24; exact bytes below.
    unsafe {
        message.m_u.raw[..4].copy_from_slice(&target.0.to_ne_bytes());
        message.m_u.raw[8..16].copy_from_slice(&physical.0.to_ne_bytes());
        message.m_u.raw[16..24].copy_from_slice(&length.0.to_ne_bytes());
    }
    let reply = perform_taskcall(transport, vm_endpoint(), VM_CALL_MAP_PHYS, &mut message);
    if reply < 0 {
        return Err(Errno::from_i32(-reply));
    }
    // SAFETY: the reply carries the virtual address at byte zero.
    let placed = unsafe { message.m_u.raw[..8].as_ptr().cast::<u64>().read() };
    Ok(VirBytes(placed))
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

    fn request_for(beneficiary: i32) -> MapRequest {
        MapRequest {
            beneficiary: Endpoint(beneficiary),
            address: VirBytes(0),
            length: VirBytes(4096),
            protection: MAP_PROTECTION_READ | MAP_PROTECTION_WRITE,
            flags: 0,
            file: -1,
            offset: 0,
        }
    }

    #[test]
    fn test_call_numbers_match_com_header() {
        assert_eq!(VM_ENDPOINT_NUMBER, 8);
        assert_eq!(VM_CALL_EXIT, 0xC00);
        assert_eq!(VM_CALL_FORK, 0xC01);
        assert_eq!(VM_CALL_BREAK, 0xC02);
        assert_eq!(VM_CALL_WILL_EXIT, 0xC05);
        assert_eq!(VM_CALL_MMAP, 0xC0A);
        assert_eq!(VM_CALL_MAP_PHYS, 0xC0F);
        assert_eq!(VM_CALL_UNMAP_PHYS, 0xC10);
        assert_eq!(VM_CALL_MUNMAP, 0xC11);
        assert_eq!(VM_CALL_REMAP, 0xC21);
        assert_eq!(VM_CALL_SHARED_UNMAP, 0xC22);
        assert_eq!(VM_CALL_GET_PHYSICAL, 0xC23);
        assert_eq!(VM_CALL_GET_REFERENCE, 0xC24);
        assert_eq!(VM_CALL_INFO, 0xC28);
        assert_eq!(VM_CALL_REMAP_READ_ONLY, 0xC2C);
        assert_eq!(VM_CALL_PROCESS_CONTROL, 0xC2D);
    }

    #[test]
    fn test_protection_and_flag_constants_match_mman_header() {
        assert_eq!(MAP_PROTECTION_NONE, 0x00);
        assert_eq!(MAP_PROTECTION_READ, 0x01);
        assert_eq!(MAP_PROTECTION_WRITE, 0x02);
        assert_eq!(MAP_PROTECTION_EXECUTE, 0x04);
        assert_eq!(MAP_FLAG_THIRD_PARTY, 0x800000);
    }

    #[test]
    fn test_third_party_flag_added_only_for_others() {
        let caller = Endpoint(5);
        let for_self = MapRequest { beneficiary: caller, ..request_for(5) };
        assert_eq!(for_self.effective_flags(caller), 0);
        let for_other = MapRequest { beneficiary: Endpoint(6), ..request_for(6) };
        assert_eq!(for_other.effective_flags(caller), MAP_FLAG_THIRD_PARTY);
    }

    #[test]
    fn test_map_payload_matches_c_field_order() {
        assert_eq!(core::mem::size_of::<MapPayload>(), 56);
        let packed = MapPayload {
            offset: 0,
            address: 0x1000,
            length: 4096,
            protection: 3,
            flags: 0,
            file: -1,
            beneficiary: 5,
            return_address: 0,
            _padding: [0; 8],
        };
        // SAFETY: plain value read-back of the struct just built above.
        // C order: offset @0, address @8, length @16, protection @24,
        // flags @28, file @32, beneficiary @36, return address @40.
        let bytes = unsafe {
            core::slice::from_raw_parts(
                (&raw const packed) as *const u8,
                core::mem::size_of::<MapPayload>(),
            )
        };
        assert_eq!(i64::from_ne_bytes(bytes[0..8].try_into().unwrap()), 0);
        assert_eq!(u64::from_ne_bytes(bytes[8..16].try_into().unwrap()), 0x1000);
        assert_eq!(u64::from_ne_bytes(bytes[16..24].try_into().unwrap()), 4096);
        assert_eq!(u32::from_ne_bytes(bytes[24..28].try_into().unwrap()), 3);
        assert_eq!(i32::from_ne_bytes(bytes[32..36].try_into().unwrap()), -1);
        assert_eq!(i32::from_ne_bytes(bytes[36..40].try_into().unwrap()), 5);
    }

    #[test]
    fn test_mmap_returns_chosen_address() {
        let mut transport = CannedTransport::new();
        let mut reply = reply_with_type(0);
        // SAFETY: test-only payload setup through the documented overlay.
        unsafe {
            reply.m_u.raw[40..48].copy_from_slice(&0x8000u64.to_ne_bytes());
        }
        transport.reply_sendrec(Ok(reply));
        let result = mmap_via(&transport, Endpoint(5), request_for(5)).unwrap();
        assert_eq!(result, Some(VirBytes(0x8000)));
    }

    #[test]
    fn test_munmap_sends_address_and_length() {
        let mut transport = CannedTransport::new();
        transport.reply_sendrec(Ok(reply_with_type(0)));
        assert_eq!(
            munmap_via(&transport, VirBytes(0x8000), VirBytes(4096)),
            Ok(())
        );
        assert_eq!(transport.sendrec_calls.get(), 1);
    }

    #[test]
    fn test_unchanged_break_skips_round_trip() {
        let transport = CannedTransport::new();
        assert_eq!(
            break_via(&transport, VirBytes(0x1000), VirBytes(0x1000)),
            Ok(VirBytes(0x1000))
        );
        assert_eq!(transport.sendrec_calls.get(), 0);
    }

    #[test]
    fn test_changed_break_calls_server_and_caches() {
        let mut transport = CannedTransport::new();
        transport.reply_sendrec(Ok(reply_with_type(0)));
        assert_eq!(
            break_via(&transport, VirBytes(0x1000), VirBytes(0x2000)),
            Ok(VirBytes(0x2000))
        );
        assert_eq!(transport.sendrec_calls.get(), 1);
    }

    #[test]
    fn test_fork_address_space_returns_child() {
        let mut transport = CannedTransport::new();
        let mut reply = reply_with_type(0);
        // SAFETY: test-only payload setup through the documented overlay.
        unsafe {
            reply.m_u.raw[..4].copy_from_slice(&9i32.to_ne_bytes());
        }
        transport.reply_sendrec(Ok(reply));
        assert_eq!(
            fork_address_space_via(&transport, Endpoint(4), 3),
            Ok(Endpoint(9))
        );
    }

    #[test]
    fn test_remap_returns_placed_address() {
        let mut transport = CannedTransport::new();
        let mut reply = reply_with_type(0);
        // SAFETY: test-only payload setup through the documented overlay.
        unsafe {
            reply.m_u.raw[..8].copy_from_slice(&0x9000u64.to_ne_bytes());
        }
        transport.reply_sendrec(Ok(reply));
        let result = remap_via(
            &transport,
            VM_CALL_REMAP,
            Endpoint(6),
            Endpoint(5),
            VirBytes(0x9000),
            VirBytes(0x8000),
            VirBytes(4096),
        )
        .unwrap();
        assert_eq!(result, Some(VirBytes(0x9000)));
    }

    #[test]
    fn test_physical_query_returns_address() {
        let mut transport = CannedTransport::new();
        let mut reply = reply_with_type(0);
        // SAFETY: test-only payload setup through the documented overlay.
        unsafe {
            reply.m_u.raw[..8].copy_from_slice(&0x1_0000u64.to_ne_bytes());
        }
        transport.reply_sendrec(Ok(reply));
        assert_eq!(
            physical_address_via(&transport, Endpoint(5), VirBytes(0x8000)),
            Ok(Some(PhysBytes(0x1_0000)))
        );
    }

    #[test]
    fn test_reference_count_returns_byte() {
        let mut transport = CannedTransport::new();
        let mut reply = reply_with_type(0);
        // SAFETY: test-only payload setup through the documented overlay.
        unsafe {
            reply.m_u.raw[0] = 3;
        }
        transport.reply_sendrec(Ok(reply));
        assert_eq!(
            reference_count_via(&transport, Endpoint(5), VirBytes(0x8000)),
            Ok(Some(3))
        );
    }

    #[test]
    fn test_map_physical_returns_virtual_address() {
        let mut transport = CannedTransport::new();
        let mut reply = reply_with_type(0);
        // SAFETY: test-only payload setup through the documented overlay.
        unsafe {
            reply.m_u.raw[..8].copy_from_slice(&0xA000u64.to_ne_bytes());
        }
        transport.reply_sendrec(Ok(reply));
        assert_eq!(
            map_physical_via(&transport, Endpoint(6), PhysBytes(0xF0000), VirBytes(4096)),
            Ok(VirBytes(0xA000))
        );
    }
}
