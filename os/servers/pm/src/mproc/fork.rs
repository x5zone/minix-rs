//! Fork system call implementation.
//!
//! This is the Rust implementation of Minix3's `do_fork` function, containing PM's private fork logic.
//!
//! # Minix3 Multi-Process Table Architecture
//! Minix3 uses a distributed process table design with 4 copies:
//! - **PM/mproc**: Process management, signals, permissions (this module)
//! - **VM/vmproc**: Virtual memory, page tables
//! - **VFS/fproc**: File descriptors, directories
//! - **Kernel/proc**: Scheduling, IPC, register saving
//!
//! # Phases
//! - Phase 2a: Parameter check and slot allocation (this file)
//! - Phase 2b: VM fork call
//! - Phase 2c: Process structure initialization
//!
//! # Why in PM crate, not minix-types?
//!
//! 1. **Separation of concerns**: fork is PM's core system call
//! 2. **Invariant protection**: fork logic binds PM internal state
//! 3. **Microkernel principle**: Other services don't need to know PM's fork implementation

use minix_types::{Pid, Endpoint, UserSlot, NR_PROCS, LAST_FEW, Clock, Uid, Gid, EAGAIN, ENOMEM, EINVAL};
use crate::mproc::{PmContext, Process, Lifecycle, Privilege, Credentials, ProcessIdentity, ProcessId, ProcessState, BlockState, WaitState, Guardianship, TraceState, ProcessResources, ProcessIpc, ProcTable, NR_ITIMERS, RemainingFlags};

/// PM -> VM: Fork request message.
#[derive(Debug, Clone, Copy)]
pub struct VmForkRequest {
    /// Parent process Endpoint.
    pub parent_endpoint: Endpoint,
    /// Child process slot index.
    pub child_index: usize,
}

/// VM -> PM: Fork response message.
#[derive(Debug, Clone, Copy)]
pub struct VmForkResponse {
    /// Child process Endpoint.
    pub child_endpoint: Endpoint,
    /// Whether successful.
    pub success: bool,
}

/// PM -> VFS: Fork request message.
#[derive(Debug, Clone, Copy)]
pub struct VfsPmForkRequest {
    /// Child process Endpoint.
    pub child_endpoint: Endpoint,
    /// Parent process Endpoint.
    pub parent_endpoint: Endpoint,
    /// Child process PID.
    pub child_pid: Pid,
    /// Real UID.
    pub real_uid: Uid,
    /// Real GID.
    pub real_gid: Gid,
}

/// VFS -> PM: Fork response message.
#[derive(Debug, Clone, Copy)]
pub struct VfsPmForkResponse {
    /// Child process Endpoint.
    pub child_endpoint: Endpoint,
    /// Whether successful.
    pub success: bool,
}

/// Fork error type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ForkError {
    /// Process table full.
    TableFull,
    /// Non-root user slot insufficient (LAST_FEW reserved).
    ReservedForRoot,
    /// System resource exhausted.
    ResourceExhausted,
    /// VM call failed.
    VmError,
    /// Internal error (should not happen).
    InternalError,
}

impl ForkError {
    /// Converts to error code.
    ///
    /// Corresponds to Minix3's `errno` values.
    /// Note: Minix3 errno numbering differs from Linux (e.g., EAGAIN=35 in Minix3 vs 11 in Linux).
    pub fn to_errno(&self) -> i32 {
        match self {
            Self::TableFull => EAGAIN,
            Self::ReservedForRoot => EAGAIN,
            Self::ResourceExhausted => ENOMEM,
            Self::VmError => ENOMEM,
            Self::InternalError => EINVAL,
        }
    }
}

/// Fork result.
#[derive(Debug, Clone, Copy)]
pub struct ForkResult {
    /// Child process index.
    pub child_index: usize,
    /// Child process PID.
    pub child_pid: Pid,
    /// Child process Endpoint.
    pub child_endpoint: Endpoint,
}

