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

#![cfg_attr(not(test), no_std)]
// Mechanical gate: the lint flags every unqualified `let _`.
#![warn(clippy::let_underscore_must_use)]


// alloc macros (vec!/format!) imported at crate level: alloc supplies them on
// the no_std side; under the test-mode std form they keep the same semantics
// as the prelude macros of the same name (dual-form init precedent).
#[macro_use]
extern crate alloc;

// Diagnostic line output (C counterpart is printf, the best-effort prints
// along main.c):
// - Production (no_std): no stderr; goes through the kernel diagctl channel
//   (minix-sys sys_diagctl_write, same as the rs verbose lines); failures are
//   swallowed — best-effort semantics.
// - Test mode (std form): keeps eprintln!, host-test behavior unchanged.
#[cfg(not(test))]
macro_rules! pm_diag {
    ($($arg:tt)*) => {{
        let line = alloc::format!($($arg)*);
        //[SD19-gate] PM send/notify/reply are fire-and-forget
        #[allow(clippy::let_underscore_must_use)]
        let _ = minix_sys::syscall::sys_diagctl_write(
            &minix_sys::syscall::DirectKernelCallTransport, &line);
    }};
}
#[cfg(test)]
macro_rules! pm_diag {
    ($($arg:tt)*) => { eprintln!($($arg)*) };
}

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

// Explicit public API surface: the former `pub use ipc::*; pub use mproc::*;`
// flattened the two module trees ipc/ and mproc/ onto the crate root — five
// pairs of same-named modules (the logic and state layers of
// fork/signal/wait/credentials/trace) exposed a single set of symbols at the
// root, leaving users unable to tell the source apart. Everything now goes
// through full module paths (`pm::ipc::*` / `pm::mproc::*` / `pm::init::*`),
// keeping only the explicit re-exports the test seams need (14 internal sites
// plus the established paths of the external integration tests).
pub use ipc::TestIpcTransport;
