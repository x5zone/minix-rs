//! Pipe file server entry point.
//!
//! Builds the server value and parks until the service runtime arrives.
//! Process startup handshake, privilege drop, signal wiring, and the event
//! loop belong to the service-runtime stage; they are wired here once that
//! stage lands (the driver table needs no changes for it).

fn main() {
    let _server = minix_fs_pfs::init();
    loop {}
}
