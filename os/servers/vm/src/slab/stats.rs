//! Slab 分配器统计信息
//!
//! 提供分配器的性能统计和内存泄漏检测功能。

use core::sync::atomic::{AtomicUsize, Ordering};

/// Slab 分配器统计信息
///
/// 记录分配器的各种统计指标，用于性能分析和泄漏检测。
/// 所有字段都是原子类型，支持多线程安全访问。
///
/// # 示例
///
/// ```rust
/// use minix_vm::slab::SlabStats;
///
/// let stats = SlabStats::new();
/// stats.record_alloc(true); // 记录快速路径分配
/// stats.record_free();      // 记录释放
/// ```
#[derive(Debug)]
pub struct SlabStats {
    /// 总分配次数
    total_allocations: AtomicUsize,
    /// 总释放次数
    total_deallocations: AtomicUsize,
    /// 当前活跃分配数
    active_allocations: AtomicUsize,
    /// 分配失败次数
    allocation_failures: AtomicUsize,
    /// 从缓存分配的次数（快速路径）
    fast_path_hits: AtomicUsize,
    /// 从页分配的次数（慢速路径）
    slow_path_hits: AtomicUsize,
    /// 当前使用的页数
    pages_in_use: AtomicUsize,
    /// 累计分配的页数
    total_pages_allocated: AtomicUsize,
}

/// 泄漏检测报告
///
/// 当检测到内存泄漏时生成此报告。
#[derive(Debug, Clone, PartialEq)]
pub struct LeakReport {
    /// 当前活跃分配数
    pub active_allocations: usize,
    /// 总分配次数
    pub total_allocations: usize,
    /// 总释放次数
    pub total_deallocations: usize,
    /// 估计泄漏的字节数
    pub leaked_bytes: usize,
}

impl SlabStats {
    /// 创建新的统计信息实例
    ///
    /// 所有计数器初始化为 0。
    pub const fn new() -> Self {
        Self {
            total_allocations: AtomicUsize::new(0),
            total_deallocations: AtomicUsize::new(0),
            active_allocations: AtomicUsize::new(0),
            allocation_failures: AtomicUsize::new(0),
            fast_path_hits: AtomicUsize::new(0),
            slow_path_hits: AtomicUsize::new(0),
            pages_in_use: AtomicUsize::new(0),
            total_pages_allocated: AtomicUsize::new(0),
        }
    }

    /// 记录一次分配操作
    ///
    /// # 参数
    /// - `fast_path`: 是否为快速路径（从现有 slab 分配）
    pub fn record_alloc(&self, fast_path: bool) {
        self.total_allocations.fetch_add(1, Ordering::Relaxed);
        self.active_allocations.fetch_add(1, Ordering::Relaxed);
        if fast_path {
            self.fast_path_hits.fetch_add(1, Ordering::Relaxed);
        } else {
            self.slow_path_hits.fetch_add(1, Ordering::Relaxed);
        }
    }

    /// 记录一次释放操作
    pub fn record_free(&self) {
        self.total_deallocations.fetch_add(1, Ordering::Relaxed);
        self.active_allocations.fetch_sub(1, Ordering::Relaxed);
    }

    /// 记录一次分配失败
    pub fn record_failure(&self) {
        self.allocation_failures.fetch_add(1, Ordering::Relaxed);
    }

    /// 记录页分配
    ///
    /// # 参数
    /// - `count`: 分配的页数
    pub fn record_page_alloc(&self, count: usize) {
        self.pages_in_use.fetch_add(count, Ordering::Relaxed);
        self.total_pages_allocated.fetch_add(count, Ordering::Relaxed);
    }

