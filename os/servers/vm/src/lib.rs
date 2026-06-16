#![cfg_attr(not(test), no_std)]

//! Minix-RS Virtual Memory (VM) server.
//!
//! This crate implements the VM server as a single-threaded user-space
//! process, matching Minix3's execution model. The VM server runs an
//! event loop that receives IPC messages, dispatches them to handlers,
//! and sends replies — one message at a time.
//!
//! # Single-threaded model
//!
//! The entire crate assumes **single-threaded execution**. This is
//! fundamental to the design and justifies several patterns:
//!
//! - `Rc`/`RefCell` are acceptable (no cross-thread sharing needed).
//! - `AssumeSyncCell` / `unsafe impl Sync` on BSS statics is safe
//!   because only the VM thread accesses them.
//! - `UnsafeCell` in process table slots is safe because the typestate
//!   system ensures exclusive access within the single event loop
//!   iteration.
//! - No mutex/lock is needed for internal data structures.
//!
//! Violating the single-threaded assumption (e.g., spawning threads,
//! sharing references across async boundaries) would introduce UB.
//!
//! # no_std
//!
//! Production builds are `no_std`. Only `core`, `alloc`, and custom
//! crates are used. `std` is only available under `#[cfg(test)]`.

extern crate alloc;

pub(crate) mod global;
pub(crate) mod vmproc;
pub(crate) mod acl;
pub(crate) mod fork;
pub(crate) mod alloc_stats;
pub(crate) mod critical_pool;
pub(crate) mod phys_mem;
pub(crate) mod region;
pub(crate) mod pagetable;
pub(crate) mod memtype;
pub(crate) mod ipc;
pub(crate) mod direct_map;
pub(crate) mod alloc_page;
pub(crate) mod heap_arena;
pub(crate) mod page_cache;
pub(crate) mod vfs_queue;
pub(crate) mod fdref;
pub(crate) mod exit;
pub(crate) mod brk;
pub(crate) mod munmap;
pub(crate) mod mmap;
pub(crate) mod map_phys;
pub(crate) mod cow_exec_pf;
pub(crate) mod rs;
pub(crate) mod query;
pub(crate) mod sanity;

pub use vm_server::VmServer;

pub use phys_mem::BootMemRegion;


mod vm_server;
