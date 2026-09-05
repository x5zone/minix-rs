//! Parallel ATA driver: controller order plus direct access.
//!
//! C correspondence: `minix3/minix/drivers/storage/at_wini/at_wini.c`
//! (2243 lines) with `at_wini.h` (ports, status bits, drive bounds).
//! This crate owns the controller order (reset, probe, identify, ready)
//! and the direct-access arm policy; the service binary owns ports,
//! interrupts, timeouts, and data movement. See document
//! `16-ahci-ata-driver.md` in
//! `notes/rewrite/fork-syscall-rewrite/16-stage-drivers/`.
//!
//! Single-threaded event loop: one message at a time, no shared mutable
//! state across threads.

#![no_std]

extern crate alloc;

pub mod controller;
pub mod dma;

/// Service initialization entry (wires the controller; transport stays out).
pub fn init() {}
