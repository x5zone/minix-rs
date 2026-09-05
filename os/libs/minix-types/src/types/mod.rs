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
//! - `diagnostic`: panic diagnostic hook registration (D-48, kernel ↔ minix-rt shared)

mod address;
mod diagnostic;
mod bitmap;
mod boot;
mod cell;
mod clock;
mod com;
mod endpoint;
mod errno;
mod id;
mod pid;
mod signal;
mod sysctl;

pub use address::*;
pub use bitmap::*;
pub use boot::*;
pub use cell::*;
pub use clock::*;
pub use com::*;
pub use endpoint::*;
pub use errno::*;
pub use id::*;
pub use pid::*;
pub use signal::*;
pub use sysctl::*;
pub use diagnostic::*;
