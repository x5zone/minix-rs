//! PM process structure definition.
//!
//! This is the Rust implementation of Minix3's `mproc` structure, containing PM's private process management logic.
//!
//! # Minix3 Multi-Process Table Architecture
//! Minix3 uses a distributed process table design with 4 copies:
//! - **PM/mproc**: Process management, signals, permissions (this module)
//! - **VM/vmproc**: Virtual memory, page tables
//! - **VFS/fproc**: File descriptors, directories
//! - **Kernel/proc**: Scheduling, IPC, register saving
//!
//! # Layered Design
//! Uses layered abstraction, dividing process fields into four layers:
//! 1. Identity: PID, endpoint, process group, name
//! 2. State machine: lifecycle, block, wait, guardianship, trace
//! 3. Resources: privilege, signals, timers, scheduling, time stats
//! 4. IPC: message reply, event subscription
//!
//! # Why in PM crate, not minix-types?
//!
//! 1. **Separation of concerns**: MProc contains private logic only PM cares about (signal handling, parent-child tree, etc.)
//! 2. **Invariant protection**: State transition logic binds PM internal complex logic, putting in public library would break invariants
//! 3. **Microkernel principle**: Follows "minimum knowledge" principle, other services don't need to know PM's internal implementation

use minix_types::{Pid, Endpoint, UserSlot, Clock, VirBytes};
use minix_ipc::Message;
use crate::mproc::{Lifecycle, BlockState, WaitState, Guardianship, TraceState, TraceOptions, Credentials, SignalState};

/// Maximum process name length.
pub const PROC_NAME_LEN: usize = 16;

/// Number of timers.
pub const NR_ITIMERS: usize = 3;

/// Process identifier.
///
/// Contains process table index and PID.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ProcessId {
    /// Process table index.
    pub index: UserSlot,
    /// Process ID.
    pub pid: Pid,
}

/// Process identity.
///
/// Basic identification information for a process.
#[derive(Debug, Clone)]
pub struct ProcessIdentity {
    /// Process identifier (index + PID).
    pub id: ProcessId,
    /// Endpoint identifier (for IPC).
    pub endpoint: Endpoint,
    /// Process group ID.
    pub procgrp: Pid,
    /// Process name.
    pub name: [u8; PROC_NAME_LEN],
}

impl Default for ProcessIdentity {
    fn default() -> Self {
        Self {
            id: ProcessId {
                index: UserSlot::new(0),
                pid: 0,
            },
            endpoint: Endpoint::default(),
            procgrp: 0,
            name: [0; PROC_NAME_LEN],
        }
    }
}

/// Process state machine.
///
/// All state information for a process.
#[derive(Debug, Clone)]
pub struct ProcessState {
    /// Lifecycle state (mutually exclusive).
    pub lifecycle: Lifecycle,
    /// Block state (can combine with lifecycle).
    pub block: BlockState,
    /// Parent wait state (stored in parent process).
    pub wait: WaitState,
    /// Guardianship relationship.
    pub guardianship: Guardianship,
    /// Trace state.
    pub trace: TraceState,
}

impl Default for ProcessState {
    fn default() -> Self {
        Self {
            lifecycle: Lifecycle::default(),
            block: BlockState::default(),
            wait: WaitState::default(),
            guardianship: Guardianship::default(),
            trace: TraceState::default(),
        }
    }
}

/// Minix timer.
#[derive(Debug, Clone, Copy)]
pub struct MinixTimer {
    /// Expiration time.
    pub expire_time: Clock,
    /// Reload time (for periodic timers).
    pub reload_time: Clock,
}

/// Privilege level.
///
/// Corresponds to Minix3's `PRIV_PROC` flag.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Privilege {
    /// Regular user process.
    User(Credentials),
    /// System process (PRIV_PROC).
    ///
    /// System processes have special privileges and don't need to wait for VFS on exit.
    Kernel,
}

impl Default for Privilege {
    fn default() -> Self {
        Self::User(Credentials::default())
    }
}

impl Privilege {
    /// Checks if this is a system process.
    pub fn is_kernel(&self) -> bool {
        matches!(self, Self::Kernel)
    }
    
