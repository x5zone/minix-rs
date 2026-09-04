//! Minix-RS MIB — entry point.
//!
//! C: `main()` — `minix3/minix/servers/mib/main.c:433-492`.
//! See `notes/rewrite/fork-syscall-rewrite/10-stage-mib/01-mib-init-main.md`.

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
        // C: mib_startup() — main.c:440. The two init names are
        // `MibInitKind::{Fresh, RestartLossy}` (sef.rs); the init body and
        // the transport loop land in 04 and the IPC shell respectively.
        //
        // C: main loop — main.c:443-488, owned by dispatch.rs: receive →
        // triage → run → reply lives there, not here.
        //
        // Spinning is intentional until the loop lands: a sysctl server
        // that cannot receive yet must not pretend otherwise.
        #[allow(clippy::empty_loop)]
        loop {}
    }
}
