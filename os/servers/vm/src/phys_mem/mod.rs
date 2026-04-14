//! 物理内存分配模块
//!
//! 提供系统级物理内存分配服务，是 VM 服务器的核心功能之一。
//!
//! # Click 单位系统
//!
//! Minix3 使用 "click" 作为内存分配的基本单位：
//! - 1 click = 4096 bytes (4KB)
//! - CLICK_SHIFT = 12 (用于位运算)
//!
//! # 主要功能
//!
//! - `alloc_mem`: 按 click 分配物理内存
//! - `free_mem`: 释放物理内存
//! - `memstats`: 查询内存统计信息
//!
//! # 使用场景
//!
//! - PM (Process Manager) fork 时分配页表和栈
//! - VM 内部结构（页表、缓存等）
//! - 驱动程序 DMA 缓冲区
//!
//! # 示例
//!
//! ```rust
//! use minix_vm::phys_mem::{PhysMemAllocator, AllocFlags};
//!
//! let mut allocator = PhysMemAllocator::new(512 * 1024 * 1024);
//!
//! // 分配 4 clicks (16KB) 物理内存
//! let phys_addr = allocator.alloc(4, AllocFlags::empty())
//!     .expect("allocation failed");
//!
//! // 使用内存...
//!
//! // 释放内存
//! allocator.free(phys_addr, 4);
//! ```

pub mod alloc_trait;
pub mod allocator;
pub mod frame;
pub mod stats;
pub mod reserved;
#[cfg(test)]
pub mod tests;

pub use alloc_trait::{AllocError, MockPhysMemAlloc, PhysMemAlloc, PhysMemGlobalAlloc};
pub use allocator::{PhysMemAllocator, AllocFlags, PhysAddr};
pub use frame::PhysFrame;
pub use stats::{MemStats, MemStatsReporter};
pub use reserved::{ReservedQueueManager, QueueInfo};

/// Click 大小：4096 bytes (4KB)
pub const CLICK_SIZE: usize = 4096;

/// Click 位移：log2(CLICK_SIZE) = 12
pub const CLICK_SHIFT: usize = 12;

/// 无效物理地址标记
pub const NO_MEM: PhysAddr = PhysAddr(0);

/// 将字节数转换为 clicks（向上取整）
#[inline]
pub const fn bytes_to_clicks(bytes: usize) -> usize {
    (bytes + CLICK_SIZE - 1) >> CLICK_SHIFT
}

/// 将 clicks 转换为字节数
#[inline]
pub const fn clicks_to_bytes(clicks: usize) -> usize {
    clicks << CLICK_SHIFT
}

/// 向下取整到 click 边界
#[inline]
pub const fn click_floor(addr: usize) -> usize {
    (addr >> CLICK_SHIFT) << CLICK_SHIFT
}

/// 向上取整到 click 边界
#[inline]
pub const fn click_ceil(addr: usize) -> usize {
    ((addr + CLICK_SIZE - 1) >> CLICK_SHIFT) << CLICK_SHIFT
}
