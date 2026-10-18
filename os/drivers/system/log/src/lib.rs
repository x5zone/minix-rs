//! System log driver: the kernel-message blackboard reader.
//!
//! C correspondence: `minix3/minix/drivers/system/log/` — `log.c` (360
//! lines, ring plus hooks), `diag.c` (54 lines, kernel-message capture),
//! `liveupdate.c` (99 lines, live-update hooks). This crate owns the
//! numbers and the policy (ring arithmetic, read/select/cancel rules,
//! message delta); the service binary owns the transport (diagnostics
//! registration, signal handling, grant copies, live-update registration).
//! See document `08-log-driver.md` in
//! `rewrite-notes/16-stage-drivers/`.
//!
//! Single-threaded event loop: one message at a time, no shared mutable
//! state across threads.

#![no_std]

extern crate alloc;

pub mod char_face;
pub mod device;
pub mod diag;
pub mod ring;
pub mod service;

use service::LogService;

/// Build the driver's service for the binary.
///
/// The face is built here; the transport (announce, birth handshake,
/// grant copies) stays in the binary via [`minix_driver_rt`].
pub fn init() -> LogService {
    LogService::new()
}
