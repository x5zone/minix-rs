//! VFS main loop and message dispatch.
//!
//! Corresponds to Minix3's main loop, message reception, and request dispatch mechanism in `main.c`.
//!
//! # Main Loop Model
//!
//! VFS main loop is message-driven, following the three-phase "receive request → process → reply" model:
//!
//! 1. `worker_yield()` — Let other threads run first.
//! 2. `send_work()` — Dispatch pending PM deferred requests.
//! 3. `get_work()` — Receive new messages.
//!
//! After receiving a message, dispatch based on message source:
//! - FS reply → `do_reply()`
//! - PM message → `service_pm()`
//! - Notification → Various notification handlers
//! - Device reply → `bdev_reply()/cdev_reply()/sdev_reply()`
//! - Normal syscall → `handle_work(do_work)`
//!
//! # Difference from Kernel Main Loop
//!
//! - Kernel is single-core interrupt-driven—triggered by hardware interrupts.
//! - VFS is multi-threaded—main thread receives messages, worker threads process.

use minix_types::{Endpoint, Message, UserSlot};
use crate::fproc::{FProcTable, FpFlags, PID_FREE};
use crate::worker::WorkerPool;
use crate::call_table::CallTable;

/// PM message type.
///
/// Corresponds to Minix3's `VFS_PM_*` message types.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PmMessageType {
    /// fork syscall.
    Fork,
    /// Server process fork.
    SrvFork,
    /// exec syscall.
    Exec,
    /// exit syscall.
    Exit,
    /// setuid call.
    Setuid,
    /// setgid call.
    Setgid,
    /// setsid call.
    Setsid,
    /// setgroups call.
    Setgroups,
    /// Core dump.
    Dumpcore,
    /// Unpause.
    Unpause,
    /// Reboot.
    Reboot,
    /// Unknown PM message.
    Unknown(i32),
}

/// Message dispatch result.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DispatchResult {
    /// Processed, continue loop.
    Continue,
    /// Need to spawn worker thread.
    SpawnWorker,
    /// Ignored message.
    Ignored,
}

/// VFS server state.
///
/// Aggregates all VFS subsystem states.
pub struct VfsState {
    /// Process table.
    pub fproc_table: FProcTable,
    /// Worker thread pool.
    pub worker_pool: WorkerPool,
    /// Syscall dispatch table.
    pub call_table: CallTable,
    /// Revive counter (number of blocked processes revived).
    pub reviving: usize,
    /// Current message.
    pub current_message: Message,
    /// Current process's fproc slot.
    pub current_fp_slot: Option<UserSlot>,
}

impl VfsState {
    /// Creates new VFS state.
    pub fn new() -> Self {
        Self {
            fproc_table: FProcTable::new(),
            worker_pool: WorkerPool::new(),
            call_table: CallTable::new(),
            reviving: 0,
            current_message: Message::default(),
            current_fp_slot: None,
        }
    }

    /// SEF initialization—fresh start.
    ///
    /// Corresponds to Minix3's `sef_cb_init_fresh()`.
    pub fn init_fresh(&mut self) {
        self.fproc_table.init_phase2();
    }

    /// Gets current fproc's endpoint.
    pub fn current_endpoint(&self) -> Endpoint {
        self.current_message.m_source
    }

    /// Checks if message is from PM.
    pub fn is_from_pm(&self) -> bool {
        self.current_endpoint() == Endpoint::PM
    }

    /// Dispatches message.
    ///
    /// Corresponds to Minix3's message dispatch logic in `main()`.
    pub fn dispatch(&mut self) -> DispatchResult {
        let source = self.current_endpoint();

        if source == Endpoint::PM {
            return DispatchResult::Continue;
        }

        let slot = source.to_user_slot();
        match slot {
            Some(s) => {
                self.current_fp_slot = Some(s);
                DispatchResult::SpawnWorker
            }
            None => DispatchResult::Ignored,
        }
    }

    /// Handles PM fork message.
    ///
    /// Corresponds to Minix3's `VFS_PM_FORK` branch in `service_pm()`.
    pub fn handle_pm_fork(
        &mut self,
        parent_ep: Endpoint,
        child_ep: Endpoint,
        child_pid: minix_types::Pid,
    ) -> Result<(), &'static str> {
        let _parent_slot = parent_ep
            .to_user_slot()
            .ok_or("Invalid parent endpoint")?;
        let child_slot = child_ep
            .to_user_slot()
            .ok_or("Invalid child endpoint")?;

        let child_fp = self.fproc_table.get_mut(child_slot)
            .ok_or("Child slot out of range")?;

        if child_fp.pid != PID_FREE {
            return Err("Child slot is not free");
        }

        child_fp.pid = child_pid;
        child_fp.endpoint = child_ep;
        child_fp.flags = FpFlags::NOFLAGS;

