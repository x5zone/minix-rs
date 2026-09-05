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
pub mod events;
pub mod lifecycle;
pub mod mib_tree;
pub mod perms;
pub mod sem;
pub mod server;
pub mod shm;

pub use dispatch::{Incoming, classify, proc_event_reply_type, should_reply, unknown_call_result};
pub use events::{EventKind, ProcEvent, Subscription, SyncAction, ack_type};
pub use lifecycle::{ShutdownVerdict, Signal, shutdown_check};
pub use mib_tree::{
    InfoRoute, KERN_IPC_TABLE, KERN_SYSVIPC, KERN_SYSVIPC_SEM, KERN_SYSVIPC_SEM_INFO,
    KERN_SYSVIPC_SHM, KERN_SYSVIPC_SHM_INFO, KernIpcChild, MOUNT_PATH, route_info_query,
};
pub use perms::{AccessVerdict, Identity, IpcPerm, IpcPermSysctl, check_perm, is_owner_or_root};
pub use server::{CallHandler, IpcServer, IpcStatus, IpcTransport, StubHandler, TransportError};
