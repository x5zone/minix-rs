//! MMC card driver: command set plus power-up order.
//!
//! C correspondence: `minix3/minix/drivers/storage/mmc/mmcblk.c`
//! (664 lines, `mmc_driver` table with open, close, transfer, ioctl,
//! and part callbacks served through `blockdriver_task`,
//! `mmcblk.c:656-662`), `emmc.c` (1030 lines, card protocol),
//! `mmchost_mmchs.c` (1267 lines, OMAP host), `mmchost_dummy.c`
//! (170 lines), and the register headers `sdmmcreg.h`, `sdhcreg.h`,
//! `omap_mmc.h`. This crate owns the card-facing order (which command
//! comes next); the service binary owns host registers and data port
//! traffic. See document `17-storage-misc-driver.md` in
//! `rewrite-notes/16-stage-drivers/`.
//!
//! Single-threaded event loop: one message at a time, no shared mutable
//! state across threads.

#![no_std]

#[cfg(test)]
extern crate std;

extern crate alloc;

pub mod commands;
pub mod face;
pub mod service;

use face::{MmcFace, MmcHost};
use service::MmcService;

/// The host layer's default: no controller is bound, so every card
/// operation is refused honestly.
///
/// The production host (the MMC/SD controller registers, the command and
/// response engine, the card bring-up sequence, and the data transfers —
/// `mmchost_mmchs.c` and `emmc.c`) is the registered seam; the binary
/// replaces this host when it lands.
pub struct UnboundHost;

impl MmcHost for UnboundHost {
    fn card_detect(&mut self, _slot: usize) -> bool {
        false
    }
    fn card_initialize(&mut self, _slot: usize) -> bool {
        false
    }
    fn block_size(&mut self, _slot: usize) -> u32 {
        0
    }
    fn read_blocks(
        &mut self,
        _slot: usize,
        _block: u64,
        _count: u32,
        _buf: &mut [u8],
    ) -> Result<(), i32> {
        Err(-minix_types::ENOSYS)
    }
    fn write_blocks(
        &mut self,
        _slot: usize,
        _block: u64,
        _count: u32,
        _buf: &[u8],
    ) -> Result<(), i32> {
        Err(-minix_types::ENOSYS)
    }
}

/// Builds the service for the binary.
pub fn init<H: MmcHost>(face: MmcFace<H>) -> MmcService<H> {
    MmcService::new(face)
}
