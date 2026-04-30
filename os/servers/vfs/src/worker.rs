//! Worker thread management.
//!
//! Corresponds to Minix3's `worker_thread` structure and thread management functions.
//!
//! VFS is the only server in Minix3 that uses multi-threading (mthread).
//! The reason for multi-threading is that blocking I/O operations cannot block
//! the entire VFS—such as pipe reads, device I/O, file system requests, etc.
//!
//! # Thread Model
//!
//! - **Main thread**: Receives messages, dispatches work, handles PM messages.
//! - **Worker threads**: Handle specific syscalls, dispatched via call_vec[].
//!
//! # Worker Thread Count
//!
//! NR_WTHREADS = 9 (Minix3 default).

use minix_types::{Endpoint, Message};

/// Worker thread count.
///
/// Corresponds to Minix3's `NR_WTHREADS`.
pub const NR_WTHREADS: usize = 9;

/// Worker thread state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkerState {
    /// Idle, waiting for work assignment.
    Idle,
    /// Processing request.
    Busy,
    /// Waiting for file system service reply.
    WaitingForFs,
}

/// Worker thread structure.
///
/// Corresponds to Minix3's `struct worker_thread`.
///
/// # Differences from C Version
///
/// - `w_fp` uses `Option<UserSlot>` instead of raw pointer.
/// - `w_task` uses `Option<Endpoint>` instead of `endpoint_t`.
/// - `w_sendrec` uses `Option<Message>` instead of pointer.
/// - `w_thread` omitted—Rust uses native threads instead of mthread.
#[derive(Debug)]
pub struct WorkerThread {
    /// Associated fproc slot.
    pub fp_slot: Option<minix_types::UserSlot>,
    /// Waiting FS endpoint.
    pub task: Option<Endpoint>,
    /// Send/receive message buffer.
    pub sendrec: Option<Message>,
    /// Thread state.
    pub state: WorkerState,
    /// Worker thread array index.
    pub self_index: usize,
    /// Pending handler function.
    pub func: Option<WorkerFunc>,
}

/// Worker thread handler function type.
///
/// In Minix3 C code, `worker_start()` accepts `void (*func)(void)` function pointer.
/// Rust version uses enum instead to avoid raw function pointers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkerFunc {
    /// Handle normal syscall (do_work).
    DoWork,
    /// Handle deferred pipe operation.
    DoPendingPipe,
    /// Handle DS event.
    DsEvent,
    /// Handle PM reboot.
    PmReboot,
}

impl WorkerThread {
    /// Creates idle worker thread.
    pub fn new(index: usize) -> Self {
        Self {
            fp_slot: None,
            task: None,
            sendrec: None,
            state: WorkerState::Idle,
            self_index: index,
            func: None,
        }
    }

    /// Checks if worker thread is idle.
    pub fn is_idle(&self) -> bool {
        self.state == WorkerState::Idle
    }

    /// Starts worker thread.
    ///
    /// Corresponds to Minix3's `worker_start()`.
    pub fn start(
        &mut self,
        fp_slot: minix_types::UserSlot,
        func: WorkerFunc,
        msg: &Message,
    ) {
        self.fp_slot = Some(fp_slot);
        self.func = Some(func);
        self.sendrec = Some(*msg);
        self.state = WorkerState::Busy;
    }

    /// Worker thread finishes, returns to idle state.
    ///
    /// Corresponds to Minix3's `worker_stop()`.
    pub fn stop(&mut self) {
        self.fp_slot = None;
        self.task = None;
        self.sendrec = None;
        self.state = WorkerState::Idle;
        self.func = None;
    }

    /// Sets waiting for FS reply.
    pub fn set_waiting(&mut self, fs_endpoint: Endpoint) {
        self.task = Some(fs_endpoint);
        self.state = WorkerState::WaitingForFs;
    }
}

/// Worker thread pool.
///
/// Manages the collection of all worker threads.
pub struct WorkerPool {
    threads: Vec<WorkerThread>,
}

