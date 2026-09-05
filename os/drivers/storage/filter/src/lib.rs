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
//! `notes/rewrite/fork-syscall-rewrite/16-stage-drivers/`.
//!
//! Single-threaded event loop: one message at a time, no shared mutable
//! state across threads.

#![no_std]

pub mod checksum;

/// Service initialization entry (wires the filter table; digest math stays out).
pub fn init() {}
