//! Minix-RS net server (uds).
//!
//! C correspondence: `minix3/minix/net/uds/` (uds.c 1417 lines, io.c 1803
//! lines, stat.c 186 lines). This crate owns the pure policy half (object
//! counts, connection states, ring arithmetic); the service binary owns
//! message traffic and socket storage. See documents `21-uds-core.md` and
//! `22-uds-io.md` in `notes/rewrite/fork-syscall-rewrite/17-stage-net/`.
//!
//! Single-threaded event loop: one message at a time, no shared mutable
//! state across threads.

#![no_std]

/// Service core policy: object count, states, hash, dispatch.
pub mod core;
/// Data plane policy: ring arithmetic, segment kinds, caps.
pub mod io;

/// Service initialization entry (wires the tables; traffic and storage stay out).
pub fn init() {}
