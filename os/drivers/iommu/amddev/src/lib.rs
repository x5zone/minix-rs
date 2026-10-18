//! AMD DEV (IOMMU) driver: register map, decode arithmetic, the
//! controller seam, and the map-request pump.
//!
//! C correspondence: `minix3/minix/drivers/iommu/amddev/amddev.c` (477
//! lines) — the capability-list search for the AMD secure device, the
//! DEV function registers, the exclusion table, and the `IOMMU_MAP`
//! request path.
//!
//! Single-threaded event loop: one message at a time, no shared mutable
//! state across threads.

#![no_std]

#[cfg(test)]
extern crate std;

pub mod controller;
pub mod device;
pub mod registers;
pub mod service;

use controller::NullController;
use service::AmddevService;

/// Build the driver's service for the binary.
///
/// The device probe happens in [`AmddevService::start`]; a machine
/// without the AMD DEV refuses to start (C's fresh init returns zero).
pub fn init() -> AmddevService<NullController> {
    AmddevService::new(NullController)
}
