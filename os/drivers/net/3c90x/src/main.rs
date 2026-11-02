//! Driver (net/3c90x) entry point (placeholder).
#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

// The freestanding side holds the link reference to minix-rt: the bin code
// itself uses none of rt's symbols, and an unreferenced rlib is dropped whole,
// taking `_start`/`#[panic_handler]`/`#[global_allocator]` with it (the
// `extern crate` keeps-the-rlib semantics of panic-halt mode). The birth
// chain's `_start` has already completed rt initialization before main (the
// `lib.rs` init contract: in no_std mode `_start` calls it automatically and
// it is idempotent), so calling it here is neither needed nor right.
#[cfg(all(not(test), target_os = "none"))]
extern crate minix_rt;

// The entry point splits into two forms by target: the none side satisfies the
// crt0 Consumer contract (name + Rust ABI + `-> i32`,
// `os/libs/minix-rt/src/crt0.rs`); the hosted and test sides keep
// `()` — since rustc 1.94, Termination is no longer implemented for i32.
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
            "drv: 3c90x placeholder (not implemented); parking\n",
        );
    }
    minix_driver_3c90x::init();
    loop {}
}
