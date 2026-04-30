//! Minix VM Server.
//!
//! Virtual memory manager service process.

struct VmServer {
}

impl VmServer {
    fn new() -> Self {
        Self {}
    }

    fn init(&mut self) {
    }

    fn run(&mut self) {
        loop {
        }
    }
}

fn main() {
    let mut server = VmServer::new();
    server.init();
    server.run();
}
