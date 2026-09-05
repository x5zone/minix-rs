//! AMD LANCE network card: small rings plus chip identity.
//!
//! C correspondence: `minix3/minix/drivers/net/lance/lance.c`
//! (895 lines). The driver registers `lance_table`
//! (`lance.c:158-167`, eight callbacks) and serves it through
//! `netdriver_task` (`lance.c:172-177`). This crate owns the table
//! half (ring sizes, chip matching); the service binary owns
//! register traffic. See document `23-net-driver-variants.md` in
//! `notes/rewrite/fork-syscall-rewrite/16-stage-drivers/`.
//!
//! Single-threaded event loop: one message at a time, no shared mutable
//! state across threads.

#![no_std]

pub mod ring;

/// Service initialization entry (wires the card table; register traffic stays out).
pub fn init() {}
