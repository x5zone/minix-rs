//! Minix-RS Kernel Entry Point
//!
//! Kernel boot entry.

#![no_std]
#![no_main]

use minix_kernel::init;
use minix_kernel::run;
use minix_kernel::PanicInfo;

/// Kernel entry point.
#[unsafe(no_mangle)]
pub extern "C" fn _start() -> ! {
    init();
    run();
}

/// Panic handler.
#[cfg(not(test))]
#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    // TODO: Output panic information
    loop {}
}
