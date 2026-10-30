//! driver (net/atl2) entry point (placeholder).
#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

// The freestanding side holds a link reference to minix-rt: the bin code
// itself uses no rt symbols, so an unreferenced rlib would be dropped
// wholesale and the trio `_start`/`#[panic_handler]`/`#[global_allocator]`
// would vanish with it (the `extern crate` retention semantics of
// panic-halt). The birth chain already ran rt initialization before main
// (`lib.rs` init contract: `_start` calls it automatically and idempotently
// in no_std mode), so there is no need — and no reason — to call it again.
#[cfg(all(not(test), target_os = "none"))]
extern crate minix_rt;

// The entry splits by target into two forms: the none side satisfies the
// crt0 Consumer contract (name + Rust ABI + `-> i32`,
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

fn real_main() {
    // Not yet implemented: run as a real service process (event loop + RS
    // startup protocol).
    minix_driver_atl2::init();
    loop {}
}
