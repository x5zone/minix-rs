//! Minix-RS System Call Library.
//!
//! System call wrappers for user-space programs.
//!
//! The communication primitives ([`ipc`]) and the system call protocol
//! ([`syscall`]) are fully implemented over an explicit transport trait.
//! Service-group wrappers below (processes, files, memory mapping) belong to
//! later stage documents and still report explicit failures until their own
//! documents land.
//!
//! # Transport model
//!
//! The zero-argument functions in this file delegate to the direct-trap
//! transport: the only transport a real binary has. In a hosted test
//! environment no kernel answers, so they return explicit errors instead of
//! faulting. Test code that needs scripted replies uses [`ipc::CannedTransport`]
//! with [`syscall::perform_syscall`] directly.

#![no_std]

extern crate alloc;

use ipc::IpcTransport;
use minix_types::{Endpoint, IpcError, Message};

pub use minix_types::{Gid, Pid, Uid};

/// Inter-process communication primitives (document 04).
pub mod ipc;
/// System call protocol above send-and-receive (document 05).
pub mod syscall;
/// Process manager call group (document 08).
pub mod pm;
/// File system call group (document 09).
pub mod vfs;
/// Virtual memory call group (document 10).
pub mod vm;
/// Miscellaneous calls: sleeping, server control, clock reads (document 11).
pub mod misc;
/// Reincarnation server queries: lookup and endpoint questions (document 12).
pub mod rs;
/// User-space socket call policy: call list, flag handling, fallback rule
/// (17-stage-net/23-libc-socket.md). Traps stay with the callers; the legacy
/// device fallback is documented but never taken ([ARCH] N-2).
pub mod socket;

/// devman client library: driver-side registration + bind handling
/// (11-stage-devman/10-libdevman-client.md).
pub mod devman_client;
/// Input-driver client library: driver-side announce, event filing, and
/// server-message handling (12-stage-input/12-libinputdriver.md).
/// Transport (label lookup, publish, blocking send) stays out until the
/// IPC transport lands — same boundary as `devman_client`.
pub mod inputdriver;
/// Remote MIB client: pure bookkeeping for mounted subtrees
/// (10-stage-mib/22-mib-rmib-client.md). Message sending and grant
/// handling stay out until the IPC transport lands.
pub mod rmib;
/// USB device modeling over the devman client
/// (11-stage-devman/11-usb-device-model.md).
pub mod usb_model;

/// File descriptor.
pub type Fd = i32;

// ── IPC system calls ──
//
// Each function delegates to the direct-trap transport (see `ipc`). The
// transport reports an explicit failure status where no kernel answers;
// the mapping to the public error type lives in one place
// (`ipc::trap_status_to_ipc_error`).

/// Sends a message (blocking).
pub fn send(dest: Endpoint, msg: &Message) -> Result<(), IpcError> {
    ipc::DirectTrapTransport
        .send(dest, msg)
        .map_err(ipc::trap_status_to_ipc_error)
}

/// Receives a message (blocking).
pub fn receive(src: Endpoint, msg: &mut Message) -> Result<(), IpcError> {
    ipc::DirectTrapTransport
        .receive(src, msg)
        .map(|_| ())
        .map_err(ipc::trap_status_to_ipc_error)
}

/// Sends and receives (synchronous call).
pub fn sendrec(dest: Endpoint, msg: &mut Message) -> Result<(), IpcError> {
    // The C library implements send-and-receive as one trap (not as
    // send-then-receive); keep the single-trap shape here as well.
    ipc::DirectTrapTransport
        .sendrec(dest, msg)
        .map_err(ipc::trap_status_to_ipc_error)
}

/// Sends a notification.
pub fn notify(dest: Endpoint, type_: minix_types::NotifyType) -> Result<(), IpcError> {
    let _ = type_;
    ipc::DirectTrapTransport
        .notify(dest)
        .map_err(ipc::trap_status_to_ipc_error)
}

// ── Process system calls ──
//
// Each function delegates to the matching `pm` wrapper over the
// direct-trap transport (see `pm`). Execution preparation (`exec`) takes a
// caller-prepared request: the stack image is built with the 01-stage size
// computation plus the 06-stage allocator, which live in `minix-rt` (this
// crate cannot depend on it without a dependency cycle).

/// Creates a child process.
pub fn fork() -> Result<Pid, Errno> {
    pm::fork_via(&ipc::DirectTrapTransport)
}

