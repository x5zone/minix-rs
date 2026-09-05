//! Electrically-erasable memory driver: page slicing policy.
//!
//! C correspondence: `minix3/minix/drivers/eeprom/cat24c256/cat24c256.c`
//! (505 lines). This crate owns the slicing half (read and write
//! splits, address width); the service binary owns bus traffic. See
//! document `24-misc-drivers.md` in
//! `notes/rewrite/fork-syscall-rewrite/16-stage-drivers/`.
//!
//! Single-threaded event loop: one message at a time, no shared mutable
//! state across threads.

#![no_std]

extern crate alloc;

pub mod pages;

/// Service initialization entry (wires the memory table; bus traffic stays out).
pub fn init() {}
