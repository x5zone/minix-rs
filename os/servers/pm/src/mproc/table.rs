//! PM 进程表结构体定义
//!
//! 这是 Minix3 `mproc[NR_PROCS]` 的 Rust 实现，包含 PM 私有的进程表管理逻辑。
//!
//! # Minix3 多进程表架构
//! Minix3 采用分布式进程表设计，共有 4 份进程表：
//! - **PM/mproc**: 进程管理、信号、权限（本模块）
//! - **VM/vmproc**: 虚拟内存、页表
//! - **VFS/fproc**: 文件描述符、目录
//! - **Kernel/proc**: 调度、IPC、寄存器保存
//!
//! # 设计决策
//! - 使用静态数组 `[Process; NR_PROCS]` 保证地址稳定
//! - 使用 `Cell<usize>` 实现内部可变性（单线程安全）
//! - 保留 `IN_USE` 语义（`Lifecycle::Unused`）
//!
//! # Endpoint 与 Generation
//!
//! Minix3 的 Endpoint 格式：
//! ```text
//! endpoint = (generation << 15) + proc_nr
//! ```
//!
//! - **低 15 位**：process slot number（进程槽位号）
//! - **高 17 位**：generation（代数）
//!
//! Generation 的作用：
//! - 防止"过时的消息发给新进程"
//! - 每次槽位释放时 generation +1
//! - 嵌入在 endpoint 中，**不需要单独存储**
//!
//! # 为什么放在 PM crate 而不是 minix-types？
//!
//! 1. **职责隔离**: 进程表槽位分配是 PM 的私有逻辑
//! 2. **不变量保护**: 槽位分配/释放逻辑绑定了 PM 内部状态
//! 3. **微内核原则**: 其他服务不需要了解 PM 的进程表实现

use core::cell::Cell;
use minix_types::{Endpoint, NR_PROCS, LAST_FEW};
use crate::mproc::{Process, Lifecycle};

/// Endpoint generation 位移
///
/// Minix3 定义：`#define _ENDPOINT_GENERATION_SHIFT 15`
pub const ENDPOINT_GENERATION_SHIFT: u32 = 15;

/// PM 进程表
///
/// 存储所有 PM 进程结构体，提供槽位分配功能
///
/// # 内存布局
/// ```text
/// ProcTable {
///     procs: [Process; 256],      // ~22.5 KB
///     procs_in_use: Cell<usize>,  // 8 bytes
///     next_child: Cell<usize>,    // 8 bytes
/// }
/// ```
///
/// # 注意
///
/// Generation 嵌入在 `Process.endpoint` 中，不需要单独存储。
/// 这符合 Minix3 的设计原则：**唯一 truth**。
#[derive(Debug)]
pub struct ProcTable {
    /// 进程数组
    pub procs: [Process; NR_PROCS],
    /// 当前使用的进程数
    pub procs_in_use: Cell<usize>,
    /// 下一个子进程槽位（轮询算法）
    pub next_child: Cell<usize>,
}

impl ProcTable {
    /// 创建新的进程表
    ///
    /// 所有槽位初始化为 `Lifecycle::Unused`
    pub fn new() -> Self {
        Self {
            procs: core::array::from_fn(|_| Process::default()),
            procs_in_use: Cell::new(0),
            next_child: Cell::new(0),
        }
    }
    
    /// 获取进程引用
    pub fn get(&self, index: usize) -> Option<&Process> {
        if index < NR_PROCS {
            Some(&self.procs[index])
        } else {
            None
        }
    }
    
    /// 获取进程可变引用
    pub fn get_mut(&mut self, index: usize) -> Option<&mut Process> {
        if index < NR_PROCS {
            Some(&mut self.procs[index])
        } else {
            None
        }
    }
    
    /// 获取当前使用的进程数
    pub fn count(&self) -> usize {
        self.procs_in_use.get()
    }
    
