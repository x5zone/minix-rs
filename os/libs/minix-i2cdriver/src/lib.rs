//! I2C device-driver helpers: announce keys, addressing, register
//! operation sequences.
//!
//! C correspondence: `minix3/minix/lib/libi2cdriver/i2cdriver.c` (366
//! lines) — the announce key composition (`i2cdriver_announce`,
//! `i2cdriver.c:12-40`), the bus label and subscription strings
//! (`i2cdriver_bus_endpoint`, `i2cdriver_subscribe_bus_updates`,
//! `i2cdriver.c:81-116`), and the register access helpers built on the
//! exec request (`i2creg_read8` through `i2creg_clear_bits8`,
//! `i2cdriver.h:20-28`).
//!
//! Data-store publication, subscription, and the exec request are
//! transport and stay with the service binary; this module owns the
//! string, address, and operation-sequence policies those services
//! follow.
//!
//! Single-threaded event loop: one message at a time.

#![no_std]

extern crate alloc;

pub mod keys;
pub mod ops;
