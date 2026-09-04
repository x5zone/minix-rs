//! Minix-RS System Call Library.
//!
//! System call wrappers for user-space programs.
//!
//! # STUB NOTICE
//!
//! This crate is a **stub** — most functions are `todo!()` and will panic
//! if called. It defines the intended API but is not yet functional.

#![no_std]

extern crate alloc;

use minix_types::{Endpoint, IpcError, Message};

pub use minix_types::{Gid, Pid, Uid};

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

/// Sends a message.
pub fn send(dest: Endpoint, msg: &Message) -> Result<(), IpcError> {
    todo!("send implementation")
}

/// Receives a message.
pub fn receive(src: Endpoint, msg: &mut Message) -> Result<(), IpcError> {
    todo!("receive implementation")
}

/// Sends and receives (synchronous call).
pub fn sendrec(dest: Endpoint, msg: &mut Message) -> Result<(), IpcError> {
    send(dest, msg)?;
    receive(dest, msg)
}

/// Sends a notification.
pub fn notify(dest: Endpoint, type_: minix_types::NotifyType) -> Result<(), IpcError> {
    todo!("notify implementation")
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
