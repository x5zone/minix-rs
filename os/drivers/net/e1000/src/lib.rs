//! Intel Pro/1000 network card: descriptor ring policy.
//!
//! C correspondence: `minix3/minix/drivers/net/e1000/e1000.c`
//! (918 lines) with `e1000.h` (ring counts, buffer size) and
//! `e1000_hw.h` (descriptor layouts). The driver registers
//! `e1000_table` (`e1000.c:38-49`, ten callbacks) and serves it
//! through `netdriver_task` (`e1000.c:55-61`). This crate owns the
//! ring half (counts, wrap); the service binary owns register
//! traffic. See document `23-net-driver-variants.md` in
//! `notes/rewrite/fork-syscall-rewrite/16-stage-drivers/`.
//!
//! Single-threaded event loop: one message at a time, no shared mutable
//! state across threads.

#![no_std]

pub mod desc;

/// Service initialization entry (wires the card table; register traffic stays out).
pub fn init() {}
