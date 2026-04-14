//! 物理内存分配器实现
//!
//! 基于位图的物理页分配器，支持连续内存分配。

use super::{bytes_to_clicks, clicks_to_bytes, click_ceil, CLICK_SIZE, NO_MEM};
use super::stats::MemStats;
use std::alloc::{alloc, dealloc, Layout};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};

/// 物理地址包装类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PhysAddr(pub u64);

impl PhysAddr {
    /// 创建新的物理地址
    pub const fn new(addr: u64) -> Self {
        PhysAddr(addr)
    }

    /// 获取地址值
    pub const fn as_u64(&self) -> u64 {
        self.0
    }

    /// 获取地址值（usize 类型）
    pub const fn as_usize(&self) -> usize {
        self.0 as usize
    }

    /// 检查是否为有效地址
    pub const fn is_valid(&self) -> bool {
        self.0 != 0
    }

    /// 地址对齐到 click 边界
    pub const fn align_up(&self) -> Self {
        PhysAddr(click_ceil(self.0 as usize) as u64)
    }

    /// 地址加上偏移
    pub const fn add(&self, offset: usize) -> Self {
        PhysAddr(self.0 + offset as u64)
    }
}

impl Default for PhysAddr {
    fn default() -> Self {
        NO_MEM
    }
}

bitflags::bitflags! {
    /// 物理内存分配标志
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct AllocFlags: u32 {
        /// 要求连续物理内存
        const CONTIG = 0x01;
        /// 4KB 对齐（默认已对齐）
        const ALIGN4K = 0x02;
        /// 清零内存
        const ZERO = 0x04;
        /// 低端内存 (<16MB，用于 DMA)
        const LOW = 0x08;
    }
}

impl Default for AllocFlags {
    fn default() -> Self {
        AllocFlags::empty()
    }
}

/// 已分配内存块记录
#[derive(Debug)]
#[allow(dead_code)]
struct AllocatedBlock {
    /// 起始地址
    addr: PhysAddr,
    /// 大小（clicks）
    clicks: usize,
    /// 分配时的标志
    flags: AllocFlags,
}

/// 物理内存分配器
///
/// 管理系统的物理内存分配，提供 alloc_mem/free_mem 功能。
/// 在用户态测试中使用 Mock 实现。
pub struct PhysMemAllocator {
    /// 已分配内存块表
    allocations: HashMap<u64, AllocatedBlock>,
    /// 下一个分配的地址（Mock 使用）
    next_addr: AtomicU64,
    /// 内存统计
    stats: MemStats,
    /// 总内存大小（bytes）
    total_mem: usize,
    /// 低端内存限制（16MB）
    low_mem_limit: u64,
}

impl PhysMemAllocator {
    /// 创建新的物理内存分配器
    ///
    /// # 参数
    /// - `total_mem`: 系统总物理内存大小（bytes）
    ///
    /// # 示例
    ///
    /// ```rust
    /// use minix_vm::phys_mem::PhysMemAllocator;
    ///
    /// let allocator = PhysMemAllocator::new(512 * 1024 * 1024); // 512MB
    /// ```
    pub fn new(total_mem: usize) -> Self {
        // 从 1MB 开始分配（避开低端内存）
        let start_addr = 0x100000;

        Self {
            allocations: HashMap::new(),
            next_addr: AtomicU64::new(start_addr),
            stats: MemStats::new(),
            total_mem,
            low_mem_limit: 16 * 1024 * 1024, // 16MB
        }
    }

