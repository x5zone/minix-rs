//! PM and VM integration test.
//!
//! **DEPRECATED**: This test file is permanently disabled.
//!
//! VM is an independent user-space process — external crates cannot access
//! its internal types (all `pub(crate)`). PM and VM communicate via IPC
//! messages, not direct table manipulation.
//!
//! Future integration tests should use mock IPC to simulate PM↔VM communication,
//! not `use minix_vm::vmproc::VmProcTable`.

/*
#[cfg(test)]
mod tests {
    use minix_pm::mproc::{ProcTable, PmContext, Process, Lifecycle, ForkError};
    use minix_vm::vmproc::{VmProcTable, VmFlags, EmptySlot};
    use minix_vm::vmproc::fork::{VmForkRequest, VmForkResponse, VmForkError};
    use minix_types::{Endpoint, UserSlot, Pid, NR_PROCS};

    struct MockPmVmIpc {
        vm_table: VmProcTable,
    }

    impl MockPmVmIpc {
        fn new() -> Self {
            Self {
                vm_table: VmProcTable::new(),
            }
        }

        fn vm_fork(&mut self, request: &VmForkRequest) -> Result<VmForkResponse, VmForkError> {
            self.vm_table.handle_fork(request)
        }
    }

    fn create_pm_context() -> PmContext<'static> {
        use std::boxed::Box;
        let table = Box::leak(Box::new(ProcTable::new()));
        table.procs[0].state.lifecycle = Lifecycle::Running;
        PmContext::new(table, 0)
    }

    fn create_vm_table() -> VmProcTable {
        let mut table = VmProcTable::new();

        let empty = table.as_empty(UserSlot::new(0)).unwrap();
        let _active = empty.activate(Endpoint::PM).unwrap();

        table
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
    fn test_pm_vm_fork_success() {
        let mut ipc = MockPmVmIpc::new();

        // Setup: Create parent process
        let empty = ipc.vm_table.as_empty(UserSlot::new(0)).unwrap();
        let _active = empty.activate(Endpoint::PM).unwrap();

        let request = VmForkRequest {
            parent_endpoint: Endpoint::PM,
            child_slot: UserSlot::new(1),
        };

        let result = ipc.vm_fork(&request);
        assert!(result.is_ok());

        let response = result.unwrap();
        assert!(response.success);
    }

    #[test]
    fn test_pm_vm_fork_child_slot_not_empty() {
        let mut ipc = MockPmVmIpc::new();

        // Setup: Fill both slots
        let empty = ipc.vm_table.as_empty(UserSlot::new(0)).unwrap();
        let _active = empty.activate(Endpoint::PM).unwrap();

        let empty = ipc.vm_table.as_empty(UserSlot::new(1)).unwrap();
        let _active = empty.activate(Endpoint::RS).unwrap();

        let request = VmForkRequest {
            parent_endpoint: Endpoint::PM,
            child_slot: UserSlot::new(1),
        };

        let result = ipc.vm_fork(&request);
        assert!(matches!(result, Err(VmForkError::ChildSlotNotEmpty)));
    }
}
*/
