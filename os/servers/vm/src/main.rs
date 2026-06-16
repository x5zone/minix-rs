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
        use minix_vm::VmServer;
        use minix_vm::BootMemRegion;

        let total_pages = 65536;
        let base = 0x100000;
        let size = total_pages * 4096;
        let free_regions = [BootMemRegion { base, size }];
        let mut server = VmServer::new(total_pages, &free_regions);
        server.init();
        server.run();
    }
}
