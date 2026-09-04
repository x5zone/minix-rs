//! Minix-RS Virtual File System (VFS).
//!
//! Virtual file system server, responsible for:
//! - File system call reception and dispatch.
//! - File descriptor management.
//! - vnode/vmnt/filp reference count management.
//! - Process lifecycle event interaction with PM (fork/exec/exit).
//! - Communication with file system services (MFS/PFS).
//!
//! # Architecture
//!
//! VFS is the only server in Minix3 that uses multi-threading (mthread).
//! The main thread receives messages and dispatches, worker threads handle specific syscalls.
//! Blocking I/O operations only block the current worker thread, not others.
//!
//! # Module Structure
//!
//! - `main_loop`: Main loop and message dispatch.
//! - `fproc`: VFS process structure.
//! - `worker`: Worker thread management.
//! - `call_table`: System call dispatch table.
//! - `ipc`: IPC message handling.

pub mod bdev;
pub mod call_table;
pub mod cdev;
pub mod coredump;
pub mod device_map;
pub mod exec;
pub mod fcntl;
pub mod filedes;
pub mod filp;
pub mod fproc;
pub mod fs_comm;
pub mod ipc;
pub mod link;
pub mod main_loop;
pub mod misc;
pub mod mount;
pub mod open;
pub mod path;
pub mod pipe;
pub mod protect;
pub mod read_write;
pub mod request;
pub mod sdev;
pub mod select;
pub mod socket;
pub mod stadir;
pub mod tll;
pub mod vmnt;
pub mod vnode;
pub mod worker;

pub use fproc::*;
pub use ipc::*;
