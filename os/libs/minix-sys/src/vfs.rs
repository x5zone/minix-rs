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

/// `VFS_PIPE2` — create a pipe (`callnr.h:98`, `VFS_BASE + 26`).
pub const VFS_CALL_PIPE2: i32 = 0x100 + 26;

/// Creates a pipe over the given transport (C: `pipe2`,
/// `minix3/minix/lib/libc/sys/pipe.c:14-27`).
///
/// Request payload is `mess_lc_vfs_pipe2`（flags@0、_unused@4、oflags@8）；
/// the server bitwise-ors the two flag fields (`pipe.c:45-46`), and the C
/// libc writes the same `flags` into both for backward compatibility
/// (`pipe.c:18-19`) — done the same way here. The OK reply carries
/// `mess_vfs_lc_fdpair { int fd0; int fd1; }`（`ipc.h:2198-2203`）in the
/// first two payload words; the return is that (read end, write end) pair.
pub fn pipe2_via(transport: &impl IpcTransport, flags: i32) -> Result<(i32, i32), Errno> {
    let mut message = crate::syscall::cleared_message();
    // SAFETY: `mess_lc_vfs_pipe2` 的两个 i32 域写在负载区偏移 0 与 8
    // （服务端 `syscalls.rs` 的 Pipe2 臂按同布局解码，互为见证）。
    unsafe {
        message.m_u.raw[0..4].copy_from_slice(&flags.to_le_bytes());
        message.m_u.raw[8..12].copy_from_slice(&flags.to_le_bytes());
    }
    perform_syscall(transport, vfs_endpoint(), VFS_CALL_PIPE2, &mut message)?;
    // SAFETY: 成功回复是 `mess_vfs_lc_fdpair`，fd0/fd1 在负载区前两字。
    let (fd0, fd1) = unsafe {
        (
            i32::from_le_bytes(message.m_u.raw[0..4].try_into().expect("four payload bytes")),
            i32::from_le_bytes(message.m_u.raw[4..8].try_into().expect("four payload bytes")),
        )
    };
    Ok((fd0, fd1))
}

