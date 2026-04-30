//! Message dispatcher for Kernel.
//!
//! Routes incoming IPC messages to appropriate handlers.

use minix_types::{KernelRequest, KernelResponse, KernelError, Endpoint, UserSlot};
use crate::proc::{KProcess, rts};

/// Dispatches Kernel requests to appropriate handlers.
///
/// This is the central routing point for all Kernel IPC messages.
pub struct MessageDispatcher;

impl MessageDispatcher {
    /// Dispatches a Kernel request to the appropriate handler.
    ///
    /// # Arguments
    /// * `procs` - Reference to the kernel process table
    /// * `request` - The incoming Kernel request
    ///
    /// # Returns
    /// The response to send back to the caller.
    pub fn dispatch(
        procs: &mut [KProcess],
        request: KernelRequest,
    ) -> KernelResponse {
        match request {
            KernelRequest::Fork { parent_endpoint, child_endpoint, child_slot } => {
                Self::handle_fork_request(procs, parent_endpoint, child_endpoint, child_slot)
            }
        }
    }

    /// Handles fork request from PM.
    fn handle_fork_request(
        procs: &mut [KProcess],
        parent_endpoint: Endpoint,
        child_endpoint: Endpoint,
        child_slot: UserSlot,
    ) -> KernelResponse {
        match handle_sys_fork(procs, parent_endpoint, child_endpoint, child_slot) {
            Ok(()) => KernelResponse::ForkOk,
            Err(e) => KernelResponse::Error(e),
        }
    }
}

/// Handles sys_fork - creates child process's kernel structure.
///
/// This is called by PM during fork to create the child's proc entry.
///
/// # Arguments
/// * `procs` - Kernel process table
/// * `parent_endpoint` - Parent process endpoint
/// * `child_endpoint` - Child process endpoint
/// * `child_slot` - Child process slot
///
/// # Returns
/// * `Ok(())` - Success
/// * `Err(e)` - Error
pub fn handle_sys_fork(
    procs: &mut [KProcess],
    parent_endpoint: Endpoint,
    child_endpoint: Endpoint,
    child_slot: UserSlot,
) -> Result<(), KernelError> {
    // 1. Find parent process
    let parent_index = find_proc_by_endpoint(procs, parent_endpoint)
        .ok_or(KernelError::InvalidEndpoint)?;

    // 2. Get child slot
    let child_index = child_slot.get();
    if child_index >= procs.len() {
        return Err(KernelError::ProcTableFull);
    }

    // 3. Check if child slot is free
    if !procs[child_index].p_rts_flags.is_set(rts::SLOT_FREE) {
        return Err(KernelError::SlotInUse);
    }

    // 4. Copy parent's proc structure to child
    copy_proc(procs, parent_index, child_index, child_endpoint);

    Ok(())
}

/// Finds process index by endpoint.
///
/// Only returns processes that are not SLOT_FREE.
fn find_proc_by_endpoint(procs: &[KProcess], endpoint: Endpoint) -> Option<usize> {
    procs.iter().position(|p| {
        p.p_endpoint == endpoint && !p.p_rts_flags.is_set(rts::SLOT_FREE)
    })
}

