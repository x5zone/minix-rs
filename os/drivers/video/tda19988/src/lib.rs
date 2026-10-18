//! TDA19988 HDMI transmitter driver: register map, CEC/HDMI access, and
//! the EDID block device.
//!
//! C correspondence: `minix3/minix/drivers/video/tda19988/tda19988.c`
//! (1010 lines) — the two i2c interfaces (CEC for display detection,
//! HDMI for everything else), the page-based register access, the
//! bring-up sequence, and the EDID block served over the block-driver
//! interface.
//!
//! Single-threaded event loop: one message at a time, no shared mutable
//! state across threads.

#![no_std]

#[cfg(test)]
extern crate std;

pub mod hdmi;
pub mod registers;
pub mod service;

use service::Tda19988Service;

/// Build the driver's service for the binary.
///
/// Bus binding and both reservations happen in
/// [`Tda19988Service::start`]; without a reachable i2c bus driver the
/// start refuses, the same fate as C's fresh-start init returning
/// `EXIT_FAILURE`.
pub fn init() -> Tda19988Service {
    Tda19988Service::new()
}
