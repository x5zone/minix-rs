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
//! `notes/rewrite/fork-syscall-rewrite/16-stage-drivers/`.
//!
//! Single-threaded event loop: one message at a time, no shared mutable
//! state across threads.

#![no_std]

pub mod commands;

/// Service initialization entry (wires the card table; host traffic stays out).
pub fn init() {}
