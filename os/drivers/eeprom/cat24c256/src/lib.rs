//! EEPROM driver: the block face (i2c read/write legs), the page-slicing
//! policy, and the service assembly.
//!
//! C correspondence: `minix3/minix/drivers/eeprom/cat24c256/` —
//! `cat24c256.c` (505 lines: the block callbacks, the bus legs, the
//! startup sequence, and the environment parse). The page slicing lives in
//! [`pages`], the block face and its bus legs in [`face`], and the startup
//! plus the notification path in [`service`].
//!
//! Single-threaded event loop: one message at a time, no shared mutable
//! state across threads.
//!
//! Registered seams: the `-args` environment parse (C's
//! `i2cdriver_env_parse`; the binary uses the documented defaults
//! `BUS_NUMBER`/`ADDRESS`), the data-store subscription for bus updates
//! (the callback is modeled — [`service::Cat24c256Service::dispatch`]
//! refreshes the binding when a notification arrives — but the subscribe
//! call itself has no Rust verb yet), and the vectored transfer pair (the
//! device leg serves the contiguous forms only; the pump decodes the
//! vector since `E-BLK-VEC` closed).

#![no_std]

#[cfg(test)]
extern crate std;

extern crate alloc;

pub mod face;
pub mod pages;
pub mod service;

use face::Cat24c256Face;
use service::Cat24c256Service;

/// The bus number the binary binds to (C's `bus` from `-args`).
pub const BUS_NUMBER: u32 = 1;
/// The slave address the binary binds to (C's `address` from `-args`).
pub const ADDRESS: u16 = 0x50;

/// Builds the service for the binary (the face is bound by
/// [`service::start`] once the bus driver is found).
pub fn init(face: Cat24c256Face) -> Cat24c256Service {
    Cat24c256Service::new(face)
}
