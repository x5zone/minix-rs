//! Real-time-clock driver: three operations on the hardware clock.
//!
//! C correspondence: `minix3/minix/drivers/clock/readclock/` —
//! `readclock.c` (191 lines, protocol loop plus decimal helpers),
//! `forward.c` (120 lines, forwarding to a chip driver), and the
//! architecture clocks behind `struct rtc`. This crate owns the numbers
//! and the policy (protocol decoding, conversions, permission gates);
//! the service binary owns the transport (copies, grants, chip access).
//! See document `10-readclock-driver.md` in
//! `notes/rewrite/fork-syscall-rewrite/16-stage-drivers/`.
//!
//! Single-threaded event loop: one message at a time, no shared mutable
//! state across threads.

#![no_std]

extern crate alloc;

pub mod clock;
pub mod device;
pub mod protocol;

/// Service initialization entry (wires the clock; transport stays out).
pub fn init() {}
