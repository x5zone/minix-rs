//! driver (net/lan8710a) entry point (placeholder).
#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

// The freestanding side holds a link reference to minix-rt: the bin code
// itself uses no symbol from rt, and an unreferenced rlib is dropped wholesale,
// taking `_start`/`#[panic_handler]`/`#[global_allocator]` with it (the
// `extern crate` retention semantics of panic-halt mode). The birth chain's
// `_start` has already completed rt initialization before main
// (`lib.rs` init contract: in no_std mode `_start` calls it automatically and
// is idempotent), so there is no need to call it here — and it must not be.
#[cfg(all(not(test), target_os = "none"))]
extern crate minix_rt;

// The entry point has two forms per target: the none side satisfies the crt0
// Consumer contract (name + Rust ABI + `-> i32`,
// `os/libs/minix-rt/src/crt0.rs`); the host and test sides keep `()` — since
// rustc 1.94, Termination is no longer implemented for i32.
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
    // Not yet implemented: this should become a real service process (event
    // loop + RS startup protocol).
    minix_driver_lan8710a::init();
    loop {}
}
