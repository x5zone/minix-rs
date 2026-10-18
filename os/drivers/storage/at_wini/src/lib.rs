//! Parallel ATA driver: controller order plus direct access.
//!
//! C correspondence: `minix3/minix/drivers/storage/at_wini/at_wini.c`
//! (2243 lines) with `at_wini.h` (ports, status bits, drive bounds).
//! This crate owns the controller order (reset, probe, identify, ready)
//! and the direct-access arm policy; the service binary owns ports,
//! interrupts, timeouts, and data movement. See document
//! `16-ahci-ata-driver.md` in
//! `rewrite-notes/16-stage-drivers/`.
//!
//! Single-threaded event loop: one message at a time, no shared mutable
//! state across threads.

#![no_std]

extern crate alloc;

pub mod block_face;
pub mod bus;
pub mod controller;
pub mod dma;
pub mod pio;
pub mod service;

use block_face::WiniBlock;
use minix_blockdriver::pump::BlockService;

/// Build the driver's service for the binary.
///
/// Nothing is probed yet: the identify exchange and partition scan are the
/// register-traffic seam the controller modules own; the block face
/// answers refusals honestly until `install_probe` runs.
pub fn init() -> BlockService<WiniBlock> {
    BlockService::new(WiniBlock::new())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_init_builds_the_block_service() {
        let service = init();
        assert!(service.opened().is_empty());
    }
}
