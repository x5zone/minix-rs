//! Virtio block driver: the framework's first full consumer.
//!
//! C correspondence:
//! `minix3/minix/drivers/storage/virtio_blk/virtio_blk.c` (754 lines).
//! This crate owns the request shape (three-segment chains, sector
//! math, status mapping) and the drive geometry; the service binary
//! owns the queue, the maps, and the sleep. See document
//! `15-virtio-blk-driver.md` in
//! `rewrite-notes/16-stage-drivers/`.
//!
//! Single-threaded event loop: one message at a time, no shared mutable
//! state across threads.

#![no_std]

extern crate alloc;

pub mod char_face;
pub mod geometry;
pub mod request;

use char_face::VirtioBlkFace;
use minix_blockdriver::pump::BlockService;

/// Build the driver's service for the binary.
///
/// The drive capacity, read-only flag, and flush capability come from the
/// virtio device configuration, which is the queue-traffic seam this crate
/// does not model yet; without a probe result the face reports zero
/// sectors, not a fabricated size.
pub fn init() -> BlockService<VirtioBlkFace> {
    BlockService::new(VirtioBlkFace::new(0, false, false))
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
