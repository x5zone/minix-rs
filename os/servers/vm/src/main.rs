//! Minix VM Server.
//!
//! Virtual memory manager service process.
//! Entry point for the VM server binary.

// In test builds, use the system allocator instead of VmAllocator.
// VmAllocator requires PAGE_ALLOC_PTR which is only set by VmServer::new(),
// but the test harness allocates memory before main() runs.
#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

// The freestanding side holds a link reference to minix-rt: the bin code itself
// touches no symbol from rt, and an unreferenced rlib is dropped wholesale,
// taking `_start`, `#[panic_handler]` and `#[global_allocator]` with it (the
// `extern crate` retention semantics of panic-halt).  The birth chain's `_start`
// has already completed rt initialization before main (`lib.rs:86` init
// contract: in no_std mode `_start` calls it automatically and idempotently), so
// it must not and need not be called again here.
#[cfg(all(not(test), target_os = "none"))]
extern crate minix_rt;

#[cfg(test)]
#[global_allocator]
static GLOBAL: std::alloc::System = std::alloc::System;

// The entry point is split by target: the none side satisfies the crt0 Consumer
// contract (name + Rust ABI + `-> i32`, os/libs/minix-rt/src/crt0.rs:40); the
// host/test side stays `()` — since rustc 1.94 nothing implements Termination
// for i32, so an i32 main on the host is E0277 (observed with docker
// minix-ci:1.94).
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
    // In test builds, the global allocator (VmAllocator) requires PAGE_ALLOC_PTR
    // which is only set by VmServer::new(). Since the test harness allocates
    // memory before main() runs, we skip the binary entirely in test mode.
    #[cfg(not(test))]
    {
        use minix_vm::{BootRoute, VmServer, boot_route, install_server, read_boot_params, run_installed, with_installed_server};

        // C: main.c:79-88 is_first_time() — fresh boot gates init_vm().
        // The kernel writes a `VmBootHandoff` page (A1 root identity + A2
        // post-bootstrap classification + boot tables + kernel footprint)
        // and maps it user read-only before scheduling VM. VM consumes it
        // once here: the free list feeds the PMM, `root_paddr` feeds
        // adoption (init_vm_self_pt), and the A2 deduction record is
        // reconciled at this consumer boundary before the allocator is
        // built (07-paging_init_design §6.0).
        let params = read_boot_params();

        // C: main.c:101-108 — if(is_first_time()) { init_vm();
        // __vm_init_fresh=1; } — rerouted through the image-slot router
        // ([ARCH: V20-P1-1], adjudicated option A: the server state is
        // image-resident, so a warm start in the same process finds the
        // slot full and skips init; a re-exec'd process finds it empty,
        // which C has no usable semantics for — the loud rejection stays).
        match boot_route(params.is_first_time) {
            BootRoute::ColdInit => {
                let server = VmServer::new_with_boot_params(params);
                // The slot refuses a second install: init runs exactly once.
                if install_server(server).is_err() {
                    panic!("VM cold boot: the image slot already holds a server");
                }
                with_installed_server(|server| server.init());
            }
            BootRoute::WarmRun => {}
            BootRoute::WarmNoState => panic!(
                "VM warm restart (is_first_time = false) with an empty image slot: \
                 the re-exec'd process carries no VM state and C main.c:101-108 \
                 has no usable semantics for this shape either (02-stage-vm/todo.md \
                 V20-P1-1, evidence case ③)"
            ),
            BootRoute::DoubleInit => panic!(
                "VM cold boot with the image slot already holding a server: \
                 init must run exactly once (02-stage-vm/todo.md V20-P1-1)"
            ),
        }

        // C: sef_local_startup() — the RS_INIT handshake happens inside the
        // main loop's priority-2 dispatch (rs_handshake), so no separate
        // SEF startup step is needed (see doc 01-vm-init-main §3.4).
        run_installed();
    }
    #[cfg(test)]
    unreachable!("bin entry unused in test builds")
}
