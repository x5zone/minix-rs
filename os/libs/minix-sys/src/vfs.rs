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
/// File status by path. C: `VFS_STAT (VFS_BASE + 21)` (`callnr.h:93`).
pub const VFS_CALL_STAT: i32 = 0x100 + 21;
/// File status by descriptor. C: `VFS_FSTAT (VFS_BASE + 22)` (`callnr.h:94`).
pub const VFS_CALL_FSTAT: i32 = 0x100 + 22;
/// File status, no symlink follow. C: `VFS_LSTAT (VFS_BASE + 23)` (`callnr.h:95`).
pub const VFS_CALL_LSTAT: i32 = 0x100 + 23;
/// Device-specific control. C: `VFS_IOCTL (VFS_BASE + 24)` (`callnr.h:96`).
pub const VFS_CALL_IOCTL: i32 = 0x100 + 24;
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
/// Get the descriptor's close-on-exec flag. C: `F_GETFD 1` (`fcntl.h:179`).
pub const FCNTL_GET_DESCRIPTOR_FLAGS: i32 = 1;
/// Set the descriptor's close-on-exec flag. C: `F_SETFD 2` (`fcntl.h:180`).
pub const FCNTL_SET_DESCRIPTOR_FLAGS: i32 = 2;
/// Get the file status flags. C: `F_GETFL 3` (`fcntl.h:181`).
pub const FCNTL_GET_STATUS_FLAGS: i32 = 3;
/// Set the file status flags. C: `F_SETFL 4` (`fcntl.h:182`).
pub const FCNTL_SET_STATUS_FLAGS: i32 = 4;
/// Get record locking information. C: `F_GETLK 7` (`fcntl.h:188`).
pub const FCNTL_GET_RECORD_LOCK: i32 = 7;
/// Set or clear a record lock. C: `F_SETLK 8` (`fcntl.h:189`).
pub const FCNTL_SET_RECORD_LOCK: i32 = 8;
/// Like `F_SETLK`, waiting when blocked. C: `F_SETLKW 9` (`fcntl.h:190`).
pub const FCNTL_SET_RECORD_LOCK_WAIT: i32 = 9;

/// Maximum scatter-gather segments per vectored call.
///
/// C: `IOV_MAX 1024` (`minix3/sys/sys/syslimits.h:92`): the sanity bound
/// checked before anything else in `vectorio.c`.
pub const MAX_IO_SEGMENTS: usize = 1024;

/// Returns the file system endpoint.
pub const fn vfs_endpoint() -> Endpoint {
    Endpoint(VFS_ENDPOINT_NUMBER)
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
    let mut message = crate::syscall::cleared_message();
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
    crate::syscall::write_payload(&mut message, bytes);
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
    let mut message = crate::syscall::cleared_message();
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
    crate::syscall::write_payload(&mut message, bytes);
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
    let mut message = crate::syscall::cleared_message();
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
    crate::syscall::write_payload(&mut message, bytes);
    perform_syscall(transport, vfs_endpoint(), VFS_CALL_CLOSE, &mut message).map(|_| ())
}

/// Stat-family payload in C field order.
///
/// C: `mess_lc_vfs_stat` (`minix3/minix/include/minix/ipc.h:874-880`) —
/// path length including the NUL, path pointer, and the caller's
/// `struct stat` buffer pointer. Shared by `stat` and `lstat` (`stat.c`
/// fills the same fields; the call number alone selects the symlink rule).
#[repr(C)]
#[derive(Debug, Clone, Copy)]
struct StatPathPayload {
    length: u64,
    name: u64,
    buffer: u64,
    _padding: [u8; 32],
}

/// Stat-by-descriptor payload in C field order.
///
/// C: `mess_lc_vfs_fstat` (`ipc.h:665-670`) — descriptor plus buffer
/// pointer, no path.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
struct StatFdPayload {
    fd: i32,
    _padding_before_buffer: [u8; 4],
    buffer: u64,
    _padding: [u8; 40],
}

/// Shared body of the two path stat variants; the call number carries the
/// symlink decision (`stat.c` runs the same message shape for both).
fn stat_via_path(
    transport: &impl IpcTransport,
    call_number: i32,
    name_address: u64,
    name_length_including_nul: usize,
    buffer_address: u64,
) -> Result<(), Errno> {
    let mut message = crate::syscall::cleared_message();
    let packed = StatPathPayload {
        length: name_length_including_nul as u64,
        name: name_address,
        buffer: buffer_address,
        _padding: [0; 32],
    };
    // SAFETY: plain 56-byte value; exact byte representation below.
    let bytes = unsafe {
        core::slice::from_raw_parts(
            (&raw const packed) as *const u8,
            core::mem::size_of::<StatPathPayload>(),
        )
    };
    crate::syscall::write_payload(&mut message, bytes);
    perform_syscall(transport, vfs_endpoint(), call_number, &mut message).map(|_| ())
}

