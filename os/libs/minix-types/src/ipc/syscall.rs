//! Syscall number definitions.
//!
//! Corresponds to Minix3's syscall numbers used in IPC message type field.

/// Syscall number.
///
/// Identifies the system call being requested via IPC.
/// Corresponds to Minix3's `SCALL_*` constants.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum SyscallNum {
    Fork = 1,
    Exit = 2,
    Exec = 3,
    Wait = 4,
    Kill = 5,
    Sigaction = 6,
    Open = 10,
    Close = 11,
    Read = 12,
    Write = 13,
    Mmap = 20,
    Munmap = 21,
}
