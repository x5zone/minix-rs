//! File system call group: descriptors, paths, and vectored input-output.
//!
//! The virtual file system server owns files, directories, and their
//! metadata. User programs reach it through wrappers that share the shape
//! established by the process manager group (see [`crate::pm`): clear a
//! message, fill the call's fields, and run the request protocol from the
//! [`crate::syscall`] module against the file system endpoint. The C sources
//! are the per-call files in `minix3/minix/lib/libc/sys/` (`read.c`,
//! `write.c`, `open.c`, `close.c`, `lseek.c`, and their companions) plus the
//! vectored helper `vectorio.c`.
//!
//! Three C patterns recur across the group and each gets one explicit Rust
//! form here:
//!
//! 1. The read-write payload: file descriptor, length, buffer address, and a
//!    reserved cumulative counter, always zeroed on send.
//! 2. The open dual path: the create flag selects a different message layout
//!    and a different call number.
//! 3. The close-and-reopen trick for duplication: `dup` is not its own call
//!    but a fixed-argument `fcntl`, and vectored input-output assembles one
//!    temporary buffer around a single read or write.
//!
//! Socket-family calls are excluded: they belong to the networking stage, as
//! the stage plan records. Message body layouts stay owned by the shared
//! types crate; field details move to the global concepts document as
//! planned.
//!
//! # Execution model
//!
//! Pure wrappers over the transport trait: no shared state, no
//! synchronization questions.

use crate::ipc::IpcTransport;
use crate::syscall::perform_syscall;
use minix_types::{Endpoint, Errno, Message};

/// File system endpoint.
///
/// C: `VFS_PROC_NR ((endpoint_t) 1)` (`minix3/minix/include/minix/com.h:60`).
pub const VFS_ENDPOINT_NUMBER: i32 = 1;

/// Read from an open file. C: `VFS_READ (VFS_BASE + 0)` (`callnr.h:72`).
pub const VFS_CALL_READ: i32 = 0x100;
/// Write to an open file. C: `VFS_WRITE (VFS_BASE + 1)` (`callnr.h:73`).
pub const VFS_CALL_WRITE: i32 = 0x101;
/// Reposition the file offset. C: `VFS_LSEEK (VFS_BASE + 2)` (`callnr.h:74`).
pub const VFS_CALL_LSEEK: i32 = 0x102;
/// Open a path without creation. C: `VFS_OPEN (VFS_BASE + 3)` (`callnr.h:75`).
pub const VFS_CALL_OPEN: i32 = 0x103;
/// Open a path with creation. C: `VFS_CREAT (VFS_BASE + 4)` (`callnr.h:76`).
pub const VFS_CALL_CREATE: i32 = 0x104;
/// Close a descriptor. C: `VFS_CLOSE (VFS_BASE + 5)` (`callnr.h:77`).
pub const VFS_CALL_CLOSE: i32 = 0x105;
/// Duplicate a descriptor. C: `VFS_FCNTL (VFS_BASE + 25)` (`callnr.h:97`),
/// used by `dup` with the fixed-duplicate command (see `dup.c`).
pub const VFS_CALL_FCNTL: i32 = 0x119;
/// Read directory entries. C: `VFS_GETDENTS (VFS_BASE + 29)` (`callnr.h:101`).
pub const VFS_CALL_GETDENTS: i32 = 0x11D;
/// Wait for descriptor readiness. C: `VFS_SELECT (VFS_BASE + 30)` (`callnr.h:102`).
pub const VFS_CALL_SELECT: i32 = 0x11E;

/// Create-if-missing flag bit.
///
/// C: `O_CREAT 0x00000200` (`minix3/sys/sys/fcntl.h:99`). Its presence
/// selects the create message layout and call number (see `open.c`).
pub const OPEN_FLAG_CREATE: i32 = 0x200;

/// Fixed-duplicate file control command.
///
/// C: `F_DUPFD 0` (`minix3/sys/sys/fcntl.h:178`): duplicate onto the lowest
/// free descriptor at or above the given number. `dup` passes zero, so the
/// duplicate lands on the lowest free descriptor.
pub const FCNTL_COMMAND_DUPLICATE: i32 = 0;

/// Maximum scatter-gather segments per vectored call.
///
/// C: `IOV_MAX 1024` (`minix3/sys/sys/syslimits.h:92`): the sanity bound
/// checked before anything else in `vectorio.c`.
pub const MAX_IO_SEGMENTS: usize = 1024;

/// Returns the file system endpoint.
pub const fn vfs_endpoint() -> Endpoint {
    Endpoint(VFS_ENDPOINT_NUMBER)
}

