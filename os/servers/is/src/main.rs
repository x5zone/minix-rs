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
        use minix_is::{IsServer, SysAcquires, sef::SysSefTransport, tty_fkey::SysFkeyCtl};

        // S23 装配:`SysSefTransport`(receive 经 minix-sef ping 透明/
        // send 阻塞/warn+diag 走 SYS_DIAGCTL 缝)、`SysFkeyCtl`
        // (fkey_ctl → minix-sys tty::fkey_ctl_via)、`SysAcquires`
        // (六通道全装:SYS_GETINFO 七 what + stacktrace + uptime +
        // GET_KMESSAGES + getsysinfo 五腿 + VM_INFO 三查询——无 fail-closed
        // 占位,取数失败按 C 的 warn-and-continue 报错继续)。
        let mut server = IsServer::new(SysSefTransport::new(), SysFkeyCtl, SysAcquires::default());

        // C: sef_local_startup() + boot anchor — main.c:40-41,94-102.
        // Boot failure is fatal (C panics on startup paths); do not enter
        // the main loop on an incomplete boot.
        match server.startup() {
            Ok(_) => server.run(),
            Err(e) => panic!("IS boot failed: {e:?}"),
        }
    }
}