    /// 检查进程表是否已满
    pub fn is_full(&self) -> bool {
        self.procs_in_use.get() >= NR_PROCS
    }
    
    /// 检查非 root 用户是否可以分配槽位
    ///
    /// 对应 Minix3 的检查：
    /// ```c
    /// if (procs_in_use >= NR_PROCS-LAST_FEW && rmp->mp_effuid != 0)
    /// ```
    pub fn can_alloc_for_user(&self, is_root: bool) -> bool {
        let count = self.procs_in_use.get();
        if count >= NR_PROCS {
            return false;
        }
        if count >= NR_PROCS - LAST_FEW && !is_root {
            return false;
        }
        true
    }
    
    /// 查找空闲槽位（轮询算法）
    ///
    /// 对应 Minix3 的 `do_fork` 中的轮询查找：
    /// ```c
    /// do {
    ///     next_child = (next_child+1) % NR_PROCS;
    ///     n++;
    /// } while((mproc[next_child].mp_flags & IN_USE) && n <= NR_PROCS);
    /// ```
    ///
    /// # 返回值
    /// - `Some(usize)`: 找到的空闲槽位索引
    /// - `None`: 进程表已满
    pub fn find_free_slot(&self) -> Option<usize> {
        let start = self.next_child.get();
        
        for i in 0..NR_PROCS {
            let idx = (start + i) % NR_PROCS;
            if !self.procs[idx].is_in_use() {
                self.next_child.set((idx + 1) % NR_PROCS);
                return Some(idx);
            }
        }
        
        None
    }
    
    /// 分配槽位
    ///
    /// 查找空闲槽位并标记为使用中
    ///
    /// # 返回值
    /// - `Some(usize)`: 分配的槽位索引
    /// - `None`: 进程表已满
    pub fn alloc_slot(&self) -> Option<usize> {
        let slot = self.find_free_slot()?;
        self.procs_in_use.set(self.procs_in_use.get() + 1);
        Some(slot)
    }
    
    /// 释放槽位
    ///
    /// 将槽位标记为未使用，增加 generation
    ///
    /// 注意：此方法不检查进程状态，只减少计数器
    /// 调用者负责确保进程状态已正确重置
    pub fn release_slot(&mut self, index: usize) {
        if index < NR_PROCS && self.procs_in_use.get() > 0 {
            self.procs_in_use.set(self.procs_in_use.get() - 1);
            
            // 增加 generation，嵌入到 endpoint 中
            let old_endpoint = self.procs[index].endpoint();
            let new_endpoint = Self::increment_endpoint_generation(old_endpoint);
            self.procs[index].identity.endpoint = new_endpoint;
        }
    }
    
    /// 计算 Endpoint
    ///
    /// Minix3 公式：`endpoint = (generation << 15) + proc_nr`
    ///
    /// 注意：这是为新进程计算初始 endpoint（generation = 0）
    pub fn calculate_endpoint(index: usize) -> Endpoint {
        if index < NR_PROCS {
            // 初始 generation = 0
            Endpoint::new(index as i32)
        } else {
            Endpoint::NONE
        }
    }
    
    /// 从 Endpoint 解析索引
    ///
    /// Minix3 公式：`proc_nr = endpoint & 0x7FFF`
    pub fn endpoint_to_index(endpoint: Endpoint) -> usize {
        (endpoint.get() & 0x7FFF) as usize
    }
    
    /// 从 Endpoint 解析代数
    ///
    /// Minix3 公式：`generation = endpoint >> 15`
    pub fn endpoint_to_generation(endpoint: Endpoint) -> u32 {
        (endpoint.get() >> ENDPOINT_GENERATION_SHIFT) as u32
    }
    
    /// 增加 Endpoint 的 generation
    ///
    /// 用于槽位释放时，防止过时消息发送到新进程
    fn increment_endpoint_generation(endpoint: Endpoint) -> Endpoint {
        let generation = Self::endpoint_to_generation(endpoint);
        let index = Self::endpoint_to_index(endpoint);
        let new_gen = generation + 1;
        
        // 新 endpoint = (new_generation << 15) + index
        let new_value = ((new_gen as i32) << ENDPOINT_GENERATION_SHIFT) + (index as i32);
        Endpoint::new(new_value)
    }
    
