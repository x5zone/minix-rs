//! Device registry: driver-side add, delete, and USB bind bookkeeping.
//!
//! C correspondence: `minix3/minix/lib/libdevman/generic.c` (275 lines,
//! generic add/delete plus serialization) and `usb.c` (301 lines, USB
//! device and interface tracking). The device manager service owns the
//! tree; this library is the driver-side half: build records, track USB
//! attachments, serialize for the wire. See document `12-gpio-devman.md`
//! in `notes/rewrite/fork-syscall-rewrite/16-stage-drivers/`.
//!
//! Single-threaded event loops use this library: one registry call at a
//! time, no shared mutable state across threads.

#![no_std]

extern crate alloc;

pub mod device;
pub mod usb;
