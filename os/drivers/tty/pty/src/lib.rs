//! Pseudo-terminal driver: master/slave pairs without hardware.
//!
//! C correspondence: `minix3/minix/drivers/tty/pty/pty.c` (860 lines,
//! master side plus pair management) with `tty.c` (1320 lines, slave side
//! line discipline — shared with the terminal driver, see the terminal
//! crate) and `ptyfs.c` (112 lines, filesystem sidecar). The slave side
//! reuses the terminal line policy; this crate owns the pair layer. See
//! document `07-pty-driver.md` in
//! `notes/rewrite/fork-syscall-rewrite/16-stage-drivers/`.
//!
//! Single-threaded event loop: one message at a time, no shared mutable
//! state across threads.

#![no_std]

extern crate alloc;

pub mod buffer;
pub mod pair;
pub mod ptyfs;
pub mod select;

/// Service initialization entry (wires the pair table; transport stays out).
pub fn init() {}
