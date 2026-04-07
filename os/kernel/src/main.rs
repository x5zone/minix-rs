//! Minix-RS Kernel Entry Point
//!
//! 内核启动入口

#![no_std]
#![no_main]

use minix_kernel::init;
use minix_kernel::run;
use minix_kernel::PanicInfo;

/// 内核入口
#[no_mangle]
pub extern "C" fn _start() -> ! {
    init();
    run();
}

/// Panic 处理
#[cfg(not(test))]
#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    // TODO: 输出 panic 信息
    loop {}
}
