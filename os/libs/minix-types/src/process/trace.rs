//! 追踪状态定义
//!
//! 独立于监护关系，描述进程因追踪而停止的状态

/// 追踪状态
///
/// 对应 Minix3 的 `TRACE_STOPPED` flag
///
/// # 说明
/// `TRACE_STOPPED` 是进程因追踪而停止的状态，
/// 可以和 `Running` 或 `Exiting` 组合
#[derive(Debug, Clone, Default)]
pub struct TraceState {
    /// 是否因追踪而停止（TRACE_STOPPED）
    pub stopped: bool,
}

impl TraceState {
    /// 创建新的追踪状态（默认未停止）
    pub fn new() -> Self {
        Self::default()
    }
    
    /// 检查是否因追踪而停止
    pub fn is_stopped(&self) -> bool {
        self.stopped
    }
}
