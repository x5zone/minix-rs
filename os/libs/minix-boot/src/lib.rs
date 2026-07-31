#![no_std]

//! Minix3 boot protocol types.
//!
//! Provides the data structures and traits shared between boot-shim and kernel
//! during the boot handoff phase. These are **not** IPC protocol types —
//! they describe the boot-time contract (memory map, kernel location, modules).

#[cfg(test)]
extern crate alloc;

pub mod kernel_info;
pub mod boot_shim;
pub mod platform;

pub use kernel_info::*;
pub use boot_shim::*;
pub use platform::{
    PlatformDescKind, PlatformDescSource, PlatformDesc, PlatformSource,
    InterruptControllerDesc, TimerDesc, ConsoleDesc,
    CpuTopology, CpuInfo, ArchMiscDesc, MAX_CPUS,
    PlatformParseError,
    DTB, RSDP,
};