    /// 分配物理内存
    ///
    /// # 参数
    /// - `clicks`: 请求的内存大小（以 click 为单位）
    /// - `flags`: 分配标志
    ///
    /// # 返回值
    /// - `Some(PhysAddr)`: 分配成功，返回物理地址
    /// - `None`: 分配失败（内存不足或无法满足标志要求）
    ///
    /// # 示例
    ///
    /// ```rust
    /// use minix_vm::phys_mem::{PhysMemAllocator, AllocFlags};
    ///
    /// let mut allocator = PhysMemAllocator::new(512 * 1024 * 1024);
    ///
    /// // 分配 4 clicks (16KB)
    /// let addr = allocator.alloc(4, AllocFlags::empty());
    /// assert!(addr.is_some());
    ///
    /// // 分配连续内存
    /// let addr2 = allocator.alloc(8, AllocFlags::CONTIG);
    /// assert!(addr2.is_some());
    /// ```
    pub fn alloc(&mut self, clicks: usize, flags: AllocFlags) -> Option<PhysAddr> {
        if clicks == 0 {
            return Some(NO_MEM);
        }

        let bytes = clicks_to_bytes(clicks);

        // 检查内存限制
        let current_usage = self.stats.total_allocated();
        if current_usage + bytes > self.total_mem {
            self.stats.record_failure();
            return None;
        }

        // 分配地址
        let addr = if flags.contains(AllocFlags::LOW) {
            // 低端内存分配
            self.alloc_low_memory(clicks)?
        } else {
            // 普通内存分配
            self.alloc_regular_memory(clicks, flags)?
        };

        // 清零内存（如果请求）
        if flags.contains(AllocFlags::ZERO) {
            self.zero_memory(addr, clicks);
        }

        // 记录分配
        let block = AllocatedBlock {
            addr,
            clicks,
            flags,
        };
        self.allocations.insert(addr.0, block);

        // 更新统计
        self.stats.record_alloc(bytes);

        Some(addr)
    }

    /// 分配常规内存
    fn alloc_regular_memory(&mut self, clicks: usize, _flags: AllocFlags) -> Option<PhysAddr> {
        let bytes = clicks_to_bytes(clicks);

        // Mock 实现：简单递增分配
        let addr = self.next_addr.fetch_add(bytes as u64, Ordering::SeqCst);

        // 检查是否超出总内存
        if addr + bytes as u64 > self.total_mem as u64 {
            self.next_addr.fetch_sub(bytes as u64, Ordering::SeqCst);
            return None;
        }

        // 检查是否超出低端内存限制
        if addr < self.low_mem_limit && addr + bytes as u64 > self.low_mem_limit {
            // 跳过低端内存区域
            let new_addr = self.low_mem_limit;
            self.next_addr.store(new_addr + bytes as u64, Ordering::SeqCst);
            return Some(PhysAddr(new_addr));
        }

        Some(PhysAddr(addr))
    }

    /// 分配低端内存（<16MB，用于 DMA）
    fn alloc_low_memory(&mut self, clicks: usize) -> Option<PhysAddr> {
        let bytes = clicks_to_bytes(clicks);

        // Mock 实现：从 1MB 开始分配低端内存
        // TODO: 实现真正的低端内存管理
        static LOW_MEM_NEXT: AtomicU64 = AtomicU64::new(0x100000);

        let addr = LOW_MEM_NEXT.fetch_add(bytes as u64, Ordering::SeqCst);

        if addr + bytes as u64 > self.low_mem_limit {
            LOW_MEM_NEXT.fetch_sub(bytes as u64, Ordering::SeqCst);
            return None;
        }

        Some(PhysAddr(addr))
    }

    /// 清零内存区域
    fn zero_memory(&self, _addr: PhysAddr, clicks: usize) {
        let bytes = clicks_to_bytes(clicks);

        // Mock 实现：实际分配并清零
        unsafe {
            let layout = Layout::from_size_align(bytes, CLICK_SIZE).unwrap();
            let ptr = alloc(layout);
            if !ptr.is_null() {
                std::ptr::write_bytes(ptr, 0, bytes);
                dealloc(ptr, layout);
            }
        }
    }