/// Creates a zeroed message, mirroring the `memset(&m, 0, sizeof(m))` that
/// opens every C wrapper in this group.
fn cleared_message() -> Message {
    Message::zeroed()
}

/// Read-write payload in C field order.
///
/// C: `mess_lc_vfs_readwrite` (`minix3/minix/include/minix/ipc.h:795-803`) —
/// file descriptor, buffer address, byte length, cumulative counter — used
/// identically by `read.c` and `write.c`. The counter is reserved for future
/// use and always sent as zero (see `write.c`: "reserved for future use").
#[repr(C)]
#[derive(Debug, Clone, Copy)]
struct ReadWritePayload {
    fd: i32,
    _padding_before_buffer: [u8; 4],
    buffer_address: u64,
    length: u64,
    cumulative: u64,
    _padding: [u8; 24],
}

/// Copies a packed payload into the message body (same helper shape as the
/// process manager group; layouts centralize in the global concepts
/// document as planned).
fn write_payload(message: &mut Message, packed: &[u8]) {
    debug_assert!(packed.len() <= minix_types::MESSAGE_PAYLOAD_SIZE);
    // SAFETY: the raw payload is 56 writable bytes; the caller guarantees
    // the packed slice fits (debug-checked above).
    unsafe {
        message.m_u.raw[..packed.len()].copy_from_slice(packed);
    }
}

/// Reads a file descriptor's bytes.
///
/// C: `read` (`minix3/minix/lib/libc/sys/read.c`): clear a message, store
/// the descriptor, length, buffer address, and a zeroed reserved counter,
/// and run the protocol. The reply message type is the byte count.
pub fn read_via(
    transport: &impl IpcTransport,
    fd: i32,
    buffer_address: u64,
    length: usize,
) -> Result<usize, Errno> {
    let mut message = cleared_message();
    let packed = ReadWritePayload {
        fd,
        _padding_before_buffer: [0; 4],
        buffer_address,
        length: length as u64,
        cumulative: 0,
        _padding: [0; 24],
    };
    // SAFETY: plain 56-byte value; exact byte representation below.
    let bytes = unsafe {
        core::slice::from_raw_parts(
            (&raw const packed) as *const u8,
            core::mem::size_of::<ReadWritePayload>(),
        )
    };
    write_payload(&mut message, bytes);
    let transferred = perform_syscall(transport, vfs_endpoint(), VFS_CALL_READ, &mut message)?;
    Ok(transferred as usize)
}

/// Writes bytes to a file descriptor.
///
/// C: `write` (`minix3/minix/lib/libc/sys/write.c`): identical shape to
/// read, with the write call number.
pub fn write_via(
    transport: &impl IpcTransport,
    fd: i32,
    buffer_address: u64,
    length: usize,
) -> Result<usize, Errno> {
    let mut message = cleared_message();
    let packed = ReadWritePayload {
        fd,
        _padding_before_buffer: [0; 4],
        buffer_address,
        length: length as u64,
        cumulative: 0,
        _padding: [0; 24],
    };
    // SAFETY: same plain-value reasoning as read_via.
    let bytes = unsafe {
        core::slice::from_raw_parts(
            (&raw const packed) as *const u8,
            core::mem::size_of::<ReadWritePayload>(),
        )
    };
    write_payload(&mut message, bytes);
    let transferred = perform_syscall(transport, vfs_endpoint(), VFS_CALL_WRITE, &mut message)?;
    Ok(transferred as usize)
}

/// Close payload in C field order.
///
/// C: `close` sets the descriptor and a zero no-block flag (`close.c`).
#[repr(C)]
#[derive(Debug, Clone, Copy)]
struct ClosePayload {
    fd: i32,
    no_block: i32,
    _padding: [u8; 48],
}

/// Closes a file descriptor.
///
/// C: `close` (`minix3/minix/lib/libc/sys/close.c`): clear a message, store
/// the descriptor with a zero no-block flag, and run the protocol.
pub fn close_via(transport: &impl IpcTransport, fd: i32) -> Result<(), Errno> {
    let mut message = cleared_message();
    let packed = ClosePayload {
        fd,
        no_block: 0,
        _padding: [0; 48],
    };
    // SAFETY: plain 56-byte value; exact byte representation below.
    let bytes = unsafe {
        core::slice::from_raw_parts(
            (&raw const packed) as *const u8,
            core::mem::size_of::<ClosePayload>(),
        )
    };
    write_payload(&mut message, bytes);
    perform_syscall(transport, vfs_endpoint(), VFS_CALL_CLOSE, &mut message).map(|_| ())
}

