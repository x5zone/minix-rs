//! Type definitions module.
//!
//! Provides Minix3 core type definitions:
//! - `com`: System-level constants (MAX_NR_TASKS, NR_PROCS, etc.)
//! - `pid`: Process ID, process index
//! - `endpoint`: Endpoint identifier (core IPC concept)
//! - `id`: User ID, Group ID
//! - `clock`: Clock ticks, timestamp, file offset
//! - `address`: Virtual/physical address types
//! - `bitmap`: Generic bitmap
//! - `boot`: Boot image types
//! - `cell`: Single-threaded interior mutability primitives
//! - `errno`: POSIX errno constants

mod com;
mod pid;
mod endpoint;
mod id;
mod clock;
mod address;
mod bitmap;
mod boot;
mod cell;
mod errno;

pub use com::*;
pub use pid::*;
pub use endpoint::*;
pub use id::*;
pub use clock::*;
pub use address::*;
pub use bitmap::*;
pub use boot::*;
pub use cell::*;
pub use errno::*;
