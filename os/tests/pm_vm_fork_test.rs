//! PM 和 VM 联合测试
//!
//! 测试 fork 系统调用中 PM 和 VM 的协作。
//!
//! # 测试范围
//!
//! 1. PM 分配进程槽位
//! 2. PM 调用 VM fork
//! 3. VM 复制地址空间
//! 4. PM 初始化子进程
//!
//! # 限制
//!
//! 由于 IPC（kernel）尚未实现，使用模拟的 IPC 机制。

#[cfg(test)]
mod tests {
    use minix_pm::mproc::{ProcTable, PmContext, Process, Lifecycle, ForkError};
    use minix_vm::vmproc::{VmProcTable, VmProc, VmFlags};
    use minix_vm::vmproc::fork::{VmForkRequest, VmForkResponse, VmForkError};
    use minix_types::{Endpoint, UserSlot, Pid, NR_PROCS};

    /// 模拟的 PM-VM IPC 接口
    ///
    /// 在真实的 Minix3 中，PM 通过 IPC 消息调用 VM。
    /// 这里我们模拟这个接口。
    struct MockPmVmIpc {
        vm_table: VmProcTable,
    }

    impl MockPmVmIpc {
        fn new() -> Self {
            Self {
                vm_table: VmProcTable::new(),
            }
        }

        /// 模拟 PM -> VM 的 VM_FORK 请求
        fn vm_fork(&mut self, request: &VmForkRequest) -> Result<VmForkResponse, VmForkError> {
            self.vm_table.handle_fork(request)
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

        for i in 0..3 {
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

        let mut child_count = 0;
        for i in 0..NR_PROCS {
            if pm_ctx.table.procs[i].is_in_use() && i != 0 {
                child_count += 1;
            }
        }
        assert_eq!(child_count, 3);
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
}
