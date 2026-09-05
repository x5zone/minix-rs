//! System log driver: the kernel-message blackboard reader.
//!
//! C correspondence: `minix3/minix/drivers/system/log/` — `log.c` (360
//! lines, ring plus hooks), `diag.c` (54 lines, kernel-message capture),
//! `liveupdate.c` (99 lines, live-update hooks). This crate owns the
//! numbers and the policy (ring arithmetic, read/select/cancel rules,
//! message delta); the service binary owns the transport (diagnostics
//! registration, signal handling, grant copies, live-update registration).
//! See document `08-log-driver.md` in
//! `notes/rewrite/fork-syscall-rewrite/16-stage-drivers/`.
//!
//! Single-threaded event loop: one message at a time, no shared mutable
//! state across threads.

#![no_std]

extern crate alloc;

pub mod device;
pub mod diag;
pub mod ring;

/// Service initialization entry (wires the device; transport stays out).
pub fn init() {}