        Ok(())
    }
}

impl Default for VfsState {
    fn default() -> Self {
        Self::new()
    }
}

/// VFS main loop.
///
/// Corresponds to Minix3's `main()` function.
///
/// # Note
///
/// Currently a mock implementation—IPC message reception uses simulation.
/// Real IPC implementation requires kernel support.
pub fn run() -> ! {
    let mut state = VfsState::new();
    state.init_fresh();

    loop {
        // worker_yield() — Let other threads run first
        // Currently single-threaded mock, no need to actually yield

        // send_work() — Dispatch pending PM deferred requests
        // Currently mock, PM deferred mechanism not implemented yet

        // get_work() — Receive new messages
        // Currently mock, using empty message
        state.current_message = Message::default();
        state.current_fp_slot = None;

        // Message dispatch logic
        // Currently mock, just showing dispatch framework
        let _ = state.dispatch();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use minix_types::Endpoint;

    #[test]
    fn test_vfs_state_new() {
        let state = VfsState::new();
        assert_eq!(state.reviving, 0);
        assert!(state.current_fp_slot.is_none());
        assert!(state.worker_pool.all_idle());
    }

    #[test]
    fn test_vfs_state_init_fresh() {
        let mut state = VfsState::new();
        state.init_fresh();
        let slot = UserSlot::new(0);
        let fp = state.fproc_table.get(slot).unwrap();
        assert!(fp.root_dir.is_none());
        assert!(fp.work_dir.is_none());
        for filp in &fp.filps {
            assert!(filp.is_none());
        }
    }

    #[test]
    fn test_vfs_state_is_from_pm() {
        let mut state = VfsState::new();
        state.current_message.m_source = Endpoint::PM;
        assert!(state.is_from_pm());

        state.current_message.m_source = Endpoint::VM;
        assert!(!state.is_from_pm());
    }

    #[test]
    fn test_vfs_state_dispatch_from_pm() {
        let mut state = VfsState::new();
        state.current_message.m_source = Endpoint::PM;
        let result = state.dispatch();
        assert_eq!(result, DispatchResult::Continue);
    }

    #[test]
    fn test_vfs_state_dispatch_from_user() {
        let mut state = VfsState::new();
        let ep = Endpoint::from_generation_slot(0, 5);
        state.current_message.m_source = ep;
        let result = state.dispatch();
        assert_eq!(result, DispatchResult::SpawnWorker);
        assert_eq!(state.current_fp_slot, Some(UserSlot::new(5)));
    }

    #[test]
    fn test_vfs_state_dispatch_from_kernel() {
        let mut state = VfsState::new();
        state.current_message.m_source = Endpoint::KERNEL;
        let result = state.dispatch();
        assert_eq!(result, DispatchResult::Ignored);
    }

    #[test]
    fn test_handle_pm_fork() {
        let mut state = VfsState::new();
        let parent_ep = Endpoint::from_generation_slot(1, 5);
        let child_ep = Endpoint::from_generation_slot(1, 10);

        let result = state.handle_pm_fork(parent_ep, child_ep, 1234);
        assert!(result.is_ok());

        let child_slot = UserSlot::new(10);
        let child_fp = state.fproc_table.get(child_slot).unwrap();
        assert_eq!(child_fp.pid, 1234);
        assert_eq!(child_fp.endpoint, child_ep);
        assert_eq!(child_fp.flags, FpFlags::NOFLAGS);
    }

    #[test]
    fn test_handle_pm_fork_invalid_parent() {
        let mut state = VfsState::new();
        state.init_fresh();

        let parent_ep = Endpoint::KERNEL;
        let child_ep = Endpoint::from_generation_slot(1, 10);
        let result = state.handle_pm_fork(parent_ep, child_ep, 1234);
        assert!(result.is_err());
    }

    #[test]
    fn test_handle_pm_fork_slot_not_free() {
        let mut state = VfsState::new();
        let parent_ep = Endpoint::from_generation_slot(1, 5);
        let child_ep = Endpoint::from_generation_slot(1, 10);

        let child_slot = UserSlot::new(10);
        state.fproc_table.get_mut(child_slot).unwrap().pid = 999;

        let result = state.handle_pm_fork(parent_ep, child_ep, 1234);
        assert!(result.is_err());
    }

    #[test]
    fn test_pm_message_type() {
        assert_eq!(PmMessageType::Fork, PmMessageType::Fork);
        assert_eq!(PmMessageType::Unknown(99), PmMessageType::Unknown(99));
    }

    #[test]
    fn test_dispatch_result() {
        assert_eq!(DispatchResult::Continue, DispatchResult::Continue);
        assert_eq!(DispatchResult::SpawnWorker, DispatchResult::SpawnWorker);
        assert_eq!(DispatchResult::Ignored, DispatchResult::Ignored);
    }
}
