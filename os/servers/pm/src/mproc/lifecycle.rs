//! 进程生命周期状态定义
//!
//! 生命周期状态是互斥的，一个进程在同一时刻只能处于一种生命周期状态
//!
//! # 状态转换图
//! ```text
//! Unused ──────→ Running ──────→ Exiting ──────→ TraceZombie ──┐
//!                  │                │                  │       │
//!                  │                │                  ↓       │
//!                  │                └─────────────→ Zombie ←───┘
//!                  │                                      │
//!                  │                                      ↓
//!                  └──────────────────────────────→ ToldParent
//! ```

use core::fmt;

/// 进程生命周期状态（互斥）
///
/// 对应 Minix3 的 `mp_flags` 中的生命周期相关位：
/// - `IN_USE` → `!Unused`
/// - `EXITING` → `Exiting`
/// - `TRACE_ZOMBIE` → `TraceZombie`
/// - `ZOMBIE` → `Zombie`
/// - `TOLD_PARENT` → `ToldParent`
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lifecycle {
    /// 槽位未使用
    ///
    /// 进程表槽位空闲，可以被分配
    Unused,
    
    /// 正常运行中
    ///
    /// 进程正在执行，可能同时有 `BlockState::stopped = true`
    Running,
    
    /// 正在退出
    ///
    /// 进程正在执行退出流程，可能同时有：
    /// - `VFS_CALL`: 等待 VFS 清理
    /// - `PROC_STOPPED`: 被信号停止
    /// - `TRACE_EXIT`: 追踪者强制退出
    Exiting {
        /// 退出状态码
        exit_code: i8,
        /// 信号状态（如果是被信号杀死）
        sig_status: i8,
    },
    
    /// 追踪僵尸状态
    ///
    /// 进程已退出，等待 tracer 收尸（当 tracer != parent 时）
    /// 对应 `TRACE_ZOMBIE` flag
    TraceZombie {
        exit_code: i8,
        sig_status: i8,
    },
    
    /// 僵尸状态
    ///
    /// 进程已退出，等待父进程收尸
    /// 对应 `ZOMBIE` flag
    Zombie {
        exit_code: i8,
        sig_status: i8,
    },
    
    /// 已通知父进程
    ///
    /// 父进程已被通知子进程退出，等待清理
    /// 对应 `TOLD_PARENT` flag
    ToldParent {
        exit_code: i8,
        sig_status: i8,
    },
}

impl Default for Lifecycle {
    fn default() -> Self {
        Self::Unused
    }
}

impl Lifecycle {
    /// 检查槽位是否在使用中
    ///
    /// 对应 `IN_USE` flag
    pub fn is_in_use(&self) -> bool {
        !matches!(self, Self::Unused)
    }
    
    /// 获取退出状态码
    ///
    /// 返回 `(exit_code, sig_status)`，如果不是退出相关状态则返回 `None`
    pub fn exit_code(&self) -> Option<(i8, i8)> {
        match self {
            Self::Exiting { exit_code, sig_status } 
            | Self::TraceZombie { exit_code, sig_status }
            | Self::Zombie { exit_code, sig_status }
            | Self::ToldParent { exit_code, sig_status } => {
                Some((*exit_code, *sig_status))
            }
            _ => None,
        }
    }
    
    /// 检查是否是僵尸状态
    ///
    /// 包括 `Zombie` 和 `TraceZombie`
    pub fn is_zombie(&self) -> bool {
        matches!(self, Self::Zombie { .. } | Self::TraceZombie { .. })
    }
    
    /// 检查是否正在退出
    pub fn is_exiting(&self) -> bool {
        matches!(self, Self::Exiting { .. })
    }
}

impl fmt::Display for Lifecycle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unused => write!(f, "Unused"),
            Self::Running => write!(f, "Running"),
            Self::Exiting { exit_code, sig_status } => {
                write!(f, "Exiting(exit={}, sig={})", exit_code, sig_status)
            }
            Self::TraceZombie { exit_code, sig_status } => {
                write!(f, "TraceZombie(exit={}, sig={})", exit_code, sig_status)
            }
            Self::Zombie { exit_code, sig_status } => {
                write!(f, "Zombie(exit={}, sig={})", exit_code, sig_status)
            }
            Self::ToldParent { exit_code, sig_status } => {
                write!(f, "ToldParent(exit={}, sig={})", exit_code, sig_status)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_default_is_unused() {
        let lifecycle = Lifecycle::default();
        assert!(matches!(lifecycle, Lifecycle::Unused));
        assert!(!lifecycle.is_in_use());
    }
    
    #[test]
    fn test_running_is_in_use() {
        let lifecycle = Lifecycle::Running;
        assert!(lifecycle.is_in_use());
        assert!(!lifecycle.is_zombie());
        assert!(!lifecycle.is_exiting());
    }
    
    #[test]
    fn test_exiting_state() {
        let lifecycle = Lifecycle::Exiting { exit_code: 0, sig_status: 9 };
        assert!(lifecycle.is_in_use());
        assert!(lifecycle.is_exiting());
        assert!(!lifecycle.is_zombie());
        assert_eq!(lifecycle.exit_code(), Some((0, 9)));
    }
    
    #[test]
    fn test_zombie_states() {
        let zombie = Lifecycle::Zombie { exit_code: 42, sig_status: 0 };
        assert!(zombie.is_zombie());
        assert_eq!(zombie.exit_code(), Some((42, 0)));
        
        let trace_zombie = Lifecycle::TraceZombie { exit_code: 0, sig_status: 11 };
        assert!(trace_zombie.is_zombie());
        assert_eq!(trace_zombie.exit_code(), Some((0, 11)));
    }
    
    #[test]
    fn test_told_parent() {
        let state = Lifecycle::ToldParent { exit_code: 0, sig_status: 0 };
        assert!(state.is_in_use());
        assert!(!state.is_zombie());
        assert!(!state.is_exiting());
        assert_eq!(state.exit_code(), Some((0, 0)));
    }
}