    /// 释放物理内存
    ///
    /// # 参数
    /// - `addr`: 要释放的物理地址（必须是通过 `alloc` 分配的）
    /// - `clicks`: 内存大小（clicks，必须与分配时一致）
    ///
    /// # Panics
    ///
    /// 如果 `addr` 不是通过本分配器分配的，或者 `clicks` 不匹配，将 panic。
    ///
    /// # 示例
    ///
    /// ```rust
    /// use minix_vm::phys_mem::{PhysMemAllocator, AllocFlags};
    ///
    /// let mut allocator = PhysMemAllocator::new(512 * 1024 * 1024);
    ///
    /// let addr = allocator.alloc(4, AllocFlags::empty()).unwrap();
    /// allocator.free(addr, 4);
    /// ```
    pub fn free(&mut self, addr: PhysAddr, clicks: usize) {
        if !addr.is_valid() {
            return;
        }

        // 查找分配记录
        let block = self.allocations.remove(&addr.0)
            .unwrap_or_else(|| panic!("attempt to free unallocated memory at {:?}", addr));

        // 验证大小
        assert_eq!(block.clicks, clicks,
            "free size mismatch: allocated {} clicks, freeing {} clicks",
            block.clicks, clicks);

        // 更新统计
        let bytes = clicks_to_bytes(clicks);
        self.stats.record_free(bytes);

        // TODO: 实际释放物理页到空闲池
        // 在 Mock 实现中，我们只是移除记录
    }

    /// 获取内存统计信息
    ///
    /// # 示例
    ///
    /// ```rust
    /// use minix_vm::phys_mem::PhysMemAllocator;
    ///
    /// let mut allocator = PhysMemAllocator::new(512 * 1024 * 1024);
    ///
    /// let addr = allocator.alloc(4, Default::default()).unwrap();
    ///
    /// assert_eq!(allocator.stats().total_allocated(), 4 * 4096);
    ///
    /// allocator.free(addr, 4);
    /// assert_eq!(allocator.stats().active_allocations(), 0);
    /// ```
    pub fn stats(&self) -> &MemStats {
        &self.stats
    }

    /// 获取总内存大小
    pub fn total_memory(&self) -> usize {
        self.total_mem
    }

    /// 获取空闲内存大小
    pub fn free_memory(&self) -> usize {
        self.total_mem.saturating_sub(self.stats.total_allocated())
    }

    /// 检查内存压力
    ///
    /// 当空闲内存低于阈值时返回 true
    pub fn is_under_pressure(&self) -> bool {
        let free = self.free_memory();
        let threshold = self.total_mem / 10; // 10% 阈值
        free < threshold
    }
}

impl Default for PhysMemAllocator {
    fn default() -> Self {
        // 默认 512MB 内存
        Self::new(512 * 1024 * 1024)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_phys_addr() {
        let addr = PhysAddr::new(0x1000);
        assert_eq!(addr.as_u64(), 0x1000);
        assert!(addr.is_valid());

        let invalid = NO_MEM;
        assert!(!invalid.is_valid());
    }

    #[test]
    fn test_alloc_free() {
        let mut allocator = PhysMemAllocator::new(64 * 1024 * 1024);

        let addr = allocator.alloc(4, AllocFlags::empty()).unwrap();
        assert!(addr.is_valid());

        allocator.free(addr, 4);
        assert_eq!(allocator.stats().active_allocations(), 0);
    }

    #[test]
    fn test_alloc_zero() {
        let mut allocator = PhysMemAllocator::new(64 * 1024 * 1024);

        // 分配 0 clicks 应该返回 NO_MEM
        let addr = allocator.alloc(0, AllocFlags::empty()).unwrap();
        assert!(!addr.is_valid());
    }

    #[test]
    fn test_memory_pressure() {
        let mut allocator = PhysMemAllocator::new(100 * 1024 * 1024); // 100MB

        assert!(!allocator.is_under_pressure());

        // 分配大部分内存（超过 90%，使空闲 < 10%）
        // 100MB 总内存，分配 93MB 后，空闲 7MB < 10MB (10%)
        let _addr = allocator.alloc(24000, AllocFlags::empty()).unwrap(); // ~96MB

        assert!(allocator.is_under_pressure());
    }
}