/// Seek payload in C field order.
///
/// C: `mess_lc_vfs_lseek` (`ipc.h:726-734`) — offset first, then descriptor
/// and origin. Note the unusual order: unlike every other payload in this
/// group, the offset leads. The struct below keeps the C order so the bytes
/// match bit for bit.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
struct SeekPayload {
    offset: i64,
    fd: i32,
    whence: i32,
    _padding: [u8; 40],
}

/// Repositions a file offset and reports the new position.
///
/// The origin travels through untouched (seek-from-start, seek-from-current,
/// and seek-from-end are server-interpreted). The reply carries the resulting
/// offset in its first eight payload bytes, mirroring the C read-back.
pub fn lseek_via(
    transport: &impl IpcTransport,
    fd: i32,
    offset: i64,
    whence: i32,
) -> Result<i64, Errno> {
    let mut message = cleared_message();
    let packed = SeekPayload {
        offset,
        fd,
        whence,
        _padding: [0; 40],
    };
    // SAFETY: plain 56-byte value; exact byte representation below.
    let bytes = unsafe {
        core::slice::from_raw_parts(
            (&raw const packed) as *const u8,
            core::mem::size_of::<SeekPayload>(),
        )
    };
    write_payload(&mut message, bytes);
    perform_syscall(transport, vfs_endpoint(), VFS_CALL_LSEEK, &mut message)?;
    // SAFETY: the reply payload is 56 readable bytes; the offset sits at
    // byte zero by the C read-back cited above.
    let position = unsafe { message.m_u.raw[..8].as_ptr().cast::<i64>().read() };
    Ok(position)
}

/// Open request after flag dispatch: which call number and layout to use.
///
/// C: `open` (`minix3/minix/lib/libc/sys/open.c`) branches on the create
/// flag: with it, the create layout plus the create call number; without it,
/// the path layout (see `_loadname`) plus the open call number.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpenDispatch {
    /// Use the create layout and call number (mode travels along).
    Create { mode: u32 },
    /// Use the path layout and call number.
    OpenExisting,
}

/// Selects the open path from the flag word.
///
/// The decision mirrors the C branch exactly: only the create bit matters,
/// every other flag passes through to the server inside the message.
pub const fn dispatch_open(flags: i32, mode: u32) -> OpenDispatch {
    if flags & OPEN_FLAG_CREATE != 0 {
        OpenDispatch::Create { mode }
    } else {
        OpenDispatch::OpenExisting
    }
}

/// One scatter-gather segment: a base address plus a byte length.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IoSegment {
    /// Address of the segment's first byte.
    pub base_address: u64,
    /// Segment length in bytes.
    pub length: usize,
}

/// Validates scatter-gather segments and totals their length.
///
/// This is the pure half of `vectorio.c`: reject a negative or oversized
/// segment count, reject any addition that would wrap past the maximum
/// representable byte count, reject a non-empty segment with a null base,
/// and report the total. A zero total means "nothing to do" (the C version
/// then stores a null pointer and returns zero without allocating).
pub fn validate_scatter_gather(segments: &[IoSegment]) -> Result<usize, Errno> {
    if segments.len() > MAX_IO_SEGMENTS {
        return Err(Errno::EINVAL);
    }
    let mut total: usize = 0;
    for segment in segments {
        total = total.checked_add(segment.length).ok_or(Errno::EINVAL)?;
        if total > i64::MAX as usize {
            return Err(Errno::EINVAL);
        }
        if segment.length > 0 && segment.base_address == 0 {
            return Err(Errno::from_i32(minix_types::EFAULT));
        }
    }
    Ok(total)
}

/// Create payload in C field order.
///
/// C: `mess_lc_vfs_creat` (`ipc.h:630-637`) — name address, length, flags,
/// mode, then padding. In 32-bit C the four header fields take 16 bytes and
/// the padding is 40; with 64-bit addresses the header takes 24 bytes, so
/// the padding shrinks to 32 and the total stays 56.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
struct CreatePayload {
    name_address: u64,
    length: u64,
    flags: i32,
    mode: u32,
    _padding: [u8; 32],
}

