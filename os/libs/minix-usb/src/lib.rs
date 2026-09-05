//! Minix-RS USB client library (libusb correspondence).
//!
//! C correspondence: `minix3/minix/lib/libusb/usb.c` (255 lines):
//! `usb_send_urb` (`usb.c:20-75`), `usb_cancle_urb` (`usb.c:80-116`),
//! `usb_init` (`usb.c:121-152`), `usb_send_info` (`usb.c:231-255`),
//! and the completion path (`usb.c:157-225`). This crate owns the
//! bookkeeping half (which request block is in flight); endpoint
//! traffic stays in the service binary. See document
//! `18-usb-framework.md` in
//! `notes/rewrite/fork-syscall-rewrite/16-stage-drivers/`.
#![no_std]

pub mod urb;
