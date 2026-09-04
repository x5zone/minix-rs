//! Minix VM Server.
//!
//! Virtual memory manager service process.
//! Entry point for the VM server binary.

// In test builds, use the system allocator instead of VmAllocator.
// VmAllocator requires PAGE_ALLOC_PTR which is only set by VmServer::new(),
// but the test harness allocates memory before main() runs.
#[cfg(test)]
#[global_allocator]
static GLOBAL: std::alloc::System = std::alloc::System;

fn main() {
    // In test builds, the global allocator (VmAllocator) requires PAGE_ALLOC_PTR
    // which is only set by VmServer::new(). Since the test harness allocates
    // memory before main() runs, we skip the binary entirely in test mode.
    #[cfg(not(test))]
    {
        use minix_vm::{VmServer, read_boot_params};

        // C: main.c:79-88 is_first_time() — fresh boot gates init_vm().
        // The kernel writes a `VmBootHandoff` page (A1 root identity + A2
        // post-bootstrap classification + boot tables + kernel footprint)
        // and maps it user read-only before scheduling VM. VM consumes it
        // once here: the free list feeds the PMM, `root_paddr` feeds
        // adoption (init_vm_self_pt), and the A2 deduction record is
        // reconciled at this consumer boundary before the allocator is
        // built (07-paging_init_design §6.0).
        let params = read_boot_params();

        let mut server = VmServer::new_with_boot_params(params);

        // C: main.c:101-108 — if(is_first_time()) { init_vm(); __vm_init_fresh=1; }
        if params.is_first_time {
            server.init();
        }

        // C: sef_local_startup() — the RS_INIT handshake happens inside the
        // main loop's priority-2 dispatch (rs_handshake), so no separate
        // SEF startup step is needed (see doc 01-vm-init-main §3.4).
        server.run();
    }
}
