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

/// Creates a child process.
pub fn fork() -> Result<Pid, Errno> {
    todo!("fork syscall")
}

/// Executes a new program.
pub fn exec(path: &str, argv: &[&str]) -> Result<(), Errno> {
    todo!("exec syscall")
}

/// Process exit.
pub fn exit(status: i32) -> ! {
    loop {}
}

/// Waits for child process.
pub fn waitpid(pid: Pid, status: &mut i32, options: i32) -> Result<Pid, Errno> {
    todo!("waitpid syscall")
}

/// Sends a signal.
pub fn kill(pid: Pid, sig: i32) -> Result<(), Errno> {
    todo!("kill syscall")
}

// ── File system calls ──

/// Opens a file.
pub fn open(path: &str, flags: i32, mode: u32) -> Result<Fd, Errno> {
    todo!("open syscall")
}

/// Closes a file.
pub fn close(fd: Fd) -> Result<(), Errno> {
    todo!("close syscall")
}

/// Reads from a file.
pub fn read(fd: Fd, buf: &mut [u8]) -> Result<usize, Errno> {
    todo!("read syscall")
}

/// Writes to a file.
pub fn write(fd: Fd, buf: &[u8]) -> Result<usize, Errno> {
    todo!("write syscall")
}

/// Memory mapping.
pub fn mmap(
    addr: *mut u8,
    len: usize,
    prot: i32,
    flags: i32,
    fd: Fd,
    offset: i64,
) -> Result<*mut u8, Errno> {
    todo!("mmap syscall")
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
