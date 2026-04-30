//! Minix-RS Kernel
//!
//! Microkernel implementation, including:
//! - Process management (proc)
//! - IPC mechanism (ipc)
//! - Scheduler (sched)
//! - Virtual memory - kernel part (vm)
//! - Hardware abstraction (hal, arch)

#![no_std]
#![cfg_attr(not(test), no_main)]

pub mod ipc;
pub mod priv_table;
pub mod proc;
pub mod sched;
pub mod vm;

#[cfg(feature = "mock")]
pub mod arch;
#[cfg(feature = "mock")]
pub mod boot;
#[cfg(feature = "mock")]
pub mod clock;
#[cfg(feature = "mock")]
pub mod debug;
#[cfg(feature = "mock")]
pub mod hal;
#[cfg(feature = "mock")]
pub mod include;
#[cfg(feature = "mock")]
pub mod system;

pub use core::panic::PanicInfo;

/// Kernel initialization.
pub fn init() {
    // TODO: Initialize subsystems
}

/// Kernel main loop.
pub fn run() -> ! {
    loop {
        // TODO: Scheduling loop
    }
}
