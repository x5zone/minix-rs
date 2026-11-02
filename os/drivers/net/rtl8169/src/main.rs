//! driver (net/rtl8169) entry point (placeholder).
#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

// The freestanding side holds a link reference to minix-rt: the bin code itself
// touches no symbol from rt, and an unreferenced rlib is dropped wholesale,
// taking `_start`, `#[panic_handler]` and `#[global_allocator]` with it (the
// `extern crate` retention semantics of panic-halt).  The birth chain's `_start`
// has already completed rt initialization before main (`lib.rs` init contract:
// in no_std mode `_start` calls it automatically and idempotently), so it must
// not and need not be called again here.
#[cfg(all(not(test), target_os = "none"))]
extern crate minix_rt;

// The entry point is split by target: the none side satisfies the crt0 Consumer
// contract (name + Rust ABI + `-> i32`, `os/libs/minix-rt/src/crt0.rs`); the
// host/test side stays `()` — since rustc 1.94 nothing implements Termination
// for i32.
#[cfg(all(not(test), target_os = "none"))]
#[unsafe(no_mangle)]
extern "Rust" fn main() -> i32 {
    real_main();
    0
}

#[cfg(any(test, not(target_os = "none")))]
fn main() {
    real_main()
}

// Parked placeholder: the busy wait is intentional (the process must not
// exit, or the server manager would keep restarting it).
#[allow(clippy::empty_loop)]
fn real_main() {
    // Placeholder driver: report the unimplemented state on the kernel
    // diagnostic channel instead of spinning silently, then park forever.
    // No hardware probe runs here and no capability is reported to the
    // server registry or the device discovery layer.
    #[cfg(target_os = "none")]
    {
        use minix_sys::syscall::{DirectKernelCallTransport, sys_diagctl_write};
        #[allow(clippy::let_underscore_must_use)] // [SD19-gate] when the report channel itself breaks, parking is the fallback (panic.c:66)
        let _ = sys_diagctl_write(
            &DirectKernelCallTransport,
            "drv: rtl8169 placeholder (not implemented); parking\n",
        );
    }
    minix_driver_rtl8169::init();
    loop {}
}
