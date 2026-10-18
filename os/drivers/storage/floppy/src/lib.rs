//! Floppy disk driver: geometry plus retry policy.
//!
//! C correspondence: `minix3/minix/drivers/storage/floppy/floppy.c`
//! (1355 lines) with `floppy.h` and `liveupdate.c` (78 lines). The
//! driver registers `f_dtab` (`floppy.c:268-274`, type `DISK` with
//! open, close, transfer, cleanup, part, and geometry callbacks) and
//! serves it through `blockdriver_task` (`floppy.c:293-299`). This
//! crate owns the pure policy half (density table, error budget);
//! the service binary owns FDC port traffic, seeks, and interrupts.
//! See document `17-storage-misc-driver.md` in
//! `rewrite-notes/16-stage-drivers/`.
//!
//! Single-threaded event loop: one message at a time, no shared mutable
//! state across threads.

#![no_std]

#[cfg(test)]
extern crate std;

extern crate alloc;

pub mod face;
pub mod geometry;
pub mod service;

use face::{FdcLeg, FloppyFace};
use service::FloppyService;

/// The device layer's default: no controller is bound, so every sector
/// operation is refused honestly.
///
/// The production leg (the FDC register sequencing, DMA setup, the motor
/// timer, and the interrupt wait, `floppy.c:530-1240`) is the registered
/// seam; the binary replaces this leg when it lands.
pub struct UnboundFdc;

impl FdcLeg for UnboundFdc {
    fn test_read(&mut self, _drive: u32, _density: usize, _test: u64) -> Result<(), i32> {
        Err(-minix_types::ENOSYS)
    }
    fn read_sector(&mut self, _drive: u32, _block: u64, _buf: &mut [u8]) -> Result<(), i32> {
        Err(-minix_types::ENOSYS)
    }
    fn write_sector(&mut self, _drive: u32, _block: u64, _buf: &[u8]) -> Result<(), i32> {
        Err(-minix_types::ENOSYS)
    }
    fn motor_running(&mut self, _drive: u32) -> bool {
        false
    }
}

/// Builds the service for the binary.
pub fn init<L: FdcLeg>(face: FloppyFace<L>) -> FloppyService<L> {
    FloppyService::new(face)
}
