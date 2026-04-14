//! 物理内存分配器 Trait 定义
//!
//! 定义物理内存分配的抽象接口，支持不同的实现（真实硬件、Mock、测试框架）。

use super::{AllocFlags, PhysAddr, PhysMemAllocator};
use super::stats::MemStats;
use std::cell::RefCell;
use std::error::Error;
use std::fmt;

/// 物理内存分配错误类型
///
/// 表示分配操作可能失败的各种原因。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AllocError {
    /// 内存不足
    OutOfMemory,
    /// 无法满足对齐要求
    AlignmentFailed,
    /// 无法满足连续内存要求
    ContiguityFailed,
    /// 低端内存耗尽
    LowMemoryExhausted,
}

impl fmt::Display for AllocError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AllocError::OutOfMemory => write!(f, "out of memory"),
            AllocError::AlignmentFailed => write!(f, "alignment requirement failed"),
            AllocError::ContiguityFailed => write!(f, "contiguity requirement failed"),
            AllocError::LowMemoryExhausted => write!(f, "low memory exhausted"),
        }
    }
}

impl Error for AllocError {}

/// 物理内存分配器接口
///
/// 定义物理内存分配的基本操作，与具体实现解耦。
/// 允许使用不同的实现（真实硬件、Mock、测试框架）。
///
/// # 示例
///
/// ```rust
/// use minix_vm::phys_mem::{PhysMemAlloc, AllocFlags, AllocError};
///
/// fn allocate_page_table(allocator: &mut dyn PhysMemAlloc) -> Result<u64, AllocError> {
///     let addr = allocator.alloc(1, AllocFlags::CONTIG | AllocFlags::ZERO)?;
///     Ok(addr.as_u64())
/// }
/// ```
pub trait PhysMemAlloc {
    /// 分配物理内存
    ///
    /// # 参数
    /// - `clicks`: 请求的内存大小（以 click 为单位）
    /// - `flags`: 分配标志
    ///
    /// # 返回值
    /// - `Ok(PhysAddr)`: 分配成功，返回物理地址
    /// - `Err(AllocError)`: 分配失败
    fn alloc(&mut self, clicks: usize, flags: AllocFlags) -> Result<PhysAddr, AllocError>;

    /// 释放物理内存
    ///
    /// # 参数
    /// - `addr`: 要释放的内存起始地址
    /// - `clicks`: 要释放的内存大小（以 click 为单位）
    fn free(&mut self, addr: PhysAddr, clicks: usize);

    /// 获取内存统计信息
    ///
    /// # 返回值
    /// 内存统计信息的引用
    fn stats(&self) -> &MemStats;
}

// PhysMemAllocator 实现 PhysMemAlloc trait
impl PhysMemAlloc for PhysMemAllocator {
    fn alloc(&mut self, clicks: usize, flags: AllocFlags) -> Result<PhysAddr, AllocError> {
        // 调用现有的 alloc 实现，将 Option 转换为 Result
        PhysMemAllocator::alloc(self, clicks, flags)
            .ok_or(AllocError::OutOfMemory)
    }

    fn free(&mut self, addr: PhysAddr, clicks: usize) {
        PhysMemAllocator::free(self, addr, clicks);
    }

    fn stats(&self) -> &MemStats {
        PhysMemAllocator::stats(self)
    }
}

/// 测试用的 Mock 物理内存分配器
///
/// 用于单元测试，可以模拟分配失败等场景。
///
/// # 示例
///
/// ```rust
/// use minix_vm::phys_mem::{MockPhysMemAlloc, PhysMemAlloc, AllocFlags, AllocError};
///
/// let mut mock = MockPhysMemAlloc::new();
/// mock.fail_after(1); // 第一次分配后失败
///
/// assert!(mock.alloc(1, AllocFlags::empty()).is_ok());
/// assert!(matches!(mock.alloc(1, AllocFlags::empty()), Err(AllocError::OutOfMemory)));
/// ```
#[derive(Debug)]
pub struct MockPhysMemAlloc {
    /// 已分配的内存块列表
    allocations: Vec<(PhysAddr, usize)>,
    /// 下一个分配的地址
    next_addr: u64,
    /// 在第 N 次分配后失败（None 表示从不失败）
    fail_after: Option<usize>,
    /// 内存统计
    stats: MemStats,
}

