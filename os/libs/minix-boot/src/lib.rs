#![no_std]

//! Minix3 boot protocol types.
//!
//! Provides the data structures and traits shared between boot-shim and kernel
//! during the boot handoff phase. These are **not** IPC protocol types —
//! they describe the boot-time contract (memory map, kernel location, modules).

pub mod kernel_info;
pub mod boot_shim;

pub use kernel_info::*;
pub use boot_shim::*;
