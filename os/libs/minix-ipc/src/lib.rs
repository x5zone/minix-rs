//! Minix-RS IPC Library
//!
//! 进程间通信协议定义

#![no_std]

pub use minix_types::{Endpoint, Message};

/// 消息类型
#[derive(Debug, Clone, Copy)]
pub enum MessageType {
    /// 系统调用请求
    Syscall(SyscallNum),
    /// 通知
    Notify(NotifyType),
    /// 自定义消息
    Custom(u32),
}

/// 系统调用号
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

/// 通知类型
#[derive(Debug, Clone, Copy)]
#[repr(u32)]
pub enum NotifyType {
    /// 硬件中断
    HardInt = 1,
    /// 时钟滴答
    ClockTick = 2,
    /// 系统事件
    SysEvent = 3,
}

/// 发送消息
pub fn send(dest: Endpoint, msg: &Message) -> Result<(), IpcError> {
    todo!("send implementation")
}

/// 接收消息
pub fn receive(src: Endpoint, msg: &mut Message) -> Result<(), IpcError> {
    todo!("receive implementation")
}

/// 发送并接收（同步调用）
pub fn sendrec(dest: Endpoint, msg: &mut Message) -> Result<(), IpcError> {
    send(dest, msg)?;
    receive(dest, msg)
}

/// 通知
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
