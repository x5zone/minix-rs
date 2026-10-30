//! Minix-RS MIB — entry point.
//!
//! C: `main()` — `minix3/minix/servers/mib/main.c:433-492`.
//! See `rewrite-notes/10-stage-mib/01-mib-init-main.md`.

#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

// In test builds, use the system allocator (the crate is no_std in
// production; the test harness allocates before main() runs).
#[cfg(test)]
#[global_allocator]
static GLOBAL: std::alloc::System = std::alloc::System;


// The freestanding side holds the link reference to minix-rt: the bin code
// itself uses no symbol from rt, and an unreferenced rlib is dropped whole,
// taking `_start`, `#[panic_handler]`, and `#[global_allocator]` with it (the
// `extern crate` retention semantics of the panic-halt pattern). The birth
// chain's `_start` already completed rt initialization before main (the
// `lib.rs:86` init contract: called automatically in no_std mode and
// idempotent), so there is no need—and no reason—to call it again here.
#[cfg(all(not(test), target_os = "none"))]
extern crate minix_rt;

// The entry point splits by target: the none side satisfies the crt0 Consumer
// contract (name + Rust ABI + `-> i32`, os/libs/minix-rt/src/crt0.rs:40); the
// host/test side keeps `()`—since rustc 1.94, Termination is no longer
// implemented for i32, and an i32 main on the host is E0277 (measured in
// docker minix-ci:1.94).
#[cfg(all(not(test), target_os = "none"))]
#[unsafe(no_mangle)]
extern "Rust" fn main() -> i32 {
    // real_main consumes its return value in place (); 0 is the process exit
    // code (crt0 Consumer contract).
    real_main();
    0
}

#[cfg(any(test, not(target_os = "none")))]
fn main() {
    real_main()
}

fn real_main() {
    // In test builds, skip the binary entirely (the test harness drives the
    // library directly).
    #[cfg(not(test))]
    {
        use minix_mib::server::{MibServer, Server, SysIpc};
        use minix_mib::transport::{SysServices, SysTransport};

        // C: mib_startup() — main.c:415-431. The fresh anchor builds the
        // static tree (`MibTree::init`, arena.rs) and resets the remote
        // slot table; the restart anchor rebuilds it (sef.rs).
        let server = MibServer::new();
        let ipc = SysIpc::default();
        let kernel = SysTransport;
        let services = SysServices::default();

        // Migration of the global heap supply to a VM_BRK-grown heap: the
        // initial break is the tail of the linked image (`minix_rt::image_end`,
        // the same symbol C's `brksize.S` uses to publish `_brksize` from
        // `_end`). The install itself issues no IPC (it only switches the
        // allocator shape); the first allocation that exceeds the fixed pool
        // actually sends VM_BRK. In the birth order MIB starts after VM (the
        // install table in `rs/boot.rs`), so VM's main loop is already taking
        // requests at install time; EINVAL (image tail is 0 = link defect)
        // fails fast. Same approach as the earlier DS and SCHED migrations.
        #[cfg(target_os = "none")]
        minix_rt::install_vm_supplier(minix_rt::image_end())
            .expect("mib: install_vm_supplier(镜像尾) 不得失败");

        // Real-machine evidence probe: right after the install, take one page
        // through the VM leg (an 8-byte slab request triggers add_slab →
        // supply_page → VM_BRK) and fail fast on error. On success print one
        // serial marker—L1 evidence that the VM_BRK growth leg works end to
        // end; the probe block returns to the slab free list on free (the VM
        // side never reclaims pages: shrinking the break never unmaps, same
        // as C).
        #[cfg(target_os = "none")]
        {
            let probe = minix_rt::alloc(8);
            assert!(!probe.is_null(), "mib: vm-brk probe 分配失败");
            minix_rt::free(probe);
            use minix_sys::syscall::{DirectKernelCallTransport, sys_diagctl_write};
            #[allow(clippy::let_underscore_must_use)] // [SD19-gate] park as the last resort when the reporting channel itself is broken (panic.c:66)
            let _ = sys_diagctl_write(
                &DirectKernelCallTransport,
                "nk4a: mib-vmbrk probe ok pages=1 via=VM_BRK\n",
            );
        }

        // C: main loop — main.c:443-488: receive → triage → dispatch →
        // reply, forever; a broken transport dies at the documented
        // 64-turn bound (server.rs, the SCHED/DS-shared deviation from
        // C's panic-on-first-failure).
        Server::new(server, ipc, kernel, services).run();
    }
}
