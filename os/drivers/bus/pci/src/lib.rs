//! PCI bus driver: enumeration database, configuration access, and the
//! query pump.
//!
//! C correspondence: `minix3/minix/drivers/bus/pci/` — `main.c` (740
//! lines, query protocol plus control codes), `pci.c` (2559 lines,
//! enumeration, bridges, interrupt routing), `pci_table.c` (37 lines,
//! bridge identifier table). Enumeration mechanics (bridge windows,
//! register-level probing) stay in the service crate behind
//! [`config::ConfigSpace`]; this crate owns the numbers and the policy
//! (protocol decoding, visibility, duplicate refusal). See document
//! `11-pci-driver.md` in
//! `rewrite-notes/16-stage-drivers/`.
//!
//! Single-threaded event loop: one message at a time, no shared mutable
//! state across threads.

#![no_std]

#[cfg(test)]
extern crate std;

extern crate alloc;

pub mod char_face;
pub mod config;
pub mod database;
pub mod protocol;
pub mod scan;
pub mod service;

use config::NullConfigSpace;
use service::PciService;

/// Build the driver's service for the binary.
///
/// The configuration-space backend rides the port-IO/chipset seam; until
/// it lands, [`NullConfigSpace`] makes the fresh-start probe refuse to
/// start the driver instead of claiming an empty bus.
pub fn init() -> PciService<NullConfigSpace> {
    PciService::new(NullConfigSpace)
}
