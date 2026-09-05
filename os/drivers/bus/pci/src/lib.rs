//! PCI bus driver: enumeration database plus configuration access.
//!
//! C correspondence: `minix3/minix/drivers/bus/pci/` — `main.c` (740
//! lines, query protocol plus control codes), `pci.c` (2559 lines,
//! enumeration, bridges, interrupt routing), `pci_table.c` (37 lines,
//! bridge identifier table). Enumeration mechanics (bridge windows,
//! register-level probing) stay in the service crate behind
//! [`config::ConfigSpace`]; this crate owns the numbers and the policy
//! (protocol decoding, visibility, duplicate refusal). See document
//! `11-pci-driver.md` in
//! `notes/rewrite/fork-syscall-rewrite/16-stage-drivers/`.
//!
//! Single-threaded event loop: one message at a time, no shared mutable
//! state across threads.

#![no_std]

extern crate alloc;

pub mod config;
pub mod database;
pub mod protocol;

/// Service initialization entry (wires the database; transport stays out).
pub fn init() {}
