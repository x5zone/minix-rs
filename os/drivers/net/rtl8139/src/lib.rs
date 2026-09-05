//! Realtek 8139 network card: slot picker plus ring wrap.
//!
//! C correspondence: `minix3/minix/drivers/net/rtl8139/rtl8139.c`
//! (1530 lines) with `rtl8139.h` (slot count, ring size, cursors).
//! The driver registers `rl_table` (`rtl8139.c:101-113`, eleven
//! callbacks) and serves it through `netdriver_task`
//! (`rtl8139.c:118-123`). This crate owns the cursor half (slot
//! rotation, ring wrap); the service binary owns register traffic.
//! See document `23-net-driver-variants.md` in
//! `notes/rewrite/fork-syscall-rewrite/16-stage-drivers/`.
//!
//! Single-threaded event loop: one message at a time, no shared mutable
//! state across threads.

#![no_std]

pub mod txrx;

/// Service initialization entry (wires the card table; register traffic stays out).
pub fn init() {}
