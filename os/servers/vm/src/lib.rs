//! Minix VM (Virtual Memory Manager) - 公共库
//!
//! 虚拟内存管理器公共类型定义，可供其他模块使用。
//!
//! # 设计原则
//!
//! 本 crate 同时提供：
//! - **库**：公共类型定义，供其他服务使用
//! - **服务**：独立的 VM 服务进程
//!
//! # 公共 API
//!
//! - [`VmProc`]: VM 进程结构体
//! - [`VmProcTable`]: VM 进程表
//! - [`VmFlags`]: 进程状态标志
//! - [`AclIndex`]: ACL 权限索引
//! - [`PageTable`]: 页表（Mock 版本）
//! - [`Region`]: 内存区域（Mock 版本）
//! - [`PhysMemAllocator`]: 物理内存分配器
//!
//! # 示例
//!
//! ```rust
//! use minix_vm::{VmProcTable, VmProc, VmFlags};
//! use minix_types::{UserSlot, Endpoint};
//!
//! // 创建进程表
//! let mut table = VmProcTable::new();
//!
//! // 分配槽位并初始化进程
//! let slot = table.alloc_slot().unwrap();
//! let mut proc = VmProc::empty(slot);
//! proc.endpoint = Endpoint::PM;
//! proc.flags |= VmFlags::IN_USE;
//! table.init_slot(proc);
//! ```

pub mod vmproc;
pub mod acl;
pub mod slab;
pub mod phys_mem;
pub mod region;
pub mod pagetable;
pub mod memtype;

pub use vmproc::*;
pub use acl::{AclManager, NO_ACL, USER_ACL, FIRST_SYS_ACL, NR_SYS_PROCS, VM_CALL_MASK_SIZE};
pub use slab::{SlabCache, SlabStats, LeakReport, MockPageAllocator};
pub use phys_mem::{PhysMemAllocator, AllocFlags, PhysAddr, MemStats, CLICK_SIZE, CLICK_SHIFT};
pub use region::{VirRegion, VrFlags, PhysRegion, PhysBlock, RegionAvl};
pub use pagetable::{PageTable, PtFlags, PageTableError, PAGE_SIZE, PT_ENTRIES, PD_ENTRIES};
pub use memtype::{
    MemType, MemTypeError, PagefaultResult,
    AnonymousMemory, DirectPhysical, SharedMemory,
    MEM_TYPE_ANON, MEM_TYPE_DIRECT, MEM_TYPE_SHARED,
};
