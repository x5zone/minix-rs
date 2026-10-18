//! I2C bus driver: reservations, the exec protocol, and the controller
//! seam.
//!
//! C correspondence: `minix3/minix/drivers/bus/i2c/i2c.c` (521 lines) —
//! the per-device reservation table keyed by the owner's data-store
//! label, the 280-byte exec block with its validation, and the SoC
//! `process` seam (`i2c.c:56-59`). See document
//! `24-misc-drivers.md` in `rewrite-notes/16-stage-drivers/`.
//!
//! Single-threaded event loop: one message at a time, no shared mutable
//! state across threads.

#![no_std]

#[cfg(test)]
extern crate std;

extern crate alloc;

pub mod controller;
pub mod service;

use controller::NullController;
use service::I2cBusService;

/// Build the driver's service for the binary.
///
/// The controller bring-up happens in [`I2cBusService::start`]; without a
/// wired controller the probe refuses, the same fate as C's fresh-start
/// init failing on an unknown board.
pub fn init() -> I2cBusService<NullController> {
    I2cBusService::new(NullController)
}
