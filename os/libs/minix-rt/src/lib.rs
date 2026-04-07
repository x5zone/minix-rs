//! Minix-RS Runtime Library
//!
//! 用户态运行时支持

#![cfg_attr(not(feature = "std"), no_std)]

// 如果使用 std，直接使用 std 的 panic_handler
#[cfg(not(feature = "std"))]
use core::panic::PanicInfo;

#[cfg(feature = "std")]
use std::process::exit;

/// 运行时初始化
pub fn init() {
    // TODO: 初始化运行时
}

/// 程序入口包装（仅在 no_std 模式下使用）
#[cfg(all(not(test), not(feature = "std")))]
#[no_mangle]
pub extern "C" fn _start() -> ! {
    init();

    // TODO: 调用 main 函数
    // let argc = ...;
    // let argv = ...;
    // let ret = main(argc, argv);

    exit(0);
}

/// 分配内存
pub fn alloc(size: usize) -> *mut u8 {
    // TODO: 实现内存分配
    core::ptr::null_mut()
}

/// 释放内存
pub fn free(ptr: *mut u8) {
    // TODO: 实现内存释放
}

/// panic 处理（仅在 no_std 模式下使用）
#[cfg(all(not(test), not(feature = "std")))]
#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    // TODO: 输出 panic 信息
    exit(1);
}
