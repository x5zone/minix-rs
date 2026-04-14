//! VM Fork 处理
//!
//! 处理来自 PM 的 VM_FORK 请求，复制进程的地址空间。
//!
//! # Minix3 对应
//!
//! `minix3/minix/servers/vm/fork.c` 中的 `do_fork()` 函数
//!
//! # 流程
//!
//! 1. PM 调用 VM_FORK 请求 VM 复制父进程的地址空间
//! 2. VM 分配新的进程槽位
//! 3. VM 复制父进程的所有内存区域
//! 4. VM 设置 CoW（写时复制）标志
//! 5. VM 创建新的页表
//! 6. VM 返回子进程的 endpoint 给 PM

use minix_types::{Endpoint, UserSlot, VirBytes};
use crate::vmproc::{VmProc, VmFlags, VmProcTable};
use crate::region::{VirRegion, VrFlags, RegionAvl};

/// VM Fork 请求消息
///
/// 来自 PM 的 fork 请求
#[derive(Debug, Clone, Copy)]
pub struct VmForkRequest {
    /// 父进程 endpoint
    pub parent_endpoint: Endpoint,
    /// 子进程 slot（由 PM 分配）
    pub child_slot: UserSlot,
}

/// VM Fork 响应消息
#[derive(Debug, Clone, Copy)]
pub struct VmForkResponse {
    /// 子进程 endpoint
    pub child_endpoint: Endpoint,
    /// 是否成功
    pub success: bool,
}

/// Fork 错误类型
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VmForkError {
    /// 父进程不存在
    ParentNotFound,
    /// 子进程槽位无效
    InvalidChildSlot,
    /// 内存不足
    OutOfMemory,
    /// 内部错误
    InternalError,
}

impl VmForkError {
    pub fn to_errno(&self) -> i32 {
        match self {
            Self::ParentNotFound => 3,      // ESRCH
            Self::InvalidChildSlot => 22,   // EINVAL
            Self::OutOfMemory => 12,        // ENOMEM
            Self::InternalError => 5,       // EIO
        }
    }
}

/// Fork 上下文
///
/// 包含 fork 操作所需的所有信息
pub struct ForkContext<'a> {
    /// VM 进程表
    pub table: &'a mut VmProcTable,
    /// 父进程索引
    pub parent_index: usize,
    /// 子进程索引
    pub child_index: usize,
}

impl<'a> ForkContext<'a> {
    /// 创建新的 fork 上下文
    pub fn new(
        table: &'a mut VmProcTable,
        parent_index: usize,
        child_index: usize,
    ) -> Self {
        Self {
            table,
            parent_index,
            child_index,
        }
    }

    /// 执行 fork 操作
    ///
    /// 对应 Minix3: `do_fork()` 主逻辑
    ///
    /// # 步骤
    ///
    /// 1. 验证父进程存在且有效
    /// 2. 初始化子进程槽位
    /// 3. 复制内存区域（带 CoW）
    /// 4. 设置子进程标志
    pub fn do_fork(&mut self) -> Result<Endpoint, VmForkError> {
        let parent_endpoint;
        let parent_total;
        let parent_total_max;
        let parent_region_count;

        {
            let parent = self.table.get_proc(UserSlot::new(self.parent_index))
                .ok_or(VmForkError::ParentNotFound)?;

            if !parent.is_in_use() {
                return Err(VmForkError::ParentNotFound);
            }

            parent_endpoint = parent.endpoint;
            parent_total = parent.total;
            parent_total_max = parent.total_max;
            parent_region_count = parent.regions.len();
        }

        let child_endpoint = Endpoint::from_generation_slot(
            parent_endpoint.generation().wrapping_add(1),
            self.child_index as i32,
        );

        {
            let child = self.table.get_proc_mut(UserSlot::new(self.child_index))
                .ok_or(VmForkError::InvalidChildSlot)?;

            child.flags &= VmFlags::IN_USE;
            child.endpoint = child_endpoint;
            child.total = parent_total;
            child.total_max = parent_total_max;
        }

        self.copy_regions_with_cow(parent_region_count)?;

        Ok(child_endpoint)
    }

