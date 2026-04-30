//! Minix-RS Runtime Library.
//!
//! User-space runtime support.

#![cfg_attr(not(feature = "std"), no_std)]

#[cfg(not(feature = "std"))]
use core::panic::PanicInfo;

#[cfg(feature = "std")]
use std::process::exit;

/// Runtime initialization.
pub fn init() {
    // TODO: Initialize runtime
}

/// Program entry wrapper (only used in no_std mode).
#[cfg(all(not(test), not(feature = "std")))]
#[no_mangle]
pub extern "C" fn _start() -> ! {
    init();

    // TODO: Call main function
    // let argc = ...;
    // let argv = ...;
    // let ret = main(argc, argv);

    exit(0);
}

/// Allocates memory.
pub fn alloc(size: usize) -> *mut u8 {
    // TODO: Implement memory allocation
    core::ptr::null_mut()
}

/// Frees memory.
pub fn free(ptr: *mut u8) {
    // TODO: Implement memory deallocation
}

/// Panic handler (only used in no_std mode).
#[cfg(all(not(test), not(feature = "std")))]
#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    // TODO: Output panic information
    exit(1);
}