/// Copies parent's proc structure to child.
fn copy_proc(
    procs: &mut [KProcess],
    parent_index: usize,
    child_index: usize,
    child_endpoint: Endpoint,
) {
    // Extract values from parent first to avoid borrow conflict
    let parent_priority;
    let parent_name;
    
    {
        let parent = &procs[parent_index];
        parent_priority = parent.p_sched.priority.load(core::sync::atomic::Ordering::Acquire);
        parent_name = parent.p_name;
    }

    // Now modify child
    let child = &mut procs[child_index];

    // Copy basic fields
    child.p_nr = child_index as i32;
    child.p_endpoint = child_endpoint;

    // Copy scheduling fields
    child.p_sched.priority.store(
        parent_priority,
        core::sync::atomic::Ordering::Release,
    );

    // Set runtime flags - child starts with NO_PRIV until PM sets privileges
    child.p_rts_flags.set(rts::NO_PRIV);

    // Copy IPC queue pointers (reset for child)
    child.p_nextready = None;
    child.p_caller_q = None;
    child.p_q_link = None;

    // Copy signal pending (clear for child)
    child.p_pending.clear();

    // Copy process name with suffix
    let mut name = parent_name;
    name.push_suffix("_c");
    child.p_name = name;

    // Clear message buffers
    child.p_sendmsg = minix_types::Message::default();
    child.p_delivermsg = minix_types::Message::default();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::proc::{RtsFlags, MiscFlags, SchedFields, Accounting, TimeStats, CyclesStats, ProcName};
    use minix_types::Endpoint;

    fn create_test_procs() -> [KProcess; 4] {
        use crate::arch::ExtRegState;

        // Slot 0: Parent process
        let parent = KProcess {
            p_nr: 0,
            p_endpoint: Endpoint::from_generation_slot(1, 0),
            p_rts_flags: RtsFlags::new(0),  // Not SLOT_FREE
            p_misc_flags: MiscFlags::new(0),
            p_sched: SchedFields::with_priority(7),
            p_accounting: Accounting::new(),
            p_time: TimeStats::new(),
            p_cycles: CyclesStats::new(),
            p_nextready: None,
            p_caller_q: None,
            p_q_link: None,
            p_getfrom_e: Endpoint::NONE,
            p_sendto_e: Endpoint::NONE,
            p_pending: crate::proc::SigSet::empty(),
            p_name: ProcName::from_str("parent"),
            p_sendmsg: minix_types::Message::default(),
            p_delivermsg: minix_types::Message::default(),
            p_delivermsg_vir: minix_types::VirBytes(0),
            p_ext_reg_state: ExtRegState::new(),
        };

        // Slots 1-3: Free slots
        let free1 = KProcess {
            p_nr: 1,
            p_endpoint: Endpoint::NONE,
            p_rts_flags: RtsFlags::new(rts::SLOT_FREE),
            p_misc_flags: MiscFlags::new(0),
            p_sched: SchedFields::new(),
            p_accounting: Accounting::new(),
            p_time: TimeStats::new(),
            p_cycles: CyclesStats::new(),
            p_nextready: None,
            p_caller_q: None,
            p_q_link: None,
            p_getfrom_e: Endpoint::NONE,
            p_sendto_e: Endpoint::NONE,
            p_pending: crate::proc::SigSet::empty(),
            p_name: ProcName::new(),
            p_sendmsg: minix_types::Message::default(),
            p_delivermsg: minix_types::Message::default(),
            p_delivermsg_vir: minix_types::VirBytes(0),
            p_ext_reg_state: ExtRegState::new(),
        };

        let free2 = KProcess {
            p_nr: 2,
            p_endpoint: Endpoint::NONE,
            p_rts_flags: RtsFlags::new(rts::SLOT_FREE),
            p_misc_flags: MiscFlags::new(0),
            p_sched: SchedFields::new(),
            p_accounting: Accounting::new(),
            p_time: TimeStats::new(),
            p_cycles: CyclesStats::new(),
            p_nextready: None,
            p_caller_q: None,
            p_q_link: None,
            p_getfrom_e: Endpoint::NONE,
            p_sendto_e: Endpoint::NONE,
            p_pending: crate::proc::SigSet::empty(),
            p_name: ProcName::new(),
            p_sendmsg: minix_types::Message::default(),
            p_delivermsg: minix_types::Message::default(),
            p_delivermsg_vir: minix_types::VirBytes(0),
            p_ext_reg_state: ExtRegState::new(),
        };

        let free3 = KProcess {
            p_nr: 3,
            p_endpoint: Endpoint::NONE,
            p_rts_flags: RtsFlags::new(rts::SLOT_FREE),
            p_misc_flags: MiscFlags::new(0),
            p_sched: SchedFields::new(),
            p_accounting: Accounting::new(),
            p_time: TimeStats::new(),
            p_cycles: CyclesStats::new(),
            p_nextready: None,
            p_caller_q: None,
            p_q_link: None,
            p_getfrom_e: Endpoint::NONE,
            p_sendto_e: Endpoint::NONE,
            p_pending: crate::proc::SigSet::empty(),
            p_name: ProcName::new(),
            p_sendmsg: minix_types::Message::default(),
            p_delivermsg: minix_types::Message::default(),
            p_delivermsg_vir: minix_types::VirBytes(0),
            p_ext_reg_state: ExtRegState::new(),
        };

        [parent, free1, free2, free3]
    }

    #[test]
    fn test_handle_sys_fork_success() {
        let mut procs = create_test_procs();

        let result = handle_sys_fork(
            &mut procs,
            Endpoint::from_generation_slot(1, 0),
            Endpoint::from_generation_slot(1, 1),
            UserSlot::new(1),
        );

        assert!(result.is_ok());
        assert_eq!(procs[1].p_endpoint, Endpoint::from_generation_slot(1, 1));
        assert!(procs[1].p_rts_flags.is_set(rts::NO_PRIV));
    }

    #[test]
    fn test_handle_sys_fork_parent_not_found() {
        let mut procs = create_test_procs();

        let result = handle_sys_fork(
            &mut procs,
            Endpoint::NONE,
            Endpoint::from_generation_slot(1, 1),
            UserSlot::new(1),
        );

        assert!(matches!(result, Err(KernelError::InvalidEndpoint)));
    }

    #[test]
    fn test_handle_sys_fork_slot_in_use() {
        let mut procs = create_test_procs();

        // Mark slot 1 as in use (clear SLOT_FREE flag)
        procs[1].p_rts_flags.clear(rts::SLOT_FREE);

        let result = handle_sys_fork(
            &mut procs,
            Endpoint::from_generation_slot(1, 0),
            Endpoint::from_generation_slot(1, 1),
            UserSlot::new(1),
        );

        assert!(matches!(result, Err(KernelError::SlotInUse)));
    }

    #[test]
    fn test_dispatch_fork_success() {
        let mut procs = create_test_procs();

        let request = KernelRequest::Fork {
            parent_endpoint: Endpoint::from_generation_slot(1, 0),
            child_endpoint: Endpoint::from_generation_slot(1, 1),
            child_slot: UserSlot::new(1),
        };

        let response = MessageDispatcher::dispatch(&mut procs, request);

        assert!(matches!(response, KernelResponse::ForkOk));
    }
}
