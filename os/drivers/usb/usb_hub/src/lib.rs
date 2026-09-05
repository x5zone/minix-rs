//! USB hub driver: port bookkeeping plus reset discipline.
//!
//! C correspondence: `minix3/minix/drivers/usb/usb_hub/usb_hub.c`
//! (937 lines) with `urb_helper.c` (111 lines). Unlike the storage
//! driver, the hub registers no block or character table (no
//! `blockdriver`/`chardriver` reference in `usb_hub.c`): it only
//! watches ports and reports arrivals and departures upward through
//! `ddekit_usb_info`. This crate owns the bookkeeping half; the
//! service binary owns control traffic. See document
//! `19-usb-storage-hub.md` in
//! `notes/rewrite/fork-syscall-rewrite/16-stage-drivers/`.
//!
//! Single-threaded event loop: one message at a time, no shared mutable
//! state across threads.

#![no_std]

pub mod ports;

/// Service initialization entry (wires the hub table; control traffic stays out).
pub fn init() {}
