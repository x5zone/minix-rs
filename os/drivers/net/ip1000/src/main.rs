//! Driver (net/ip1000) entry point (placeholder).
#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

// The freestanding side holds a link reference to minix-rt: the bin code
// itself uses no rt symbol, so an unreferenced rlib would be dropped
// wholesale and `_start` / `#[panic_handler]` / `#[global_allocator]`
// would vanish with it (the retention semantics of `extern crate` in
// panic-halt mode). The birth chain `_start` has already run rt
// initialization before main (the `lib.rs` init contract: in no_std mode
// `_start` calls it automatically and idempotently), so there is no need
// here, nor any reason, to call it again.
#[cfg(all(not(test), target_os = "none"))]
extern crate minix_rt;

// The entry point has two forms by target: the none side satisfies the
// crt0 Consumer contract (name + Rust ABI + `-> i32`, see
// `os/libs/minix-rt/src/crt0.rs`); the host and test side keeps `()` —
// since rustc 1.94, Termination is no longer implemented for i32.
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
            "drv: ip1000 placeholder (not implemented); parking\n",
        );
    }
    minix_driver_ip1000::init();
    loop {}
}
