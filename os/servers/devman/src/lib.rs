#![cfg_attr(not(test), no_std)]

//! Minix-RS DEVMAN server.
//!
//! Device manager service: exposes the device tree as a virtual filesystem
//! (VTreeFS shape) and routes driver messages (ADD/DEL/BIND/UNBIND).
//!
//! # Single-threaded model
//!
//! Like the VM server (`os/servers/vm/src/lib.rs`), the entire crate assumes
//! **single-threaded event-loop execution**: one IPC message or VFS request
//! at a time. `Cell`/`RefCell` state (e.g. [`hooks::FirstGuard`]) is safe
//! here; no `Arc`/`Mutex` is needed. Do not spawn threads or share these
//! types across threads.
//!
//! # no_std
//!
//! Production builds are `no_std` (`core` + `alloc` only).
//! `std` is available under `#[cfg(test)]` for the test harness.

extern crate alloc;

pub mod add_device;
pub mod bind;
pub mod buf;
pub mod del_device;
pub mod device_tree;
pub mod event_queue;
pub mod files;
pub mod hooks;
pub mod ipc;
pub mod rs_contract;
pub mod server;
pub mod structs;
pub mod vtreefs;
pub mod wire;

pub use hooks::{FirstGuard, FsHooks, RootStat, SefLifecycle, ServerConfig};

/// Crate-level init entry (called by `main.rs`; real init runs on mount).
pub fn init() {}
