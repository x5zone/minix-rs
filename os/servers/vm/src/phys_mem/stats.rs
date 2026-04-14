//! 物理内存统计模块
//!
//! 提供内存分配统计和报告功能。

use std::sync::atomic::{AtomicUsize, Ordering};

/// 内存统计信息
///
/// 跟踪物理内存分配的各种统计指标。
#[derive(Debug)]
pub struct MemStats {
    /// 总分配次数
    total_allocations: AtomicUsize,
    /// 总释放次数
    total_deallocations: AtomicUsize,
    /// 当前活跃分配数
    active_allocations: AtomicUsize,
    /// 分配失败次数
    allocation_failures: AtomicUsize,
    /// 总分配字节数（累计）
    total_allocated_bytes: AtomicUsize,
    /// 总释放字节数（累计）
    total_freed_bytes: AtomicUsize,
    /// 当前分配字节数
    current_allocated_bytes: AtomicUsize,
    /// 峰值分配字节数
    peak_allocated_bytes: AtomicUsize,
}

impl MemStats {
    /// 创建新的内存统计实例
    pub const fn new() -> Self {
        Self {
            total_allocations: AtomicUsize::new(0),
            total_deallocations: AtomicUsize::new(0),
            active_allocations: AtomicUsize::new(0),
            allocation_failures: AtomicUsize::new(0),
            total_allocated_bytes: AtomicUsize::new(0),
            total_freed_bytes: AtomicUsize::new(0),
            current_allocated_bytes: AtomicUsize::new(0),
            peak_allocated_bytes: AtomicUsize::new(0),
        }
    }

    /// 记录一次分配
    pub fn record_alloc(&self, bytes: usize) {
        self.total_allocations.fetch_add(1, Ordering::Relaxed);
        self.active_allocations.fetch_add(1, Ordering::Relaxed);
        self.total_allocated_bytes.fetch_add(bytes, Ordering::Relaxed);

        let current = self.current_allocated_bytes.fetch_add(bytes, Ordering::Relaxed) + bytes;

        // 更新峰值
        let mut peak = self.peak_allocated_bytes.load(Ordering::Relaxed);
        while current > peak {
            match self.peak_allocated_bytes.compare_exchange_weak(
                peak,
                current,
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) => break,
                Err(actual) => peak = actual,
            }
        }
    }

    /// 记录一次释放
    pub fn record_free(&self, bytes: usize) {
        self.total_deallocations.fetch_add(1, Ordering::Relaxed);
        self.active_allocations.fetch_sub(1, Ordering::Relaxed);
        self.total_freed_bytes.fetch_add(bytes, Ordering::Relaxed);
        self.current_allocated_bytes.fetch_sub(bytes, Ordering::Relaxed);
    }

    /// 记录一次分配失败
    pub fn record_failure(&self) {
        self.allocation_failures.fetch_add(1, Ordering::Relaxed);
    }

    /// 获取总分配次数
    pub fn total_allocations(&self) -> usize {
        self.total_allocations.load(Ordering::Relaxed)
    }

    /// 获取总释放次数
    pub fn total_deallocations(&self) -> usize {
        self.total_deallocations.load(Ordering::Relaxed)
    }

    /// 获取当前活跃分配数
    pub fn active_allocations(&self) -> usize {
        self.active_allocations.load(Ordering::Relaxed)
    }

    /// 获取分配失败次数
    pub fn allocation_failures(&self) -> usize {
        self.allocation_failures.load(Ordering::Relaxed)
    }

    /// 获取总分配字节数（累计）
    pub fn total_allocated_bytes(&self) -> usize {
        self.total_allocated_bytes.load(Ordering::Relaxed)
    }

    /// 获取总释放字节数（累计）
    pub fn total_freed_bytes(&self) -> usize {
        self.total_freed_bytes.load(Ordering::Relaxed)
    }

    /// 获取当前分配字节数
    pub fn current_allocated_bytes(&self) -> usize {
        self.current_allocated_bytes.load(Ordering::Relaxed)
    }

    /// 获取峰值分配字节数
    pub fn peak_allocated_bytes(&self) -> usize {
        self.peak_allocated_bytes.load(Ordering::Relaxed)
    }

    /// 获取总分配量（以 clicks 为单位）
    pub fn total_allocated(&self) -> usize {
        self.current_allocated_bytes()
    }

    /// 生成统计报告
    pub fn generate_report(&self) -> String {
        format!(
            "Memory Statistics:\n\
             Total allocations: {}\n\
             Total deallocations: {}\n\
             Active allocations: {}\n\
             Allocation failures: {}\n\
             Current allocated: {} bytes\n\
             Peak allocated: {} bytes\n\
             Total allocated (cumulative): {} bytes\n\
             Total freed: {} bytes",
            self.total_allocations(),
            self.total_deallocations(),
            self.active_allocations(),
            self.allocation_failures(),
            self.current_allocated_bytes(),
            self.peak_allocated_bytes(),
            self.total_allocated_bytes(),
            self.total_freed_bytes(),
        )
    }
}

impl Default for MemStats {
    fn default() -> Self {
        Self::new()
    }
}

/// 内存统计报告器
///
/// 用于定期输出内存统计信息。
pub struct MemStatsReporter {
    stats: MemStats,
    name: String,
}

impl MemStatsReporter {
    /// 创建新的统计报告器
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            stats: MemStats::new(),
            name: name.into(),
        }
    }

    /// 获取统计信息
    pub fn stats(&self) -> &MemStats {
        &self.stats
    }

    /// 打印统计报告
    pub fn print_report(&self) {
        println!("=== {} ===", self.name);
        println!("{}", self.stats.generate_report());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stats_basic() {
        let stats = MemStats::new();

        stats.record_alloc(4096);
        assert_eq!(stats.total_allocations(), 1);
        assert_eq!(stats.active_allocations(), 1);
        assert_eq!(stats.current_allocated_bytes(), 4096);

        stats.record_free(4096);
        assert_eq!(stats.total_deallocations(), 1);
        assert_eq!(stats.active_allocations(), 0);
        assert_eq!(stats.current_allocated_bytes(), 0);
    }

    #[test]
    fn test_peak_tracking() {
        let stats = MemStats::new();

        stats.record_alloc(1000);
        assert_eq!(stats.peak_allocated_bytes(), 1000);

        stats.record_alloc(500);
        assert_eq!(stats.peak_allocated_bytes(), 1500);

        stats.record_free(500);
        // 峰值应该保持不变
        assert_eq!(stats.peak_allocated_bytes(), 1500);
    }

    #[test]
    fn test_failure_tracking() {
        let stats = MemStats::new();

        stats.record_failure();
        stats.record_failure();

        assert_eq!(stats.allocation_failures(), 2);
    }
}
