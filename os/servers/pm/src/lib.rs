//! Minix-RS Process Manager (PM).
//!
//! Responsible for:
//! - Process creation and destruction (fork, exec, exit)
//! - Process state management
//! - Signal handling
//! - Waiting for child processes (wait)
//!
//! # Architecture
//!
//! Following Minix3 microkernel design, PM has its own process table (`mproc`),
//! linked to VM, VFS, and Kernel process tables via `endpoint`.
//!
//! # Why MProc is in PM crate, not minix-types?
//!
//! 1. **Separation of concerns**: MProc contains private logic only PM cares about (signal handling, parent-child tree, etc.)
//! 2. **Invariant protection**: State transition logic binds PM internal complex logic, putting in public library would break invariants
//! 3. **Microkernel principle**: Follows "minimum knowledge" principle, other services don't need to know PM's internal implementation
//!
//! # Module Structure
//!
//! - `mproc`: PM process table module (private)
//! - `fork`: fork system call entry
//! - `exec`: exec system call
//! - `exit`: exit system call
//! - `signal`: signal handling
//! - `wait`: wait for child process
//! - `ipc`: IPC message handling

extern crate alloc;

pub mod mproc;
pub mod fork;
pub mod exec;
pub mod exit;
pub mod signal;
pub mod wait;
pub mod ipc;

pub use mproc::*;
pub use ipc::*;

pub fn init() {
}

pub fn run() -> ! {
    loop {
    }
}
