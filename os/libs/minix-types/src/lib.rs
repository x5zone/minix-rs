#![doc = include_str!("../README.md")]
#![no_std]
// Mechanized gate for the silent-error-swallowing family (owned by this
// crate): the `write!` calls that render error sentences return
// `fmt::Result`, which fails silently when dropped; this restriction lint
// exposes it immediately in `cargo clippy`. Classified sites follow the
// family convention and carry `//[SD19-gate] <reason>` plus a
// statement-level `#[allow]`.
#![warn(clippy::let_underscore_must_use)]

//! Minix3 core protocol types.
//!
//! This crate provides the minimal set of types required for cross-service
//! communication in Minix3, equivalent to `include/minix/` in C.
//!
//! # Design Principles
//!
//! 1. **Minimalism**: Only the minimal set required for cross-service communication.
//! 2. **Stability**: These types are the "protocol" between services; changes require care.
//! 3. **No Business Logic**: No private implementation details of any service.
//!
//! # Core Types
//!
//! - `Endpoint`: Endpoint identifier (for IPC).
//! - `Pid`: Process ID.
//! - `UserSlot`: Process table index.
//! - `Uid`/`Gid`: User/Group ID.
//! - `Message`: IPC message.
//!
//! # Why Not Include MProc and Other Process Table Structures?
//!
//! Per Minix3 microkernel design, each service has its own private process table:
//! - **PM/mproc**: Process management, signals, permissions → in `minix-pm` crate.
//! - **VM/vmproc**: Virtual memory, page tables → in `minix-vm` crate.
//! - **VFS/fproc**: File descriptors, directories → in `minix-vfs` crate.
//! - **Kernel/proc**: Scheduling, IPC, registers → in `minix-kernel` crate.
//!
//! Reasons for this design:
//! 1. **Responsibility Isolation**: Each service's process table contains extensive private logic.
//! 2. **Invariant Protection**: State transition logic is bound to complex internal service logic.
//! 3. **Microkernel Principle**: Follows the "minimum knowledge" principle.

// Test builds link std so test modules can use `format!` etc.
// (workspace idiom: kernel/src/lib.rs:29, minix-elf/src/lib.rs:38).
#[cfg(test)]
extern crate std;

pub mod ipc;
pub mod types;

pub use ipc::*;
pub use types::*;