    /// Gets credentials.
    ///
    /// Returns `None` if this is a system process.
    pub fn credentials(&self) -> Option<&Credentials> {
        match self {
            Self::User(creds) => Some(creds),
            Self::Kernel => None,
        }
    }
}

bitflags::bitflags! {
    /// Remaining flags.
    ///
    /// These flags have not yet been categorized into specific state machines.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct RemainingFlags: u32 {
        /// Timer started.
        const ALARM_ON = 0x00010;
        /// Deferred call.
        const DELAY_CALL = 0x00040;
        /// VFS call in progress.
        const VFS_CALL = 0x00200;
        /// Parent changed.
        const NEW_PARENT = 0x00800;
        /// Partial exec.
        const PARTIAL_EXEC = 0x04000;
        /// Tainted flag.
        const TAINTED = 0x40000;
    }
}

/// Process resources.
///
/// Resource management information for a process.
#[derive(Debug, Clone)]
pub struct ProcessResources {
    /// Privilege level.
    pub privilege: Privilege,
    /// Signal handling state.
    pub signals: SignalState,
    /// Child process user time accumulator.
    pub child_utime: Clock,
    /// Child process system time accumulator.
    pub child_stime: Clock,
    /// Process start time.
    pub started: Clock,
    /// Process timer.
    pub timer: Option<MinixTimer>,
    /// Interval timers.
    pub intervals: [Clock; NR_ITIMERS],
    /// Nice value.
    pub nice: i32,
    /// Scheduler endpoint.
    pub scheduler: Endpoint,
    /// Uncategorized flags.
    pub flags: RemainingFlags,
}

impl Default for ProcessResources {
    fn default() -> Self {
        Self {
            privilege: Privilege::default(),
            signals: SignalState::default(),
            child_utime: 0,
            child_stime: 0,
            started: 0,
            timer: None,
            intervals: [0; NR_ITIMERS],
            nice: 0,
            scheduler: Endpoint::default(),
            flags: RemainingFlags::empty(),
        }
    }
}

/// IPC context.
///
/// Inter-process communication related information.
#[derive(Debug, Clone)]
pub struct ProcessIpc {
    /// IPC reply message (lazy loaded).
    pub reply: Option<Message>,
    /// Event subscriber.
    pub event_subscriber: Option<UserSlot>,
    /// Stack frame address.
    pub frame_addr: VirBytes,
    /// Stack frame length.
    pub frame_len: usize,
}

impl Default for ProcessIpc {
    fn default() -> Self {
        Self {
            reply: None,
            event_subscriber: None,
            frame_addr: VirBytes(0),
            frame_len: 0,
        }
    }
}

/// PM process structure.
///
/// This is the Rust rewrite of Minix3's `mproc` structure.
///
/// # Minix3 Multi-Process Table Architecture
/// This structure corresponds to PM's `mproc` table, linked to VM's `vmproc`,
/// VFS's `fproc`, and Kernel's `proc` via `endpoint`.
///
/// # Layered Design
/// Uses layered abstraction for better code organization:
///
/// ```text
/// Process
/// ├── identity: ProcessIdentity   // Identity
/// ├── state: ProcessState         // State machine
/// ├── resources: ProcessResources // Resources
/// └── ipc: ProcessIpc             // IPC context
/// ```
///
/// # Minix3 Mapping
/// | Minix3 field | Rust field |
/// |--------------|------------|
/// | mp_pid, mp_endpoint | identity.id, identity.endpoint |
/// | mp_flags (state) | state.lifecycle, state.block |
/// | mp_realuid, mp_effuid | resources.privilege |
/// | mp_reply | ipc.reply |
#[derive(Debug, Clone)]
#[repr(C)]
pub struct Process {
    /// Identity information.
    pub identity: ProcessIdentity,
    /// State machine.
    pub state: ProcessState,
    /// Resources.
    pub resources: ProcessResources,
    /// IPC context.
    pub ipc: ProcessIpc,
}

impl Default for Process {
    fn default() -> Self {
        Self {
            identity: ProcessIdentity::default(),
            state: ProcessState::default(),
            resources: ProcessResources::default(),
            ipc: ProcessIpc::default(),
        }
    }
}

