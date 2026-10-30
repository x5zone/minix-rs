//! Minix-RS Information Server — entry point.
//!
//! C: `main()` — `minix3/minix/servers/is/main.c:31-71`.
//! See `rewrite-notes/08-stage-is/01-is-init-main.md`.

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

        // Assembly: `SysSefTransport` (receive transparent through the minix-sef
        // ping, send blocking, warn+diag through the SYS_DIAGCTL seam),
        // `SysFkeyCtl` (fkey_ctl -> minix-sys tty::fkey_ctl_via), and
        // `SysAcquires` (all six channels: SYS_GETINFO's seven whats + stacktrace +
        // uptime + GET_KMESSAGES + getsysinfo's five legs + VM_INFO's three
        // queries — no fail-closed placeholder; a failed fetch is reported and the
        // run continues, C's warn-and-continue).
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
