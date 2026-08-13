#![no_std]
#![doc = include_str!("../README.md")]

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

pub mod ipc;
pub mod types;

pub use ipc::*;
pub use types::*;