impl WorkerPool {
    /// Creates worker thread pool.
    pub fn new() -> Self {
        let threads = (0..NR_WTHREADS)
            .map(|i| WorkerThread::new(i))
            .collect();
        Self { threads }
    }

    /// Gets idle worker thread.
    pub fn get_idle(&self) -> Option<&WorkerThread> {
        self.threads.iter().find(|t| t.is_idle())
    }

    /// Gets idle worker thread (mutable).
    pub fn get_idle_mut(&mut self) -> Option<&mut WorkerThread> {
        self.threads.iter_mut().find(|t| t.is_idle())
    }

    /// Gets available worker thread count.
    pub fn available_count(&self) -> usize {
        self.threads.iter().filter(|t| t.is_idle()).count()
    }

    /// Checks if there are available worker threads.
    pub fn has_available(&self) -> bool {
        self.available_count() > 0
    }

    /// Checks if all worker threads are idle.
    pub fn all_idle(&self) -> bool {
        self.threads.iter().all(|t| t.is_idle())
    }

    /// Gets worker thread by index.
    pub fn get(&self, index: usize) -> Option<&WorkerThread> {
        self.threads.get(index)
    }

    /// Gets worker thread by index (mutable).
    pub fn get_mut(&mut self, index: usize) -> Option<&mut WorkerThread> {
        self.threads.get_mut(index)
    }

    /// Total worker thread count.
    pub fn len(&self) -> usize {
        self.threads.len()
    }

    /// Checks if worker thread pool is empty.
    pub fn is_empty(&self) -> bool {
        self.threads.is_empty()
    }
}

impl Default for WorkerPool {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use minix_types::UserSlot;

    #[test]
    fn test_worker_thread_new() {
        let wt = WorkerThread::new(0);
        assert!(wt.is_idle());
        assert!(wt.fp_slot.is_none());
        assert!(wt.task.is_none());
        assert!(wt.sendrec.is_none());
        assert_eq!(wt.self_index, 0);
        assert!(wt.func.is_none());
    }

    #[test]
    fn test_worker_thread_start_stop() {
        let mut wt = WorkerThread::new(0);
        let slot = UserSlot::new(5);
        let msg = Message::default();

        wt.start(slot, WorkerFunc::DoWork, &msg);
        assert!(!wt.is_idle());
        assert_eq!(wt.fp_slot, Some(slot));
        assert_eq!(wt.func, Some(WorkerFunc::DoWork));
        assert_eq!(wt.state, WorkerState::Busy);

        wt.stop();
        assert!(wt.is_idle());
        assert!(wt.fp_slot.is_none());
    }

    #[test]
    fn test_worker_thread_set_waiting() {
        let mut wt = WorkerThread::new(0);
        wt.set_waiting(Endpoint::MFS);
        assert_eq!(wt.state, WorkerState::WaitingForFs);
        assert_eq!(wt.task, Some(Endpoint::MFS));
    }

    #[test]
    fn test_worker_pool_new() {
        let pool = WorkerPool::new();
        assert_eq!(pool.len(), NR_WTHREADS);
        assert!(pool.all_idle());
        assert_eq!(pool.available_count(), NR_WTHREADS);
    }

    #[test]
    fn test_worker_pool_get_idle() {
        let mut pool = WorkerPool::new();
        let idle = pool.get_idle_mut();
        assert!(idle.is_some());

        let slot = UserSlot::new(0);
        let msg = Message::default();
        idle.unwrap().start(slot, WorkerFunc::DoWork, &msg);

        assert_eq!(pool.available_count(), NR_WTHREADS - 1);
        assert!(!pool.all_idle());
    }

    #[test]
    fn test_worker_pool_all_busy() {
        let mut pool = WorkerPool::new();
        let msg = Message::default();

        for i in 0..NR_WTHREADS {
            let idle = pool.get_idle_mut().unwrap();
            let slot = UserSlot::new(i);
            idle.start(slot, WorkerFunc::DoWork, &msg);
        }

        assert!(pool.all_idle() == false);
        assert_eq!(pool.available_count(), 0);
        assert!(pool.get_idle_mut().is_none());
    }
}
