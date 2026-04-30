//! Minix-RS IPC Library.
//!
//! Inter-process communication protocol definitions.

#![no_std]

pub use minix_types::{Endpoint, Message};

/// Message type.
#[derive(Debug, Clone, Copy)]
pub enum MessageType {
    /// Syscall request.
    Syscall(SyscallNum),
    /// Notification.
    Notify(NotifyType),
    /// Custom message.
    Custom(u32),
}

/// Syscall number.
#[derive(Debug, Clone, Copy)]
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

/// Notification type.
#[derive(Debug, Clone, Copy)]
#[repr(u32)]
pub enum NotifyType {
    /// Hardware interrupt.
    HardInt = 1,
    /// Clock tick.
    ClockTick = 2,
    /// System event.
    SysEvent = 3,
}

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
pub fn notify(dest: Endpoint, type_: NotifyType) -> Result<(), IpcError> {
    todo!("notify implementation")
}

#[derive(Debug)]
pub enum IpcError {
    InvalidEndpoint,
    WouldBlock,
    Interrupted,
    NoPerm,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_endpoint() {
        let ep = Endpoint(42);
        assert_eq!(ep.get(), 42);
        assert!(ep.is_valid());
    }
}
