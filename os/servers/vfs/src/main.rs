//! Minix-RS VFS Entry Point

// Form chosen per target (following the init precedent,
// `os/commands/sbin/init/src/main.rs:27-35`): host and test builds keep the
// std form; the `target_os = "none"` side is the freestanding real thing
// (a boot image module).
#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

// The freestanding side holds the link reference to minix-rt: the binary's own
// code uses no rt symbol, so an unreferenced rlib is dropped wholesale and the
// three pieces `_start` / `#[panic_handler]` / `#[global_allocator]` vanish
// with it (the panic-halt pattern). The birth chain's `_start` has already
// completed rt initialization before main (lib.rs:86 contract, idempotent), so
// it is not called again here.
#[cfg(all(not(test), target_os = "none"))]
extern crate minix_rt;

// The entry point is split in two by target: the none side satisfies the crt0
// Consumer contract (name + Rust ABI + `-> i32`,
// os/libs/minix-rt/src/crt0.rs:40); the host/test side keeps `()` — since rustc
// 1.94 Termination is no longer implemented for i32, so a host i32 main is
// E0277 (measured with docker minix-ci:1.94).
#[cfg(all(not(test), target_os = "none"))]
#[unsafe(no_mangle)]
extern "Rust" fn main() -> i32 {
    real_main()
}

#[cfg(any(test, not(target_os = "none")))]
fn main() {
    real_main()
}

fn real_main() -> ! {
    use minix_vfs::main_loop;

    // run() diverges (main_loop.rs:7310 `-> !`), so the tail expression is
    // real_main's function tail.
    main_loop::run()
}
