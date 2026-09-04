//! Minix-RS data store — entry point.
//!
//! C: `main()` — `minix3/minix/servers/ds/main.c:28-88`.
//! See `notes/rewrite/fork-syscall-rewrite/07-stage-ds/01-ds-init-main.md`.

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
        // C: env_setargs + sef_local_startup() — main.c:38-39. The two init
        // names are `DsInitKind::{Fresh, RestartStateful}` plus the
        // `LiveUpdateHook::DsStateTransfer` promise (sef.rs); the fresh
        // body and the transfer body land in 06, transport in the shell.
        //
        // C: main loop — main.c:42-86, owned by dispatch.rs: receive →
        // triage → dispatch → reply lives there, not here.
        //
        // Spinning is intentional until the loop lands: a registry that
        // cannot receive yet must not pretend otherwise.
        #[allow(clippy::empty_loop)]
        loop {}
    }
}