    /// 复制内存区域并设置 CoW
    ///
    /// 对应 Minix3: 复制 `vm_regions_avl` 并设置共享标志
    ///
    /// 在 Minix3 中，fork 时子进程共享父进程的物理页面，
    /// 通过增加引用计数实现。当任一进程写入时触发 CoW。
    fn copy_regions_with_cow(&mut self, _region_count: usize) -> Result<(), VmForkError> {
        let child = self.table.get_proc_mut(UserSlot::new(self.child_index))
            .ok_or(VmForkError::InvalidChildSlot)?;

        for region in child.regions.iter_mut() {
            region.flags.insert(VrFlags::WRITABLE);

            for phys_block in &mut region.physblocks {
                if let Some(pb) = phys_block {
                    if let Some(block) = pb.ph {
                        unsafe {
                            (*block).add_ref();
                        }
                    }
                }
            }

            unsafe {
                region.prepare_cow();
            }
        }

        Ok(())
    }
}

impl VmProcTable {
    /// 处理 VM_FORK 请求
    ///
    /// 这是 VM 服务的主入口点，由 PM 调用。
    pub fn handle_fork(
        &mut self,
        request: &VmForkRequest,
    ) -> Result<VmForkResponse, VmForkError> {
        let parent_proc = self.find_by_endpoint(request.parent_endpoint)
            .ok_or(VmForkError::ParentNotFound)?;

        let parent_index = parent_proc.slot.get();

        let child_index = request.child_slot.get();

        let mut ctx = ForkContext::new(self, parent_index, child_index);
        let child_endpoint = ctx.do_fork()?;

        Ok(VmForkResponse {
            child_endpoint,
            success: true,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vmproc::VmProc;

    fn create_test_table() -> VmProcTable {
        let mut table = VmProcTable::new();

        let slot = table.alloc_slot().unwrap();
        let mut parent = VmProc::empty(slot);
        parent.endpoint = Endpoint::PM;
        parent.flags |= VmFlags::IN_USE;
        table.init_slot(parent);

        table
    }

    #[test]
    fn test_fork_request_creation() {
        let request = VmForkRequest {
            parent_endpoint: Endpoint::PM,
            child_slot: UserSlot::new(1),
        };

        assert_eq!(request.parent_endpoint, Endpoint::PM);
        assert_eq!(request.child_slot.get(), 1);
    }

    #[test]
    fn test_fork_response_creation() {
        let response = VmForkResponse {
            child_endpoint: Endpoint(100),
            success: true,
        };

        assert!(response.success);
    }

    #[test]
    fn test_fork_error_to_errno() {
        assert_eq!(VmForkError::ParentNotFound.to_errno(), 3);
        assert_eq!(VmForkError::OutOfMemory.to_errno(), 12);
        assert_eq!(VmForkError::InternalError.to_errno(), 5);
    }

    #[test]
    fn test_fork_context_creation() {
        let mut table = create_test_table();
        let ctx = ForkContext::new(&mut table, 0, 1);

        assert_eq!(ctx.parent_index, 0);
        assert_eq!(ctx.child_index, 1);
    }

    #[test]
    fn test_handle_fork_parent_not_found() {
        let mut table = create_test_table();

        let request = VmForkRequest {
            parent_endpoint: Endpoint::NONE,
            child_slot: UserSlot::new(1),
        };

        let result = table.handle_fork(&request);
        assert!(matches!(result, Err(VmForkError::ParentNotFound)));
    }

    #[test]
    fn test_handle_fork_success() {
        let mut table = create_test_table();

        let child_slot = table.alloc_slot().unwrap();
        let child = VmProc::empty(child_slot);
        table.init_slot(child);

        let request = VmForkRequest {
            parent_endpoint: Endpoint::PM,
            child_slot,
        };

        let result = table.handle_fork(&request);
        assert!(result.is_ok());

        let response = result.unwrap();
        assert!(response.success);
    }
}
