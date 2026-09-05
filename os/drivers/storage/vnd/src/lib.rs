//! Virtual node disk driver: file-backed loop device layout.
//!
//! C correspondence: `minix3/minix/drivers/storage/vnd/vnd.c`
//! (603 lines, single file). The driver registers `vnd_dtab`
//! (`vnd.c:39-45`, type `DISK` with open, close, transfer, ioctl,
//! part, and geometry callbacks — the fullest table of the five
//! miscellaneous drivers) and serves it through `blockdriver_task`
//! (`vnd.c:592-600`). This crate owns the layout half (chunk split,
//! geometry derivation); the service binary owns file descriptor
//! traffic (`pread`, `pwrite`, `fsync`). See document
//! `17-storage-misc-driver.md` in
//! `notes/rewrite/fork-syscall-rewrite/16-stage-drivers/`.
//!
//! Single-threaded event loop: one message at a time, no shared mutable
//! state across threads.

#![no_std]

extern crate alloc;

pub mod layout;

/// Service initialization entry (wires the loop table; file traffic stays out).
pub fn init() {}