impl<'a> PmContext<'a> {
    /// fork system call (first half).
    ///
    /// Corresponds to the beginning of Minix3's `do_fork` function:
    /// ```c
    /// int do_fork(void) {
    ///   register struct mproc *rmp;   // Parent process pointer
    ///   register struct mproc *rmc;   // Child process pointer
    ///   static unsigned int next_child = 0;
    ///   int n = 0;
    ///
    ///   rmp = mp;  // Current process
    ///   
    ///   // 1. Check if process table is full
    ///   if ((procs_in_use == NR_PROCS) ||
    ///       (procs_in_use >= NR_PROCS-LAST_FEW && rmp->mp_effuid != 0)) {
    ///     printf("PM: warning, process table is full!\n");
    ///     return(EAGAIN);
    ///   }
    ///
    ///   // 2. Find free slot
    ///   do {
    ///     next_child = (next_child+1) % NR_PROCS;
    ///     n++;
    ///   } while((mproc[next_child].mp_flags & IN_USE) && n <= NR_PROCS);
    ///   
    ///   if(n > NR_PROCS)
    ///     panic("do_fork can't find child slot");
    ///   
    ///   // ... subsequent processing
    /// }
    /// ```
    ///
    /// # Returns
    /// - `Ok(ForkResult)`: Successfully allocated slot
    /// - `Err(ForkError)`: Allocation failed
    pub fn do_fork_prepare(&mut self) -> Result<ForkResult, ForkError> {
        if self.table.is_full() {
            return Err(ForkError::TableFull);
        }
        
        if !self.can_alloc() {
            return Err(ForkError::ReservedForRoot);
        }
        
        let child_index = self.table.alloc_slot().ok_or(ForkError::TableFull)?;
        
        let child_pid = self.generate_child_pid();
        let child_endpoint = self.table.procs[child_index].endpoint();
        
        Ok(ForkResult {
            child_index,
            child_pid,
            child_endpoint,
        })
    }
    
    /// Generates child process PID.
    ///
    /// Uses PidGenerator to generate unique PID.
    ///
    /// # Minix3 Mapping
    ///
    /// Corresponds to Minix3's `get_free_pid()` function:
    /// ```c
    /// pid_t get_free_pid()
    /// {
    ///   static pid_t next_pid = INIT_PID + 1;
    ///   register struct mproc *rmp;
    ///   int t;
    ///
    ///   do {
    ///     t = 0;
    ///     next_pid = (next_pid < NR_PIDS ? next_pid + 1 : INIT_PID + 1);
    ///     for (rmp = &mproc[0]; rmp < &mproc[NR_PROCS]; rmp++)
    ///       if (rmp->mp_pid == next_pid || rmp->mp_procgrp == next_pid) {
    ///         t = 1;
    ///         break;
    ///       }
    ///   } while (t);
    ///
    ///   return(next_pid);
    /// }
    /// ```
    ///
    /// # Complexity
    ///
    /// - Expected: O(1) (because conflict probability is extremely low, ~0.8%)
    /// - Worst-case: O(N) (extremely rare)
    fn generate_child_pid(&self) -> Pid {
        self.table.pid_generator.get_free_pid(self.table)
    }
    
    /// Creates child process from parent.
    ///
    /// Corresponds to Minix3's process structure copy:
    /// ```c
    /// *rmc = *rmp;  // Copy entire structure
    /// ```
    ///
    /// But we use explicit `fork_from` method, forcing check of every field.
    pub fn fork_child_from_parent(&mut self, child_index: usize, child_pid: Pid, child_endpoint: Endpoint) {
        let parent = self.current_proc().clone();
        
        let child = Process::fork_from(&parent, child_index, child_pid, child_endpoint, self.current);
        
        self.table.procs[child_index] = child;
    }
}

/// Gets current clock ticks.
///
/// Corresponds to Minix3's `getticks()` function.
///
/// # TODO
/// Currently returns 0, need to implement real clock acquisition later:
/// - Request time from CLOCK task via IPC
/// - Or use kernel-provided clock interface
fn getticks() -> Clock {
    0
}

