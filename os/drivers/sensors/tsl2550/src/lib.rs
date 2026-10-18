//! TSL2550 ambient-light sensor: ADC polling, the lux calculation, and
//! the read pump.
//!
//! C correspondence: `minix3/minix/drivers/sensors/tsl2550/tsl2550.c`
//! (435 lines) — the command set and ADC masks, the two lookup tables
//! from the TAOS designer's note, the power-up and range checks, the
//! 400-millisecond valid-bit poll, and the character-driver read path.
//!
//! Single-threaded event loop: one message at a time, no shared mutable
//! state across threads.

#![no_std]

#[cfg(test)]
extern crate std;

extern crate alloc;

pub mod driver;
pub mod service;

use service::Tsl2550Service;

/// Build the driver's service for the binary.
///
/// Bus binding and the power-up checks happen in
/// [`Tsl2550Service::start`]; without a reachable i2c bus driver the
/// start refuses, the same fate as C's fresh-start init returning
/// `EXIT_FAILURE`.
pub fn init() -> Tsl2550Service {
    Tsl2550Service::new()
}
