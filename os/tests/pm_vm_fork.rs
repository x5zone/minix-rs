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
use minix_pm::mproc::{ProcTable, PmContext, Lifecycle, ForkError};
use minix_vm::vmproc::VmProcTable;
use minix_vm::vmproc::fork::{VmForkRequest, VmForkResponse, VmForkError};
use minix_types::{Endpoint, UserSlot, NR_PROCS};

/// Mock PM-VM IPC context.
pub struct MockPmVmIpc {
    pub vm_table: VmProcTable,
}

impl MockPmVmIpc {
    pub fn new() -> Self {
        Self {
            vm_table: VmProcTable::new(),
        }
    }

    pub fn vm_fork(&mut self, request: &VmForkRequest) -> Result<VmForkResponse, VmForkError> {
        self.vm_table.handle_fork(request)
    }
}

impl Default for MockPmVmIpc {
    fn default() -> Self {
        Self::new()
    }
}

fn create_pm_context() -> PmContext<'static> {
    use std::boxed::Box;
    let table = Box::leak(Box::new(ProcTable::new()));
    table.procs[0].state.lifecycle = Lifecycle::Running;
    PmContext::new(table, 0)
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
fn test_pm_vm_fork_endpoint_generation() {
    let mut ipc = MockPmVmIpc::new();

    // Create parent process in slot 0
    let empty = ipc.vm_table.get_empty(UserSlot::new(0)).unwrap();
    let mut active = empty.activate_relaxed(Endpoint::PM);
    active.init_page_table().unwrap();
    active.init_regions();

    // Fork to slot 1 (child slot should be empty)
    let child_slot = UserSlot::new(1);
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
*/
