//! VM 全局状态模块
//!
//! 提供 VM 模块的全局状态管理，包括：
//! - `BootImage`: 启动镜像信息
//! - `MemoryStats`: 物理内存统计
//! - `VmGlobalState`: 全局状态封装

use core::sync::atomic::{AtomicU32, Ordering};

use minix_mock::PhysAddr;
use minix_types::Endpoint;

/// 启动进程数量（系统进程 + 任务）
pub const NR_BOOT_PROCS: usize = 32;

/// 启动镜像信息
///
/// 内核启动时传递给 VM，描述需要预加载的系统进程。
/// 对应 Minix3: `struct boot_image`
#[derive(Debug, Clone, Copy)]
pub struct BootImage {
    /// 进程名称（16字节，C风格字符串）
    pub proc_name: [u8; 16],
    /// 预设端点
    pub endpoint: Endpoint,
    /// 代码起始物理地址
    pub start_addr: PhysAddr,
    /// 代码长度（字节）
    pub len: usize,
    /// 启动标志
    pub flags: u32,
}

impl BootImage {
    /// 创建空的启动镜像
    pub const fn empty() -> Self {
        Self {
            proc_name: [0; 16],
            endpoint: Endpoint::NONE,
            start_addr: PhysAddr(0),
            len: 0,
            flags: 0,
        }
    }

    /// 获取进程名称字符串
    pub fn name(&self) -> &str {
        let len = self.proc_name.iter().position(|&b| b == 0).unwrap_or(16);
        core::str::from_utf8(&self.proc_name[..len]).unwrap_or("<invalid>")
    }
}

impl Default for BootImage {
    fn default() -> Self {
        Self::empty()
    }
}

/// 物理内存统计
///
/// VM 作为系统内存管理器，需要跟踪全局内存使用情况。
/// 这些统计用于内存分配决策和系统监控。
#[derive(Debug, Clone, Copy)]
pub struct MemoryStats {
    /// 系统总物理页数
    ///
    /// 由内核启动时检测并传递给 VM。
    pub total_pages: usize,

    /// 空闲物理页数
    ///
    /// 维护在物理内存分配器中（如伙伴系统）。
    pub free_pages: usize,

    /// 已用物理页数
    ///
    /// 计算：total_pages - free_pages
    pub used_pages: usize,
}

impl MemoryStats {
    /// 创建新的内存统计
    pub const fn new(total_pages: usize) -> Self {
        Self {
            total_pages,
            free_pages: total_pages,
            used_pages: 0,
        }
    }

    /// 计算内存使用率
    pub fn usage_ratio(&self) -> f64 {
        if self.total_pages == 0 {
            return 0.0;
        }
        self.used_pages as f64 / self.total_pages as f64
    }

    /// 检查内存压力
    pub fn is_under_pressure(&self) -> bool {
        self.usage_ratio() > 0.9
    }

    /// 分配页面
    pub fn alloc_pages(&mut self, count: usize) -> bool {
        if self.free_pages >= count {
            self.free_pages -= count;
            self.used_pages += count;
            true
        } else {
            false
        }
    }

    /// 释放页面
    pub fn free_pages(&mut self, count: usize) {
        self.free_pages += count;
        self.used_pages -= count;
    }
}

impl Default for MemoryStats {
    fn default() -> Self {
        Self::new(0)
    }
}

/// VM 全局状态
///
/// 封装所有 VM 模块的全局变量，提供类型安全的访问接口。
pub struct VmGlobalState {
    /// 启动信息（只读）
    pub boot_info: [BootImage; NR_BOOT_PROCS],

    /// 内存统计
    pub memory_stats: MemoryStats,

    /// VM 实例计数（原子操作）
    vm_instance_count: AtomicU32,
}

impl VmGlobalState {
    /// 创建新的全局状态
    pub const fn new() -> Self {
        Self {
            boot_info: [BootImage::empty(); NR_BOOT_PROCS],
            memory_stats: MemoryStats::new(0),
            vm_instance_count: AtomicU32::new(0),
        }
    }