impl Process {
    /// Fork semantics: create child process from parent.
    ///
    /// Strategy: Explicit Construction
    /// Advantage: Compiler forces checking of new fields, no implicit behavior
    ///
    /// # Minix3 Mapping
    /// | Minix3 Behavior | Rust Behavior |
    /// |------------|----------|
    /// | `*rmc = *rmp` | Explicitly copy each field |
    /// | `rmc->mp_pid = next_pid` | `identity.id.pid = child_pid` |
    /// | `rmc->mp_flags &= ~TRACE_EXIT` | `trace.stopped = false` |
    /// | `rmc->mp_child_utime = 0` | `resources.child_utime = 0` |
    /// | `rmc->mp_flags &= (IN_USE\|DELAY_CALL\|TAINTED)` | `flags` only keeps TAINTED |
    /// | `rmc->mp_started = getticks()` | `started = getticks()` |
    /// | Privileged process scheduler | `Endpoint::RS` |
    pub fn fork_from(
        parent: &Process, 
        child_index: usize, 
        child_pid: Pid, 
        child_endpoint: Endpoint,
        parent_index: usize,
    ) -> Self {
        
        let identity = ProcessIdentity {
            id: ProcessId { 
                index: UserSlot::new(child_index),
                pid: child_pid,
            },
            endpoint: child_endpoint,
            procgrp: parent.identity.procgrp, 
            name: parent.identity.name,
        };

        let state = ProcessState {
            lifecycle: Lifecycle::Running,
            block: BlockState::default(),
            wait: WaitState::default(),
            guardianship: Guardianship::Normal { 
                parent: UserSlot::new(parent_index),
            },
            trace: TraceState::default(),
        };

        let resources = ProcessResources {
            privilege: parent.resources.privilege.clone(),
            signals: parent.resources.signals.clone(),
            
            child_utime: 0,
            child_stime: 0,
            started: getticks(),
            timer: None,
            intervals: [0; NR_ITIMERS],
            nice: parent.resources.nice,
            
            scheduler: if parent.resources.privilege.is_kernel() {
                Endpoint::RS
            } else {
                parent.resources.scheduler
            },
            
            flags: {
                let mut flags = RemainingFlags::empty();
                if parent.resources.flags.contains(RemainingFlags::TAINTED) {
                    flags |= RemainingFlags::TAINTED;
                }
                if parent.resources.flags.contains(RemainingFlags::DELAY_CALL) {
                    flags |= RemainingFlags::DELAY_CALL;
                }
                flags
            },
        };

        let ipc = ProcessIpc::default();

        Self { identity, state, resources, ipc }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    extern crate std;
    use std::boxed::Box;
    
    fn create_test_context() -> PmContext<'static> {
        let table = Box::leak(Box::new(ProcTable::new()));
        table.procs[0].state.lifecycle = Lifecycle::Running;
        table.procs[0].resources.privilege = Privilege::User(Credentials::new(0, 0));
        PmContext::new(table, 0)
    }
    
    #[test]
    fn test_fork_prepare() {
        let mut ctx = create_test_context();
        let result = ctx.do_fork_prepare().unwrap();
        assert!(result.child_index < NR_PROCS);
        assert!(result.child_pid > 0);
    }
    
    #[test]
    fn test_fork_table_full() {
        let mut ctx = create_test_context();
        
        for i in 0..NR_PROCS {
            ctx.table.procs[i].state.lifecycle = Lifecycle::Running;
        }
        ctx.table.procs_in_use.set(NR_PROCS);
        
        let result = ctx.do_fork_prepare();
        assert!(matches!(result, Err(ForkError::TableFull)));
    }
    
    #[test]
    fn test_fork_reserved_for_root() {
        let table = Box::leak(Box::new(ProcTable::new()));
        
        for i in 0..(NR_PROCS - LAST_FEW) {
            table.procs[i].state.lifecycle = Lifecycle::Running;
        }
        table.procs_in_use.set(NR_PROCS - LAST_FEW);
        
        table.procs[0].state.lifecycle = Lifecycle::Running;
        table.procs[0].resources.privilege = Privilege::User(Credentials::new(1000, 100));
        
        let mut ctx = PmContext::new(table, 0);
        
        let result = ctx.do_fork_prepare();
        assert!(matches!(result, Err(ForkError::ReservedForRoot)));
    }
    
    #[test]
    fn test_fork_child_from_parent() {
        let mut ctx = create_test_context();
        
        let fork_result = ctx.do_fork_prepare().unwrap();
        ctx.fork_child_from_parent(
            fork_result.child_index,
            fork_result.child_pid,
            fork_result.child_endpoint,
        );
        
        let child = ctx.table.get(fork_result.child_index).unwrap();
        assert!(child.is_in_use());
        assert_eq!(child.pid(), fork_result.child_pid);
        assert_eq!(child.state.guardianship.parent(), UserSlot::new(0));
    }
    
