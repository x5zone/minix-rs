//! USB host daemon: request protocol plus device enumeration.
//!
//! C correspondence: `minix3/minix/drivers/usb/usbd/` (18 files, 5661
//! lines): `base/usbd.c` (184 lines, startup through `usbd_start`),
//! `hcd/hcd.c` (1314 lines, generic enumeration and transfer state
//! machine), `hcd/hcd_common.c` (761 lines, device management),
//! `hcd/hcd_ddekit.c` (484 lines, device-kit bridge),
//! `hcd/hcd_schedule.c` (305 lines, at most 16 request blocks in
//! flight, `HCD_MAX_URBS`), and the hardware backend `hcd/musb/`
//! (BeagleBone mentor-graphics controller). The client library
//! `minix3/minix/lib/libusb/usb.c` (255 lines) sends the five
//! requests; the server dispatch (`usb_server.c:731-758`) answers
//! them. This crate owns the numbering and ordering halves; the
//! service binary owns packet traffic and controller registers. See
//! document `18-usb-framework.md` in
//! `notes/rewrite/fork-syscall-rewrite/16-stage-drivers/`.
//!
//! Single-threaded event loop: one message at a time, no shared mutable
//! state across threads.

#![no_std]

pub mod enumerate;
pub mod protocol;

/// Service initialization entry (wires the daemon table; packet traffic stays out).
pub fn init() {}