/// Retrieves file status by path, following symlinks.
///
/// C: `stat` (`minix3/minix/lib/libc/sys/stat.c`): clear a message, store
/// the path length (including NUL), path pointer, and stat buffer pointer,
/// and run the protocol. The file-system server fills the caller's
/// [`minix_types::Stat`] buffer through a magic grant, so the reply only
/// reports success or the errno.
pub fn stat_via(
    transport: &impl IpcTransport,
    name_address: u64,
    name_length_including_nul: usize,
    buffer_address: u64,
) -> Result<(), Errno> {
    stat_via_path(
        transport,
        VFS_CALL_STAT,
        name_address,
        name_length_including_nul,
        buffer_address,
    )
}

/// Retrieves file status by path without following the final symlink.
///
/// C: `lstat` (same file and message shape as [`stat_via`]; `stadir.c:419`
/// routes the lookup with `PATH_RET_SYMLINK`).
pub fn lstat_via(
    transport: &impl IpcTransport,
    name_address: u64,
    name_length_including_nul: usize,
    buffer_address: u64,
) -> Result<(), Errno> {
    stat_via_path(
        transport,
        VFS_CALL_LSTAT,
        name_address,
        name_length_including_nul,
        buffer_address,
    )
}

/// Retrieves file status for an open descriptor.
///
/// C: `fstat` (`stat.c`): descriptor and buffer pointer, no path.
pub fn fstat_via(
    transport: &impl IpcTransport,
    fd: i32,
    buffer_address: u64,
) -> Result<(), Errno> {
    let mut message = crate::syscall::cleared_message();
    let packed = StatFdPayload {
        fd,
        _padding_before_buffer: [0; 4],
        buffer: buffer_address,
        _padding: [0; 40],
    };
    // SAFETY: plain 56-byte value; exact byte representation below.
    let bytes = unsafe {
        core::slice::from_raw_parts(
            (&raw const packed) as *const u8,
            core::mem::size_of::<StatFdPayload>(),
        )
    };
    crate::syscall::write_payload(&mut message, bytes);
    perform_syscall(transport, vfs_endpoint(), VFS_CALL_FSTAT, &mut message).map(|_| ())
}

/// Ioctl payload in C field order.
///
/// C: `mess_lc_vfs_ioctl` (`ipc.h:699-705`) — descriptor, request number
/// (the C `unsigned long` grows to 64 bits), and the user buffer address.
/// The driver-side access to that buffer rides a request-encoded grant
/// (`device.c` `make_ioctl_grant`), not this message.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
struct IoctlPayload {
    fd: i32,
    _padding_before_request: [u8; 4],
    request: u64,
    argument: u64,
    _padding: [u8; 32],
}

/// Runs a device-specific control request.
///
/// C: `ioctl` (`minix3/minix/lib/libc/sys/ioctl.c`, after its rewrite of
/// the terminal-local commands into `fcntl` calls): clear a message, store
/// descriptor, request, and the raw argument, and run the protocol. The
/// argument is the caller's buffer address when the request carries one;
/// the request number alone defines its shape and direction.
pub fn ioctl_via(
    transport: &impl IpcTransport,
    fd: i32,
    request: u64,
    argument: u64,
) -> Result<i32, Errno> {
    let mut message = crate::syscall::cleared_message();
    let packed = IoctlPayload {
        fd,
        _padding_before_request: [0; 4],
        request,
        argument,
        _padding: [0; 32],
    };
    // SAFETY: plain 56-byte value; exact byte representation below.
    let bytes = unsafe {
        core::slice::from_raw_parts(
            (&raw const packed) as *const u8,
            core::mem::size_of::<IoctlPayload>(),
        )
    };
    crate::syscall::write_payload(&mut message, bytes);
    perform_syscall(transport, vfs_endpoint(), VFS_CALL_IOCTL, &mut message)
}

/// Fcntl payload in C field order.
///
/// C: `mess_lc_vfs_fcntl` (`ipc.h:655-662`) — descriptor, command, and the
/// two argument shapes side by side: exactly one of `argument_int` /
/// `argument_ptr` is meaningful per command (`fcntl.c`), the other stays
/// zero on the wire.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
struct FcntlPayload {
    fd: i32,
    command: i32,
    argument_int: i32,
    _padding_before_pointer: [u8; 4],
    argument_ptr: u64,
    _padding: [u8; 32],
}