    #[test]
    fn test_fork_error_to_errno() {
        assert_eq!(ForkError::TableFull.to_errno(), EAGAIN);
        assert_eq!(ForkError::ReservedForRoot.to_errno(), EAGAIN);
        assert_eq!(ForkError::ResourceExhausted.to_errno(), ENOMEM);
        assert_eq!(ForkError::InternalError.to_errno(), EINVAL);
    }
    
    #[test]
    fn test_fork_child_index_correct() {
        let parent = Process::new(0, 100);
        let child = Process::fork_from(&parent, 5, 200, Endpoint(50), 0);
        
        assert_eq!(child.identity.id.index, UserSlot::new(5));
        assert_eq!(child.identity.id.pid, 200);
        assert_eq!(child.identity.endpoint, Endpoint(50));
    }
    
    #[test]
    fn test_fork_inherited_fields() {
        let mut parent = Process::new(0, 100);
        parent.identity.procgrp = 500;
        parent.resources.nice = 10;
        
        let child = Process::fork_from(&parent, 5, 200, Endpoint(50), 0);
        
        assert_eq!(child.identity.procgrp, 500);
        assert_eq!(child.resources.nice, 10);
    }
    
    #[test]
    fn test_fork_cleared_fields() {
        let mut parent = Process::new(0, 100);
        parent.resources.child_utime = 1000;
        parent.resources.child_stime = 2000;
        parent.resources.intervals = [100, 200, 300];
        
        let child = Process::fork_from(&parent, 5, 200, Endpoint(50), 0);
        
        assert_eq!(child.resources.child_utime, 0);
        assert_eq!(child.resources.child_stime, 0);
        assert_eq!(child.resources.intervals, [0; NR_ITIMERS]);
    }
    
    #[test]
    fn test_fork_flags_inheritance() {
        let mut parent = Process::new(0, 100);
        parent.resources.flags = RemainingFlags::TAINTED | RemainingFlags::DELAY_CALL | RemainingFlags::ALARM_ON | RemainingFlags::NEW_PARENT;
        
        let child = Process::fork_from(&parent, 5, 200, Endpoint(50), 0);
        
        assert!(child.resources.flags.contains(RemainingFlags::TAINTED));
        assert!(child.resources.flags.contains(RemainingFlags::DELAY_CALL));
        assert!(!child.resources.flags.contains(RemainingFlags::ALARM_ON));
        assert!(!child.resources.flags.contains(RemainingFlags::NEW_PARENT));
    }
    
    #[test]
    fn test_fork_flags_no_tainted() {
        let mut parent = Process::new(0, 100);
        parent.resources.flags = RemainingFlags::ALARM_ON | RemainingFlags::NEW_PARENT;
        
        let child = Process::fork_from(&parent, 5, 200, Endpoint(50), 0);
        
        assert!(child.resources.flags.is_empty());
    }
    
    #[test]
    fn test_fork_privilege_scheduler() {
        let mut parent = Process::new(0, 100);
        parent.resources.privilege = Privilege::Kernel;
        parent.resources.scheduler = Endpoint::NONE;
        
        let child = Process::fork_from(&parent, 5, 200, Endpoint(50), 0);
        
        assert_eq!(child.resources.scheduler, Endpoint::RS);
    }
    
    #[test]
    fn test_fork_normal_scheduler() {
        let mut parent = Process::new(0, 100);
        parent.resources.privilege = Privilege::User(Credentials::new(1000, 100));
        parent.resources.scheduler = Endpoint::PM;
        
        let child = Process::fork_from(&parent, 5, 200, Endpoint(50), 0);
        
        assert_eq!(child.resources.scheduler, Endpoint::PM);
    }
    
    #[test]
    fn test_fork_parent_relationship() {
        let parent = Process::new(10, 100);
        let child = Process::fork_from(&parent, 5, 200, Endpoint(50), 10);
        
        assert_eq!(child.state.guardianship.parent(), UserSlot::new(10));
    }
    
    #[test]
    fn test_fork_ipc_reset() {
        let mut parent = Process::new(0, 100);
        parent.ipc.reply = Some(minix_types::Message::default());
        parent.ipc.event_subscriber = Some(UserSlot::new(5));
        
        let child = Process::fork_from(&parent, 5, 200, Endpoint(50), 0);
        
        assert!(child.ipc.reply.is_none());
        assert!(child.ipc.event_subscriber.is_none());
    }
}
