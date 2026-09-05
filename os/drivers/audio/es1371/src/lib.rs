//! Ensoniq 1371 sound card: sample-rate policy.
//!
//! C correspondence: `minix3/minix/drivers/audio/es1371/es1371.c`
//! (656 lines), `SRC.c` (196 lines), `codec.c` (264 lines),
//! `AC97.c` (496 lines), `sample_rate_converter.c` (240 lines).
//! This crate owns the rate policy half (bounds, channel routing);
//! the service binary owns codec writes and converter traffic. See
//! document `21-audio-drivers.md` in
//! `notes/rewrite/fork-syscall-rewrite/16-stage-drivers/`.
//!
//! Single-threaded event loop: one message at a time, no shared mutable
//! state across threads.

#![no_std]

pub mod rate;

/// Service initialization entry (wires the card table; codec traffic stays out).
pub fn init() {}