/// Executes a prepared program image.
///
/// The request is prepared by the caller (see `pm::prepare_exec`); a
/// successful execution never returns, so the result is always the failure
/// that came back.
pub fn exec(prepared: pm::PreparedExec) -> Errno {
    pm::exec_via(&ipc::DirectTrapTransport, prepared)
}

/// Process exit.
pub fn exit(status: i32) -> ! {
    pm::exit_via(&ipc::DirectTrapTransport, status)
}

/// Waits for a child process, reporting how it ended in `status`.
pub fn waitpid(pid: Pid, status: &mut i32, options: i32) -> Result<Pid, Errno> {
    let (child, child_status) =
        pm::waitpid_via(&ipc::DirectTrapTransport, pid, options, 0)?;
    *status = child_status;
    Ok(child)
}

/// Sends a signal.
pub fn kill(pid: Pid, sig: i32) -> Result<(), Errno> {
    pm::kill_via(&ipc::DirectTrapTransport, pid, sig)
}

// ── File system calls ──
//
// Each function delegates to the matching `vfs` wrapper over the
// direct-trap transport (see `vfs`). Buffer addresses come from the caller:
// user programs pass their own buffers, whose addresses the server reads
// through granted memory.

/// Opens a file.
///
/// Dispatches on the create flag (see `vfs::dispatch_open`): the create path
/// is fully implemented; the open-existing path reports `ENOSYS` until the
/// global concepts document settles the 64-bit path message layout.
pub fn open(path: &str, flags: i32, mode: u32) -> Result<Fd, Errno> {
    vfs::open_via(
        &ipc::DirectTrapTransport,
        path.as_ptr() as u64,
        path.len().saturating_add(1),
        flags,
        mode,
    )
}

/// Closes a file.
pub fn close(fd: Fd) -> Result<(), Errno> {
    vfs::close_via(&ipc::DirectTrapTransport, fd)
}

/// Reads from a file into the caller's buffer.
pub fn read(fd: Fd, buf: &mut [u8]) -> Result<usize, Errno> {
    vfs::read_via(
        &ipc::DirectTrapTransport,
        fd,
        buf.as_mut_ptr() as u64,
        buf.len(),
    )
}

/// Writes the caller's buffer to a file.
pub fn write(fd: Fd, buf: &[u8]) -> Result<usize, Errno> {
    vfs::write_via(
        &ipc::DirectTrapTransport,
        fd,
        buf.as_ptr() as u64,
        buf.len(),
    )
}

/// Memory mapping.
///
/// Maps memory for the caller itself (the third-party flag stays clear).
/// A failed call reports `None` instead of the mapped-failed sentinel, so
/// callers cannot mistake failure for address minus one.
pub fn mmap(
    addr: *mut u8,
    len: usize,
    prot: i32,
    flags: i32,
    fd: Fd,
    offset: i64,
) -> Result<*mut u8, Errno> {
    let request = vm::MapRequest {
        beneficiary: Endpoint(0),
        address: minix_types::VirBytes(addr as u64),
        length: minix_types::VirBytes(len as u64),
        protection: prot as u32,
        flags: flags as u32,
        file: fd,
        offset,
    };
    // The caller endpoint is unknown inside this shim; self-mapping is the
    // overwhelmingly common case and matches the C `mmap` wrapper, which
    // always passes SELF. Third-party mappings use `vm::mmap_via` directly
    // with an explicit beneficiary.
    let placed = vm::mmap_via(&ipc::DirectTrapTransport, Endpoint(0), request)?;
    Ok(placed.map(|address| address.0 as *mut u8).unwrap_or(core::ptr::null_mut()))
}

/// Error number — single shared ABI type.
///
/// Re-exported from `minix-types` (the Redox `redox_syscall::Error` pattern:
/// one shared error type across the ABI boundary). The local enum previously
/// duplicated `minix_types::Errno` with a different naming style (`Eperm`)
/// and **lacked the RS-required values** `ENOSYS`/`EDEADEPT`/`EDONTREPLY`/
/// `EGENERIC`/`ERESTART` (errno.h:78/211/199/200/196); two Errno types force
/// a conversion at every `KernelApi` boundary (todo §11 N4).
pub use minix_types::Errno;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_errno_values() {
        assert_eq!(Errno::EPERM.to_i32(), 1);
        assert_eq!(Errno::EINVAL.to_i32(), 22);
        // The RS-required values the old enum lacked must be reachable from
        // the shared type (todo §11 N4).
        assert_eq!(Errno::ENOSYS.to_i32(), 78);
    }
}
