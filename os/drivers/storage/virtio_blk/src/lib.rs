//! Virtio block driver: the framework's first full consumer.
//!
//! C correspondence:
//! `minix3/minix/drivers/storage/virtio_blk/virtio_blk.c` (754 lines).
//! This crate owns the request shape (three-segment chains, sector
//! math, status mapping) and the drive geometry; the service binary
//! owns the queue, the maps, and the sleep. See document
//! `15-virtio-blk-driver.md` in
//! `notes/rewrite/fork-syscall-rewrite/16-stage-drivers/`.
//!
//! Single-threaded event loop: one message at a time, no shared mutable
//! state across threads.

#![no_std]

extern crate alloc;

pub mod geometry;
pub mod request;

/// Service initialization entry (wires the drive; transport stays out).
pub fn init() {}
