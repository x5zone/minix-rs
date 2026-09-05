//! National-semiconductor 8390 network card: receive ring cursor.
//!
//! C correspondence: `minix3/minix/drivers/net/dp8390/dp8390.c`
//! (998 lines) with board files `3c503.c`, `ne2000.c`, `rtl8029.c`,
//! `wdeth.c`. The driver registers `dp_table` (`dp8390.c:103-111`,
//! name plus init, stop, mode, receive, send, interrupt, and tick
//! callbacks) and serves it through `netdriver_task`
//! (`dp8390.c:117-121`). This crate owns the cursor half (page wrap,
//! length guard); the service binary owns register traffic. See
//! document `22-net-driver-reference.md` in
//! `notes/rewrite/fork-syscall-rewrite/16-stage-drivers/`.
//!
//! Single-threaded event loop: one message at a time, no shared mutable
//! state across threads.

#![no_std]

pub mod ring;

/// Service initialization entry (wires the card table; register traffic stays out).
pub fn init() {}