/// Runs a file control command.
///
/// C: `fcntl` (`minix3/minix/lib/libc/sys/fcntl.c`): the reply message type
/// carries the command's result — a new descriptor for `F_DUPFD`, the flag
/// word for the get forms, zero otherwise.
pub fn fcntl_via(
    transport: &impl IpcTransport,
    fd: i32,
    command: i32,
    argument_int: i32,
    argument_ptr: u64,
) -> Result<i32, Errno> {
    let mut message = crate::syscall::cleared_message();
    let packed = FcntlPayload {
        fd,
        command,
        argument_int,
        _padding_before_pointer: [0; 4],
        argument_ptr,
        _padding: [0; 32],
    };
    // SAFETY: plain 56-byte value; exact byte representation below.
    let bytes = unsafe {
        core::slice::from_raw_parts(
            (&raw const packed) as *const u8,
            core::mem::size_of::<FcntlPayload>(),
        )
    };
    crate::syscall::write_payload(&mut message, bytes);
    perform_syscall(transport, vfs_endpoint(), VFS_CALL_FCNTL, &mut message)
}

/// Changes the process root directory.
///
/// C: `VFS_CHROOT (VFS_BASE + 28)` (`callnr.h:100`).
pub const VFS_CALL_CHROOT: i32 = 0x100 + 28;
/// `F_DUPFD` — duplicate the descriptor (`sys/sys/fcntl.h:178`).
pub const F_DUPFD: i32 = 0;
/// `F_GETFL` — get the status flags (`sys/sys/fcntl.h:181`); also the
/// cheapest validity probe for an existing descriptor.
pub const F_GETFL: i32 = 3;
/// Descriptor upper bound (`sys/sys/syslimits.h:38`); C `dup2` answers
/// EBADF above it (`minix3/minix/lib/libc/sys/dup2.c:17-19`).
pub const OPEN_MAX: i32 = 255;

/// Duplicates a descriptor onto an exact slot (C: `dup2`,
/// `minix3/minix/lib/libc/sys/dup2.c:12-32`).
///
/// POSIX shapes dup2 as "almost, but not quite fcntl" (`dup2.c:6-8`), and
/// the C wrapper keeps three of the differences: a range check that answers
/// EBADF without a round trip (`dup2.c:17-19`), an `F_GETFL` validity probe
/// on the source descriptor whose error travels (`dup2.c:22-26`), and the
/// same-descriptor short circuit that succeeds without closing
/// (`dup2.c:27-29`). Otherwise: close the target slot (its failure is
/// ignored, `dup2.c:30`), then `F_DUPFD` with `fd2` as the floor — the
/// reply is the new descriptor.
pub fn dup2_via(transport: &impl IpcTransport, fd: i32, fd2: i32) -> Result<i32, Errno> {
    if !(0..=OPEN_MAX).contains(&fd2) {
        return Err(Errno::EBADF);
    }
    fcntl_via(transport, fd, F_GETFL, 0, 0)?;
    if fd == fd2 {
        return Ok(fd2);
    }
    let _ = close_via(transport, fd2);
    fcntl_via(transport, fd, F_DUPFD, fd2, 0)
}

/// Changes the process root directory (C: `chroot`,
/// `minix3/minix/lib/libc/sys/chroot.c:12-19`).
///
/// C loads the name inline into the message (`_loadname`, chroot.c:15);
/// this face reuses the open-existing payload shape — address, length,
/// then the path bytes themselves (see `open_existing_via`, where the
/// inline boundary and its ENAMETOOLONG answer live too). The caller
/// keeps the path buffer alive until the reply: a real server reads it
/// through the caller's address space, the way `chroot`'s `loadname`
/// made the message itself carry the bytes.
pub fn chroot_via(
    transport: &impl IpcTransport,
    name_address: u64,
    name_length_including_nul: usize,
) -> Result<(), Errno> {
    if name_length_including_nul == 0 || name_length_including_nul > OPEN_PATH_INLINE_MAX {
        return Err(Errno::ENAMETOOLONG);
    }
    // SAFETY: the caller's path buffer lives in this same address space
    // (user library reads its own argument — C loadname.c:16-17 同型).
    let path_bytes = unsafe {
        core::slice::from_raw_parts(name_address as *const u8, name_length_including_nul)
    };
    let mut packed = [0u8; core::mem::size_of::<OpenPathPayload>()];
    packed[0..8].copy_from_slice(&name_address.to_le_bytes());
    packed[8..16].copy_from_slice(&(name_length_including_nul as u64).to_le_bytes());
    // flags/mode lanes stay zero — chroot carries neither.
    packed[24..24 + path_bytes.len()].copy_from_slice(path_bytes);
    let mut message = crate::syscall::cleared_message();
    crate::syscall::write_payload(&mut message, &packed);
    perform_syscall(transport, vfs_endpoint(), VFS_CALL_CHROOT, &mut message).map(|_| ())
}

