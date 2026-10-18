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
//! `rewrite-notes/16-stage-drivers/`.
//!
//! Single-threaded event loop: one message at a time, no shared mutable
//! state across threads.

#![no_std]

extern crate alloc;

pub mod char_face;
pub mod display;
pub mod mode;
pub mod service;

use char_face::FbFace;
use service::{FbService, NullFramebuffer};

/// Build the driver's service for the binary.
///
/// No framebuffer is mapped yet (the mode and its memory arrive with the
/// board's video device); [`NullFramebuffer`] answers `EIO` honestly on
/// every access, and the device size is zero until a mode is chosen.
pub fn init() -> FbService<NullFramebuffer> {
    FbService::new(FbFace::new(0, 0), NullFramebuffer)
}
