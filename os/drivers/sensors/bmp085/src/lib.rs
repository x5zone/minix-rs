//! Pressure sensor driver: calibration math.
//!
//! C correspondence: `minix3/minix/drivers/sensors/bmp085/bmp085.c`
//! (583 lines). This crate owns the math half (temperature from
//! calibration plus raw reading); the service binary owns bus
//! traffic. See document `24-misc-drivers.md` in
//! `notes/rewrite/fork-syscall-rewrite/16-stage-drivers/`.
//!
//! Single-threaded event loop: one message at a time, no shared mutable
//! state across threads.

#![no_std]

pub mod convert;

/// Service initialization entry (wires the sensor table; bus traffic stays out).
pub fn init() {}
