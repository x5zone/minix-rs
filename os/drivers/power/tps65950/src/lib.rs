//! TPS65950 PMIC/RTC driver: register map, RTC sequences, chip check,
//! and the RTCDEV pump.
//!
//! C correspondence: `minix3/minix/drivers/power/tps65950/` —
//! `tps65950.c` (352 lines: the four sub-module addresses, the IDCODE
//! check, the RTC start, and the RTCDEV server loop) and `rtc.c` (189
//! lines: the BCD conversions and the read/set register sequences).
//!
//! Single-threaded event loop: one message at a time, no shared mutable
//! state across threads.

#![no_std]

#[cfg(test)]
extern crate std;

pub mod device;
pub mod protocol;
pub mod registers;
pub mod rtc;
pub mod service;

use service::{PmIdentity, Tps65950Service};

/// Build the driver's service for the binary.
///
/// Bus binding, the four reservations, the chip check, and the RTC start
/// happen in [`Tps65950Service::start`]; without a reachable i2c bus
/// driver or a recognized chip the start refuses, the same fate as C's
/// fresh-start init returning `EXIT_FAILURE`.
pub fn init() -> Tps65950Service<PmIdentity> {
    Tps65950Service::new(PmIdentity)
}
