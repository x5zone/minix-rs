//! Slab 分配器
//!
//! VM 专用的内存分配器，用于高效分配固定大小的对象。
//! 对应 Minix3: `minix3/minix/servers/vm/slaballoc.c`
//!
//! # 设计原则
//!
//! - **固定大小对象**：每个 SlabCache 只分配特定大小的对象
//! - **O(1) 分配/释放**：使用位图快速定位空闲对象
//! - **减少碎片**：相同大小的对象放在一起
//! - **硬件 Mock**：所有物理内存操作通过 Mock 实现，支持用户态测试
//!
//! # 使用示例
//!
//! ```rust
//! use minix_vm::slab::SlabCache;
//!
//! // 创建分配 64 字节对象的缓存
//! let mut cache = SlabCache::new(64);
//!
//! // 分配对象
//! let ptr = cache.allocate().expect("allocation failed");
//!
//! // 使用对象...
//!
//! // 释放对象
//! unsafe { cache.free(ptr); }
//! ```

pub mod cache;
pub mod stats;
pub mod mock;

#[cfg(test)]
pub mod tests;

pub use cache::SlabCache;
pub use stats::{SlabStats, LeakReport};
pub use mock::MockPageAllocator;

/// 页大小（4KB）
pub const PAGE_SIZE: usize = 4096;

/// 最小对象大小
pub const MIN_OBJECT_SIZE: usize = 8;

/// 最大对象大小（一个页内）
pub const MAX_OBJECT_SIZE: usize = PAGE_SIZE / 2;

/// 计算对象大小对应的 slab 索引
///
/// 将任意大小对齐到最小对象大小的倍数
pub fn size_to_index(size: usize) -> usize {
    if size <= MIN_OBJECT_SIZE {
        0
    } else {
        // 向上对齐到 2 的幂次
        let aligned = size.next_power_of_two();
        aligned.trailing_zeros() as usize - MIN_OBJECT_SIZE.trailing_zeros() as usize
    }
}

/// 计算 slab 索引对应的对象大小
pub fn index_to_size(index: usize) -> usize {
    MIN_OBJECT_SIZE << index
}

#[cfg(test)]
mod size_tests {
    use super::*;

    #[test]
    fn test_size_conversions() {
        assert_eq!(size_to_index(8), 0);
        assert_eq!(size_to_index(16), 1);
        assert_eq!(size_to_index(32), 2);
        assert_eq!(size_to_index(64), 3);

        assert_eq!(index_to_size(0), 8);
        assert_eq!(index_to_size(1), 16);
        assert_eq!(index_to_size(2), 32);
        assert_eq!(index_to_size(3), 64);
    }
}