    /// 记录页释放
    ///
    /// # 参数
    /// - `count`: 释放的页数
    pub fn record_page_free(&self, count: usize) {
        self.pages_in_use.fetch_sub(count, Ordering::Relaxed);
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

    /// 获取快速路径命中次数
    pub fn fast_path_hits(&self) -> usize {
        self.fast_path_hits.load(Ordering::Relaxed)
    }

    /// 获取慢速路径命中次数
    pub fn slow_path_hits(&self) -> usize {
        self.slow_path_hits.load(Ordering::Relaxed)
    }

    /// 获取当前使用的页数
    pub fn pages_in_use(&self) -> usize {
        self.pages_in_use.load(Ordering::Relaxed)
    }

    /// 获取累计分配的页数
    pub fn total_pages_allocated(&self) -> usize {
        self.total_pages_allocated.load(Ordering::Relaxed)
    }

    /// 检查是否有内存泄漏
    ///
    /// 如果活跃分配数大于 0，返回泄漏报告。
    ///
    /// # 参数
    /// - `object_size`: 对象大小（用于估计泄漏字节数）
    ///
    /// # 返回值
    /// - `Some(LeakReport)`: 检测到泄漏
    /// - `None`: 无泄漏
    pub fn check_leak(&self, object_size: usize) -> Option<LeakReport> {
        let active = self.active_allocations.load(Ordering::Relaxed);

        if active > 0 {
            Some(LeakReport {
                active_allocations: active,
                total_allocations: self.total_allocations.load(Ordering::Relaxed),
                total_deallocations: self.total_deallocations.load(Ordering::Relaxed),
                leaked_bytes: active * object_size,
            })
        } else {
            None
        }
    }

    /// 重置所有统计
    ///
    /// 将所有计数器重置为 0。
    /// 注意：这不会释放任何内存，只是重置计数器。
    pub fn reset(&self) {
        self.total_allocations.store(0, Ordering::Relaxed);
        self.total_deallocations.store(0, Ordering::Relaxed);
        self.active_allocations.store(0, Ordering::Relaxed);
        self.allocation_failures.store(0, Ordering::Relaxed);
        self.fast_path_hits.store(0, Ordering::Relaxed);
        self.slow_path_hits.store(0, Ordering::Relaxed);
        self.pages_in_use.store(0, Ordering::Relaxed);
        self.total_pages_allocated.store(0, Ordering::Relaxed);
    }
}

impl Default for SlabStats {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_stats() {
        let stats = SlabStats::new();

        stats.record_alloc(true);
        assert_eq!(stats.total_allocations(), 1);
        assert_eq!(stats.active_allocations(), 1);
        assert_eq!(stats.fast_path_hits(), 1);

        stats.record_free();
        assert_eq!(stats.total_deallocations(), 1);
        assert_eq!(stats.active_allocations(), 0);
    }

    #[test]
    fn test_leak_detection() {
        let stats = SlabStats::new();

        // 无泄漏
        assert!(stats.check_leak(64).is_none());

        // 分配但不释放
        stats.record_alloc(true);
        stats.record_alloc(true);

        let report = stats.check_leak(64).expect("should detect leak");
        assert_eq!(report.active_allocations, 2);
        assert_eq!(report.leaked_bytes, 128);

        // 释放后无泄漏
        stats.record_free();
        stats.record_free();
        assert!(stats.check_leak(64).is_none());
    }

    #[test]
    fn test_page_stats() {
        let stats = SlabStats::new();

        stats.record_page_alloc(5);
        assert_eq!(stats.pages_in_use(), 5);
        assert_eq!(stats.total_pages_allocated(), 5);

        stats.record_page_free(2);
        assert_eq!(stats.pages_in_use(), 3);
        assert_eq!(stats.total_pages_allocated(), 5); // 累计不变
    }

    #[test]
    fn test_reset() {
        let stats = SlabStats::new();

        stats.record_alloc(true);
        stats.record_page_alloc(10);
        stats.record_failure();

        stats.reset();

        assert_eq!(stats.total_allocations(), 0);
        assert_eq!(stats.pages_in_use(), 0);
        assert_eq!(stats.allocation_failures(), 0);
    }
}
