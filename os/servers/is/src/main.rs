//! Minix-RS Information Server — entry point.
//!
//! C: `main()` — `minix3/minix/servers/is/main.c:31-71`.
//! See `notes/rewrite/fork-syscall-rewrite/08-stage-is/01-is-init-main.md`.

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
        use minix_is::{IsServer, UnimplementedAcquires, sef::SysSefTransport, tty_fkey::SysFkeyCtl};

        // S23 片 1:`SysSefTransport` 生产装配——receive 走 minix-sef
        // (ping 透明)、send 走 trap 阻塞 send、warn/diag 走 SYS_DIAGCTL
        // 诊断缝(A-6)。Fkey/Acquires 两件仍在制(S23 片 2/3),以
        // fail-closed 占位——它们只在对应 dump 请求到达时才触达,启动
        // 与主循环可真实运转。
        let mut server = IsServer::new(SysSefTransport::new(), SysFkeyCtl, UnimplementedAcquires);

        // C: sef_local_startup() + boot anchor — main.c:40-41,94-102.
        // Boot failure is fatal (C panics on startup paths); do not enter
        // the main loop on an incomplete boot.
        match server.startup() {
            Ok(_) => server.run(),
            Err(e) => panic!("IS boot failed: {e:?}"),
        }
    }
}
