//! ACPI service: the bridge tree, the routing table, and the request loop.
//!
//! C correspondence: `minix3/minix/drivers/power/acpi/` — the driver's own
//! 1,548 lines (`acpi.c` 167, `osminixxf.c` 1,049, `pci.c` 325) over the
//! vendored ACPICA interpreter (83,279 lines across 212 files). This
//! crate models the driver's own logic — the gates, the bridge tree, the
//! interrupt routing table, and the two request handlers — and puts the
//! interpreter behind [`interpreter::AcpiInterpreter`] (registered
//! `E-ACPICA`; the porting options are adjudicated in document
//! `24-misc-drivers.md`). See also `rewrite-notes/16-stage-drivers/`.
//!
//! Single-threaded event loop: one message at a time, no shared mutable
//! state across threads.

#![no_std]

#[cfg(test)]
extern crate std;

extern crate alloc;

pub mod bridge;
pub mod interpreter;
pub mod protocol;
pub mod service;

use interpreter::UnboundAcpi;
use service::AcpiService;

/// Builds the service for the binary: the driver's logic over the
/// unbound interpreter (registered `E-ACPICA`), which reports "ACPI
/// failed" rather than pretending to have read tables.
pub fn init() -> AcpiService<UnboundAcpi> {
    AcpiService::new(UnboundAcpi)
}
