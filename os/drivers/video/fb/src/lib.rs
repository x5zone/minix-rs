//! Frame buffer driver: mode choice plus open counting.
//!
//! C correspondence: `minix3/minix/drivers/video/fb/fb.c` (404 lines,
//! `fb_tab` with open, close, read, write, and ioctl callbacks served
//! through `chardriver_task`, `fb.c:44-51,307-316`), `fb_edid.c`
//! (187 lines, monitor report through the block protocol,
//! `fb_edid.c:85-186`), and `arch/earm/fb_arch.c` (408 lines, the
//! single OMAP3 hardware backend). The driver has no memory-map
//! callback: user processes reach pixels through read and write plus
//! the four ioctl requests. This crate owns the choice and counting
//! halves; the service binary owns register writes and copy traffic.
//! See document `20-fb-driver.md` in
//! `notes/rewrite/fork-syscall-rewrite/16-stage-drivers/`.
//!
//! Single-threaded event loop: one message at a time, no shared mutable
//! state across threads.

#![no_std]

pub mod display;
pub mod mode;

/// Service initialization entry (wires the display table; register traffic stays out).
pub fn init() {}
