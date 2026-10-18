//! Pressure sensor driver: calibration math, sensor transactions, and the
//! read pump.
//!
//! C correspondence: `minix3/minix/drivers/sensors/bmp085/bmp085.c`
//! (583 lines) — the constants and formulas, the i2c transactions, and
//! the character-driver read path. This crate owns the math and the
//! request pump; the i2c bus traffic goes through `minix-i2cdriver`'s
//! client half. See document `24-misc-drivers.md` in
//! `rewrite-notes/16-stage-drivers/`.
//!
//! Single-threaded event loop: one message at a time, no shared mutable
//! state across threads.

#![no_std]

#[cfg(test)]
extern crate std;

extern crate alloc;

pub mod convert;
pub mod measure;
pub mod service;

use service::Bmp085Service;

/// Build the driver's service for the binary.
///
/// The bus binding, chip check, and calibration read happen in
/// [`Bmp085Service::start`] once the binary has its transport; without a
/// reachable i2c bus driver the start refuses, the same fate as C's
/// fresh-start init returning `EXIT_FAILURE`.
pub fn init() -> Bmp085Service {
    Bmp085Service::new()
}
