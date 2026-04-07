//! Minix-RS Kernel
//!
//! 微内核实现，包含：
//! - 进程管理 (proc)
//! - IPC 机制 (ipc)
//! - 调度器 (sched)
//! - 虚拟内存（内核部分）(vm)
//! - 硬件抽象 (hal, arch)

#![no_std]
#![cfg_attr(not(test), no_main)]

// 子模块
pub mod ipc;
pub mod proc;
pub mod sched;
pub mod vm;

// 条件编译的模块
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

// 错误处理
pub use core::panic::PanicInfo;

/// 内核初始化
pub fn init() {
    // TODO: 初始化各个子系统
}

/// 内核主循环
pub fn run() -> ! {
    loop {
        // TODO: 调度循环
    }
}