    /// 初始化全局状态
    ///
    /// # Safety
    ///
    /// 只能在 VM 初始化时调用一次。
    pub unsafe fn init(&mut self, total_pages: usize) {
        self.memory_stats = MemoryStats::new(total_pages);
    }

    /// 增加 VM 实例计数
    pub fn inc_vm_instance(&self) {
        self.vm_instance_count.fetch_add(1, Ordering::SeqCst);
    }

    /// 减少 VM 实例计数
    pub fn dec_vm_instance(&self) {
        self.vm_instance_count.fetch_sub(1, Ordering::SeqCst);
    }

    /// 获取当前 VM 实例数
    pub fn vm_instance_count(&self) -> u32 {
        self.vm_instance_count.load(Ordering::SeqCst)
    }

    /// 查找启动镜像
    pub fn find_boot_image(&self, endpoint: Endpoint) -> Option<&BootImage> {
        self.boot_info.iter().find(|b| b.endpoint == endpoint)
    }
}

impl Default for VmGlobalState {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_boot_image_empty() {
        let img = BootImage::empty();
        assert_eq!(img.name(), "");
        assert_eq!(img.endpoint, Endpoint::NONE);
    }

    #[test]
    fn test_boot_image_name() {
        let mut img = BootImage::empty();
        img.proc_name = *b"kernel\0\0\0\0\0\0\0\0\0\0";
        assert_eq!(img.name(), "kernel");
    }

    #[test]
    fn test_memory_stats_new() {
        let stats = MemoryStats::new(1024);
        assert_eq!(stats.total_pages, 1024);
        assert_eq!(stats.free_pages, 1024);
        assert_eq!(stats.used_pages, 0);
    }

    #[test]
    fn test_memory_stats_alloc() {
        let mut stats = MemoryStats::new(100);
        assert!(stats.alloc_pages(50));
        assert_eq!(stats.free_pages, 50);
        assert_eq!(stats.used_pages, 50);

        // 分配超过可用数量
        assert!(!stats.alloc_pages(100));
        assert_eq!(stats.free_pages, 50);
    }

    #[test]
    fn test_memory_stats_free() {
        let mut stats = MemoryStats::new(100);
        stats.alloc_pages(30);
        stats.free_pages(10);
        assert_eq!(stats.free_pages, 80);
        assert_eq!(stats.used_pages, 20);
    }

    #[test]
    fn test_memory_stats_usage_ratio() {
        let mut stats = MemoryStats::new(100);
        assert_eq!(stats.usage_ratio(), 0.0);

        stats.alloc_pages(50);
        assert_eq!(stats.usage_ratio(), 0.5);
        assert!(!stats.is_under_pressure());

        stats.alloc_pages(41);  // 91/100 = 0.91 > 0.9
        assert!(stats.is_under_pressure());
    }

    #[test]
    fn test_vm_global_state_new() {
        let state = VmGlobalState::new();
        assert_eq!(state.vm_instance_count(), 0);
        assert_eq!(state.memory_stats.total_pages, 0);
    }

    #[test]
    fn test_vm_instance_count() {
        let state = VmGlobalState::new();
        state.inc_vm_instance();
        assert_eq!(state.vm_instance_count(), 1);

        state.inc_vm_instance();
        assert_eq!(state.vm_instance_count(), 2);

        state.dec_vm_instance();
        assert_eq!(state.vm_instance_count(), 1);
    }

    #[test]
    fn test_find_boot_image() {
        let mut state = VmGlobalState::new();
        state.boot_info[0].endpoint = Endpoint::PM;
        state.boot_info[0].proc_name = *b"pm\0\0\0\0\0\0\0\0\0\0\0\0\0\0";

        let found = state.find_boot_image(Endpoint::PM);
        assert!(found.is_some());
        assert_eq!(found.unwrap().name(), "pm");

        let not_found = state.find_boot_image(Endpoint::VM);
        assert!(not_found.is_none());
    }
}
