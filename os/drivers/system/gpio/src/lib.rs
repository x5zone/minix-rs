//! GPIO driver: pins as files, plus driver-side device registration.
//!
//! C correspondence: `minix3/minix/drivers/system/gpio/gpio.c` (290
//! lines, pin export through the virtual-tree filesystem) with
//! `minix3/minix/lib/libdevman/` (driver-side device add/delete plus USB
//! bind tracking, see the `minix-devman` crate). This crate owns the pin
//! numbers and the export rules; the service binary owns the filesystem
//! and the wire. See document `12-gpio-devman.md` in
//! `notes/rewrite/fork-syscall-rewrite/16-stage-drivers/`.
//!
//! Single-threaded event loop: one message at a time, no shared mutable
//! state across threads.

#![no_std]

extern crate alloc;

pub mod files;
pub mod pins;

/// Service initialization entry (wires the pin table; transport stays out).
pub fn init() {}
