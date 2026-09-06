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

// 公共 API 面显式化（P2-4，2026-09-06）：原 `pub use ipc::*; pub use mproc::*;`
// 把 ipc/ 与 mproc/ 两棵模块树压平到 crate 根——顶层与 mproc 下 5 对同名
// 模块（fork/signal/wait/credentials/trace 的 logic 层与 state 层）在根上
// 只暴露一份符号，使用者无法分辨来源。现在统一走完整模块路径
// （`pm::ipc::*` / `pm::mproc::*` / `pm::init::*`），仅保留测试接缝的
// 显式 re-export（内部 14 处 + 外部集成测试的既有惯例路径）。
pub use ipc::TestIpcTransport;