/// Reads directory entries in the getdents wire format.
///
/// C: `getdents` (`minix3/minix/lib/libc/sys/getdents.c`) reuses the
/// read/write message with a zeroed reserved counter; the reply is the
/// number of bytes written into the caller's buffer (`read.c:282-312`
/// rejects a nonzero counter outright).
pub fn getdents_via(
    transport: &impl IpcTransport,
    fd: i32,
    buffer_address: u64,
    length: usize,
) -> Result<usize, Errno> {
    let mut message = crate::syscall::cleared_message();
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
    crate::syscall::write_payload(&mut message, bytes);
    let transferred = perform_syscall(transport, vfs_endpoint(), VFS_CALL_GETDENTS, &mut message)?;
    Ok(transferred as usize)
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
    let mut message = crate::syscall::cleared_message();
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
    crate::syscall::write_payload(&mut message, bytes);
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
/// Scope note: both dispatch arms are implemented — create below, and the
/// open-existing path through [`open_existing_via`], which carries the
/// 64-bit layout adjudication (LP64: the inline buffer shrinks from 40 to
/// 32 bytes including the NUL; longer paths return `ENAMETOOLONG` where C's
/// `loadname` silently skipped the inline copy).
/// C: `mess_lc_vfs_path` (`ipc.h:754-768`) — open-existing 的载荷形状。
/// i386 原始布局：name/len/flags/mode/buf[40] = 56 字节。
#[repr(C)]
#[derive(Debug, Clone, Copy)]
struct OpenPathPayload {
    name: u64,
    len: u64,
    flags: i32,
    mode: i32,
    buf: [u8; 32],
}

/// open-existing 内联容量（LP64 判例：指针 4→8 后 buf 由 40 收缩为 32；
/// 含 NUL 的总长 ≤ 32 才能内联）。
pub const OPEN_PATH_INLINE_MAX: usize = 32;

/// open-existing 路径的 wire 裁决（99 篇定稿 + 09 篇 §3.2 缺口闭合）：
/// 路径 ≤ [`OPEN_PATH_INLINE_MAX`] 字节（含 NUL）时内联进载荷 buf；
/// 超长返回 `ENAMETOOLONG`——C 的 loadname（loadname.c:18）在超长时
/// 跳过 strcpy 但不报错（截断静默发生），minix-rs 诚实化为显式错误
/// （DELIBERATE DIVERGENCE：加固而非行为漂移，登记于 09 篇 §3.2）。
pub fn open_existing_via(
    transport: &impl IpcTransport,
    name_address: u64,
    name_length_including_nul: usize,
    flags: i32,
) -> Result<i32, Errno> {
    if name_length_including_nul == 0 || name_length_including_nul > OPEN_PATH_INLINE_MAX {
        return Err(Errno::ENAMETOOLONG);
    }
    // SAFETY: the caller's path buffer lives in this same address space
    // (user library reads its own argument — C loadname.c:16-17 同型)。
    let path_bytes = unsafe {
        core::slice::from_raw_parts(name_address as *const u8, name_length_including_nul)
    };
    let mut packed = [0u8; core::mem::size_of::<OpenPathPayload>()];
    packed[0..8].copy_from_slice(&name_address.to_le_bytes());
    packed[8..16].copy_from_slice(&(name_length_including_nul as u64).to_le_bytes());
    packed[16..20].copy_from_slice(&flags.to_le_bytes());
    // mode @20..24 = 0（非创建路径无 mode，C open.c:31 同填 0）。
    packed[24..24 + path_bytes.len()].copy_from_slice(path_bytes);
    let mut message = crate::syscall::cleared_message();
    crate::syscall::write_payload(&mut message, &packed);
    perform_syscall(transport, vfs_endpoint(), VFS_CALL_OPEN, &mut message)
}

pub fn open_via(
    transport: &impl IpcTransport,
    name_address: u64,
    name_length_including_nul: usize,
    flags: i32,
    mode: u32,
) -> Result<i32, Errno> {
    let mut message = crate::syscall::cleared_message();
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
            crate::syscall::write_payload(&mut message, bytes);
            perform_syscall(transport, vfs_endpoint(), VFS_CALL_CREATE, &mut message)
        }
        OpenDispatch::OpenExisting => {
            open_existing_via(transport, name_address, name_length_including_nul, flags)
        }
    }
}

/// Server-control request number for the file-system group.
///
/// C: `VFS_SVRCTL (VFS_BASE + 43)` (`minix3/minix/include/minix/callnr.h:115`).
pub const VFS_CALL_SERVER_CONTROL: i32 = 0x100 + 43;

/// A wall-clock split used as the select timeout buffer.
///
/// C: `struct timeval` (`minix3/sys/sys/time.h`): whole seconds plus a
/// microsecond fraction, both 64-bit on LP64. The server may write the
/// remaining time back through the caller's pointer; callers that ignore
/// it simply drop the buffer, exactly like the C sleep path.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct TimeVal {
    /// Whole seconds.
    pub seconds: i64,
    /// Microsecond fraction.
    pub microseconds: i64,
}

