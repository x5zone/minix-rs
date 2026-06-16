//! Minix-RS System Call Library.
//!
//! System call wrappers for user-space programs.
//!
//! # STUB NOTICE
//!
//! This crate is a **stub** — most functions are `todo!()` and will panic
//! if called. It defines the intended API but is not yet functional.

#![no_std]

use minix_types::{Endpoint, Message, IpcError};

pub use minix_types::{Gid, Pid, Uid};

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

/// Error number.
#[derive(Debug, Clone, Copy)]
#[repr(i32)]
pub enum Errno {
    Eperm = 1,
    Enoent = 2,
    Esrch = 3,
    Eintr = 4,
    Eio = 5,
    Enxio = 6,
    E2big = 7,
    Enoexec = 8,
    Ebadf = 9,
    Echild = 10,
    Eagain = 11,
    Enomem = 12,
    Eacces = 13,
    Efault = 14,
    Ebusy = 16,
    Eexist = 17,
    Exdev = 18,
    Enodev = 19,
    Enotdir = 20,
    Eisdir = 21,
    Einval = 22,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_errno_values() {
        assert_eq!(Errno::Eperm as i32, 1);
        assert_eq!(Errno::Enoent as i32, 2);
    }
}
