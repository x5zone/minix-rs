//! Minix-RS IPC Library
//!
//! 进程间通信协议定义

#![no_std]

/// 消息结构
#[derive(Debug, Clone, Copy)]
pub struct Message {
    pub source: Endpoint,
    pub mtype: MessageType,
    pub payload: [u64; 6],
}

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

/// 端点标识
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Endpoint(pub u32);

impl Endpoint {
    pub const NONE: Endpoint = Endpoint(0);
    pub const KERNEL: Endpoint = Endpoint(1);
    pub const PM: Endpoint = Endpoint(2);
    pub const VFS: Endpoint = Endpoint(3);
    pub const VM: Endpoint = Endpoint(4);
    pub const RS: Endpoint = Endpoint(5);
    
    /// 创建进程端点
    pub fn process(pid: u32) -> Self {
        Endpoint(pid)
    }
    
    /// 获取进程 ID
    pub fn pid(&self) -> u32 {
        self.0
    }
    
    /// 是否有效
    pub fn is_valid(&self) -> bool {
        self.0 != 0
    }
}

/// 发送消息
pub fn send(dest: Endpoint, msg: &Message) -> Result<(), IpcError> {
    // TODO: 实现 IPC 发送
    todo!("send implementation")
}

/// 接收消息
pub fn receive(src: Endpoint, msg: &mut Message) -> Result<(), IpcError> {
    // TODO: 实现 IPC 接收
    todo!("receive implementation")
}

/// 发送并接收（同步调用）
pub fn sendrec(dest: Endpoint, msg: &mut Message) -> Result<(), IpcError> {
    send(dest, msg)?;
    receive(dest, msg)
}

/// 通知
pub fn notify(dest: Endpoint, type_: NotifyType) -> Result<(), IpcError> {
    // TODO: 实现通知
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
        let ep = Endpoint::process(42);
        assert_eq!(ep.pid(), 42);
        assert!(ep.is_valid());
    }
}