/// Waits with empty descriptor sets and a timeout — the select shape the
/// sleep composition needs.
///
/// C: `select(0, NULL, NULL, NULL, &timeout)` (`minix3/minix/lib/libc/sys/
/// nanosleep.c:58`): zero descriptors, three empty set pointers, and one
/// timeout buffer whose address travels in the last payload lane. The wire
/// payload is `mess_lc_vfs_select` (`minix3/minix/include/minix/ipc.h:
/// 800-813`) adapted to LP64: descriptor count at byte 0, the three set
/// pointers at 8/16/24, the timeout pointer at 32.
///
/// A positive result means the wait completed inside the timeout; a failed
/// round trip is an `Err` (interrupted sleeps carry `EINTR`, and the
/// caller measures any remaining time with wall-clock snapshots — the C
/// composition uses `gettimeofday` before and after for the same effect).
pub fn select_empty_via(
    transport: &impl IpcTransport,
    timeout: &mut TimeVal,
) -> Result<i32, Errno> {
    let mut message = Message::zeroed();
    // SAFETY: five lanes of the select payload — nfds (u32 @0), three
    // null set pointers (@8/16/24), and the timeout address (@32).
    unsafe {
        message.m_u.raw[..4].copy_from_slice(&0u32.to_ne_bytes());
        message.m_u.raw[32..40].copy_from_slice(&(timeout as *mut TimeVal as u64).to_ne_bytes());
    }
    perform_syscall(transport, vfs_endpoint(), VFS_CALL_SELECT, &mut message)
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
        assert_eq!(VFS_CALL_SERVER_CONTROL, 0x12B);
        assert_eq!(VFS_ENDPOINT_NUMBER, 1);
        assert_eq!(OPEN_FLAG_CREATE, 0x200);
        assert_eq!(FCNTL_COMMAND_DUPLICATE, 0);
        assert_eq!(MAX_IO_SEGMENTS, 1024);
    }

    #[test]
    fn test_select_empty_sends_zero_count_and_timeout_pointer() {
        let mut transport = CannedTransport::new();
        transport.reply_sendrec(Ok(reply_with_type(0)));
        let mut timeout = TimeVal { seconds: 2, microseconds: 1 };
        assert_eq!(select_empty_via(&transport, &mut timeout), Ok(0));
        let sent = transport.sent.borrow();
        assert_eq!(sent.len(), 1);
        assert_eq!(sent[0].0, vfs_endpoint());
        assert_eq!(sent[0].1.m_type, VFS_CALL_SELECT);
        // SAFETY: test-only payload read-back of the outgoing wire bytes.
        let raw = unsafe { &sent[0].1.m_u.raw };
        assert_eq!(&raw[..4], &0u32.to_ne_bytes());
        assert_eq!(&raw[8..16], &0u64.to_ne_bytes());
        assert_eq!(&raw[16..24], &0u64.to_ne_bytes());
        assert_eq!(&raw[24..32], &0u64.to_ne_bytes());
        assert_eq!(
            &raw[32..40],
            &(&mut timeout as *mut TimeVal as u64).to_ne_bytes()
        );
    }

    #[test]
    fn test_select_empty_propagates_interruption() {
        // Transport failures carry the positive errno (TrapStatus sign
        // contract); the wrapper hands the interruption through as the
        // typed error.
        let mut transport = CannedTransport::new();
        transport.reply_sendrec(Err(crate::ipc::TrapStatus(minix_types::EINTR)));
        let mut timeout = TimeVal::default();
        assert_eq!(
            select_empty_via(&transport, &mut timeout),
            Err(Errno::from_i32(minix_types::EINTR))
        );
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
    fn test_open_existing_path_sends_inline_layout() {
        // The 64-bit layout landed with the 99-global-concepts adjudication
        // (inline buf shrunk to 32, ENAMETOOLONG past it): a short path now
        // travels the inline layout and the reply descriptor comes back.
        // The path bytes must live at a real address — the inline copy
        // reads them.
        let path = b"/etc/motd\0";
        let mut transport = CannedTransport::new();
        transport.reply_sendrec(Ok(reply_with_type(3)));
        assert_eq!(open_via(&transport, path.as_ptr() as u64, path.len(), 0, 0), Ok(3));
        assert_eq!(transport.sendrec_calls.get(), 1);
    }

    // ── L10:stat 族 / ioctl / fcntl / getdents 的 wire 回放 ──

    /// stat 载荷逐 lane：len(含 NUL)/name/buf（C mess_lc_vfs_stat，
    /// ipc.h:874-880），调用号 VFS_STAT=0x115。
    #[test]
    fn test_stat_wire_roundtrip() {
        let mut transport = CannedTransport::new();
        transport.reply_sendrec(Ok(reply_with_type(0)));
        stat_via(&transport, 0x9000, 12, 0xA000).unwrap();
        let (_, sent) = transport.sent.borrow().last().cloned().unwrap();
        assert_eq!(sent.m_type, VFS_CALL_STAT);
        assert_eq!(sent.m_type, 0x115);
        // SAFETY(test): 读回三 lane 的 raw 字节（与 StatPathPayload 同布局）。
        let raw = unsafe { &sent.m_u.raw };
        assert_eq!(u64::from_ne_bytes(raw[0..8].try_into().unwrap()), 12);
        assert_eq!(u64::from_ne_bytes(raw[8..16].try_into().unwrap()), 0x9000);
        assert_eq!(u64::from_ne_bytes(raw[16..24].try_into().unwrap()), 0xA000);
    }

    /// lstat 与 stat 同载荷异调用号（stat.c 单文件两函数的形状）。
    #[test]
    fn test_lstat_uses_lstat_call_number() {
        let mut transport = CannedTransport::new();
        transport.reply_sendrec(Ok(reply_with_type(0)));
        lstat_via(&transport, 0x9000, 8, 0xA000).unwrap();
        let (_, sent) = transport.sent.borrow().last().cloned().unwrap();
        assert_eq!(sent.m_type, VFS_CALL_LSTAT);
        assert_eq!(sent.m_type, 0x117);
    }

    /// fstat 载荷：fd @0、buf @8（ipc.h:665-670 的 LP64 换算）。
    #[test]
    fn test_fstat_wire_roundtrip() {
        let mut transport = CannedTransport::new();
        transport.reply_sendrec(Ok(reply_with_type(0)));
        fstat_via(&transport, 7, 0xA000).unwrap();
        let (_, sent) = transport.sent.borrow().last().cloned().unwrap();
        assert_eq!(sent.m_type, VFS_CALL_FSTAT);
        assert_eq!(sent.m_type, 0x116);
        // SAFETY(test): 读回 fd 与 buf lane。
        let raw = unsafe { &sent.m_u.raw };
        assert_eq!(i32::from_ne_bytes(raw[0..4].try_into().unwrap()), 7);
        assert_eq!(u64::from_ne_bytes(raw[8..16].try_into().unwrap()), 0xA000);
    }

    /// ioctl 三 lane：fd/req(64 位)/arg（ipc.h:699-705）；应答原样上浮
    /// （成功值经 m_type 返回，如请求编码的返回值）。
    #[test]
    fn test_ioctl_wire_roundtrip() {
        let mut transport = CannedTransport::new();
        transport.reply_sendrec(Ok(reply_with_type(0x25)));
        assert_eq!(ioctl_via(&transport, 4, 0x8927, 0xB000), Ok(0x25));
        let (_, sent) = transport.sent.borrow().last().cloned().unwrap();
        assert_eq!(sent.m_type, VFS_CALL_IOCTL);
        assert_eq!(sent.m_type, 0x118);
        // SAFETY(test): 读回 fd/req/arg lane。
        let raw = unsafe { &sent.m_u.raw };
        assert_eq!(i32::from_ne_bytes(raw[0..4].try_into().unwrap()), 4);
        assert_eq!(u64::from_ne_bytes(raw[8..16].try_into().unwrap()), 0x8927);
        assert_eq!(u64::from_ne_bytes(raw[16..24].try_into().unwrap()), 0xB000);
    }

    /// fcntl：fd/cmd/arg_int 并排 arg_ptr（ipc.h:655-662；同调用只填
    /// 其一）；应答 = 命令结果（F_DUPFD 的新 fd）。
    #[test]
    fn test_fcntl_wire_roundtrip() {
        let mut transport = CannedTransport::new();
        transport.reply_sendrec(Ok(reply_with_type(5)));
        assert_eq!(fcntl_via(&transport, 2, FCNTL_COMMAND_DUPLICATE, 0, 0), Ok(5));
        let (_, sent) = transport.sent.borrow().last().cloned().unwrap();
        assert_eq!(sent.m_type, VFS_CALL_FCNTL);
        // SAFETY(test): 读回 fd/cmd/arg_int/arg_ptr lane。
        let raw = unsafe { &sent.m_u.raw };
        assert_eq!(i32::from_ne_bytes(raw[0..4].try_into().unwrap()), 2);
        assert_eq!(i32::from_ne_bytes(raw[4..8].try_into().unwrap()), 0);
        assert_eq!(i32::from_ne_bytes(raw[8..12].try_into().unwrap()), 0);
        assert_eq!(u64::from_ne_bytes(raw[16..24].try_into().unwrap()), 0);
    }

    /// getdents 复用 read/write 载荷（getdents.c 直接填 m_lc_vfs_readwrite），
    /// 应答 = 实读字节数。
    #[test]
    fn test_getdents_wire_roundtrip() {
        let mut transport = CannedTransport::new();
        transport.reply_sendrec(Ok(reply_with_type(96)));
        assert_eq!(getdents_via(&transport, 6, 0xC000, 512), Ok(96));
        let (_, sent) = transport.sent.borrow().last().cloned().unwrap();
        assert_eq!(sent.m_type, VFS_CALL_GETDENTS);
        assert_eq!(sent.m_type, 0x11D);
        // SAFETY(test): 读回 fd/buf/len lane（与 ReadWritePayload 同布局）。
        let raw = unsafe { &sent.m_u.raw };
        assert_eq!(i32::from_ne_bytes(raw[0..4].try_into().unwrap()), 6);
        assert_eq!(u64::from_ne_bytes(raw[8..16].try_into().unwrap()), 0xC000);
        assert_eq!(u64::from_ne_bytes(raw[16..24].try_into().unwrap()), 512);
        // 保留计数器恒零（read.c:282-312 对非零 EINVAL）。
        assert_eq!(u64::from_ne_bytes(raw[24..32].try_into().unwrap()), 0);
    }
}

