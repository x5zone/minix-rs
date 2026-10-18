//! Virtual node disk driver: file-backed block device (face, layout,
//! control protocol, and service assembly).
//!
//! C correspondence: `minix3/minix/drivers/storage/vnd/vnd.c`
//! (603 lines, single file). The driver registers `vnd_dtab`
//! (`vnd.c:39-45`, type `DISK` with open, close, transfer, ioctl, part,
//! and geometry callbacks) and serves it through `blockdriver_task`
//! (`vnd.c:592-600`). The layout half (chunk split, geometry derivation)
//! lives in [`layout`], the block face and its control bodies in
//! [`face`], the ioctl numbers and structures in [`protocol`], the
//! backing-file seam in [`backend`], and the control-request decode plus
//! the shared pump in [`service`].
//!
//! Single-threaded event loop: one message at a time, no shared mutable
//! state across threads.
//!
//! Registered seams: the `instance` environment parse (the binary uses the
//! documented default) and the termination-signal handler (C's `vnd_signal`
//! parks until the last close; the rewrite records the intent and the
//! loop's owner decides). The backing-file legs are all real: `copyfd` and
//! `fsync` reach VFS through the client wrappers that landed with this
//! slice (`minix-sys::vfs::copyfd_via`/`fsync_via`).

#![no_std]

#[cfg(test)]
extern crate std;

extern crate alloc;

pub mod backend;
pub mod face;
pub mod layout;
pub mod protocol;
pub mod service;

use backend::FileBackend;
use face::VndFace;
use service::VndService;

/// The unit number the binary serves (C's `instance` from `-args`).
pub const INSTANCE: u32 = 0;

/// Builds the service for the binary.
pub fn init<B: FileBackend>(backend: B) -> VndService<B> {
    VndService::new(VndFace::new(INSTANCE, backend))
}
