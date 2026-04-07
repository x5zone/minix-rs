//! 信号处理状态定义
//!
//! 提供进程的信号掩码、待处理信号等管理

use crate::types::VirBytes;

/// 信号集（64 位无符号整数）
///
/// 在 64 位系统中，支持最多 64 个信号
pub type SigSet = u64;

/// 信号数量
pub const _NSIG: usize = 64;

/// 信号处理状态
///
/// 存储进程的信号相关信息
///
/// # Minix3 映射
/// - `mp_sigmask` → `mask`
/// - `mp_sigmask2` → `mask_saved`
/// - `mp_sigpending` → `pending`
/// - `mp_ksigpending` → `kernel_pending`
/// - `mp_sigtrace` → `trace_mask`
/// - `SIGSUSPENDED` → `suspended`
/// - `mp_sigreturn` → `sigreturn_addr`
#[derive(Debug, Clone, Default)]
pub struct SignalState {
    /// 信号掩码（阻塞的信号）
    pub mask: SigSet,
    /// 保存的信号掩码（用于 sigsuspend 恢复）
    pub mask_saved: SigSet,
    /// 待处理的信号
    pub pending: SigSet,
    /// 内核待处理的信号
    pub kernel_pending: SigSet,
    /// 追踪信号掩码
    pub trace_mask: SigSet,
    /// 是否处于 sigsuspend 状态（SIGSUSPENDED）
    pub suspended: bool,
    /// sigreturn 函数地址
    pub sigreturn_addr: VirBytes,
}

/// 信号处理动作
///
/// 对应 C 的 `struct sigaction`
#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct SigAction {
    /// 信号处理函数地址或特殊值
    ///
    /// - `0` (SIG_DFL): 默认处理
    /// - `1` (SIG_IGN): 忽略
    /// - 其他: 用户定义的处理函数地址
    pub sa_handler: usize,
    /// 处理期间阻塞的信号
    pub sa_mask: SigSet,
    /// 信号处理标志
    pub sa_flags: i32,
}

impl SignalState {
    /// 创建新的信号状态（默认无阻塞信号）
    pub fn new() -> Self {
        Self::default()
    }
    
    /// 检查是否有待处理的信号
    pub fn has_pending(&self) -> bool {
        self.pending != 0 || self.kernel_pending != 0
    }
    
    /// 检查信号是否被阻塞
    ///
    /// # 参数
    /// - `signo`: 信号编号（1-64）
    ///
    /// # 返回
    /// 如果信号被阻塞，返回 `true`
    pub fn is_blocked(&self, signo: u32) -> bool {
        if signo == 0 || signo > 64 {
            return false;
        }
        (self.mask & (1u64 << (signo - 1))) != 0
    }
    
    /// 添加待处理信号
    ///
    /// # 参数
    /// - `signo`: 信号编号（1-64）
    /// - `from_kernel`: 是否来自内核
    pub fn add_pending(&mut self, signo: u32, from_kernel: bool) {
        if signo == 0 || signo > 64 {
            return;
        }
        let bit = 1u64 << (signo - 1);
        self.pending |= bit;
        if from_kernel {
            self.kernel_pending |= bit;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_signal_state_default() {
        let state = SignalState::default();
        assert_eq!(state.mask, 0);
        assert_eq!(state.pending, 0);
        assert!(!state.has_pending());
        assert!(!state.suspended);
    }
    
    #[test]
    fn test_is_blocked() {
        let mut state = SignalState::default();
        
        assert!(!state.is_blocked(1));
        assert!(!state.is_blocked(0));
        assert!(!state.is_blocked(65));
        
        state.mask = 0b101;
        assert!(state.is_blocked(1));
        assert!(!state.is_blocked(2));
        assert!(state.is_blocked(3));
    }
    
    #[test]
    fn test_add_pending() {
        let mut state = SignalState::default();
        
        state.add_pending(1, false);
        assert!(state.has_pending());
        assert_eq!(state.pending, 1);
        assert_eq!(state.kernel_pending, 0);
        
        state.add_pending(2, true);
        assert_eq!(state.pending, 0b11);
        assert_eq!(state.kernel_pending, 0b10);
    }
    
    #[test]
    fn test_add_pending_invalid() {
        let mut state = SignalState::default();
        
        state.add_pending(0, false);
        state.add_pending(65, false);
        assert!(!state.has_pending());
    }
    
    #[test]
    fn test_sig_action() {
        let action = SigAction {
            sa_handler: 0x1000,
            sa_mask: 0xFF,
            sa_flags: 0,
        };
        assert_eq!(action.sa_handler, 0x1000);
        assert_eq!(action.sa_mask, 0xFF);
    }
}