#[cfg(test)]
mod payload_layout_tests {
    use super::*;
    use crate::ipc::CannedTransport;
    use core::mem::{offset_of, size_of};

    /// E-MINTYPES-RUNTIME 第②步：VFS 族五个 payload 的 56 字节/偏移断言
    /// （C ipc.h:795-803 readwrite、close、seek、create——C 字段顺序）。
    #[test]
    fn test_readwrite_payload_layout() {
        assert_eq!(size_of::<ReadWritePayload>(), 56);
        assert_eq!(offset_of!(ReadWritePayload, fd), 0);
        assert_eq!(offset_of!(ReadWritePayload, buffer_address), 8);
        assert_eq!(offset_of!(ReadWritePayload, length), 16);
        assert_eq!(offset_of!(ReadWritePayload, cumulative), 24);
    }

    #[test]
    fn test_seek_payload_layout() {
        // C: mess_lc_vfs_seek（offset 头 + whence）——C 字段顺序逐域。
        assert_eq!(size_of::<SeekPayload>(), 56);
        assert_eq!(offset_of!(SeekPayload, offset), 0);
    }

    #[test]
    fn test_create_payload_layout() {
        assert_eq!(size_of::<CreatePayload>(), 56);
        assert_eq!(offset_of!(CreatePayload, name_address), 0);
    }