/// Opens a path, dispatching on the create flag.
///
/// C: `open` (`minix3/minix/lib/libc/sys/open.c`): with the create flag, the
/// create layout plus the create call number; without it, the path layout
/// plus the open call number. The reply message type is the new descriptor.
///
/// Scope note: the create path is fully implemented below. The
/// open-existing path needs the 64-bit path message layout (name and length
/// grow to eight bytes each, so the 40-byte inline buffer no longer fits the
/// 56-byte payload); that layout decision belongs to the global concepts
/// document, which owns all message body layouts. Until it lands, the
/// open-existing path reports `ENOSYS` explicitly instead of sending a
/// malformed message.
pub fn open_via(
    transport: &impl IpcTransport,
    name_address: u64,
    name_length_including_nul: usize,
    flags: i32,
    mode: u32,
) -> Result<i32, Errno> {
    let mut message = cleared_message();
    match dispatch_open(flags, mode) {
        OpenDispatch::Create { mode } => {
            let packed = CreatePayload {
                name_address,
                length: name_length_including_nul as u64,
                flags,
                mode,
                _padding: [0; 32],
            };
            // SAFETY: plain 56-byte value; exact byte representation below.
            let bytes = unsafe {
                core::slice::from_raw_parts(
                    (&raw const packed) as *const u8,
                    core::mem::size_of::<CreatePayload>(),
                )
            };
            write_payload(&mut message, bytes);
            perform_syscall(transport, vfs_endpoint(), VFS_CALL_CREATE, &mut message)
        }
        OpenDispatch::OpenExisting => Err(Errno::ENOSYS),
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
        assert_eq!(VFS_CALL_READ, 0x100);
        assert_eq!(VFS_CALL_WRITE, 0x101);
        assert_eq!(VFS_CALL_LSEEK, 0x102);
        assert_eq!(VFS_CALL_OPEN, 0x103);
        assert_eq!(VFS_CALL_CREATE, 0x104);
        assert_eq!(VFS_CALL_CLOSE, 0x105);
        assert_eq!(VFS_CALL_FCNTL, 0x119);
        assert_eq!(VFS_CALL_GETDENTS, 0x11D);
        assert_eq!(VFS_CALL_SELECT, 0x11E);
        assert_eq!(VFS_ENDPOINT_NUMBER, 1);
        assert_eq!(OPEN_FLAG_CREATE, 0x200);
        assert_eq!(FCNTL_COMMAND_DUPLICATE, 0);
        assert_eq!(MAX_IO_SEGMENTS, 1024);
    }

    #[test]
    fn test_read_returns_byte_count() {
        let mut transport = CannedTransport::new();
        transport.reply_sendrec(Ok(reply_with_type(512)));
        assert_eq!(read_via(&transport, 3, 0x5000, 512), Ok(512));
    }

    #[test]
    fn test_read_payload_matches_c_field_order() {
        let packed = ReadWritePayload {
            fd: 3,
            _padding_before_buffer: [0; 4],
            buffer_address: 0x5000,
            length: 512,
            cumulative: 0,
            _padding: [0; 24],
        };
        assert_eq!(core::mem::size_of::<ReadWritePayload>(), 56);
        // SAFETY: plain value read-back of the struct just built above.
        // C order: descriptor @0, buffer @8, length @16, counter @24.
        let bytes = unsafe {
            core::slice::from_raw_parts(
                (&raw const packed) as *const u8,
                core::mem::size_of::<ReadWritePayload>(),
            )
        };
        assert_eq!(i32::from_ne_bytes(bytes[0..4].try_into().unwrap()), 3);
        assert_eq!(u64::from_ne_bytes(bytes[8..16].try_into().unwrap()), 0x5000);
        assert_eq!(u64::from_ne_bytes(bytes[16..24].try_into().unwrap()), 512);
        assert_eq!(u64::from_ne_bytes(bytes[24..32].try_into().unwrap()), 0);
    }

    #[test]
    fn test_write_returns_byte_count() {
        let mut transport = CannedTransport::new();
        transport.reply_sendrec(Ok(reply_with_type(100)));
        assert_eq!(write_via(&transport, 1, 0x6000, 100), Ok(100));
    }

    #[test]
    fn test_write_error_propagates() {
        let mut transport = CannedTransport::new();
        transport.reply_sendrec(Ok(reply_with_type(-5)));
        assert_eq!(
            write_via(&transport, 1, 0x6000, 100),
            Err(Errno::from_i32(5))
        );
    }

    #[test]
    fn test_close_sends_descriptor() {
        let mut transport = CannedTransport::new();
        transport.reply_sendrec(Ok(reply_with_type(0)));
        assert_eq!(close_via(&transport, 3), Ok(()));
        assert_eq!(transport.sendrec_calls.get(), 1);
    }

    #[test]
    fn test_lseek_returns_new_position() {
        let mut transport = CannedTransport::new();
        let mut reply = reply_with_type(0);
        // SAFETY: test-only payload setup through the documented overlay.
        unsafe {
            reply.m_u.raw[..8].copy_from_slice(&2048i64.to_ne_bytes());
        }
        transport.reply_sendrec(Ok(reply));
        assert_eq!(lseek_via(&transport, 3, 1024, 0), Ok(2048));
    }

    #[test]
    fn test_seek_payload_matches_c_field_order() {
        let packed = SeekPayload {
            offset: 1024,
            fd: 3,
            whence: 0,
            _padding: [0; 40],
        };
        assert_eq!(core::mem::size_of::<SeekPayload>(), 56);
        // SAFETY: plain value read-back of the struct just built above.
        // C order: offset @0 (unusually first), descriptor @8, origin @12.
        let bytes = unsafe {
            core::slice::from_raw_parts(
                (&raw const packed) as *const u8,
                core::mem::size_of::<SeekPayload>(),
            )
        };
        assert_eq!(i64::from_ne_bytes(bytes[0..8].try_into().unwrap()), 1024);
        assert_eq!(i32::from_ne_bytes(bytes[8..12].try_into().unwrap()), 3);
        assert_eq!(i32::from_ne_bytes(bytes[12..16].try_into().unwrap()), 0);
    }

    #[test]
    fn test_open_dispatch_follows_create_flag() {
        assert_eq!(
            dispatch_open(OPEN_FLAG_CREATE, 0o644),
            OpenDispatch::Create { mode: 0o644 }
        );
        assert_eq!(dispatch_open(0, 0), OpenDispatch::OpenExisting);
        // Other flags pass through; only the create bit decides.
        assert_eq!(dispatch_open(0x1, 0), OpenDispatch::OpenExisting);
        assert_eq!(
            dispatch_open(0x1 | OPEN_FLAG_CREATE, 0),
            OpenDispatch::Create { mode: 0 }
        );
    }

    #[test]
    fn test_scatter_gather_totals_valid_segments() {
        let segments = [
            IoSegment { base_address: 0x1000, length: 100 },
            IoSegment { base_address: 0x2000, length: 200 },
        ];
        assert_eq!(validate_scatter_gather(&segments), Ok(300));
        assert_eq!(validate_scatter_gather(&[]), Ok(0));
    }

    #[test]
    fn test_scatter_gather_rejects_null_base() {
        let segments = [IoSegment { base_address: 0, length: 10 }];
        assert_eq!(
            validate_scatter_gather(&segments),
            Err(Errno::from_i32(minix_types::EFAULT))
        );
        // Zero length with null base is harmless.
        let segments = [IoSegment { base_address: 0, length: 0 }];
        assert_eq!(validate_scatter_gather(&segments), Ok(0));
    }

    #[test]
    fn test_scatter_gather_rejects_wrapping_total() {
        let segments = [
            IoSegment { base_address: 0x1000, length: usize::MAX },
            IoSegment { base_address: 0x2000, length: 1 },
        ];
        assert_eq!(validate_scatter_gather(&segments), Err(Errno::EINVAL));
    }

    #[test]
    fn test_create_payload_matches_c_field_order() {
        let packed = CreatePayload {
            name_address: 0x7000,
            length: 9,
            flags: OPEN_FLAG_CREATE,
            mode: 0o644,
            _padding: [0; 32],
        };
        assert_eq!(core::mem::size_of::<CreatePayload>(), 56);
        // SAFETY: plain value read-back of the struct just built above.
        // C order: name @0, length @8, flags @16, mode @20.
        let bytes = unsafe {
            core::slice::from_raw_parts(
                (&raw const packed) as *const u8,
                core::mem::size_of::<CreatePayload>(),
            )
        };
        assert_eq!(u64::from_ne_bytes(bytes[0..8].try_into().unwrap()), 0x7000);
        assert_eq!(u64::from_ne_bytes(bytes[8..16].try_into().unwrap()), 9);
        assert_eq!(i32::from_ne_bytes(bytes[16..20].try_into().unwrap()), OPEN_FLAG_CREATE);
        assert_eq!(u32::from_ne_bytes(bytes[20..24].try_into().unwrap()), 0o644);
    }

    #[test]
    fn test_open_create_path_returns_descriptor() {
        let mut transport = CannedTransport::new();
        transport.reply_sendrec(Ok(reply_with_type(5)));
        assert_eq!(open_via(&transport, 0x7000, 9, OPEN_FLAG_CREATE, 0o644), Ok(5));
    }

    #[test]
    fn test_open_existing_path_waits_for_layout_decision() {
        // The 64-bit path layout belongs to the global concepts document;
        // until it lands, this path reports ENOSYS instead of sending a
        // malformed message.
        let mut transport = CannedTransport::new();
        assert_eq!(
            open_via(&transport, 0x7000, 9, 0, 0),
            Err(Errno::ENOSYS)
        );
        assert_eq!(transport.sendrec_calls.get(), 0);
    }
}
