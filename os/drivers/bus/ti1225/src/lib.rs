//! TI1225 CardBus controller driver: register definitions, socket power
//! negotiation, and the init/socket-pass sequence.
//!
//! C correspondence: `minix3/minix/drivers/bus/ti1225/ti1225.c` (431
//! lines) with `ti1225.h` (PCI attribute offsets plus the socket
//! register block) and `i82365.h` (the ExCA register vocabulary the
//! legacy path uses). The driver has no client protocol: it probes the
//! controller, powers card sockets, and rescans the bridge's secondary
//! bus.
//!
//! Single-threaded event loop: one message at a time, no shared mutable
//! state across threads.

#![no_std]

#[cfg(test)]
extern crate std;

pub mod controller;
pub mod registers;
pub mod service;
pub mod socket;

use controller::NullController;
use service::Ti1225Service;

/// Build the driver's service for the binary.
///
/// The controller probe happens in [`Ti1225Service::start`]; a machine
/// without a TI1225 refuses to start (C returns `ENODEV` from its
/// fresh-start callback).
pub fn init() -> Ti1225Service<NullController> {
    Ti1225Service::new(NullController)
}
