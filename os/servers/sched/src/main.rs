//! Minix-RS scheduler — entry point.
//!
//! C: `main()` — `minix3/minix/servers/sched/main.c:22-96`.
//! See `notes/rewrite/fork-syscall-rewrite/06-stage-sched/01-sched-init-main.md`.

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
        use minix_sched::sef::{MachineInfo, init_fresh};

        // C: sef_local_startup() — main.c:111-121. The two init names are
        // `SchedInitKind::{Fresh, RestartStateful}`; restart needs no
        // sched code (libsef generic), so there is no registration table
        // to build here.

        // C: sys_getmachine(&machine) — main.c:130. The kernel binding
        // lands with the KernelApi wiring (12-kernel-interface.md); a
        // failed query is fatal, exactly as C `panic`s (main.c:131) —
        // never enter the loop half-knowing the machine.
        let machine = MachineInfo {
            processors_count: 1,
            bsp_id: 0,
        };
        let _ready = init_fresh(machine);

        // C: init_scheduling() — main.c:133, owned by 11-balance-queues.md.
        // C: main loop — main.c:35-96, owned by 02-sched-message-surface.md:
        // receive → dispatch → reply lives there, not here.
        //
        // Spinning is intentional until 02 lands the loop body: a scheduler
        // that cannot receive yet must not pretend otherwise.
        #[allow(clippy::empty_loop)]
        loop {}
    }
}
