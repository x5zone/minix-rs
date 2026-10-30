//! driver (net/vt6105) entry point (placeholder).
#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

// The freestanding side holds a link reference to minix-rt: the bin code itself uses no
// rt symbol, so an unreferenced rlib would be dropped wholesale and `_start`/
// `#[panic_handler]`/`#[global_allocator]` would vanish with it (the `extern crate`
// retention semantics of panic-halt mode). The birth chain's `_start` has already
// completed rt initialization before main (`lib.rs` init contract: `_start` calls it
// automatically in no_std mode, and it is idempotent), so it must not and need not be
// called here.
#[cfg(all(not(test), target_os = "none"))]
extern crate minix_rt;

// The entry comes in two forms by target: the none side satisfies the crt0 Consumer
// contract (name + Rust ABI + `-> i32`, `os/libs/minix-rt/src/crt0.rs`); the host and
// test side stays `()` — since rustc 1.94, Termination is no longer implemented for i32.
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

fn real_main() {
    // Not yet implemented as a real service process (event loop + RS start protocol).
    minix_driver_vt6105::init();
    loop {}
}
