//! Minix IPC server.
//!
//! User-space service managing System V semaphores (`semget`/`semctl`/`semop`)
//! and shared memory (`shmget`/`shmat`/`shmdt`/`shmctl`). Single-threaded
//! event loop — one message at a time, no locks.
//!
//! C: `minix3/minix/servers/ipc/` (main.c, sem.c, shm.c, utility.c).
//! Documents: `notes/rewrite/fork-syscall-rewrite/13-stage-ipc/`.
//!
//! Production builds are `no_std`. Only `core`, `alloc`, and the
//! `minix-types` / `minix-sys` protocol crates are available; `std` enters
//! exclusively under `#[cfg(test)]` (test doubles, assertions).

#![cfg_attr(not(test), no_std)]

extern crate alloc;

pub mod dispatch;
pub mod mib_tree;
pub mod server;

pub use dispatch::{Incoming, classify, proc_event_reply_type, should_reply, unknown_call_result};
pub use mib_tree::{
    InfoRoute, KERN_IPC_TABLE, KERN_SYSVIPC, KERN_SYSVIPC_SEM, KERN_SYSVIPC_SEM_INFO,
    KERN_SYSVIPC_SHM, KERN_SYSVIPC_SHM_INFO, KernIpcChild, MOUNT_PATH, route_info_query,
};
pub use server::{CallHandler, IpcServer, IpcStatus, IpcTransport, StubHandler, TransportError};
