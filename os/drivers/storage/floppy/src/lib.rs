//! Floppy disk driver: geometry plus retry policy.
//!
//! C correspondence: `minix3/minix/drivers/storage/floppy/floppy.c`
//! (1355 lines) with `floppy.h` and `liveupdate.c` (78 lines). The
//! driver registers `f_dtab` (`floppy.c:268-274`, type `DISK` with
//! open, close, transfer, cleanup, part, and geometry callbacks) and
//! serves it through `blockdriver_task` (`floppy.c:293-299`). This
//! crate owns the pure policy half (density table, error budget);
//! the service binary owns FDC port traffic, seeks, and interrupts.
//! See document `17-storage-misc-driver.md` in
//! `notes/rewrite/fork-syscall-rewrite/16-stage-drivers/`.
//!
//! Single-threaded event loop: one message at a time, no shared mutable
//! state across threads.

#![no_std]

pub mod geometry;

/// Service initialization entry (wires the drive table; FDC traffic stays out).
pub fn init() {}
