//! Minix-RS Reincarnation Server — entry point.
//!
//! C: `main()` — `minix3/minix/servers/rs/main.c:38-131`.
//! See `notes/rewrite/fork-syscall-rewrite/03-stage-rs/01-rs-boot-init.md`.

// In test builds, use the system allocator (the crate is no_std in
// production; the test harness allocates before main() runs).
#[cfg(test)]
#[global_allocator]
static GLOBAL: std::alloc::System = std::alloc::System;

fn main() {
    // In test builds, skip the binary entirely (the test harness drives the
    // library directly).
    #[cfg(not(test))]
    {
        use minix_rs::{RsServer, SefInitType, boot::BootTables};

        // C: sef_local_startup() — main.c:51. The SEF callback set is the
        // `SefCallbacks` trait implemented by `RsServer` itself (N5); there
        // is no separate registration value to construct.

        // C: sys_getimage(image) — main.c:196. Placeholder until the
        // sys_getimage wiring lands (19-rs-external-interfaces.md); the
        // production path injects the kernel's boot_image[] copy here
        // (ARCH A-13).
        let tables = BootTables::placeholder();

        let mut server = RsServer::new(tables);

        // C: sef_startup() → sef_cb_init_fresh() — main.c:151,158-494.
        // The KernelApi production impl is DEFERRED (19); the fresh-boot
        // path fails closed (Err(ENOSYS)) until then. C treats boot failure
        // as fatal (main.c:226 `panic(...)`); do not enter the main loop on
        // an incomplete boot — an RS that never finished booting cannot
        // manage services.
        if let Err(e) = server.init(SefInitType::Fresh) {
            panic!(
                "RS boot failed: {e:?} (kernel API wiring pending — 19-rs-external-interfaces.md)"
            );
        }

        // C: main loop — main.c:50-131 (skeleton; details in 06).
        server.run();
    }
}
