//! Minix-RS scheduler — entry point.
//!
//! C: `main()` — `minix3/minix/servers/sched/main.c:22-96`.
//! See `notes/rewrite/fork-syscall-rewrite/06-stage-sched/01-sched-init-main.md`
//! (the startup order) and `02-sched-message-surface.md` (the loop's turn).

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
        use minix_sched::cpu::MachineTopology;
        use minix_sched::kernel_api::transport::{KernelApi, KernelIpcTransport, SysKernelApi};
        use minix_sched::sef::init_fresh;
        use minix_sched::server::SchedServer;

        // C: sef_local_startup() — main.c:111-121. The two init names are
        // `SchedInitKind::{Fresh, RestartStateful}`; restart needs no sched
        // code (libsef generic), so there is no registration table to build
        // here and the fresh path below runs unconditionally.

        // C: sys_getmachine(&machine) — main.c:130. A failed query is fatal,
        // exactly as C `panic`s (main.c:131) — never enter the loop
        // half-knowing the machine.
        let mut kernel = SysKernelApi::new();
        let ready = kernel
            .get_machine()
            .map(init_fresh)
            .unwrap_or_else(|errno| panic!("couldn't get machine info: {errno}"));

        // The ready token carries the machine; the server is born around it
        // (01 D3: no boot, no wait — the token is the order, not a formality).
        let mut server = SchedServer::new(MachineTopology {
            processors_count: ready.machine.processors_count,
            bsp_id: ready.machine.bsp_id,
        });

        // C: init_scheduling() — main.c:133, owned by 11-balance-queues.md:
        // five seconds times the clock rate, armed once. C panics on a
        // failed arm (schedule.c:340-341); the Err dies here, at the binary
        // layer, where every startup failure already dies (01 D2).
        server
            .init_scheduling(&mut kernel)
            .unwrap_or_else(|errno| panic!("sys_setalarm failed: {errno}"));

        // C: main loop — main.c:35-96, owned by 02-sched-message-surface.md:
        // receive → dispatch → reply, forever (02's `run` adds the
        // transport-failure bound; see server.rs for the documented
        // deviation from C's panic-on-first-failure).
        server.run(&KernelIpcTransport::new(), &mut kernel);
    }
}