    #[test]
    fn test_time_val_layout() {
        // C: struct timespec 的 seconds/nsec 双 64 位形态（LP64 16B）。
        assert_eq!(size_of::<TimeVal>(), 16);
        assert_eq!(offset_of!(TimeVal, seconds), 0);
        assert_eq!(offset_of!(TimeVal, microseconds), 8);
    }
}

#[cfg(test)]
mod open_path_tests {
    use super::*;
    use core::mem::offset_of;
    use crate::ipc::CannedTransport;

    /// open-existing 载荷布局见证：LP64 判例下总长 56（C i386 buf[40]
    /// 收缩为 32），字段序 name/len/flags/mode/buf。
    #[test]
    fn test_open_path_payload_layout() {
        assert_eq!(size_of::<OpenPathPayload>(), 56);
        assert_eq!(offset_of!(OpenPathPayload, name), 0);
        assert_eq!(offset_of!(OpenPathPayload, len), 8);
        assert_eq!(offset_of!(OpenPathPayload, flags), 16);
        assert_eq!(offset_of!(OpenPathPayload, mode), 20);
        assert_eq!(offset_of!(OpenPathPayload, buf), 24);
    }

    /// 内联拷贝：路径字节进 buf、len 含 NUL——C loadname.c:16-18 同型。
    #[test]
    fn test_open_path_inline_boundary() {
        // 恰好 32（含 NUL）：可内联。
        assert_eq!(31, OPEN_PATH_INLINE_MAX - 1);
        // 超界 33：ENAMETOOLONG。
        let long = [b'a'; 34];
        assert_eq!(
            open_existing_via(
                &CannedTransport::new(),
                long.as_ptr() as u64,
                long.len(),
                0
            )
            .unwrap_err(),
            Errno::ENAMETOOLONG
        );
    }

}

#[cfg(test)]
mod dup2_wire_tests {
    use super::*;
    use crate::ipc::CannedTransport;
    use minix_types::Message;

    fn reply_with_type(message_type: i32) -> Message {
        let mut message = Message::zeroed();
        message.m_type = message_type;
        message
    }

