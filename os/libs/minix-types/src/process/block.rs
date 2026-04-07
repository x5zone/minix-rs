//! 进程阻塞状态定义
//!
//! 阻塞状态可以和生命周期状态组合
//!
//! # 关键约束
//! - `PROC_STOPPED` 可以和 `Running` 或 `Exiting` 组合
//! - `VFS_CALL` 可以和 `Exiting` 组合

use core::fmt;

/// 进程阻塞状态
///
/// 对应 Minix3 的 `mp_flags` 中的阻塞相关位：
/// - `PROC_STOPPED` → `stopped`
/// - `VFS_CALL` / `EVENT_CALL` / `DELAY_CALL` → `ipc_blocked`
/// - `UNPAUSED` → `unpaused`
#[derive(Debug, Clone, Copy, Default)]
pub struct BlockState {
    /// 是否在内核中停止（PROC_STOPPED）
    ///
    /// 可以和 `Running` / `Exiting` 组合
    pub stopped: bool,
    
    /// IPC 阻塞原因
    ///
    /// 进程正在等待 IPC 响应
    pub ipc_blocked: Option<IpcBlockReason>,
    
    /// VFS 已回复 unpause 请求（UNPAUSED）
    pub unpaused: bool,
}

/// IPC 阻塞原因（互斥）
///
/// 对应 Minix3 的三种 IPC 阻塞状态
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IpcBlockReason {
    /// 等待 VFS 回复（VFS_CALL）
    ///
    /// 进程正在等待文件系统操作完成
    VfsCall,
    
    /// 等待进程事件订阅者（EVENT_CALL）
    ///
    /// 进程正在等待事件通知
    EventCall,
    
    /// 等待调用完成后再发送信号（DELAY_CALL）
    ///
    /// 信号需要延迟到 IPC 完成后发送
    DelayedSignal,
}

impl BlockState {
    /// 创建新的阻塞状态（默认无阻塞）
    pub fn new() -> Self {
        Self::default()
    }
    
    /// 检查进程是否被阻塞
    ///
    /// 包括被停止或等待 IPC
    pub fn is_blocked(&self) -> bool {
        self.stopped || self.ipc_blocked.is_some()
    }
    
    /// 检查是否在等待 VFS
    pub fn is_vfs_blocked(&self) -> bool {
        matches!(self.ipc_blocked, Some(IpcBlockReason::VfsCall))
    }
    
    /// 检查是否在等待事件
    pub fn is_event_blocked(&self) -> bool {
        matches!(self.ipc_blocked, Some(IpcBlockReason::EventCall))
    }
}

impl fmt::Display for BlockState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut first = true;
        
        if self.stopped {
            write!(f, "stopped")?;
            first = false;
        }
        if let Some(reason) = self.ipc_blocked {
            if !first {
                write!(f, ", ")?;
            }
            match reason {
                IpcBlockReason::VfsCall => write!(f, "vfs_blocked")?,
                IpcBlockReason::EventCall => write!(f, "event_blocked")?,
                IpcBlockReason::DelayedSignal => write!(f, "delayed_signal")?,
            }
            first = false;
        }
        if self.unpaused {
            if !first {
                write!(f, ", ")?;
            }
            write!(f, "unpaused")?;
        }
        
        if first {
            write!(f, "none")?;
        }
        Ok(())
    }
}
