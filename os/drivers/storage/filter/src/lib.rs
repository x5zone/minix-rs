//! Filter driver: checksum policy plus mirror fallback.
//!
//! C correspondence: `minix3/minix/drivers/storage/filter/driver.c`
//! (1051 lines), `main.c` (412 lines), `sum.c` (620 lines), `crc.c`
//! (88 lines), `md5.c` (315 lines), `util.c` (68 lines). The driver
//! registers `filter_tab` (`main.c:83-87`, type `OTHER` with open,
//! close, transfer, and ioctl callbacks) and serves it through
//! `blockdriver_task` (`main.c:332-342`). This crate owns the policy
//! half (checksum kinds, group layout, mirror health); the service
//! binary owns digest math and lower-driver traffic. See document
//! `17-storage-misc-driver.md` in
//! `rewrite-notes/16-stage-drivers/`.
//!
//! Single-threaded event loop: one message at a time, no shared mutable
//! state across threads.

#![no_std]

#[cfg(test)]
extern crate std;

extern crate alloc;

pub mod checksum;
pub mod face;
pub mod service;

use face::{FilterFace, LowerLeg};
use service::FilterService;

/// The middle layer's default: no lower driver is bound yet, so every
/// transfer is refused honestly.
///
/// The production leg (the block client over the two lower drivers, with
/// the checksum engine and the async retry machinery) is the registered
/// seam; the binary replaces this leg when it lands.
pub struct UnboundLeg;

impl LowerLeg for UnboundLeg {
    fn transfer(
        &mut self,
        _position: u64,
        _buf: &mut [u8],
        _size: u64,
        _do_write: bool,
    ) -> Result<u64, i32> {
        Err(-minix_types::ENOSYS)
    }
    fn check_drivers(&mut self) -> Result<(), i32> {
        Err(-minix_types::ENOSYS)
    }
    fn raw_size(&mut self) -> u64 {
        0
    }
}

/// Builds the service for the binary.
pub fn init<L: LowerLeg>(face: FilterFace<L>) -> FilterService<L> {
    FilterService::new(face)
}