impl Process {
    /// Creates a new process.
    ///
    /// # Parameters
    /// - `index`: Process table index
    /// - `pid`: Process ID
    pub fn new(index: usize, pid: Pid) -> Self {
        Self {
            identity: ProcessIdentity {
                id: ProcessId {
                    index: UserSlot::new(index),
                    pid,
                },
                ..Default::default()
            },
            ..Self::default()
        }
    }
    
    /// Checks if slot is in use.
    pub fn is_in_use(&self) -> bool {
        self.state.lifecycle.is_in_use()
    }
    
    /// Gets process PID.
    pub fn pid(&self) -> Pid {
        self.identity.id.pid
    }
    
    /// Gets process index.
    pub fn index(&self) -> UserSlot {
        self.identity.id.index
    }
    
    /// Gets endpoint.
    pub fn endpoint(&self) -> Endpoint {
        self.identity.endpoint
    }
    
    /// Gets process group ID.
    pub fn procgrp(&self) -> Pid {
        self.identity.procgrp
    }
    
    /// Gets parent process index.
    pub fn parent(&self) -> UserSlot {
        self.state.guardianship.parent()
    }
    
    /// Gets tracer index.
    pub fn tracer(&self) -> Option<UserSlot> {
        self.state.guardianship.tracer()
    }
    
    /// Checks if this is a system process.
    pub fn is_kernel_process(&self) -> bool {
        self.resources.privilege.is_kernel()
    }
    
    /// Checks if this is a zombie process.
    pub fn is_zombie(&self) -> bool {
        self.state.lifecycle.is_zombie()
    }
    
    /// Checks if process is exiting.
    pub fn is_exiting(&self) -> bool {
        self.state.lifecycle.is_exiting()
    }
    
    /// Checks if process is stopped.
    pub fn is_stopped(&self) -> bool {
        self.state.block.stopped || self.state.trace.stopped
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_process_default() {
        let proc = Process::default();
        assert!(!proc.is_in_use());
        assert_eq!(proc.pid(), 0);
        assert!(!proc.is_zombie());
        assert!(!proc.is_exiting());
    }
    
    #[test]
    fn test_process_new() {
        let proc = Process::new(5, 100);
        assert_eq!(proc.index(), UserSlot::new(5));
        assert_eq!(proc.pid(), 100);
        assert!(!proc.is_in_use());
    }
    
    #[test]
    fn test_process_lifecycle() {
        let mut proc = Process::new(0, 1);
        proc.state.lifecycle = Lifecycle::Running;
        assert!(proc.is_in_use());
        assert!(!proc.is_zombie());
        
        proc.state.lifecycle = Lifecycle::Exiting { exit_code: 0, sig_status: 9 };
        assert!(proc.is_exiting());
        
        proc.state.lifecycle = Lifecycle::Zombie { exit_code: 42, sig_status: 0 };
        assert!(proc.is_zombie());
    }
    
    #[test]
    fn test_process_guardianship() {
        let mut proc = Process::default();
        proc.state.guardianship = Guardianship::Normal { parent: UserSlot::new(10) };
        assert_eq!(proc.parent(), UserSlot::new(10));
        assert!(proc.tracer().is_none());
        
        proc.state.guardianship = Guardianship::Traced {
            parent: UserSlot::new(10),
            tracer: UserSlot::new(5),
            trace_exit: false,
            trace_options: TraceOptions::empty(),
        };
        assert_eq!(proc.tracer(), Some(UserSlot::new(5)));
    }
    
    #[test]
    fn test_process_stopped() {
        let mut proc = Process::default();
        assert!(!proc.is_stopped());
        
        proc.state.block.stopped = true;
        assert!(proc.is_stopped());
        
        proc.state.block.stopped = false;
        proc.state.trace.stopped = true;
        assert!(proc.is_stopped());
    }
    
    #[test]
    fn test_process_privilege() {
        let mut proc = Process::default();
        assert!(!proc.is_kernel_process());
        
        proc.resources.privilege = Privilege::Kernel;
        assert!(proc.is_kernel_process());
    }
    
    #[test]
    fn test_layered_structure() {
        let proc = Process::default();
        
        assert_eq!(proc.identity.id.pid, proc.pid());
        assert_eq!(proc.identity.id.index, proc.index());
        assert_eq!(proc.identity.endpoint, proc.endpoint());
    }
}
