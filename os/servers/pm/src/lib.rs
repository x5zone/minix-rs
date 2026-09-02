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
//! - `init`: PM startup chain (SEF init_fresh equivalent)
//! - `fork`: fork system call entry
//! - `exec`: exec system call
//! - `exit`: exit system call
//! - `signal`: signal handling
//! - `wait`: wait for child process
//! - `ipc`: IPC message handling

extern crate alloc;

pub mod event;
pub mod exec;
pub mod exit;
pub mod fork;
pub mod init;
pub mod ipc;
pub mod credentials;
pub mod mproc;
pub mod sched;
pub mod misc;
pub mod signal;
pub mod signal_flow;
pub mod signal_handlers;
pub mod time;
pub mod timer;
pub mod trace;
pub mod wait;

pub use ipc::*;
pub use mproc::*;
