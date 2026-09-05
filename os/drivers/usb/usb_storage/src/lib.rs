//! Mass storage driver: bulk-only transport plus SCSI framing.
//!
//! C correspondence: `minix3/minix/drivers/usb/usb_storage/usb_storage.c`
//! (1806 lines, `mass_storage` block table of type `DISK` with open,
//! close, transfer, ioctl, cleanup, part, and geometry callbacks,
//! `usb_storage.c:105-118`, served through `blockdriver_process`,
//! `usb_storage.c:589`), `scsi.c` (288 lines, command building),
//! `bulk.c` (39 lines, wrapper builders), and `urb_helper.c`
//! (111 lines, endpoint setup plus blocking submit). This crate owns
//! the framing half (signatures, tag pairing, transfer guard); the
//! service binary owns endpoint traffic. See document
//! `19-usb-storage-hub.md` in
//! `notes/rewrite/fork-syscall-rewrite/16-stage-drivers/`.
//!
//! Single-threaded event loop: one message at a time, no shared mutable
//! state across threads.

#![no_std]

pub mod cbw;

/// Service initialization entry (wires the storage table; endpoint traffic stays out).
pub fn init() {}
