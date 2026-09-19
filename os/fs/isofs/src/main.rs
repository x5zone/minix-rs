//! ISO9660 server binary.
//!
//! The semantic layer (mount scan, lookup, reads, listings, status) lives
//! in `server.rs` and is exercised hosted against in-memory images; the
//! binary itself stays parked until the production disc channel (the
//! tracked E-FSBDEV seam) can hand it a real image — wiring `serve` here
//! before that seam would pretend a disc exists.

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
