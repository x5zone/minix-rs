//! Minix-RS Reincarnation Server — entry point.
//!
//! C: `main()` — `minix3/minix/servers/rs/main.c:38-131`.
//! See `rewrite-notes/03-stage-rs/01-rs-boot-init.md`.

#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

// In test builds, use the system allocator (the crate is no_std in
// production; the test harness allocates before main() runs).
#[cfg(test)]
#[global_allocator]
static GLOBAL: std::alloc::System = std::alloc::System;


// The freestanding side holds a link reference to minix-rt: the bin code
// itself uses no rt symbol, so an unreferenced rlib would be dropped whole
// and `_start`/`#[panic_handler]`/`#[global_allocator]` would vanish with
// it (the `extern crate` retention semantics of panic-halt mode). The birth
// chain `_start` has already completed rt initialization before main
// (`lib.rs:86` init contract: in no_std mode `_start` calls it
// automatically, idempotently), so it must not and need not be called here
// again.
#[cfg(all(not(test), target_os = "none"))]
extern crate minix_rt;

// The entry point splits into two forms per target: the none side satisfies
// the crt0 Consumer contract (name + Rust ABI + `-> i32`,
// `os/libs/minix-rt/src/crt0.rs:40`); the host/test sides keep `()` —
// since rustc 1.94 Termination is no longer implemented for i32, and an i32
// main on the host is an immediate E0277 (measured in docker
// minix-ci:1.94).
#[cfg(all(not(test), target_os = "none"))]
#[unsafe(no_mangle)]
extern "Rust" fn main() -> i32 {
    // real_main consumes the () it returns; 0 is the process exit code
    // (crt0 Consumer contract).
    real_main();
    0
}

#[cfg(any(test, not(target_os = "none")))]
fn main() {
    real_main()
}

fn real_main() {
    // In test builds, skip the binary entirely (the test harness drives the
    // library directly).
    #[cfg(not(test))]
    {
        use minix_rs::{RsServer, SefInitType, boot::BootTables};

        // C: sef_local_startup() — main.c:51. The SEF callback set is the
        // `SefCallbacks` trait implemented by `RsServer` itself; there is no
        // separate registration value to construct.

        // C: sys_getimage(image) — main.c:196. The kernel's GET_IMAGE
        // answer is the boot truth (do_getinfo.c:86-90); a failure stops
        // the boot (C panic, main.c:196-198) — half-true tables must not
        // reach the boot invariants below (ARCH A-13 resolved: the tables
        // now come from the kernel, not the placeholder).
        let tables = BootTables::acquire_from(&minix_sys::syscall::DirectKernelCallTransport)
            .unwrap_or_else(|e| panic!("RS boot: sys_getimage failed: errno {e}"));

        // C main.c:171 — env_parse("rs_verbose", "d", 0, &rs_verbose, 0, 1)
        // over the monitor parameters (GET_MONPARAMS is the boot truth
        // chain). C gates the extra printf output on rs_verbose at many
        // points; the Rust side grows that consumption surface once the
        // wiring lands, so here we only take the value and confirm through
        // the diagnostic seam that it parsed.
        let mut monparams = [0u8; 1024]; // MULTIBOOT_PARAM_BUF_SIZE — param.h:28
        let monparams_ok =
            minix_sys::syscall::sys_getmonparams(&minix_sys::syscall::DirectKernelCallTransport, &mut monparams)
                .is_ok();
        let rs_verbose = monparams_ok && minix_rs::boot::parse_rs_verbose(&monparams);
        if rs_verbose {
            use minix_sys::syscall::{sys_diagctl, DirectKernelCallTransport};
            let line = b"RS: running in verbose mode\n";
            // [SD19-gate] best-effort wrap-up; discarding the Result does
            // not change the already-established main result (C is
            // void/ignores at the same spot).
            #[allow(clippy::let_underscore_must_use)]
            let _ = sys_diagctl(&DirectKernelCallTransport, 1, line.as_ptr() as u64, line.len() as i32);
        }

        let mut server = RsServer::new(tables);

        // C: sef_startup() → sef_cb_init_fresh() — main.c:151,158-494.
        // The production KernelApi is TrapKernelApi (a direct trap to the
        // kernel; `RsServer::new` assembles it by default in its
        // constructor). C treats boot failure
        // as fatal (main.c:226 `panic(...)`); do not enter the main loop on
        // an incomplete boot — an RS that never finished booting cannot
        // manage services. The SEF face carries a bare errno; the typed
        // cause distinguishes "kernel face not wired" (`ENOSYS`) from
        // "boot invariant violation" (`EINVAL`, e.g. a corrupt boot table).
        if let Err(e) = server.init(SefInitType::Fresh) {
            panic!(
                "RS boot failed: {e:?} (boot diagnostic: {:?})",
                server.boot_diagnostic()
            );
        }

        // C: main loop — main.c:50-131 (skeleton; details in doc 06). C's
        // while(TRUE) never returns; leaving the loop means a
        // transport/boot failure and is fatal — the same panic as a boot
        // failure (silently swallowing the Err in `let _ =` would exit the
        // process with status 0 and fake success).
        if let Err(e) = server.run() {
            panic!("RS main loop terminated: {e:?}");
        }
    }
}