impl MockPhysMemAlloc {
    /// 创建新的 Mock 分配器
    ///
    /// # 示例
    ///
    /// ```rust
    /// use minix_vm::phys_mem::MockPhysMemAlloc;
    ///
    /// let mock = MockPhysMemAlloc::new();
    /// ```
    pub fn new() -> Self {
        Self {
            allocations: Vec::new(),
            next_addr: 0x100000, // 从 1MB 开始
            fail_after: None,
            stats: MemStats::new(),
        }
    }

    /// 设置在第 N 次分配后失败
    ///
    /// # 参数
    /// - `n`: 允许成功分配的次数，第 n+1 次开始失败
    ///
    /// # 示例
    ///
    /// ```rust
    /// use minix_vm::phys_mem::{MockPhysMemAlloc, PhysMemAlloc, AllocFlags, AllocError};
    ///
    /// let mut mock = MockPhysMemAlloc::new();
    /// mock.fail_after(2);
    ///
    /// assert!(mock.alloc(1, AllocFlags::empty()).is_ok());
    /// assert!(mock.alloc(1, AllocFlags::empty()).is_ok());
    /// assert!(matches!(mock.alloc(1, AllocFlags::empty()), Err(AllocError::OutOfMemory)));
    /// ```
    pub fn fail_after(&mut self, n: usize) {
        self.fail_after = Some(n);
    }

    /// 获取已分配内存块的数量
    pub fn allocation_count(&self) -> usize {
        self.allocations.len()
    }

    /// 检查指定地址是否已分配
    pub fn is_allocated(&self, addr: PhysAddr) -> bool {
        self.allocations.iter().any(|(a, _)| *a == addr)
    }
}

impl Default for MockPhysMemAlloc {
    fn default() -> Self {
        Self::new()
    }
}

impl PhysMemAlloc for MockPhysMemAlloc {
    fn alloc(&mut self, clicks: usize, flags: AllocFlags) -> Result<PhysAddr, AllocError> {
        // 检查是否应该失败
        if let Some(fail_after) = self.fail_after {
            if self.allocations.len() >= fail_after {
                self.stats.record_failure();
                return Err(AllocError::OutOfMemory);
            }
        }

        // 模拟低端内存限制
        if flags.contains(AllocFlags::LOW) && self.next_addr >= 0x1000000 {
            self.stats.record_failure();
            return Err(AllocError::LowMemoryExhausted);
        }

        let addr = PhysAddr::new(self.next_addr);
        let bytes = clicks * super::CLICK_SIZE;
        self.next_addr += bytes as u64;
        self.allocations.push((addr, clicks));
        self.stats.record_alloc(bytes);

        Ok(addr)
    }

    fn free(&mut self, addr: PhysAddr, clicks: usize) {
        let initial_len = self.allocations.len();
        self.allocations.retain(|(a, c)| *a != addr || *c != clicks);

        // 如果确实释放了内存，更新统计
        if self.allocations.len() < initial_len {
            let bytes = clicks * super::CLICK_SIZE;
            self.stats.record_free(bytes);
        }
    }

    fn stats(&self) -> &MemStats {
        &self.stats
    }
}

/// 将 PhysMemAllocator 包装为 GlobalAlloc（仅用于测试）
///
/// 注意：这仅用于测试环境，实际内核不应该使用 GlobalAlloc。
///
/// # 示例
///
/// ```rust,ignore
/// use minix_vm::phys_mem::PhysMemGlobalAlloc;
/// use std::alloc::GlobalAlloc;
///
/// let alloc = PhysMemGlobalAlloc::new(512 * 1024 * 1024);
///
/// unsafe {
///     let ptr = alloc.alloc(Layout::from_size_align(4096, 4096).unwrap());
///     // 使用内存...
///     alloc.dealloc(ptr, Layout::from_size_align(4096, 4096).unwrap());
/// }
/// ```
pub struct PhysMemGlobalAlloc {
    inner: RefCell<PhysMemAllocator>,
}

