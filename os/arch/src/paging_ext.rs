//! 分页扩展 trait
//!
//! 可选的页表功能扩展，非所有架构都支持：
//! - `PagingWithId`: TLB 进程标识（PCID/ASID）
//! - `HugePages`: 大页支持
//! - `VmPagingExt`: VM 进程管理专用操作
//!
//! # 未来扩展
//!
//! 以下功能待实际需求出现时添加到此模块：
//!
//! - **`clear_dirty()`**：页面置换算法（Clock/LRU）需要周期性清除 dirty 位
//!   以检测页面被重新写入。当前可通过 `update_flags()` 实现，但专用方法
//!   可在 x86-64 上使用原子 RMW 操作（如 `LOCK CMPXCHG16B`），避免
//!   读取-修改-写回期间硬件更新丢失。

use minix_types::{Endpoint, PhysBytes, VirBytes};
use crate::paging::{PageFlags, PageTableError, Paging};

/// VM process management paging operations (optional trait)
///
/// These operations correspond to Minix3's `pt_bind()` and `pt_mapkernel()`.
/// They are VM policy operations, not pure paging hardware mechanisms:
///
/// - `bind_to_process()`: involves kernel IPC (`sys_vmctl_set_addrspace`),
///   not a pure page table operation
/// - `map_kernel()`: writes kernel mappings into a user process page table,
///   a VM strategy decision rather than hardware mechanism
///
/// Separated from `Paging` trait because they are VM-layer policy,
/// not hardware abstraction.
pub trait VmPagingExt: Paging {
    /// Bind this page table to a process in the kernel.
    ///
    /// Corresponds to Minix3's `pt_bind()` which calls
    /// `sys_vmctl_set_addrspace()` to register the page table
    /// with the kernel for the given process endpoint.
    fn bind_to_process(&self, endpoint: Endpoint) -> Result<(), PageTableError>;

    /// Map kernel address space into this page table.
    ///
    /// Corresponds to Minix3's `pt_mapkernel()`. Must be called
    /// after `Paging::new()` before the process runs, so that
    /// kernel entry points are accessible from user space.
    fn map_kernel(&mut self) -> Result<(), PageTableError>;
}

/// TLB 进程标识支持（可选 trait）
///
/// 为 TLB 条目标记进程标识，避免上下文切换时刷新整个 TLB，
/// 从而显著减少 TLB miss 开销。
///
/// **注意**：本 trait 不定义 ASID/PCID 的生命周期语义和复用策略。
/// ASID 的分配/回收策略、TLB shootdown 一致性维护、generation counter
/// 等机制由上层 VM 管理器负责。本 trait 仅提供底层硬件操作的原语。
pub trait PagingWithId: Paging {
    type AddressSpaceId: Copy + Eq + core::fmt::Debug + Send;

    /// Allocate an Address Space ID (ASID/PCID).
    ///
    /// The `&self` receiver allows the implementation to decide the allocation
    /// strategy: it may use a global pool, a per-CPU pool, or a per-page-table
    /// pool. Minix3 does not implement PCID/ASID management, so there is no
    /// direct C counterpart. The allocation strategy is an implementation detail
    /// left to each architecture.
    fn alloc_asid(&self) -> Result<Self::AddressSpaceId, PageTableError>;

    /// Free a previously allocated ASID/PCID.
    ///
    /// The `&self` receiver mirrors `alloc_asid` so that the same allocation
    /// context is used for both operations.
    fn free_asid(&self, id: Self::AddressSpaceId);

    /// Activate this page table with the given ASID on the current CPU.
    ///
    /// # Safety
    ///
    /// Caller must ensure:
    /// - The page table is fully initialized (kernel mappings present)
    /// - The ASID was allocated via `alloc_asid()` and has not been freed
    /// - On SMP systems, proper TLB shootdown is performed if needed
    unsafe fn switch_with_asid(&self, id: Self::AddressSpaceId);

    /// Flush TLB entries for the given ASID.
    ///
    /// # Safety
    ///
    /// Caller must ensure:
    /// - This is called in a valid MMU context (a page table is active)
    /// - The ASID is valid and currently in use
    unsafe fn flush_tlb_asid(&self, id: Self::AddressSpaceId);

    /// Flush the TLB entry for a single virtual address within the given ASID.
    ///
    /// # Safety
    ///
    /// Caller must ensure:
    /// - `vaddr` falls within the currently active page table's valid range
    /// - The ASID is valid and currently in use
    unsafe fn flush_tlb_addr_asid(&self, vaddr: VirBytes, id: Self::AddressSpaceId);
}

/// 大页支持（可选 trait）
pub trait HugePages: Paging {
    const HUGE_PAGE_SIZES: &'static [usize];

    fn map_huge(
        &mut self,
        vaddr: VirBytes,
        paddr: PhysBytes,
        size: usize,
        flags: PageFlags,
    ) -> Result<(), PageTableError>;

    fn supports_huge_page(size: usize) -> bool {
        Self::HUGE_PAGE_SIZES.contains(&size)
    }
}
