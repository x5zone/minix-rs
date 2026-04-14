//! PM 和 VM 联合测试
//!
//! 测试 fork 系统调用中 PM 和 VM 的协作。
//!
//! # 注意
//!
//! 由于 IPC 属于 kernel 模块（当前未实现），本测试使用 mock IPC。
//! 后续 kernel 实现后，需要替换为真实的 IPC 调用。

use minix_pm::mproc::{ProcTable, PmContext, Process, Lifecycle, ForkError};
use minix_vm::vmproc::{VmProcTable, VmProc, VmFlags};
use minix_vm::vmproc::fork::{VmForkRequest, VmForkResponse, VmForkError};
use minix_types::{Endpoint, UserSlot, Pid, NR_PROCS};

/// Mock PM-VM IPC 上下文
///
/// 模拟 PM 和 VM 之间的 IPC 通信。
/// 在真实系统中，这由 kernel 提供。
pub struct MockPmVmIpc {
    /// VM 进程表（模拟 VM 服务）
    pub vm_table: VmProcTable,
}

impl MockPmVmIpc {
    /// 创建新的 mock IPC 上下文
    pub fn new() -> Self {
        Self {
            vm_table: VmProcTable::new(),
        }
    }

    /// 模拟 PM -> VM 的 VM_FORK 请求
    ///
    /// 在真实系统中，这通过 IPC 消息发送：
    /// ```c
    /// message.m_type = VM_FORK;
    /// message.VMF_ENDPOINT = parent_endpoint;
    /// message.VMF_SLOT = child_slot;
    /// ipc_sendrec(VM_PROC_NR, &message);
    /// ```
    pub fn vm_fork(&mut self, request: &VmForkRequest) -> Result<VmForkResponse, VmForkError> {
        self.vm_table.handle_fork(request)
    }
}

impl Default for MockPmVmIpc {
    fn default() -> Self {
        Self::new()
    }
}

/// 创建测试用的 PM 上下文
fn create_pm_context() -> PmContext<'static> {
    use std::boxed::Box;
    let table = Box::leak(Box::new(ProcTable::new()));
    table.procs[0].state.lifecycle = Lifecycle::Running;
    PmContext::new(table, 0)
}

/// 创建测试用的 VM 进程表
fn create_vm_table() -> VmProcTable {
    let mut table = VmProcTable::new();

    let slot = table.alloc_slot().unwrap();
    let mut parent = VmProc::empty(slot);
    parent.endpoint = Endpoint::PM;
    parent.flags |= VmFlags::IN_USE;
    table.init_slot(parent);

    table
}

#[test]
fn test_pm_vm_fork_basic() {
    let mut pm_ctx = create_pm_context();
    let mut ipc = MockPmVmIpc::new();

    let parent_slot = ipc.vm_table.alloc_slot().unwrap();
    let parent = VmProc::empty(parent_slot);
    ipc.vm_table.init_slot(parent);

    let parent_endpoint = Endpoint::PM;
    ipc.vm_table.get_proc_mut(parent_slot).unwrap().endpoint = parent_endpoint;
    ipc.vm_table.get_proc_mut(parent_slot).unwrap().flags |= VmFlags::IN_USE;

    let fork_result = pm_ctx.do_fork_prepare().unwrap();

    let vm_request = VmForkRequest {
        parent_endpoint,
        child_slot: UserSlot::new(fork_result.child_index),
    };

    let child_slot = ipc.vm_table.alloc_slot().unwrap();
    let child = VmProc::empty(child_slot);
    ipc.vm_table.init_slot(child);

    let vm_response = ipc.vm_fork(&vm_request);
    assert!(vm_response.is_ok());

    pm_ctx.fork_child_from_parent(
        fork_result.child_index,
        fork_result.child_pid,
        fork_result.child_endpoint,
    );

    let child_proc = pm_ctx.table.get(fork_result.child_index).unwrap();
    assert!(child_proc.is_in_use());
    assert_eq!(child_proc.pid(), fork_result.child_pid);
}

#[test]
fn test_pm_vm_fork_parent_not_found() {
    let mut ipc = MockPmVmIpc::new();

    let request = VmForkRequest {
        parent_endpoint: Endpoint::NONE,
        child_slot: UserSlot::new(1),
    };

    let result = ipc.vm_fork(&request);
    assert!(matches!(result, Err(VmForkError::ParentNotFound)));
}

#[test]
fn test_pm_vm_fork_multiple_children() {
    let mut pm_ctx = create_pm_context();
    let mut ipc = MockPmVmIpc::new();

    let parent_slot = ipc.vm_table.alloc_slot().unwrap();
    let mut parent = VmProc::empty(parent_slot);
    parent.endpoint = Endpoint::PM;
    parent.flags |= VmFlags::IN_USE;
    ipc.vm_table.init_slot(parent);

    for _ in 0..3 {
        let fork_result = pm_ctx.do_fork_prepare().unwrap();

        let child_slot = ipc.vm_table.alloc_slot().unwrap();
        let child = VmProc::empty(child_slot);
        ipc.vm_table.init_slot(child);

        let vm_request = VmForkRequest {
            parent_endpoint: Endpoint::PM,
            child_slot: UserSlot::new(fork_result.child_index),
        };

        let vm_response = ipc.vm_fork(&vm_request);
        assert!(vm_response.is_ok());

        pm_ctx.fork_child_from_parent(
            fork_result.child_index,
            fork_result.child_pid,
            fork_result.child_endpoint,
        );
    }

    let used_count = pm_ctx.table.procs_in_use.get();
    assert!(used_count >= 3, "expected at least 3 processes in use");
}

#[test]
fn test_pm_vm_fork_endpoint_generation() {
    let mut ipc = MockPmVmIpc::new();

    let parent_slot = ipc.vm_table.alloc_slot().unwrap();
    let mut parent = VmProc::empty(parent_slot);
    parent.endpoint = Endpoint::PM;
    parent.flags |= VmFlags::IN_USE;
    ipc.vm_table.init_slot(parent);

    let child_slot = ipc.vm_table.alloc_slot().unwrap();
    let child = VmProc::empty(child_slot);
    ipc.vm_table.init_slot(child);

    let request = VmForkRequest {
        parent_endpoint: Endpoint::PM,
        child_slot,
    };

    let response = ipc.vm_fork(&request).unwrap();

    assert!(response.success);
    assert_ne!(response.child_endpoint, Endpoint::PM);
}

#[test]
fn test_pm_vm_fork_table_full() {
    let mut pm_ctx = create_pm_context();

    for i in 0..NR_PROCS {
        pm_ctx.table.procs[i].state.lifecycle = Lifecycle::Running;
    }
    pm_ctx.table.procs_in_use.set(NR_PROCS);

    let result = pm_ctx.do_fork_prepare();
    assert!(matches!(result, Err(ForkError::TableFull)));
}