impl PhysMemGlobalAlloc {
    /// 创建新的 GlobalAlloc 包装
    ///
    /// # 参数
    /// - `total_mem`: 总内存大小（bytes）
    pub fn new(total_mem: usize) -> Self {
        Self {
            inner: RefCell::new(PhysMemAllocator::new(total_mem)),
        }
    }
}

// 注意：这里不能安全地实现 GlobalAlloc，因为 PhysMemAllocator::alloc
// 返回的是物理地址，而 GlobalAlloc 期望的是虚拟地址。
// 这个实现仅作为示例，展示 trait 设计的可能性。

#[cfg(test)]
mod tests {
    use super::*;

    /// 测试 MockPhysMemAlloc 基本功能
    #[test]
    fn test_mock_alloc_free() {
        let mut mock = MockPhysMemAlloc::new();

        // 分配内存
        let addr = mock.alloc(1, AllocFlags::empty()).expect("alloc failed");
        assert!(addr.is_valid());
        assert_eq!(mock.allocation_count(), 1);

        // 释放内存
        mock.free(addr, 1);
        assert_eq!(mock.allocation_count(), 0);
    }

    /// 测试 fail_after 功能
    #[test]
    fn test_mock_fail_after() {
        let mut mock = MockPhysMemAlloc::new();
        mock.fail_after(2);

        // 前两次分配成功
        assert!(mock.alloc(1, AllocFlags::empty()).is_ok());
        assert!(mock.alloc(1, AllocFlags::empty()).is_ok());

        // 第三次分配失败
        let result = mock.alloc(1, AllocFlags::empty());
        assert!(matches!(result, Err(AllocError::OutOfMemory)));
    }

    /// 测试低端内存限制
    #[test]
    fn test_mock_low_memory() {
        let mut mock = MockPhysMemAlloc::new();
        // 设置 next_addr 接近 16MB 边界
        mock.next_addr = 0xFF0000; // 约 16MB - 64KB

        // 分配小量低端内存应该成功
        let result = mock.alloc(1, AllocFlags::LOW);
        assert!(result.is_ok());

        // 设置 next_addr 超过 16MB
        mock.next_addr = 0x1000000; // 16MB

        // 分配低端内存应该失败
        let result = mock.alloc(1, AllocFlags::LOW);
        assert!(matches!(result, Err(AllocError::LowMemoryExhausted)));
    }

    /// 测试通过 trait 对象使用
    #[test]
    fn test_trait_object() {
        fn allocate_through_trait(alloc: &mut dyn PhysMemAlloc) -> Result<PhysAddr, AllocError> {
            alloc.alloc(4, AllocFlags::CONTIG)
        }

        let mut mock = MockPhysMemAlloc::new();
        let addr = allocate_through_trait(&mut mock).expect("alloc failed");
        assert!(addr.is_valid());
    }

    /// 测试统计信息
    #[test]
    fn test_mock_stats() {
        let mut mock = MockPhysMemAlloc::new();

        let initial_allocated = mock.stats().total_allocated();

        // 分配内存
        let addr = mock.alloc(2, AllocFlags::empty()).expect("alloc failed");
        let after_alloc = mock.stats().total_allocated();
        assert!(after_alloc > initial_allocated);

        // 释放内存
        mock.free(addr, 2);
        let after_free = mock.stats().total_allocated();
        assert_eq!(after_free, initial_allocated);
    }

    /// 测试 AllocError 的 Display 实现
    #[test]
    fn test_alloc_error_display() {
        assert_eq!(AllocError::OutOfMemory.to_string(), "out of memory");
        assert_eq!(AllocError::AlignmentFailed.to_string(), "alignment requirement failed");
        assert_eq!(AllocError::ContiguityFailed.to_string(), "contiguity requirement failed");
        assert_eq!(AllocError::LowMemoryExhausted.to_string(), "low memory exhausted");
    }

    /// 测试 PhysMemAllocator 通过 trait 使用
    #[test]
    fn test_real_allocator_through_trait() {
        let mut allocator = PhysMemAllocator::new(512 * 1024 * 1024);

        // 通过 trait 对象使用
        let trait_alloc: &mut dyn PhysMemAlloc = &mut allocator;

        let addr = trait_alloc.alloc(1, AllocFlags::empty()).expect("alloc failed");
        assert!(addr.is_valid());

        trait_alloc.free(addr, 1);
    }
}
