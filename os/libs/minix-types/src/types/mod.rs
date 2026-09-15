//! Type definitions module.
//!
//! Provides Minix3 core type definitions:
//! - `com`: System-level constants (MAX_NR_TASKS, NR_PROCS, etc.)
//! - `pid`: Process ID, process index
//! - `endpoint`: Endpoint identifier (core IPC concept)
//! - `proc_nr`: Process number (kernel process table slot number)
//! - `id`: User ID, Group ID
//! - `clock`: Clock ticks, timestamp, file offset
//! - `address`: Virtual/physical address types
//! - `bitmap`: Generic bitmap
//! - `boot`: Boot image types
//! - `cell`: Single-threaded interior mutability primitives
//! - `errno`: POSIX errno constants
//! - `diagnostic`: panic diagnostic hook registration (D-48, kernel ↔ minix-rt shared)
//! - `grant`: grant wire layout (`cp_grant_t` family — kernel ↔ user-space
//!   grant-table contract, E-DSWIRE)
//! - `kerninfo`: `MINIX_KERNINFO` shared page ABI (`minix_kerninfo` /
//!   `kuserinfo` + OS release constants, E-KERNINFO)

mod address;
mod bitmap;
mod boot;
mod cell;
mod clock;
mod com;
mod diagnostic;
mod endpoint;
mod errno;
mod grant;
mod id;
mod kerninfo;
mod pid;
mod proc_info;
mod proc_nr;
mod signal;
mod sysctl;
mod sysctl_abi;

pub use address::*;
pub use bitmap::*;
pub use boot::*;
pub use cell::*;
pub use clock::*;
pub use com::*;
pub use diagnostic::*;
pub use endpoint::*;
pub use errno::*;
pub use grant::*;
pub use id::*;
pub use kerninfo::*;
pub use pid::*;
pub use proc_info::*;
pub use proc_nr::*;
pub use signal::*;
pub use sysctl::*;
pub use sysctl_abi::*;
