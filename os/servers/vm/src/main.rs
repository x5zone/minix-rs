//! Minix VM Server.
//!
//! Virtual memory manager service process.
//! Entry point for the VM server binary.

use minix_vm::VmServer;
use minix_vm::BootMemRegion;

fn main() {
    let total_pages = 65536;
    let base = 0x100000;
    let size = total_pages * 4096;
    let free_regions = [BootMemRegion { base, size }];
    let mut server = VmServer::new(total_pages, &free_regions);
    server.init();
    server.run();
}