/// Changes the process root directory (C: `chroot`,
/// `minix3/minix/lib/libc/sys/chroot.c:12-19`).
///
/// C loads the name inline into the message (`_loadname`, chroot.c:15);
/// this face reuses the open-existing payload shape — address, length,
/// then the path bytes themselves (see `open_existing_via`, where the
/// inline boundary and the server-side fetch for longer names live too).
/// The caller keeps the path buffer alive until the reply: a real server
/// reads it through the caller's address space, the way `chroot`'s
/// `loadname` made the message itself carry the bytes.
pub fn chroot_via(
    transport: &impl IpcTransport,
    name_address: u64,
    name_length_including_nul: usize,
) -> Result<(), Errno> {
    if name_length_including_nul == 0 {
        return Err(Errno::ENAMETOOLONG);
    }
    let mut packed = [0u8; core::mem::size_of::<OpenPathPayload>()];
    packed[0..8].copy_from_slice(&name_address.to_le_bytes());
    packed[8..16].copy_from_slice(&(name_length_including_nul as u64).to_le_bytes());
    // flags/mode lanes stay zero — chroot carries neither.
    if name_length_including_nul <= OPEN_PATH_INLINE_MAX {
        // SAFETY: the caller's path buffer lives in this same address space
        // (user library reads its own argument — C loadname.c:16-17 同型)。
        let path_bytes = unsafe {
            core::slice::from_raw_parts(name_address as *const u8, name_length_including_nul)
        };
        packed[24..24 + path_bytes.len()].copy_from_slice(path_bytes);
    }
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
/// 64-bit layout adjudication (LP64: the inline window is 32 bytes
/// including the NUL under the 56-byte payload invariant; longer paths
/// travel by the pointer lane and the server fetches them from the
/// caller's address space, exactly the split C's `copy_path` makes).
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

/// open-existing 内联容量（LP64 判例：指针 4→8 后 buf 由 C i386 的
/// `M_PATH_STRING_MAX` 40 收缩为 32；含 NUL 的总长 ≤ 32 才能内联）。
/// 超长路径走指针车道，由服务端 fetch（C `copy_path` 双支，
/// utility.c:31-32）——内联是优化不是上限。
pub const OPEN_PATH_INLINE_MAX: usize = 32;

/// open-existing 路径的 wire 语义（C `_loadname`，loadname.c:7-19）：
/// **地址与 NUL 含长度恒发送**（:15-17），路径 ≤ [`OPEN_PATH_INLINE_MAX`]
/// 字节（含 NUL）才内联进载荷 buf（:18 的 strcpy 是优化非语义；C 容量是
/// `M_PATH_STRING_MAX` 40，本线 LP64 载荷 56 字节不变量下内联窗为 32，
/// 99 篇 §1.2 判例）。超长路径不内联，由服务端从调用方地址空间取回
/// （C `copy_path` → `fetch_name`，utility.c:31-32/:60-90：上限 PATH_MAX、
/// 尾字节 NUL 校验）——超长与否是服务端裁决，客户端不发 `ENAMETOOLONG`。
pub fn open_existing_via(
    transport: &impl IpcTransport,
    name_address: u64,
    name_length_including_nul: usize,
    flags: i32,
) -> Result<i32, Errno> {
    if name_length_including_nul == 0 {
        return Err(Errno::ENAMETOOLONG);
    }
    let mut packed = [0u8; core::mem::size_of::<OpenPathPayload>()];
    packed[0..8].copy_from_slice(&name_address.to_le_bytes());
    packed[8..16].copy_from_slice(&(name_length_including_nul as u64).to_le_bytes());
    packed[16..20].copy_from_slice(&flags.to_le_bytes());
    // mode @20..24 = 0（非创建路径无 mode，C open.c:31 同填 0）。
    if name_length_including_nul <= OPEN_PATH_INLINE_MAX {
        // SAFETY: the caller's path buffer lives in this same address space
        // (user library reads its own argument — C loadname.c:18 同型)。
        let path_bytes = unsafe {
            core::slice::from_raw_parts(name_address as *const u8, name_length_including_nul)
        };
        packed[24..24 + path_bytes.len()].copy_from_slice(path_bytes);
    }
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

// ── 文件操作族（NL4 高优先批，2026-09-21）──────────────────────────────
//
// 十六个调用号，全部对应 `minix3/minix/include/minix/callnr.h:78-106` 的
// `VFS_BASE + N`；服务端 `os/servers/vfs/src/syscalls.rs` 的各臂早已就绪，
// 本批是纯客户端封装缺口（new_edge2.md NL4 侦察结论）。

/// `VFS_LINK (VFS_BASE + 6)` (`callnr.h:78`).
pub const VFS_CALL_LINK: i32 = 0x100 + 6;
/// `VFS_UNLINK (VFS_BASE + 7)` (`callnr.h:79`).
pub const VFS_CALL_UNLINK: i32 = 0x100 + 7;
/// `VFS_CHDIR (VFS_BASE + 8)` (`callnr.h:80`).
pub const VFS_CALL_CHDIR: i32 = 0x100 + 8;
/// `VFS_MKDIR (VFS_BASE + 9)` (`callnr.h:81`).
pub const VFS_CALL_MKDIR: i32 = 0x100 + 9;
/// `VFS_MKNOD (VFS_BASE + 10)` (`callnr.h:82`).
pub const VFS_CALL_MKNOD: i32 = 0x100 + 10;
/// `VFS_CHMOD (VFS_BASE + 11)` (`callnr.h:83`).
pub const VFS_CALL_CHMOD: i32 = 0x100 + 11;
/// `VFS_CHOWN (VFS_BASE + 12)` (`callnr.h:84`).
pub const VFS_CALL_CHOWN: i32 = 0x100 + 12;
/// `VFS_ACCESS (VFS_BASE + 15)` (`callnr.h:87`).
pub const VFS_CALL_ACCESS: i32 = 0x100 + 15;
/// `VFS_RENAME (VFS_BASE + 17)` (`callnr.h:89`).
pub const VFS_CALL_RENAME: i32 = 0x100 + 17;
/// `VFS_RMDIR (VFS_BASE + 18)` (`callnr.h:90`).
pub const VFS_CALL_RMDIR: i32 = 0x100 + 18;
/// `VFS_SYMLINK (VFS_BASE + 19)` (`callnr.h:91`).
pub const VFS_CALL_SYMLINK: i32 = 0x100 + 19;
/// `VFS_READLINK (VFS_BASE + 20)` (`callnr.h:92`).
pub const VFS_CALL_READLINK: i32 = 0x100 + 20;
/// `VFS_UMASK (VFS_BASE + 27)` (`callnr.h:99`).
pub const VFS_CALL_UMASK: i32 = 0x100 + 27;
/// `VFS_FCHDIR (VFS_BASE + 31)` (`callnr.h:103`).
pub const VFS_CALL_FCHDIR: i32 = 0x100 + 31;
/// `VFS_TRUNCATE (VFS_BASE + 33)` (`callnr.h:105`).
pub const VFS_CALL_TRUNCATE: i32 = 0x100 + 33;
/// `VFS_FTRUNCATE (VFS_BASE + 34)` (`callnr.h:106`).
pub const VFS_CALL_FTRUNCATE: i32 = 0x100 + 34;
/// `VFS_SYNC (VFS_BASE + 16)` (`callnr.h:88`).
pub const VFS_CALL_SYNC: i32 = 0x100 + 16;
/// `VFS_UTIMENS (VFS_BASE + 37)` (`callnr.h:109`).
pub const VFS_CALL_UTIMENS: i32 = 0x100 + 37;

/// `UTIME_NOW` — set the timestamp to the current time
/// (`sys/sys/stat.h:235`: `((1 << 30) - 1)`).
pub const UTIME_NOW: i64 = (1 << 30) - 1;
/// `UTIME_OMIT` — leave the timestamp unchanged
/// (`sys/sys/stat.h:236`: `((1 << 30) - 2)`).
pub const UTIME_OMIT: i64 = (1 << 30) - 2;
/// `AT_FDCWD` — relative paths resolve against the caller's working
/// directory (`sys/sys/fcntl.h:297`, consumed by
/// `minix3/minix/lib/libc/sys/utimensat.c:36`).
pub const AT_FDCWD: i32 = -100;
/// `AT_SYMLINK_NOFOLLOW` — do not follow a trailing symlink
/// (`sys/sys/fcntl.h:299`, used by `lutimens` in
/// `minix3/lib/libc/gen/utimens.c`).
pub const AT_SYMLINK_NOFOLLOW: i32 = 0x200;

/// FIFO file-type bit (`sys/sys/stat.h` `__S_IFIFO`, used by C `mkfifo`).
pub const S_IFIFO: u32 = 0o010000;

/// Six inline-path calls (unlink, rmdir, chdir, mkdir, chmod, access) share
/// one request shape: `mess_lc_vfs_path` — address @0, NUL-inclusive length
/// @8, flags @16, mode @20, inline path bytes @24.
///
/// C: each wrapper memsets the message, runs `_loadname` (address, length,
/// inline strcpy), and for mkdir/chmod/access alone sets
/// `m_lc_vfs_path.mode` — `mkdir.c`/`chmod.c` the permission bits,
/// `access.c` the `amode` (`R_OK|W_OK|X_OK` or `F_OK`). The `flags` lane
/// stays zero for all six; the three path-only calls also leave `mode`
/// zero. The mode lane's i32 type follows [`OpenPathPayload`]'s field.
///
/// The inline boundary is shared with [`open_existing_via`] (identical
/// layout, identical split): paths at or under [`OPEN_PATH_INLINE_MAX`]
/// bytes (NUL included) ride in the payload buffer; longer ones leave the
/// inline window zero and the server fetches them from the caller's
/// address space (C `copy_path` → `fetch_name`, utility.c:31-32) — the
/// same two branches C's six path-request servers run.
fn path_request_via(
    transport: &impl IpcTransport,
    call_number: i32,
    name_address: u64,
    name_length_including_nul: usize,
    mode: i32,
) -> Result<(), Errno> {
    if name_length_including_nul == 0 {
        return Err(Errno::ENAMETOOLONG);
    }
    let mut packed = [0u8; core::mem::size_of::<OpenPathPayload>()];
    packed[0..8].copy_from_slice(&name_address.to_le_bytes());
    packed[8..16].copy_from_slice(&(name_length_including_nul as u64).to_le_bytes());
    // flags @16..20 stays zero for all six calls (C memset, never written).
    packed[20..24].copy_from_slice(&mode.to_le_bytes());
    if name_length_including_nul <= OPEN_PATH_INLINE_MAX {
        // SAFETY: the caller's path buffer lives in this same address space
        // (user library reads its own argument — C loadname.c:16-17 同型)。
        let path_bytes = unsafe {
            core::slice::from_raw_parts(name_address as *const u8, name_length_including_nul)
        };
        packed[24..24 + path_bytes.len()].copy_from_slice(path_bytes);
    }
    let mut message = crate::syscall::cleared_message();
    crate::syscall::write_payload(&mut message, &packed);
    perform_syscall(transport, vfs_endpoint(), call_number, &mut message).map(|_| ())
}

/// Deletes a directory entry (C: `unlink`,
/// `minix3/minix/lib/libc/sys/unlink.c:9-18`).
pub fn unlink_via(
    transport: &impl IpcTransport,
    name_address: u64,
    name_length_including_nul: usize,
) -> Result<(), Errno> {
    path_request_via(transport, VFS_CALL_UNLINK, name_address, name_length_including_nul, 0)
}

/// Removes an empty directory (C: `rmdir`,
/// `minix3/minix/lib/libc/sys/rmdir.c:9-18`).
pub fn rmdir_via(
    transport: &impl IpcTransport,
    name_address: u64,
    name_length_including_nul: usize,
) -> Result<(), Errno> {
    path_request_via(transport, VFS_CALL_RMDIR, name_address, name_length_including_nul, 0)
}

/// Changes the working directory (C: `chdir`,
/// `minix3/minix/lib/libc/sys/chdir.c:9-18`).
pub fn chdir_via(
    transport: &impl IpcTransport,
    name_address: u64,
    name_length_including_nul: usize,
) -> Result<(), Errno> {
    path_request_via(transport, VFS_CALL_CHDIR, name_address, name_length_including_nul, 0)
}

/// Creates a directory (C: `mkdir`,
/// `minix3/minix/lib/libc/sys/mkdir.c:8-17`; the server applies the
/// process umask to the low permission bits, `open.c:583` 对位的
/// `do_mkdir` 臂).
pub fn mkdir_via(
    transport: &impl IpcTransport,
    name_address: u64,
    name_length_including_nul: usize,
    mode: u32,
) -> Result<(), Errno> {
    path_request_via(
        transport,
        VFS_CALL_MKDIR,
        name_address,
        name_length_including_nul,
        mode as i32,
    )
}

/// Sets a path's permission bits (C: `chmod`,
/// `minix3/minix/lib/libc/sys/chmod.c:8-17`).
pub fn chmod_via(
    transport: &impl IpcTransport,
    name_address: u64,
    name_length_including_nul: usize,
    mode: u32,
) -> Result<(), Errno> {
    path_request_via(
        transport,
        VFS_CALL_CHMOD,
        name_address,
        name_length_including_nul,
        mode as i32,
    )
}

/// Checks real-uid access bits without opening (C: `access`,
/// `minix3/minix/lib/libc/sys/access.c:8-17`; the `amode` word is
/// `R_OK|W_OK|X_OK` or `F_OK`, validated server-side at
/// `protect.c:199-233` 对位的 Access 臂).
pub fn access_via(
    transport: &impl IpcTransport,
    name_address: u64,
    name_length_including_nul: usize,
    amode: i32,
) -> Result<(), Errno> {
    path_request_via(transport, VFS_CALL_ACCESS, name_address, name_length_including_nul, amode)
}

/// Shared request shape of link, symlink, and rename:
/// `mess_lc_vfs_link` — first address @0, second @8, first NUL-inclusive
/// length @16, second @24 (`ipc.h:708-715`). Unlike the six above, these
/// three carry no inline bytes: the C wrappers pass both pointers and the
/// server fetches the names through the caller's address space
/// (`SysPathFetcher::fetch`, 服务端 Link/Symlink/Rename 臂同见证).
///
/// C sets exactly these four fields in `link.c`/`symlink.c`/`rename.c`
/// after a memset; no length pre-check happens client-side because no
/// inline window exists to overflow.
fn link_names_via(
    transport: &impl IpcTransport,
    call_number: i32,
    name1_address: u64,
    name1_length_including_nul: usize,
    name2_address: u64,
    name2_length_including_nul: usize,
) -> Result<(), Errno> {
    let mut message = crate::syscall::cleared_message();
    // SAFETY: the four lanes follow `mess_lc_vfs_link` field order; the
    // server's Link/Symlink/Rename arms decode the same offsets.
    unsafe {
        message.m_u.raw[0..8].copy_from_slice(&name1_address.to_le_bytes());
        message.m_u.raw[8..16].copy_from_slice(&name2_address.to_le_bytes());
        message.m_u.raw[16..24].copy_from_slice(&(name1_length_including_nul as u64).to_le_bytes());
        message.m_u.raw[24..32].copy_from_slice(&(name2_length_including_nul as u64).to_le_bytes());
    }
    perform_syscall(transport, vfs_endpoint(), call_number, &mut message).map(|_| ())
}

/// Creates a hard link (C: `link(name, name2)`,
/// `minix3/minix/lib/libc/sys/link.c:9-19`).
pub fn link_via(
    transport: &impl IpcTransport,
    name_address: u64,
    name_length_including_nul: usize,
    new_name_address: u64,
    new_name_length_including_nul: usize,
) -> Result<(), Errno> {
    link_names_via(
        transport,
        VFS_CALL_LINK,
        name_address,
        name_length_including_nul,
        new_name_address,
        new_name_length_including_nul,
    )
}

/// Creates a symbolic link (C: `symlink(name, name2)`,
/// `minix3/minix/lib/libc/sys/symlink.c:9-19`; `name` is the target text,
/// `name2` the link path being created).
pub fn symlink_via(
    transport: &impl IpcTransport,
    target_address: u64,
    target_length_including_nul: usize,
    link_path_address: u64,
    link_path_length_including_nul: usize,
) -> Result<(), Errno> {
    link_names_via(
        transport,
        VFS_CALL_SYMLINK,
        target_address,
        target_length_including_nul,
        link_path_address,
        link_path_length_including_nul,
    )
}

/// Moves a path (C: `rename(name, name2)`,
/// `minix3/minix/lib/libc/sys/rename.c:9-19`).
pub fn rename_via(
    transport: &impl IpcTransport,
    old_address: u64,
    old_length_including_nul: usize,
    new_address: u64,
    new_length_including_nul: usize,
) -> Result<(), Errno> {
    link_names_via(
        transport,
        VFS_CALL_RENAME,
        old_address,
        old_length_including_nul,
        new_address,
        new_length_including_nul,
    )
}

/// Sets a path's owner and group (C: `chown`,
/// `minix3/minix/lib/libc/sys/chown.c:9-20`).
///
/// `mess_lc_vfs_chown`（`ipc.h:611-619`）：name@0、len@8、fd@16、
/// owner@20、group@24。fd lane 是 fchown 复用的字段，path 半按 C memset
/// 保持 0（chown.c 只填 name/len/owner/group，服务端按调用号判别）。
pub fn chown_via(
    transport: &impl IpcTransport,
    name_address: u64,
    name_length_including_nul: usize,
    owner: u32,
    group: u32,
) -> Result<(), Errno> {
    let mut message = crate::syscall::cleared_message();
    // SAFETY: lanes follow `mess_lc_vfs_chown` field order; the server's
    // Chown|Fchown arm decodes the same offsets.
    unsafe {
        message.m_u.raw[0..8].copy_from_slice(&name_address.to_le_bytes());
        message.m_u.raw[8..16].copy_from_slice(&(name_length_including_nul as u64).to_le_bytes());
        message.m_u.raw[20..24].copy_from_slice(&owner.to_le_bytes());
        message.m_u.raw[24..28].copy_from_slice(&group.to_le_bytes());
    }
    perform_syscall(transport, vfs_endpoint(), VFS_CALL_CHOWN, &mut message).map(|_| ())
}

/// Sets a path's length (C: `truncate`,
/// `minix3/minix/lib/libc/sys/truncate.c:9-17`).
///
/// `mess_lc_vfs_truncate`（`ipc.h:894-902`）：offset@0、fd@8、name@16、
/// len@24。fd lane 是 ftruncate 复用的字段，path 半按 C memset 保持 0；
/// name 走地址由服务端 fetcher 取回。
pub fn truncate_via(
    transport: &impl IpcTransport,
    name_address: u64,
    name_length_including_nul: usize,
    offset: i64,
) -> Result<(), Errno> {
    let mut message = crate::syscall::cleared_message();
    // SAFETY: lanes follow `mess_lc_vfs_truncate` field order; the
    // server's Truncate arm decodes the same offsets.
    unsafe {
        message.m_u.raw[0..8].copy_from_slice(&offset.to_le_bytes());
        message.m_u.raw[16..24].copy_from_slice(&name_address.to_le_bytes());
        message.m_u.raw[24..32].copy_from_slice(&(name_length_including_nul as u64).to_le_bytes());
    }
    perform_syscall(transport, vfs_endpoint(), VFS_CALL_TRUNCATE, &mut message).map(|_| ())
}

/// Sets an open descriptor's length (C: `ftruncate`,
/// `minix3/minix/lib/libc/sys/ftruncate.c:9-16`; same payload struct as
/// [`truncate_via`], fd half).
pub fn ftruncate_via(transport: &impl IpcTransport, fd: i32, offset: i64) -> Result<(), Errno> {
    let mut message = crate::syscall::cleared_message();
    // SAFETY: offset@0、fd@8（`mess_lc_vfs_truncate` 的 fd 半）。
    unsafe {
        message.m_u.raw[0..8].copy_from_slice(&offset.to_le_bytes());
        message.m_u.raw[8..12].copy_from_slice(&fd.to_le_bytes());
    }
    perform_syscall(transport, vfs_endpoint(), VFS_CALL_FTRUNCATE, &mut message).map(|_| ())
}

/// Changes the working directory to an open descriptor's (C: `fchdir`,
/// `minix3/minix/lib/libc/sys/chdir.c:21-29`; `mess_lc_vfs_fchdir`
/// 只有一个 int fd @0，`ipc.h:640-644`).
pub fn fchdir_via(transport: &impl IpcTransport, fd: i32) -> Result<(), Errno> {
    let mut message = crate::syscall::cleared_message();
    // SAFETY: `mess_lc_vfs_fchdir { int fd; }` — descriptor at byte zero.
    unsafe {
        message.m_u.raw[0..4].copy_from_slice(&fd.to_le_bytes());
    }
    perform_syscall(transport, vfs_endpoint(), VFS_CALL_FCHDIR, &mut message).map(|_| ())
}

/// Sets the file-creation mask and reports the previous one (C: `umask`,
/// `minix3/minix/lib/libc/sys/umask.c:9-17`).
///
/// `mess_lc_vfs_umask`（`ipc.h:905-909`）只有一个 mode_t mask @0（4 字节
/// lane）。C 把回复的 message type 直接当旧 mask 读回（umask 是唯一经
/// m_type 值通道返回 mode_t 的文件调用），服务端 Umask 臂
/// （protect.c:186-190 对位）同形回复 `Ok(old)`。
pub fn umask_via(transport: &impl IpcTransport, mask: u32) -> Result<u32, Errno> {
    let mut message = crate::syscall::cleared_message();
    // SAFETY: `mess_lc_vfs_umask { mode_t mask; }` — mask at byte zero.
    unsafe {
        message.m_u.raw[0..4].copy_from_slice(&mask.to_le_bytes());
    }
    let old = perform_syscall(transport, vfs_endpoint(), VFS_CALL_UMASK, &mut message)?;
    Ok(old as u32)
}

/// Reads a symlink target into the caller's buffer (C: `readlink`,
/// `minix3/minix/lib/libc/sys/readlink.c:13-24`).
///
/// `mess_lc_vfs_readlink`（`ipc.h:785-792`）：name@0、namelen@8、buf@16、
/// bufsize@24——C 的 wire **没有内联路径字段**（i386 与 LP64 换算皆然，
/// 剩余载荷区是 padding），客户端只发四个 lane，路径由服务端从调用方
/// 地址空间取回（C `do_rdlink` 恒 `fetch_name`，link.c:494：上限
/// PATH_MAX、尾字节 NUL 校验）。此前 minix-rs 在 @32 起自造过 24 字节
/// 内联窗（`READLINK_INLINE_MAX`），与 C 形状不符且给长路径加了客户端
/// 上限——NL10 拆除，双侧恒指针。namelen 为 0 仍是本面的诚实拒绝
/// （C `strlen+1` 不可达 0）。
///
/// The reply's message type carries the byte count the server wrote into
/// `buf` (C: `_syscall` 的非负 m_type 直接作为 ssize_t 返回).
pub fn readlink_via(
    transport: &impl IpcTransport,
    name_address: u64,
    name_length_including_nul: usize,
    buffer_address: u64,
    buffer_size: usize,
) -> Result<usize, Errno> {
    if name_length_including_nul == 0 {
        return Err(Errno::ENAMETOOLONG);
    }
    let mut message = crate::syscall::cleared_message();
    // SAFETY: lanes follow `mess_lc_vfs_readlink` field order; the server's
    // Readlink arm decodes name@0, name_len@8, buf@16, bufsize@24 and
    // fetches the path through the caller's address space (C link.c:494).
    unsafe {
        message.m_u.raw[0..8].copy_from_slice(&name_address.to_le_bytes());
        message.m_u.raw[8..16].copy_from_slice(&(name_length_including_nul as u64).to_le_bytes());
        message.m_u.raw[16..24].copy_from_slice(&buffer_address.to_le_bytes());
        message.m_u.raw[24..32].copy_from_slice(&(buffer_size as u64).to_le_bytes());
    }
    let written = perform_syscall(transport, vfs_endpoint(), VFS_CALL_READLINK, &mut message)?;
    Ok(written as usize)
}

/// Creates a special file node (C: `mknod`,
/// `minix3/minix/lib/libc/sys/mknod.c:9-19`).
///
/// `mess_lc_vfs_mknod`（`ipc.h:736-744`）：device@0（dev_t，8 字节）、
/// name@8、len@16、mode@24（mode_t，4 字节 lane）。mode 高位携带文件
/// 类型（`S_IFIFO` 等），服务端只放行非超级用户的 FIFO（open.c:530-531
/// 对位的 Mknod 臂）；name 走地址由服务端 fetcher 取回。
pub fn mknod_via(
    transport: &impl IpcTransport,
    name_address: u64,
    name_length_including_nul: usize,
    mode: u32,
    device: u64,
) -> Result<(), Errno> {
    let mut message = crate::syscall::cleared_message();
    // SAFETY: lanes follow `mess_lc_vfs_mknod` field order; the server's
    // Mknod arm decodes device@0, name@8, len@16, mode@24.
    unsafe {
        message.m_u.raw[0..8].copy_from_slice(&device.to_le_bytes());
        message.m_u.raw[8..16].copy_from_slice(&name_address.to_le_bytes());
        message.m_u.raw[16..24].copy_from_slice(&(name_length_including_nul as u64).to_le_bytes());
        message.m_u.raw[24..28].copy_from_slice(&mode.to_le_bytes());
    }
    perform_syscall(transport, vfs_endpoint(), VFS_CALL_MKNOD, &mut message).map(|_| ())
}

/// Flushes all mounted file systems' dirty state (C: `sync`,
/// `minix3/minix/lib/libc/sys/sync.c:12-17` — a cleared message, return
/// value discarded by the C wrapper itself).
///
/// The server's `Sync | Fsync` arm (`syscalls.rs:1306`) walks every
/// mount; the reply type is the errno face only.
pub fn sync_via(transport: &impl IpcTransport) -> Result<(), Errno> {
    let mut message = crate::syscall::cleared_message();
    perform_syscall(transport, vfs_endpoint(), VFS_CALL_SYNC, &mut message).map(|_| ())
}

/// Shared packing for the two `VFS_UTIMENS` shapes (see
/// [`utimensat_via`] and [`futimens_via`]).
///
/// Wire: `mess_vfs_utimens` (`ipc.h:2355-2367`, LP64): atime i64@0,
/// mtime i64@8, ansec i64@16, mnsec i64@24, len u64@32, name u64@40,
/// fd i32@48, flags i32@52 — matching the server Utimens arm decode
/// (`syscalls.rs:3241-3264`) byte for byte. `name_address == 0` selects
/// the fd half (C `futimens`), otherwise the server fetches the path
/// through `SysPathFetcher` (address-based, no inline window).
/// `ansec`/`mnsec` carry either nanoseconds or the `UTIME_NOW` /
/// `UTIME_OMIT` sentinels — both are server-interpreted, passed through.
#[allow(clippy::too_many_arguments)] // 8 payload lanes, one per C struct field
fn utimens_request_via(
    transport: &impl IpcTransport,
    name_address: u64,
    name_length_including_nul: u64,
    atime_seconds: i64,
    atime_nanoseconds: i64,
    mtime_seconds: i64,
    mtime_nanoseconds: i64,
    fd: i32,
    flags: i32,
) -> Result<(), Errno> {
    let mut message = crate::syscall::cleared_message();
    // SAFETY: lanes follow `mess_vfs_utimens` field order; the server's
    // Utimens arm decodes atime@0, mtime@8, ansec@16, mnsec@24, len@32,
    // name@40, fd@48, flags@52.
    unsafe {
        message.m_u.raw[0..8].copy_from_slice(&atime_seconds.to_le_bytes());
        message.m_u.raw[8..16].copy_from_slice(&mtime_seconds.to_le_bytes());
        message.m_u.raw[16..24].copy_from_slice(&atime_nanoseconds.to_le_bytes());
        message.m_u.raw[24..32].copy_from_slice(&mtime_nanoseconds.to_le_bytes());
        message.m_u.raw[32..40].copy_from_slice(&name_length_including_nul.to_le_bytes());
        message.m_u.raw[40..48].copy_from_slice(&name_address.to_le_bytes());
        message.m_u.raw[48..52].copy_from_slice(&fd.to_le_bytes());
        message.m_u.raw[52..56].copy_from_slice(&flags.to_le_bytes());
    }
    perform_syscall(transport, vfs_endpoint(), VFS_CALL_UTIMENS, &mut message).map(|_| ())
}

/// Sets a path's access and modification times (C: `utimensat`,
/// `minix3/minix/lib/libc/sys/utimensat.c:30-52`).
///
/// The C client-layer guards are reproduced before any round trip:
/// empty name is `ENOENT`, a relative name with a directory fd other
/// than `AT_FDCWD` is `EINVAL` ("Not supported" — the wire carries the
/// fd only for the fd half), and flags beyond `SHRT_MAX` is `EINVAL`.
/// `NULL` timespec pairs are a caller concern: pass `UTIME_NOW` lanes
/// explicitly (the C defaulting to now lives in the libc wrapper, see
/// the top-level `utimensat`).
#[allow(clippy::too_many_arguments)] // mirrors the C wire one-for-one
pub fn utimensat_via(
    transport: &impl IpcTransport,
    dirfd: i32,
    name_address: u64,
    name_length_including_nul: usize,
    atime_seconds: i64,
    atime_nanoseconds: i64,
    mtime_seconds: i64,
    mtime_nanoseconds: i64,
    flags: i32,
) -> Result<(), Errno> {
    if name_length_including_nul <= 1 {
        // C utimensat.c:32-35: name[0]=='\0' — POSIX requires ENOENT.
        return Err(Errno::ENOENT);
    }
    let first = // SAFETY: the caller promises a NUL-inclusive name at
        // this address; only the first byte is inspected, matching C.
        unsafe { (name_address as *const u8).read() };
    if first != b'/' && dirfd != AT_FDCWD {
        // C utimensat.c:36-39.
        return Err(Errno::EINVAL);
    }
    if flags > i16::MAX as i32 || flags < 0 {
        // C utimensat.c:40-42: `(unsigned)flags > SHRT_MAX` (the cast
        // also rejects negatives).
        return Err(Errno::EINVAL);
    }
    utimens_request_via(
        transport,
        name_address,
        name_length_including_nul as u64,
        atime_seconds,
        atime_nanoseconds,
        mtime_seconds,
        mtime_nanoseconds,
        dirfd,
        flags,
    )
}

/// Sets an open file's access and modification times (C: `futimens`,
/// `minix3/minix/lib/libc/sys/futimens.c:1-20` — same wire, name
/// pointer NULL so the server takes its fd half, flags zero).
pub fn futimens_via(
    transport: &impl IpcTransport,
    fd: i32,
    atime_seconds: i64,
    atime_nanoseconds: i64,
    mtime_seconds: i64,
    mtime_nanoseconds: i64,
) -> Result<(), Errno> {
    utimens_request_via(
        transport,
        0,
        0,
        atime_seconds,
        atime_nanoseconds,
        mtime_seconds,
        mtime_nanoseconds,
        fd,
        0,
    )
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
        assert_eq!(VFS_CALL_PIPE2, 0x11A);
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
    use alloc::vec;
    use core::mem::offset_of;
    use crate::ipc::CannedTransport;
    use minix_types::Message;

    fn reply_with_type(message_type: i32) -> Message {
        let mut message = Message::zeroed();
        message.m_type = message_type;
        message
    }

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

    /// 内联边界与指针车道分界：恰好 32（含 NUL）内联拷贝；33 起不再读
    /// 调用方缓冲，buf 保持零、地址/长度 lane 照发（C loadname.c:15-18
    /// 的 strcpy 跳过同型；服务端 fetch 见 utility.c:31-32）。
    #[test]
    fn test_open_path_inline_boundary_and_pointer_lane() {
        // 恰好 32（含 NUL）：可内联。
        assert_eq!(31, OPEN_PATH_INLINE_MAX - 1);
        let mut inline_transport = CannedTransport::new();
        inline_transport.reply_sendrec(Ok(reply_with_type(3)));
        let short = b"/tmp/abcdefghijklmnopqrstuvwxyz\0";
        assert_eq!(short.len(), OPEN_PATH_INLINE_MAX);
        assert_eq!(
            open_existing_via(&inline_transport, short.as_ptr() as u64, short.len(), 0),
            Ok(3)
        );
        let sent = inline_transport.sent.borrow();
        // SAFETY: test-only read-back of the outgoing wire bytes.
        let raw = unsafe { &sent[0].1.m_u.raw };
        assert_eq!(&raw[24..24 + short.len()], short, "内联支拷贝路径字节");

        // 超界 33：不发 ENAMETOOLONG，照发往返——buf 零、lane 在。
        let mut transport = CannedTransport::new();
        transport.reply_sendrec(Ok(reply_with_type(4)));
        let long = [b'a'; OPEN_PATH_INLINE_MAX + 1];
        assert_eq!(
            open_existing_via(&transport, long.as_ptr() as u64, long.len(), 0x1),
            Ok(4)
        );
        let sent = transport.sent.borrow();
        assert_eq!(sent[0].1.m_type, VFS_CALL_OPEN);
        // SAFETY: test-only read-back of the outgoing wire bytes.
        let raw = unsafe { &sent[0].1.m_u.raw };
        assert_eq!(raw[0..8].to_vec(), (long.as_ptr() as u64).to_le_bytes(), "name@0");
        assert_eq!(raw[8..16].to_vec(), (long.len() as u64).to_le_bytes(), "len@8");
        assert_eq!(raw[16..20].to_vec(), 0x1i32.to_le_bytes(), "flags@16");
        assert_eq!(raw[24..56].to_vec(), vec![0; 32], "超长不内联，buf 保持零");
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
mod pipe_wire_tests {
    use super::*;
    use crate::ipc::CannedTransport;
    use minix_types::Message;

    /// OK 回复携带 `mess_vfs_lc_fdpair { int fd0; int fd1; }`（ipc.h:2198-2203）
    /// 于负载区前两字（服务端 main_loop.rs 的 Pipe2 收尾同布局写入）。
    fn reply_fdpair(fd0: i32, fd1: i32) -> Message {
        let mut message = Message::zeroed();
        message.m_type = 0;
        // SAFETY: test-only canned reply, fdpair 前两字。
        unsafe {
            message.m_u.raw[0..4].copy_from_slice(&fd0.to_le_bytes());
            message.m_u.raw[4..8].copy_from_slice(&fd1.to_le_bytes());
        }
        message
    }

    fn reply_with_type(message_type: i32) -> Message {
        let mut message = Message::zeroed();
        message.m_type = message_type;
        message
    }

    /// 请求腿：flags 同时写 @0 与 @8（C libc 双写向后兼容，pipe.c:18-19），
    /// 调用号 VFS_PIPE2 = VFS_BASE + 26（callnr.h:98），目的端 VFS。
    /// 回复腿：fd 对从 fdpair 读出。
    #[test]
    fn test_pipe2_wire_roundtrip_both_flag_fields() {
        let mut transport = CannedTransport::new();
        transport.reply_sendrec(Ok(reply_fdpair(3, 4)));
        assert_eq!(pipe2_via(&transport, 0o2000000), Ok((3, 4)));
        let sent = transport.sent.borrow();
        assert_eq!(sent.len(), 1);
        assert_eq!(sent[0].0, vfs_endpoint());
        assert_eq!(sent[0].1.m_type, VFS_CALL_PIPE2);
        // SAFETY: test-only payload read-back of the outgoing wire bytes.
        let raw = unsafe { &sent[0].1.m_u.raw };
        assert_eq!(&raw[0..4], &0o2000000i32.to_le_bytes(), "flags@0");
        assert_eq!(&raw[4..8], &[0; 4], "_unused@4 保持零");
        assert_eq!(&raw[8..12], &0o2000000i32.to_le_bytes(), "oflags@8 同值双写");
    }

    /// `pipe()` 顶层形态 = `pipe2(_, 0)`（C pipe.c:30-33）：flags 两字段皆零。
    #[test]
    fn test_pipe2_zero_flags_matches_pipe_shim() {
        let mut transport = CannedTransport::new();
        transport.reply_sendrec(Ok(reply_fdpair(5, 6)));
        assert_eq!(pipe2_via(&transport, 0), Ok((5, 6)));
        let sent = transport.sent.borrow();
        // SAFETY: test-only payload read-back of the outgoing wire bytes.
        let raw = unsafe { &sent[0].1.m_u.raw };
        assert_eq!(&raw[0..12], &[0u8; 12]);
    }

    /// 服务端错误（如 PFS 未挂载时的 EIO，syscalls.rs Pipe2 臂 fail-closed）
    /// 以负 m_type 回来，fd 对不被读出。
    #[test]
    fn test_pipe2_error_lane_returns_errno() {
        let mut transport = CannedTransport::new();
        transport.reply_sendrec(Ok(reply_with_type(-minix_types::EIO)));
        assert_eq!(pipe2_via(&transport, 0), Err(Errno::EIO));
    }
}

#[cfg(test)]
mod chroot_wire_tests {
    use super::*;
    use alloc::vec;
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

    /// 越界路径与 open 同构：超内联不本地拒绝，buf 保持零、地址/长度
    /// lane 照发（指针车道，服务端 fetch）；0 长仍零回合拒绝。
    #[test]
    fn test_chroot_long_path_travels_by_pointer_lane() {
        let mut transport = CannedTransport::new();
        transport.reply_sendrec(Ok(reply_with_type(0)));
        let long = [b'a'; OPEN_PATH_INLINE_MAX + 1];
        assert_eq!(
            chroot_via(&transport, long.as_ptr() as u64, long.len()),
            Ok(())
        );
        let sent = transport.sent.borrow();
        assert_eq!(sent.len(), 1, "超长照发往返");
        // SAFETY: test-only read-back of the outgoing wire bytes.
        let raw = unsafe { &sent[0].1.m_u.raw };
        assert_eq!(raw[0..8].to_vec(), (long.as_ptr() as u64).to_le_bytes(), "name@0");
        assert_eq!(raw[8..16].to_vec(), (long.len() as u64).to_le_bytes(), "len@8");
        assert_eq!(raw[24..56].to_vec(), vec![0; 32], "超长不内联");

        let quiet = CannedTransport::new();
        assert_eq!(chroot_via(&quiet, long.as_ptr() as u64, 0), Err(Errno::ENAMETOOLONG));
        assert!(quiet.sent.borrow().is_empty(), "0 长零回合拒绝");
    }
}

#[cfg(test)]
mod fileop_wire_tests {
    use super::*;
    use alloc::{vec, vec::Vec};
    use crate::ipc::CannedTransport;
    use minix_types::Message;

    fn reply_with_type(message_type: i32) -> Message {
        let mut message = Message::zeroed();
        message.m_type = message_type;
        message
    }

    fn last_sent(transport: &CannedTransport) -> (minix_types::Endpoint, Message) {
        transport.sent.borrow().last().cloned().unwrap()
    }

    /// 请求腿的 lane 读出（测试断言专用，与 chroot wire 测试同法）。
    fn raw_lane(msg: &Message, range: core::ops::Range<usize>) -> Vec<u8> {
        // SAFETY: test-only read-back of the outgoing wire bytes.
        unsafe { msg.m_u.raw[range].to_vec() }
    }

    /// C 绝对值 pin：16 个调用号对 `callnr.h:78-106` 逐一定死。
    #[test]
    fn test_fileop_call_numbers_match_c() {
        assert_eq!(VFS_CALL_LINK, 0x100 + 6);
        assert_eq!(VFS_CALL_UNLINK, 0x100 + 7);
        assert_eq!(VFS_CALL_CHDIR, 0x100 + 8);
        assert_eq!(VFS_CALL_MKDIR, 0x100 + 9);
        assert_eq!(VFS_CALL_MKNOD, 0x100 + 10);
        assert_eq!(VFS_CALL_CHMOD, 0x100 + 11);
        assert_eq!(VFS_CALL_CHOWN, 0x100 + 12);
        assert_eq!(VFS_CALL_ACCESS, 0x100 + 15);
        assert_eq!(VFS_CALL_RENAME, 0x100 + 17);
        assert_eq!(VFS_CALL_RMDIR, 0x100 + 18);
        assert_eq!(VFS_CALL_SYMLINK, 0x100 + 19);
        assert_eq!(VFS_CALL_READLINK, 0x100 + 20);
        assert_eq!(VFS_CALL_UMASK, 0x100 + 27);
        assert_eq!(VFS_CALL_FCHDIR, 0x100 + 31);
        assert_eq!(VFS_CALL_TRUNCATE, 0x100 + 33);
        assert_eq!(VFS_CALL_FTRUNCATE, 0x100 + 34);
    }

    /// mkdir 请求腿全 lane：address@0、len@8、flags@16 零、mode@20、
    /// 内联字节@24（C mkdir.c：memset 后只经 mode 与 loadname）。
    #[test]
    fn test_mkdir_wire_lanes_and_inline_path() {
        let mut transport = CannedTransport::new();
        transport.reply_sendrec(Ok(reply_with_type(0)));
        let path = b"/tmp/ndir\0";
        assert_eq!(mkdir_via(&transport, path.as_ptr() as u64, path.len(), 0o755), Ok(()));
        let (dest, sent) = last_sent(&transport);
        assert_eq!(dest, vfs_endpoint());
        assert_eq!(sent.m_type, VFS_CALL_MKDIR);
        assert_eq!(raw_lane(&sent, 0..8), (path.as_ptr() as u64).to_le_bytes(), "name@0");
        assert_eq!(raw_lane(&sent, 8..16), (path.len() as u64).to_le_bytes(), "len@8");
        assert_eq!(raw_lane(&sent, 16..20), [0; 4], "flags@16 保持零");
        assert_eq!(raw_lane(&sent, 20..24), 0o755u32.to_le_bytes(), "mode@20");
        assert_eq!(raw_lane(&sent, 24..24 + path.len()), path.to_vec(), "inline@24");
    }

    /// unlink/rmdir/chdir 共用同一形状，mode lane 也保持零（C 侧只
    /// memset + loadname）。错误经负 m_type 上行。
    #[test]
    fn test_path_only_calls_carry_zero_mode_and_error_lane_travels() {
        let mut transport = CannedTransport::new();
        transport.reply_sendrec(Ok(reply_with_type(0)));
        transport.reply_sendrec(Ok(reply_with_type(0)));
        transport.reply_sendrec(Ok(reply_with_type(0)));
        let path = b"/tmp/f\0";
        assert_eq!(unlink_via(&transport, path.as_ptr() as u64, path.len()), Ok(()));
        assert_eq!(rmdir_via(&transport, path.as_ptr() as u64, path.len()), Ok(()));
        assert_eq!(chdir_via(&transport, path.as_ptr() as u64, path.len()), Ok(()));
        let types: Vec<i32> = transport.sent.borrow().iter().map(|(_, m)| m.m_type).collect();
        assert_eq!(types, vec![VFS_CALL_UNLINK, VFS_CALL_RMDIR, VFS_CALL_CHDIR]);
        for (_, msg) in transport.sent.borrow().iter() {
            assert_eq!(raw_lane(msg, 16..24), [0; 8], "flags/mode 全零");
        }
        // 错误腿：ENOENT 以负 m_type 回来。
        transport.reply_sendrec(Ok(reply_with_type(-minix_types::ENOENT)));
        assert_eq!(unlink_via(&transport, path.as_ptr() as u64, path.len()), Err(Errno::ENOENT));
    }

    /// chmod 与 access 复用 mode lane：chmod 传新权限位，access 传
    /// amode（C 两文件同写 `m_lc_vfs_path.mode`）。
    #[test]
    fn test_chmod_and_access_share_mode_lane() {
        let mut transport = CannedTransport::new();
        transport.reply_sendrec(Ok(reply_with_type(0)));
        transport.reply_sendrec(Ok(reply_with_type(0)));
        let path = b"/x\0";
        assert_eq!(chmod_via(&transport, path.as_ptr() as u64, path.len(), 0o644), Ok(()));
        assert_eq!(access_via(&transport, path.as_ptr() as u64, path.len(), 4 | 2), Ok(()));
        let sent = transport.sent.borrow();
        assert_eq!(raw_lane(&sent[0].1, 20..24), 0o644u32.to_le_bytes(), "mode@20");
        assert_eq!(raw_lane(&sent[1].1, 20..24), 6i32.to_le_bytes(), "amode@20");
        assert_eq!(sent[1].1.m_type, VFS_CALL_ACCESS);
    }

    /// 内联族超长走指针车道：33 字节起不内联、lane 照发（服务端 fetch，
    /// utility.c:31-32）；mode lane 在超长支也保持（mkdir 的 0o700 在）；
    /// 0 长仍零回合拒绝（本面 API 契约，C strlen+1 不可达）。
    #[test]
    fn test_path_family_long_path_travels_by_pointer_lane() {
        let mut transport = CannedTransport::new();
        transport.reply_sendrec(Ok(reply_with_type(0)));
        transport.reply_sendrec(Ok(reply_with_type(0)));
        let long = [b'a'; OPEN_PATH_INLINE_MAX + 1];
        assert_eq!(
            mkdir_via(&transport, long.as_ptr() as u64, long.len(), 0o700),
            Ok(())
        );
        assert_eq!(unlink_via(&transport, long.as_ptr() as u64, long.len()), Ok(()));
        let sent = transport.sent.borrow();
        assert_eq!(sent.len(), 2, "超长照发往返");
        assert_eq!(sent[0].1.m_type, VFS_CALL_MKDIR);
        // SAFETY: test-only read-back of the outgoing wire bytes.
        let m = unsafe { &sent[0].1.m_u.raw };
        assert_eq!(m[0..8].to_vec(), (long.as_ptr() as u64).to_le_bytes(), "name@0");
        assert_eq!(m[8..16].to_vec(), (long.len() as u64).to_le_bytes(), "len@8");
        assert_eq!(m[20..24].to_vec(), 0o700u32.to_le_bytes(), "mode@20 超长支仍在");
        assert_eq!(m[24..56].to_vec(), vec![0; 32], "超长不内联");
        // SAFETY: test-only read-back of the outgoing wire bytes.
        let u = unsafe { &sent[1].1.m_u.raw };
        assert_eq!(u[24..56].to_vec(), vec![0; 32], "unlink 超长同样不内联");

        let quiet = CannedTransport::new();
        assert_eq!(access_via(&quiet, long.as_ptr() as u64, 0, 0), Err(Errno::ENAMETOOLONG));
        assert!(quiet.sent.borrow().is_empty(), "0 长零回合拒绝");
    }

    /// link 四 lane：name1@0、name2@8、len1@16、len2@24（地址制，无
    /// 内联字节；symlink/rename 共用 helper，各以调用号区分）。
    #[test]
    fn test_link_family_carries_both_names_by_address() {
        let mut transport = CannedTransport::new();
        transport.reply_sendrec(Ok(reply_with_type(0)));
        transport.reply_sendrec(Ok(reply_with_type(0)));
        transport.reply_sendrec(Ok(reply_with_type(0)));
        let a = b"/p/a\0";
        let b = b"/p/b\0";
        assert_eq!(
            link_via(&transport, a.as_ptr() as u64, a.len(), b.as_ptr() as u64, b.len()),
            Ok(())
        );
        assert_eq!(
            symlink_via(&transport, a.as_ptr() as u64, a.len(), b.as_ptr() as u64, b.len()),
            Ok(())
        );
        assert_eq!(
            rename_via(&transport, a.as_ptr() as u64, a.len(), b.as_ptr() as u64, b.len()),
            Ok(())
        );
        let sent: Vec<Message> = transport.sent.borrow().iter().map(|(_, m)| *m).collect();
        assert_eq!(
            Vec::from(sent.iter().map(|m| m.m_type).collect::<Vec<_>>().as_slice()),
            vec![VFS_CALL_LINK, VFS_CALL_SYMLINK, VFS_CALL_RENAME]
        );
        for msg in sent.iter() {
            // SAFETY: test-only read-back of the outgoing wire bytes.
            let raw = unsafe { &msg.m_u.raw };
            assert_eq!(raw[0..8].to_vec(), (a.as_ptr() as u64).to_le_bytes(), "name1@0");
            assert_eq!(raw[8..16].to_vec(), (b.as_ptr() as u64).to_le_bytes(), "name2@8");
            assert_eq!(raw[16..24].to_vec(), (a.len() as u64).to_le_bytes(), "len1@16");
            assert_eq!(raw[24..32].to_vec(), (b.len() as u64).to_le_bytes(), "len2@24");
            assert_eq!(raw[32..56].to_vec(), vec![0; 24], "尾部保持零");
        }
    }

    /// chown：name@0、len@8、fd@16 零（path 半按 C memset 不写）、
    /// owner@20、group@24。
    #[test]
    fn test_chown_wire_lanes() {
        let mut transport = CannedTransport::new();
        transport.reply_sendrec(Ok(reply_with_type(0)));
        let path = b"/f\0";
        assert_eq!(chown_via(&transport, path.as_ptr() as u64, path.len(), 1000, 1001), Ok(()));
        let (_, sent) = last_sent(&transport);
        assert_eq!(sent.m_type, VFS_CALL_CHOWN);
        assert_eq!(raw_lane(&sent, 0..8), (path.as_ptr() as u64).to_le_bytes(), "name@0");
        assert_eq!(raw_lane(&sent, 8..16), (path.len() as u64).to_le_bytes(), "len@8");
        assert_eq!(raw_lane(&sent, 16..20), [0; 4], "fd@16 零（path 半）");
        assert_eq!(raw_lane(&sent, 20..24), 1000u32.to_le_bytes(), "owner@20");
        assert_eq!(raw_lane(&sent, 24..28), 1001u32.to_le_bytes(), "group@24");
    }

    /// truncate：offset@0、fd@8 零（path 半）、name@16、len@24；
    /// ftruncate：offset@0、fd@8。
    #[test]
    fn test_truncate_family_wire_lanes() {
        let mut transport = CannedTransport::new();
        transport.reply_sendrec(Ok(reply_with_type(0)));
        transport.reply_sendrec(Ok(reply_with_type(0)));
        let path = b"/f\0";
        assert_eq!(
            truncate_via(&transport, path.as_ptr() as u64, path.len(), -1),
            Ok(())
        );
        assert_eq!(ftruncate_via(&transport, 7, 4096), Ok(()));
        let sent = transport.sent.borrow();
        assert_eq!(sent[0].1.m_type, VFS_CALL_TRUNCATE);
        // SAFETY: test-only read-back of the outgoing wire bytes.
        let t = unsafe { &sent[0].1.m_u.raw };
        assert_eq!(t[0..8].to_vec(), (-1i64).to_le_bytes(), "offset@0");
        assert_eq!(t[8..12].to_vec(), [0; 4], "fd@8 零（path 半）");
        assert_eq!(t[16..24].to_vec(), (path.as_ptr() as u64).to_le_bytes(), "name@16");
        assert_eq!(t[24..32].to_vec(), (path.len() as u64).to_le_bytes(), "len@24");
        assert_eq!(sent[1].1.m_type, VFS_CALL_FTRUNCATE);
        // SAFETY: test-only read-back of the outgoing wire bytes.
        let f = unsafe { &sent[1].1.m_u.raw };
        assert_eq!(f[0..8].to_vec(), 4096i64.to_le_bytes(), "offset@0");
        assert_eq!(f[8..12].to_vec(), 7i32.to_le_bytes(), "fd@8");
    }

    /// fchdir 只有一个 int fd lane @0（`ipc.h:640-644`）。
    #[test]
    fn test_fchdir_wire_lane() {
        let mut transport = CannedTransport::new();
        transport.reply_sendrec(Ok(reply_with_type(0)));
        assert_eq!(fchdir_via(&transport, 3), Ok(()));
        let (_, sent) = last_sent(&transport);
        assert_eq!(sent.m_type, VFS_CALL_FCHDIR);
        assert_eq!(raw_lane(&sent, 0..4), 3i32.to_le_bytes());
        assert_eq!(raw_lane(&sent, 4..56), vec![0; 52], "其余保持零");
    }

    /// umask：mask@0（4 字节 lane）；旧 mask 经回复 m_type 值通道带回
    /// （服务端 `Ok(old as i32)`，protect.c:186-190 对位）。
    #[test]
    fn test_umask_mask_lane_and_old_value_reply() {
        let mut transport = CannedTransport::new();
        transport.reply_sendrec(Ok(reply_with_type(0o022)));
        assert_eq!(umask_via(&transport, 0o077), Ok(0o022));
        let (_, sent) = last_sent(&transport);
        assert_eq!(sent.m_type, VFS_CALL_UMASK);
        assert_eq!(raw_lane(&sent, 0..4), 0o077u32.to_le_bytes(), "mask@0");
    }

    /// readlink：name@0、namelen@8、buf@16、bufsize@24 四 lane，@32 起
    /// 是 C padding 保持零（wire 无内联字段，C readlink.c:17-20 同形）；
    /// 写字节数经回复 m_type 带回；路径长度无客户端上限（服务端
    /// `do_rdlink` 恒 fetch_name，link.c:494）。
    #[test]
    fn test_readlink_wire_and_value_reply() {
        let mut transport = CannedTransport::new();
        transport.reply_sendrec(Ok(reply_with_type(6)));
        let path = b"/link\0";
        let mut buf = [0u8; 64];
        assert_eq!(
            readlink_via(
                &transport,
                path.as_ptr() as u64,
                path.len(),
                buf.as_mut_ptr() as u64,
                buf.len(),
            ),
            Ok(6)
        );
        let (_, sent) = last_sent(&transport);
        assert_eq!(sent.m_type, VFS_CALL_READLINK);
        assert_eq!(raw_lane(&sent, 0..8), (path.as_ptr() as u64).to_le_bytes(), "name@0");
        assert_eq!(raw_lane(&sent, 8..16), (path.len() as u64).to_le_bytes(), "namelen@8");
        assert_eq!(raw_lane(&sent, 24..32), (buf.len() as u64).to_le_bytes(), "bufsize@24");
        assert_eq!(
            raw_lane(&sent, 32..56),
            vec![0; 24],
            "padding@32 保持零（无内联窗）"
        );
        // 长名照发（此前 24 字节自造窗已拆）。
        transport.reply_sendrec(Ok(reply_with_type(0)));
        let long = [b'a'; 40];
        assert_eq!(
            readlink_via(&transport, long.as_ptr() as u64, long.len(), 0, 0),
            Ok(0)
        );
    }

    /// mknod：device@0（8B）、name@8、len@16、mode@24（4B lane）。
    #[test]
    fn test_mknod_wire_lanes() {
        let mut transport = CannedTransport::new();
        transport.reply_sendrec(Ok(reply_with_type(0)));
        let path = b"/dev/fifo\0";
        let mode = 0o600 | S_IFIFO;
        assert_eq!(
            mknod_via(&transport, path.as_ptr() as u64, path.len(), mode, 0),
            Ok(())
        );
        let (_, sent) = last_sent(&transport);
        assert_eq!(sent.m_type, VFS_CALL_MKNOD);
        assert_eq!(raw_lane(&sent, 0..8), 0u64.to_le_bytes(), "device@0");
        assert_eq!(raw_lane(&sent, 8..16), (path.as_ptr() as u64).to_le_bytes(), "name@8");
        assert_eq!(raw_lane(&sent, 16..24), (path.len() as u64).to_le_bytes(), "len@16");
        assert_eq!(raw_lane(&sent, 24..28), mode.to_le_bytes(), "mode@24");
        assert_eq!(raw_lane(&sent, 28..56), vec![0; 28], "尾部保持零");
    }
}

#[cfg(test)]
mod time_sync_wire_tests {
    use super::*;
    use alloc::{vec, vec::Vec};
    use crate::ipc::CannedTransport;
    use minix_types::Message;

    fn reply_with_type(message_type: i32) -> Message {
        let mut message = Message::zeroed();
        message.m_type = message_type;
        message
    }

    fn last_sent(transport: &CannedTransport) -> (minix_types::Endpoint, Message) {
        transport.sent.borrow().last().cloned().unwrap()
    }

    fn raw_lane(msg: &Message, range: core::ops::Range<usize>) -> Vec<u8> {
        // SAFETY: test-only read-back of the outgoing wire bytes.
        unsafe { msg.m_u.raw[range].to_vec() }
    }

    #[test]
    fn test_sync_utimens_call_numbers_match_c() {
        // C: callnr.h:88/109（VFS_BASE + 16 / + 37）。
        assert_eq!(VFS_CALL_SYNC, 0x100 + 16);
        assert_eq!(VFS_CALL_UTIMENS, 0x100 + 37);
        // sys/sys/stat.h:235-236、sys/sys/fcntl.h:297/299。
        assert_eq!(UTIME_NOW, (1 << 30) - 1);
        assert_eq!(UTIME_OMIT, (1 << 30) - 2);
        assert_eq!(AT_FDCWD, -100);
        assert_eq!(AT_SYMLINK_NOFOLLOW, 0x200);
    }

    #[test]
    fn test_sync_wire() {
        let mut transport = CannedTransport::new();
        transport.reply_sendrec(Ok(reply_with_type(0)));
        assert_eq!(sync_via(&transport), Ok(()));
        let (dest, sent) = last_sent(&transport);
        assert_eq!(dest, vfs_endpoint());
        assert_eq!(sent.m_type, VFS_CALL_SYNC);
        assert_eq!(raw_lane(&sent, 0..56), vec![0; 56], "sync 载荷全零（C memset）");
    }

    #[test]
    fn test_utimensat_wire_lanes() {
        let mut transport = CannedTransport::new();
        transport.reply_sendrec(Ok(reply_with_type(0)));
        let path = b"/tmp/f\0";
        assert_eq!(
            utimensat_via(
                &transport,
                AT_FDCWD,
                path.as_ptr() as u64,
                path.len(),
                100,
                UTIME_OMIT,
                200,
                300,
                AT_SYMLINK_NOFOLLOW,
            ),
            Ok(())
        );
        let (_, sent) = last_sent(&transport);
        assert_eq!(sent.m_type, VFS_CALL_UTIMENS);
        assert_eq!(raw_lane(&sent, 0..8), 100i64.to_le_bytes(), "atime@0");
        assert_eq!(raw_lane(&sent, 8..16), 200i64.to_le_bytes(), "mtime@8");
        assert_eq!(raw_lane(&sent, 16..24), UTIME_OMIT.to_le_bytes(), "ansec@16");
        assert_eq!(raw_lane(&sent, 24..32), 300i64.to_le_bytes(), "mnsec@24");
        assert_eq!(raw_lane(&sent, 32..40), (path.len() as u64).to_le_bytes(), "len@32");
        assert_eq!(raw_lane(&sent, 40..48), (path.as_ptr() as u64).to_le_bytes(), "name@40");
        assert_eq!(raw_lane(&sent, 48..52), AT_FDCWD.to_le_bytes(), "fd@48");
        assert_eq!(raw_lane(&sent, 52..56), AT_SYMLINK_NOFOLLOW.to_le_bytes(), "flags@52");
    }

    #[test]
    fn test_utimensat_guards_answer_without_roundtrip() {
        let transport = CannedTransport::new();
        let abs = b"/tmp/f\0";
        let rel = b"tmp/f\0";
        // 空名 → ENOENT（C utimensat.c:32-35）。
        assert_eq!(
            utimensat_via(&transport, AT_FDCWD, abs.as_ptr() as u64, 1, 0, 0, 0, 0, 0),
            Err(Errno::ENOENT)
        );
        // 相对名 + dirfd != AT_FDCWD → EINVAL（C :36-39）。
        assert_eq!(
            utimensat_via(&transport, 7, rel.as_ptr() as u64, rel.len(), 0, 0, 0, 0, 0),
            Err(Errno::EINVAL)
        );
        // flags 超界 → EINVAL（C :40-42）。
        assert_eq!(
            utimensat_via(&transport, AT_FDCWD, abs.as_ptr() as u64, abs.len(), 0, 0, 0, 0, 70000),
            Err(Errno::EINVAL)
        );
        assert_eq!(
            utimensat_via(&transport, AT_FDCWD, abs.as_ptr() as u64, abs.len(), 0, 0, 0, 0, -1),
            Err(Errno::EINVAL)
        );
        // 全部被挡：零往返。
        assert!(transport.sent.borrow().is_empty());
    }

    #[test]
    fn test_futimens_wire_lanes() {
        let mut transport = CannedTransport::new();
        transport.reply_sendrec(Ok(reply_with_type(0)));
        assert_eq!(futimens_via(&transport, 4, 0, UTIME_NOW, 12345, 678), Ok(()));
        let (_, sent) = last_sent(&transport);
        assert_eq!(sent.m_type, VFS_CALL_UTIMENS);
        assert_eq!(raw_lane(&sent, 0..8), 0i64.to_le_bytes(), "atime@0=0（C now 形态）");
        assert_eq!(raw_lane(&sent, 8..16), 12345i64.to_le_bytes(), "mtime@8");
        assert_eq!(raw_lane(&sent, 16..24), UTIME_NOW.to_le_bytes(), "ansec@16");
        assert_eq!(raw_lane(&sent, 24..32), 678i64.to_le_bytes(), "mnsec@24");
        assert_eq!(raw_lane(&sent, 32..48), vec![0; 16], "len/name=0 走 fd 半");
        assert_eq!(raw_lane(&sent, 48..52), 4i32.to_le_bytes(), "fd@48");
        assert_eq!(raw_lane(&sent, 52..56), 0i32.to_le_bytes(), "flags@52=0（C futimens）");
    }

    #[test]
    fn test_utimens_error_leg() {
        let mut transport = CannedTransport::new();
        transport.reply_sendrec(Ok(reply_with_type(-minix_types::ENOENT)));
        assert_eq!(
            sync_via(&transport),
            Err(Errno::from_i32(minix_types::ENOENT))
        );
        transport.reply_sendrec(Ok(reply_with_type(-minix_types::EPERM)));
        assert_eq!(
            futimens_via(&transport, 4, 1, 2, 3, 4),
            Err(Errno::from_i32(minix_types::EPERM))
        );
    }
}
