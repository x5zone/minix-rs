//! Minix VM Server.
//!
//! Virtual memory manager service process.
//! Entry point for the VM server binary.

use minix_vm::VmServer;

fn main() {
    let mut server = VmServer::new();
    server.init();
    server.run();
}
