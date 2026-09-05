//! Faulty block device: fault-injection proxy rules.
//!
//! C correspondence: `minix3/minix/drivers/storage/fbd/fbd.c`
//! (442 lines), `action.c` (302 lines), `rule.c` (184 lines). The
//! driver registers `fbd_dtab` (`fbd.c:34-38`, type `OTHER` with
//! open, close, transfer, and ioctl callbacks) and forwards every
//! request to a lower driver through `ipc_sendrec` (e.g.
//! `fbd.c:151`). This crate owns the rule-matching half; the service
//! binary owns forwarding and message traffic. See document
//! `17-storage-misc-driver.md` in
//! `notes/rewrite/fork-syscall-rewrite/16-stage-drivers/`.
//!
//! Single-threaded event loop: one message at a time, no shared mutable
//! state across threads.

#![no_std]

pub mod rules;

/// Service initialization entry (wires the proxy table; forwarding stays out).
pub fn init() {}
