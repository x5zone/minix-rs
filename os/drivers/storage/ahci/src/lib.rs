//! AHCI host-bus-adapter driver: ports plus identify parsing.
//!
//! C correspondence: `minix3/minix/drivers/storage/ahci/ahci.c` (2734
//! lines) with `ahci.h` (command tables, frames, port registers). This
//! crate owns the port order (stop, program, start, issue, timeout,
//! reset) and the identify parsing; the service binary owns ports,
//! tables, interrupts, and timeouts. See document
//! `16-ahci-ata-driver.md` in
//! `notes/rewrite/fork-syscall-rewrite/16-stage-drivers/`.
//!
//! Single-threaded event loop: one message at a time, no shared mutable
//! state across threads.

#![no_std]

extern crate alloc;

pub mod identify;
pub mod port;

/// Service initialization entry (wires the ports; transport stays out).
pub fn init() {}
