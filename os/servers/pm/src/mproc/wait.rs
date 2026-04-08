//! 父进程等待状态定义
//!
//! ⚠️ **重要**：`WAITING` 是父进程的状态，不是子进程的状态！
//!
//! 当父进程调用 `wait()` 或 `waitpid()` 时，父进程的 `WaitState` 会被设置

use minix_types::{Pid, VirBytes};

/// 父进程等待状态
///
/// 对应 Minix3 的 `WAITING` flag 和 `mp_wpid`、`mp_waddr` 字段
#[derive(Debug, Clone, Default)]
pub struct WaitState {
    /// 是否正在等待子进程（WAITING）
    pub waiting: bool,
    
    /// 等待目标（mp_wpid）
    pub target: WaitTarget,
    
    /// rusage 地址（mp_waddr）
    ///
    /// 用于存储子进程的资源使用情况
    pub rusage_addr: VirBytes,
}

/// 等待目标（互斥）
///
/// 对应 `waitpid()` 的第一个参数
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WaitTarget {
    /// `wait()` - 等待任意子进程
    ///
    /// 对应 `pid == -1`
    AnyChild,
    
    /// `waitpid(pid)` - 等待特定子进程
    ///
    /// 对应 `pid > 0`
    SpecificChild(Pid),
    
    /// `waitpid(-pgrp)` - 等待进程组
    ///
    /// 对应 `pid < -1`，等待进程组 `-pid` 中的任意子进程
    Group(Pid),
}

impl Default for WaitTarget {
    fn default() -> Self {
        Self::AnyChild
    }
}

impl WaitState {
    /// 创建新的等待状态（默认不等待）
    pub fn new() -> Self {
        Self::default()
    }
    
    /// 检查是否在等待指定的子进程
    ///
    /// # 参数
    /// - `child_pid`: 子进程 PID
    /// - `child_procgrp`: 子进程的进程组
    ///
    /// # 返回
    /// 如果父进程正在等待该子进程，返回 `true`
    pub fn is_waiting_for(&self, child_pid: Pid, child_procgrp: Pid) -> bool {
        if !self.waiting {
            return false;
        }
        
        match self.target {
            WaitTarget::AnyChild => true,
            WaitTarget::SpecificChild(pid) => child_pid == pid,
            WaitTarget::Group(pgrp) => child_procgrp == -pgrp,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_default_not_waiting() {
        let state = WaitState::default();
        assert!(!state.waiting);
    }
    
    #[test]
    fn test_waiting_for_any_child() {
        let mut state = WaitState::default();
        state.waiting = true;
        state.target = WaitTarget::AnyChild;
        
        assert!(state.is_waiting_for(1234, 100));
        assert!(state.is_waiting_for(5678, 200));
    }
    
    #[test]
    fn test_waiting_for_specific_child() {
        let mut state = WaitState::default();
        state.waiting = true;
        state.target = WaitTarget::SpecificChild(1234);
        
        assert!(state.is_waiting_for(1234, 100));
        assert!(!state.is_waiting_for(5678, 100));
    }
    
    #[test]
    fn test_waiting_for_group() {
        let mut state = WaitState::default();
        state.waiting = true;
        state.target = WaitTarget::Group(-100);
        
        assert!(state.is_waiting_for(1234, 100));
        assert!(!state.is_waiting_for(5678, 200));
    }
}
