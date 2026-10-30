//! Minix-RS scheduler — entry point.
//!
//! C: `main()` — `minix3/minix/servers/sched/main.c:22-96`.
//! See `rewrite-notes/06-stage-sched/01-sched-init-main.md`
//! (the startup order) and `02-sched-message-surface.md` (the loop's turn).

#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

// In test builds, use the system allocator (the crate is no_std in
// production; the test harness allocates before main() runs).
#[cfg(test)]
#[global_allocator]
static GLOBAL: std::alloc::System = std::alloc::System;

// The freestanding side holds a link reference to minix-rt: the bin code itself uses no
// rt symbol, so an unreferenced rlib would be dropped wholesale and `_start`/
// `#[panic_handler]`/`#[global_allocator]` would vanish with it (the `extern crate`
// retention semantics of panic-halt mode). The birth chain's `_start` has already
// completed rt initialization before main (`lib.rs:86` init contract: `_start` calls it
// automatically in no_std mode, and it is idempotent), so it must not and need not be
// called here.
#[cfg(all(not(test), target_os = "none"))]
extern crate minix_rt;

// The entry comes in two forms by target: the none side satisfies the crt0 Consumer
// contract (name + Rust ABI + `-> i32`, os/libs/minix-rt/src/crt0.rs:40); the host/test
// side stays `()` — since rustc 1.94, Termination is no longer implemented for i32, so
// an i32 main on the host is E0277 (measured with docker minix-ci:1.94).
#[cfg(all(not(test), target_os = "none"))]
#[unsafe(no_mangle)]
extern "Rust" fn main() -> i32 {
    real_main()
}

#[cfg(any(test, not(target_os = "none")))]
fn main() {
    real_main()
}

fn real_main() -> ! {
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

        // Install point of the second VM_BRK migration (the first, for DS, used the same
        // approach via `minix_rt::install_vm_supplier`): switch the global heap supply to
        // the VM_BRK growing heap, with the initial break = the linker image end (C
        // `brksize.S` initializes `_brksize` from `_end`, the same symbol). The install
        // itself does no IPC; only the first allocation beyond the fixed pool actually
        // issues VM_BRK. In birth order SCHED comes after VM (`rs/boot.rs` boot table), so
        // VM's main loop is already receiving requests at install time; EINVAL (image end
        // of 0 = a link defect) fails fast.
        #[cfg(target_os = "none")]
        minix_rt::install_vm_supplier(minix_rt::image_end())
            .expect("sched: install_vm_supplier(镜像尾) 不得失败");

        // On-hardware evidence probe for the second VM_BRK migration (same approach as
        // DS): an 8-byte slab request is forced through `add_slab`→`supply_page`→`VM_BRK`,
        // and success prints one serial line — evidence that the VM_BRK leg is live for an
        // earlier-born service; the probe block returns to the slab free list with free
        // (the VM side does not reclaim pages: shrinking brk never unmaps, same as C).
        #[cfg(target_os = "none")]
        {
            let probe = minix_rt::alloc(8);
            assert!(!probe.is_null(), "sched: vm-brk probe 分配失败");
            minix_rt::free(probe);
            use minix_sys::syscall::{DirectKernelCallTransport, sys_diagctl_write};
            // [SD19-gate] fallback to park when the report channel itself is broken (panic.c:66)
            #[allow(clippy::let_underscore_must_use)]
            let _ = sys_diagctl_write(
                &DirectKernelCallTransport,
                "nk4a: sched-vmbrk probe ok pages=1 via=VM_BRK\n",
            );
        }

        // C: main loop — main.c:35-96, owned by 02-sched-message-surface.md:
        // receive → dispatch → reply, forever (02's `run` adds the
        // transport-failure bound; see server.rs for the documented
        // deviation from C's panic-on-first-failure).
        server.run(&KernelIpcTransport::new(), &mut kernel);
    }
    #[cfg(test)]
    unreachable!("bin entry unused in test builds")
}
