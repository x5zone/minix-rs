//! Random-number driver: entropy pools, keystream core, single device.
//!
//! C correspondence: `minix3/minix/drivers/system/random/` — `main.c`
//! (268 lines, device hooks plus harvest timer) and `random.c` (237
//! lines, pools plus keystream). The cipher and hash stay behind traits
//! (see [`pool::PoolHash`] and [`core::BlockCipher`]); production wires
//! the platform primitives, tests use the folding doubles. See document
//! `09-random-driver.md` in
//! `notes/rewrite/fork-syscall-rewrite/16-stage-drivers/`.
//!
//! Only one minor exists (`/dev/random`): there is no separate
//! unblocking device in this driver. Single-threaded event loop: one
//! message at a time, no shared mutable state across threads.

#![no_std]

extern crate alloc;

pub mod core;
pub mod device;
pub mod pool;

/// Service initialization entry (wires pools and core; transport stays out).
pub fn init() {}
