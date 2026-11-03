//! Faulty block device driver: fault-injection proxy over a lower block
//! driver (rules, actions, forwarding face, and service assembly).
//!
//! C correspondence: `minix3/minix/drivers/storage/fbd/` — `fbd.c` (442
//! lines: the block callbacks, the two transfer paths, and the startup),
//! `rule.c` (184 lines: the rule table and the three hooks), and
//! `action.c` (302 lines: the per-action pre/io/post behavior). The pure
//! rule and action arithmetic lives in [`rules`], the wire format of the
//! rule structure in [`protocol`], the block face in [`face`], and the
//! forwarded opens/controls plus the shared pump in [`service`].
//!
//! Single-threaded event loop: one message at a time, no shared mutable
//! state across threads.
//!
//! Registered seams: the `label`/`minor` environment parse (the binary
//! uses the documented defaults), the zero-copy direct path (C forwards
//! the caller's grant through an indirect grant; the rewrite always copies
//! through the staging buffer), the forwarding of foreign ioctls (same
//! indirect-grant reason); the vectored transfer pair stays refused per
//! the 2.11 evidence (16 篇 §8) even though the pump now decodes it.

#![no_std]

#[cfg(test)]
extern crate std;

extern crate alloc;

pub mod face;
pub mod protocol;
pub mod rules;
pub mod service;

use face::FbdFace;
use service::FbdService;

/// The lower driver's minor the binary forwards to (C's `minor` from
/// `-args`).
pub const DRIVER_MINOR: u32 = 0;

/// The data-store label of the lower driver the binary forwards to (C's
/// `label` from `-args`; the default matches the ramdisk FS server).
pub const DRIVER_LABEL: &str = "fs_imgrd";

/// Builds the service for the binary (the lower driver's endpoint is
/// resolved by [`service::FbdService::bind`] once the label is known).
pub fn init(face: FbdFace) -> FbdService {
    FbdService::new(face)
}
