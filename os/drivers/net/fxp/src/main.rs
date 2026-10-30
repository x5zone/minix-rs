//! driver (net/fxp) entry point (placeholder).
#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

// The freestanding side holds a link reference to minix-rt: the bin code
// itself uses no rt symbol, so an unreferenced rlib is dropped wholesale and
// `_start`/`#[panic_handler]`/`#[global_allocator]` disappear with it (the
// `extern crate` retention semantics of panic-halt mode). The birth chain's
// `_start` has already completed rt initialization before main (`lib.rs` init
// contract: in no_std mode `_start` calls it automatically, idempotently), so
// there is no need -- and no call -- for a second one here.
#[cfg(all(not(test), target_os = "none"))]
extern crate minix_rt;

// The entry point splits into two shapes by target: on none it satisfies the
// crt0 Consumer contract (name + Rust ABI + `-> i32`,
// `os/libs/minix-rt/src/crt0.rs`); host and test builds keep `()` -- since
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
    // Placeholder: not yet a real service process (event loop + RS startup
    // protocol).
    minix_driver_fxp::init();
    loop {}
}
