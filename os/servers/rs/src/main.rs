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

        // C: sys_getimage(image) — main.c:196. The kernel's GET_IMAGE
        // answer is the boot truth (do_getinfo.c:86-90); a failure stops
        // the boot (C panic, main.c:196-198) — half-true tables must not
        // reach the boot invariants below (ARCH A-13 resolved: the tables
        // now come from the kernel, not the placeholder).
        let tables = BootTables::acquire_from(&minix_sys::syscall::DirectKernelCallTransport)
            .unwrap_or_else(|e| panic!("RS boot: sys_getimage failed: errno {e}"));

        // C main.c:171 — env_parse("rs_verbose", "d", 0, &rs_verbose, 0, 1)
        // over the monitor parameters (GET_MONPARAMS → S42 批一真值链)。
        // verbose 带来的 printf 增量在 C 遍布 rs_verbose 门控点；Rust 侧
        // 的消费面随 19/E-11 接线补齐，此处先取真值并经诊断缝确认解析。
        let mut monparams = [0u8; 1024]; // MULTIBOOT_PARAM_BUF_SIZE — param.h:28
        let monparams_ok =
            minix_sys::syscall::sys_getmonparams(&minix_sys::syscall::DirectKernelCallTransport, &mut monparams)
                .is_ok();
        let rs_verbose = monparams_ok && minix_rs::boot::parse_rs_verbose(&monparams);
        if rs_verbose {
            use minix_sys::syscall::{sys_diagctl, DirectKernelCallTransport};
            let line = b"RS: running in verbose mode\n";
            let _ = sys_diagctl(&DirectKernelCallTransport, 1, line.as_ptr() as u64, line.len() as i32);
        }

        let mut server = RsServer::new(tables);

        // C: sef_startup() → sef_cb_init_fresh() — main.c:151,158-494.
        // The KernelApi production impl is DEFERRED (19); the fresh-boot
        // path fails closed (Err(ENOSYS)) until then. C treats boot failure
        // as fatal (main.c:226 `panic(...)`); do not enter the main loop on
        // an incomplete boot — an RS that never finished booting cannot
        // manage services. The SEF face carries a bare errno; the typed
        // cause (E-6) distinguishes "kernel face not wired" (`ENOSYS`) from
        // "boot invariant violation" (`EINVAL`, e.g. a corrupt boot table).
        if let Err(e) = server.init(SefInitType::Fresh) {
            panic!(
                "RS boot failed: {e:?} (boot diagnostic: {:?})",
                server.boot_diagnostic()
            );
        }

        // C: main loop — main.c:50-131 (skeleton; details in 06).
        // Fail-closed (T2): the unwired receive face returns ENOSYS, ending
        // the run loop with an error instead of spinning or panicking. The
        // 19 wiring replaces the seam and the loop becomes long-running.
        let _ = server.run();
    }
}
