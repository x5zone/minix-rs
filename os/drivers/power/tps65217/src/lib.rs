//! TPS65217 PMIC driver: register map, chip/interrupt decisions, and the
//! notification pump.
//!
//! C correspondence: `minix3/minix/drivers/power/tps65217/tps65217.c`
//! (402 lines) — the register map and bit masks, the chip check and
//! interrupt setup at startup, the power-off enable, and the
//! notification-only main loop whose power-button path tells init to
//! shut down.
//!
//! Single-threaded event loop: one message at a time, no shared mutable
//! state across threads.

#![no_std]

#[cfg(test)]
extern crate std;

pub mod device;
pub mod registers;
pub mod service;

use service::Tps65217Service;

/// Build the driver's service for the binary.
///
/// Bus binding and the chip check happen in
/// [`Tps65217Service::start`]; without a reachable i2c bus driver or a
/// recognized chip the start refuses, the same fate as C's fresh-start
/// init returning `EXIT_FAILURE`.
pub fn init() -> Tps65217Service {
    Tps65217Service::new()
}
