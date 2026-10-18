//! VirtualBox guest driver: host-time synchronization and the HGCM
//! connection service.
//!
//! C correspondence: `minix3/minix/drivers/vmm_guest/vbox/` — `vbox.c`
//! (the backdoor device bring-up, the clock drift rule, the event
//! interrupt) and `hgcm.c` (the connection table, the four operations,
//! and the asynchronous request lifecycle), with `err.c`'s result-code
//! table in [`device`].
//!
//! Single-threaded event loop: one message at a time, no shared mutable
//! state across threads.
//!
//! Registered seams: the PCI probe (the port and interrupt line come from
//! the PCI server, whose attribute passthrough is that driver's own
//! registered seam and answers ENOSYS today — the probe fails honestly
//! rather than inventing a port), the interrupt re-enable (`sys_irqenable`
//! has no Rust verb yet; [`backdoor::KernelBackdoor`] reports ENOSYS),
//! and the `-args` environment plumbing (the documented defaults are
//! used).

#![no_std]

#[cfg(test)]
extern crate std;

pub mod backdoor;
pub mod device;
pub mod protocol;
pub mod service;
pub mod vmmdev;

use backdoor::Backdoor;
use service::VboxService;

/// Builds the service for the binary.
///
/// C's `vbox_init` parses `interval` and `drift` from its environment;
/// the environment plumbing is a registered seam, so the documented
/// defaults ([`device::DEFAULT_INTERVAL`], [`device::DEFAULT_DRIFT`])
/// are used.
pub fn init<B: Backdoor>(backdoor: B) -> VboxService<B> {
    VboxService::new(backdoor)
}