    /// 验证 Endpoint 是否有效
    ///
    /// 检查 endpoint 的 generation 是否与进程表中的匹配
    pub fn validate_endpoint(&self, endpoint: Endpoint) -> bool {
        let index = Self::endpoint_to_index(endpoint);
        
        if index >= NR_PROCS {
            return false;
        }
        
        // 检查 endpoint 是否匹配
        self.procs[index].endpoint() == endpoint
    }
}

impl Default for ProcTable {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_proc_table_new() {
        let table = ProcTable::new();
        assert_eq!(table.count(), 0);
        assert!(!table.is_full());
    }
    
    #[test]
    fn test_find_free_slot() {
        let table = ProcTable::new();
        let slot = table.find_free_slot().unwrap();
        assert!(slot < NR_PROCS);
    }
    
    #[test]
    fn test_alloc_slot() {
        let table = ProcTable::new();
        let slot = table.alloc_slot().unwrap();
        assert!(slot < NR_PROCS);
        assert_eq!(table.count(), 1);
    }
    
    #[test]
    fn test_release_slot() {
        let mut table = ProcTable::new();
        
        let slot = table.alloc_slot().unwrap();
        assert_eq!(table.count(), 1);
        
        // 设置进程的 endpoint
        table.procs[slot].identity.endpoint = ProcTable::calculate_endpoint(slot);
        let gen_before = ProcTable::endpoint_to_generation(table.procs[slot].endpoint());
        
        table.release_slot(slot);
        assert_eq!(table.count(), 0);
        
        let gen_after = ProcTable::endpoint_to_generation(table.procs[slot].endpoint());
        assert_eq!(gen_after, gen_before + 1);
    }
    
    #[test]
    fn test_can_alloc_for_user() {
        let table = ProcTable::new();
        
        assert!(table.can_alloc_for_user(false));
        assert!(table.can_alloc_for_user(true));
    }
    
    #[test]
    fn test_endpoint_calculation() {
        let endpoint = ProcTable::calculate_endpoint(5);
        let index = ProcTable::endpoint_to_index(endpoint);
        let gen_val = ProcTable::endpoint_to_generation(endpoint);
        
        assert_eq!(index, 5);
        assert_eq!(gen_val, 0);
    }
    
    #[test]
    fn test_endpoint_generation_increment() {
        let endpoint1 = ProcTable::calculate_endpoint(5);
        assert_eq!(ProcTable::endpoint_to_generation(endpoint1), 0);
        
        let endpoint2 = ProcTable::increment_endpoint_generation(endpoint1);
        assert_eq!(ProcTable::endpoint_to_generation(endpoint2), 1);
        assert_eq!(ProcTable::endpoint_to_index(endpoint2), 5);
        
        let endpoint3 = ProcTable::increment_endpoint_generation(endpoint2);
        assert_eq!(ProcTable::endpoint_to_generation(endpoint3), 2);
        assert_eq!(ProcTable::endpoint_to_index(endpoint3), 5);
    }
    
    #[test]
    fn test_endpoint_after_release() {
        let mut table = ProcTable::new();
        
        let slot = table.alloc_slot().unwrap();
        table.procs[slot].identity.endpoint = ProcTable::calculate_endpoint(slot);
        
        let endpoint_before = table.procs[slot].endpoint();
        
        table.release_slot(slot);
        
        let endpoint_after = table.procs[slot].endpoint();
        assert_ne!(endpoint_before, endpoint_after);
        
        // 旧的 endpoint 不再有效
        assert!(!table.validate_endpoint(endpoint_before));
        // 新的 endpoint 有效
        assert!(table.validate_endpoint(endpoint_after));
    }
}
