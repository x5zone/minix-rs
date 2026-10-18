#![doc = include_str!("../README.md")]
#![no_std]
// SD-19 静默吞错家族机械化门（本 crate 接手）：错误句子渲染里的 `write!`
// 返回 `fmt::Result`，掉空即静默；这个 restriction lint 让它在 `cargo clippy`
// 当场现形。已定性的站点按同族惯例写 `//[SD19-gate] <理由>` + 语句级
// `#[allow]`。
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