    /// C 绝对值 pin：F_DUPFD=0 / F_GETFL=3（sys/sys/fcntl.h:178/181）、
    /// OPEN_MAX=255（syslimits.h:38）。
    #[test]
    fn test_dup2_constants_match_c() {
        assert_eq!(F_DUPFD, 0);
        assert_eq!(F_GETFL, 3);
        assert_eq!(OPEN_MAX, 255);
    }

    /// 全序：F_GETFL 探测 → close(fd2) → F_DUPFD(fd, fd2)，回复即新
    /// 描述符（dup2.c:22-31）。
    #[test]
    fn test_dup2_probes_closes_and_duplicates() {
        let mut transport = CannedTransport::new();
        transport.reply_sendrec(Ok(reply_with_type(0))); // F_GETFL ok
        transport.reply_sendrec(Ok(reply_with_type(0))); // close ok
        transport.reply_sendrec(Ok(reply_with_type(3))); // F_DUPFD → fd 3
        assert_eq!(dup2_via(&transport, 4, 3), Ok(3));
        let sent = transport.sent.borrow();
        assert_eq!(sent.len(), 3);
        assert_eq!(sent[0].1.m_type, VFS_CALL_FCNTL);
        assert_eq!(sent[2].1.m_type, VFS_CALL_FCNTL);
    }

    /// 同槽短路：fd == fd2 在探测之后直接成功——close 与 F_DUPFD
    /// 都不发生（探测本身在前，dup2.c:22-29）；越界目标零回合 EBADF
    /// （dup2.c:17-19）。
    #[test]
    fn test_dup2_same_fd_short_circuits_and_bounds_reject() {
        let mut transport = CannedTransport::new();
        transport.reply_sendrec(Ok(reply_with_type(0))); // F_GETFL probe
        assert_eq!(dup2_via(&transport, 2, 2), Ok(2));
        assert_eq!(transport.sent.borrow().len(), 1);
        assert_eq!(dup2_via(&transport, 1, OPEN_MAX + 1), Err(Errno::EBADF));
        assert_eq!(dup2_via(&transport, 1, -1), Err(Errno::EBADF));
        assert_eq!(transport.sent.borrow().len(), 1, "bounds rejects add no round trips");
    }

    /// 源描述符失效：F_GETFL 的错误原样上行（dup2.c:22-26），不发后续。
    #[test]
    fn test_dup2_source_probe_failure_travels() {
        let mut transport = CannedTransport::new();
        transport.reply_sendrec(Err(crate::ipc::TrapStatus(minix_types::EBADF)));
        assert_eq!(dup2_via(&transport, 9, 0), Err(Errno::EBADF));
        assert_eq!(transport.sent.borrow().len(), 1);
    }
}

#[cfg(test)]
mod chroot_wire_tests {
    use super::*;
    use crate::ipc::CannedTransport;
    use minix_types::Message;

    fn reply_with_type(message_type: i32) -> Message {
        let mut message = Message::zeroed();
        message.m_type = message_type;
        message
    }

    /// C 绝对值 pin：VFS_CHROOT = VFS_BASE + 28（callnr.h:100）。
    #[test]
    fn test_chroot_call_number_matches_c() {
        assert_eq!(VFS_CALL_CHROOT, 0x100 + 28);
    }

    /// 载荷形态沿 open_existing：地址@0、长度@8、flags/mode 零、内联
    /// 路径字节 @24（chroot.c:15 loadname 内联语义的载荷等价物）。
    #[test]
    fn test_chroot_payload_carries_inline_path() {
        let mut transport = CannedTransport::new();
        transport.reply_sendrec(Ok(reply_with_type(0)));
        let path = b"/mnt/newroot\0";
        assert_eq!(chroot_via(&transport, path.as_ptr() as u64, path.len()), Ok(()));
        let (dest, sent) = transport.sent.borrow().last().cloned().unwrap();
        assert_eq!(dest, vfs_endpoint());
        assert_eq!(sent.m_type, VFS_CALL_CHROOT);
        // SAFETY: byte-level read of the union overlay lanes for test
        // assertions only (same as the sigaction wire test).
        let raw = unsafe { &sent.m_u.raw };
        assert_eq!(u64::from_le_bytes(raw[0..8].try_into().unwrap()), path.as_ptr() as u64);
        assert_eq!(u64::from_le_bytes(raw[8..16].try_into().unwrap()), path.len() as u64);
        assert_eq!(&raw[24..24 + path.len()], path);
    }

    /// 越界路径与 open 同判 ENAMETOOLONG（内联边界同源）。
    #[test]
    fn test_chroot_rejects_over_inline_boundary() {
        let transport = CannedTransport::new();
        let long = [b'a'; OPEN_PATH_INLINE_MAX + 1];
        assert_eq!(
            chroot_via(&transport, long.as_ptr() as u64, long.len()),
            Err(Errno::ENAMETOOLONG)
        );
    }
}
