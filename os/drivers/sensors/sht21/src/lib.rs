//! SHT21 humidity/temperature sensor: transactions, CRC, and the read
//! pump.
//!
//! C correspondence: `minix3/minix/drivers/sensors/sht21/sht21.c` (486
//! lines) — the command set, the soft-reset startup checks, the
//! hold-mode measurement read with its CRC-8, the linear conversions,
//! and the one-second sampling cache that limits self-heating.
//!
//! Single-threaded event loop: one message at a time, no shared mutable
//! state across threads.

#![no_std]

#[cfg(test)]
extern crate std;

extern crate alloc;

pub mod driver;
pub mod service;

use service::Sht21Service;

/// Build the driver's service for the binary.
///
/// Bus binding and the reset/register checks happen in
/// [`Sht21Service::start`]; without a reachable i2c bus driver the start
/// refuses, the same fate as C's fresh-start init returning
/// `EXIT_FAILURE`.
pub fn init() -> Sht21Service {
    Sht21Service::new()
}
