//! Entry point for the net/dec21140a driver (placeholder).
#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

// The freestanding side holds the minix-rt link reference: the bin code
// itself uses no symbol from rt, so an unreferenced rlib is dropped
// wholesale, and `_start`/`#[panic_handler]`/`#[global_allocator]`
// disappear with it (the `extern crate` retention semantics of the
// panic-halt pattern). The birth chain's `_start` has already completed
// rt initialization before main (the `lib.rs` init contract: in no_std
// mode `_start` calls it automatically and is idempotent), so calling it
// here is neither needed nor appropriate.
#[cfg(all(not(test), target_os = "none"))]
extern crate minix_rt;

// The entry splits by target: the none side satisfies the crt0 Consumer
// contract (name + Rust ABI + `-> i32`, `os/libs/minix-rt/src/crt0.rs`);
// the host and test side keeps `()` — since rustc 1.94 `Termination` is
// no longer implemented for i32.
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
    // Placeholder: to be implemented as a real service process (event
    // loop + RS start protocol).
    minix_driver_dec21140a::init();
    loop {}
}
