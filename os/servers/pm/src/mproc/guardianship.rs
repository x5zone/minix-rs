//! 监护关系定义
//!
//! 解决 Minix3 中 `mp_parent` 和 `mp_tracer` 的杂糅问题
//!
//! # 设计改进
//! 在 `Normal` 状态下没有 `tracer` 字段，防止误操作

use minix_types::ProcIndex;
use bitflags::bitflags;

/// 监护关系
///
/// 描述进程的父进程和追踪者关系
///
/// # Minix3 映射
/// - `mp_parent` → `Normal { parent }` 或 `Traced { parent, .. }`
/// - `mp_tracer` → `Traced { tracer, .. }`
/// - `TRACE_EXIT` → `Traced { trace_exit: true, .. }`
/// - `mp_trace_flags` → `Traced { trace_options, .. }`
/// - `NO_TRACER (-1)` → `Normal`
#[derive(Debug, Clone)]
pub enum Guardianship {
    /// 正常状态：只有一个父进程
    Normal { 
        /// 父进程索引
        parent: ProcIndex 
    },
    
    /// 调试状态：被 tracer 劫持
    ///
    /// tracer 可能不等于 parent
    Traced{
        /// 父进程索引
        parent: ProcIndex,
        /// 追踪者索引
        tracer: ProcIndex,
        /// TRACE_EXIT flag：tracer 正在强制进程退出
        trace_exit: bool,
        /// 追踪选项（mp_trace_flags）
        trace_options: TraceOptions,
    },
}

impl Default for Guardianship {
    fn default() -> Self {
        Self::Normal {
            parent: ProcIndex::new(0),
        }
    }
}

bitflags! {
    /// 追踪选项
    ///
    /// 对应 Minix3 的 `mp_trace_flags` 字段
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct TraceOptions: u32 {
        /// TO_TRACEFORK: 自动 attach 到 fork 的子进程
        const TRACEFORK = 0x1;
        /// TO_ALTEXEC: exec 成功时发送 SIGSTOP
        const ALTEXEC = 0x2;
        /// TO_NOEXEC: exec 成功时不发送信号
        const NOEXEC = 0x4;
    }
}

impl Guardianship {
    /// 获取父进程索引
    ///
    /// 无论是否被追踪，父进程始终存在
    pub fn parent(&self) -> ProcIndex {
        match self {
            Self::Normal { parent } => *parent,
            Self::Traced { parent, .. } => *parent,
        }
    }
    
    /// 获取追踪者索引
    ///
    /// 如果进程没有被追踪，返回 `None`
    pub fn tracer(&self) -> Option<ProcIndex> {
        match self {
            Self::Normal { .. } => None,
            Self::Traced { tracer, .. } => Some(*tracer),
        }
    }
    
    /// 检查进程是否被追踪
    pub fn is_traced(&self) -> bool {
        matches!(self, Self::Traced { .. })
    }
    
    /// 获取 TRACE_EXIT 标志
    ///
    /// 如果进程没有被追踪，返回 `false`
    pub fn trace_exit(&self) -> bool {
        match self {
            Self::Normal { .. } => false,
            Self::Traced { trace_exit, .. } => *trace_exit,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_default_is_normal() {
        let g = Guardianship::default();
        assert!(matches!(g, Guardianship::Normal { .. }));
        assert!(!g.is_traced());
        assert!(g.tracer().is_none());
    }
    
    #[test]
    fn test_normal_parent() {
        let g = Guardianship::Normal { parent: ProcIndex::new(5) };
        assert_eq!(g.parent(), ProcIndex::new(5));
        assert!(!g.is_traced());
        assert!(g.tracer().is_none());
        assert!(!g.trace_exit());
    }
    
    #[test]
    fn test_traced_state() {
        let g = Guardianship::Traced {
            parent: ProcIndex::new(1),
            tracer: ProcIndex::new(2),
            trace_exit: false,
            trace_options: TraceOptions::empty(),
        };
        assert!(g.is_traced());
        assert_eq!(g.parent(), ProcIndex::new(1));
        assert_eq!(g.tracer(), Some(ProcIndex::new(2)));
        assert!(!g.trace_exit());
    }
    
    #[test]
    fn test_trace_exit_flag() {
        let g = Guardianship::Traced {
            parent: ProcIndex::new(1),
            tracer: ProcIndex::new(2),
            trace_exit: true,
            trace_options: TraceOptions::empty(),
        };
        assert!(g.trace_exit());
    }
    
    #[test]
    fn test_trace_options() {
        let opts = TraceOptions::TRACEFORK | TraceOptions::ALTEXEC;
        assert!(opts.contains(TraceOptions::TRACEFORK));
        assert!(opts.contains(TraceOptions::ALTEXEC));
        assert!(!opts.contains(TraceOptions::NOEXEC));
    }
}
