//! AHCI host-bus-adapter driver: ports plus identify parsing.
//!
//! C correspondence: `minix3/minix/drivers/storage/ahci/ahci.c` (2734
//! lines) with `ahci.h` (command tables, frames, port registers). This
//! crate owns the port order (stop, program, start, issue, timeout,
//! reset) and the identify parsing; the service binary owns ports,
//! tables, interrupts, and timeouts. See document
//! `16-ahci-ata-driver.md` in
//! `rewrite-notes/16-stage-drivers/`.
//!
//! Single-threaded event loop: one message at a time, no shared mutable
//! state across threads.

#![no_std]

extern crate alloc;

pub mod block_face;
pub mod identify;
pub mod port;

use block_face::AhciFace;
use minix_blockdriver::pump::BlockService;

/// Build the driver's service for the binary.
///
/// The port capacity comes from the IDENTIFY exchange, which is the
/// register-traffic seam this crate does not model yet; without a probe
/// result the face reports zero sectors rather than a fabricated size.
pub fn init() -> BlockService<AhciFace> {
    BlockService::new(AhciFace::new(0))
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
