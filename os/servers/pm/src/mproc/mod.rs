//! PM process table module (mproc).
//!
//! This is the Rust implementation of Minix3 `mproc` structure, containing PM's private process management logic.
//!
//! # Architecture
//!
//! Following Minix3 microkernel design, process tables are distributed across multiple services:
//! - **PM/mproc**: Process management, signals, permissions (this module)
//! - **VM/vmproc**: Virtual memory, page tables
//! - **VFS/fproc**: File descriptors, directories
//! - **Kernel/proc**: Scheduling, IPC, register saving
//!
//! Each process table is linked via `endpoint`.
//!
//! # Why in PM crate, not minix-types?
//!
//! 1. **Separation of concerns**: MProc contains private logic only PM cares about (signal handling, parent-child tree, etc.)
//! 2. **Invariant protection**: State transition logic binds PM internal complex logic, putting in public library would break invariants
//! 3. **Microkernel principle**: Follows "minimum knowledge" principle, other services don't need to know PM's internal implementation
//!
//! # Module Structure
//!
//! - `constants`: PM private constants (NR_PIDS, INIT_PID, etc.)
//! - `pid_gen`: PID generator
//! - `mproc`: PM process structure definition
//! - `table`: PM process table management
//! - `lifecycle`: Process lifecycle state machine
//! - `block`: Block state
//! - `wait`: Parent wait state
//! - `guardianship`: Guardianship relationship (parent/tracer)
//! - `trace`: Trace state
//! - `signal`: Signal handling state
//! - `credentials`: Credentials
//! - `context`: PM context
//! - `fork`: fork implementation

mod constants;
mod pid_gen;
mod mproc;
mod table;
mod lifecycle;
mod block;
mod wait;
mod guardianship;
mod trace;
mod signal;
mod credentials;
mod context;
mod fork;

pub use constants::*;
pub use pid_gen::*;
pub use mproc::*;
pub use table::*;
pub use lifecycle::*;
pub use block::*;
pub use wait::*;
pub use guardianship::*;
pub use trace::*;
pub use signal::*;
pub use credentials::*;
pub use context::*;
pub use fork::*;
