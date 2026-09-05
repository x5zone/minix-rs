//! SoundBlaster 16 sound card: processor command policy.
//!
//! C correspondence: `minix3/minix/drivers/audio/sb16/sb16.c`
//! (449 lines) with `sb16.h` (command bytes, ports, limits) and
//! `mixer.c` (254 lines). This crate owns the command half (which
//! bytes mean what, how a rate splits); the service binary owns port
//! traffic. See document `21-audio-drivers.md` in
//! `notes/rewrite/fork-syscall-rewrite/16-stage-drivers/`.
//!
//! Single-threaded event loop: one message at a time, no shared mutable
//! state across threads.

#![no_std]

pub mod dsp;

/// Service initialization entry (wires the card table; port traffic stays out).
pub fn init() {}
