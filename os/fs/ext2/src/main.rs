//! ext2 server binary.
//!
//! The read-only semantic layer (mount gates, inode reads, block walk,
//! lookup, reads, listings, status) lives in `server.rs` and is exercised
//! hosted against in-memory images; the binary stays parked until the
//! production disc channel (the tracked E-FSBDEV seam) hands it a real
//! image, and the write half (F3c: balloc/ialloc, namespace operations)
//! lands after that.

#[cfg(test)]
#[global_allocator]
static GLOBAL: std::alloc::System = std::alloc::System;

fn main() {
    #[cfg(not(test))]
    {
        // Production disc channel pending (E-FSBDEV); fail-closed park.
        loop {
            core::hint::spin_loop();
        }
    }
}
