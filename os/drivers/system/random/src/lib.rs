//! Random-number driver: entropy pools, keystream core, single device.
//!
//! C correspondence: `minix3/minix/drivers/system/random/` — `main.c`
//! (268 lines, device hooks plus harvest timer) and `random.c` (237
//! lines, pools plus keystream). The cipher and hash stay behind traits
//! (see [`pool::PoolHash`] and [`core::BlockCipher`]); production wires
//! the platform primitives, tests use the folding doubles. See document
//! `09-random-driver.md` in
//! `rewrite-notes/16-stage-drivers/`.
//!
//! Only one minor exists (`/dev/random`): there is no separate
//! unblocking device in this driver. Single-threaded event loop: one
//! message at a time, no shared mutable state across threads.

#![no_std]

extern crate alloc;

pub mod char_face;
pub mod core;
pub mod crypto;
pub mod device;
pub mod pool;
pub mod service;

use char_face::RandomFace;
use crypto::{Aes256, Sha256};
use pool::PoolSet;
use service::RandomService;

/// Build the driver's service for the binary.
///
/// Production wires the AES-256 keystream (G4) and the SHA-256 pool
/// hashes; the construction key is a zero seed because the generator
/// passes its current key per block (`crypto.rs`'s cached-schedule note,
/// `random.c:90,198`). The first trusted write trips the reseed threshold
/// (`MIN_SAMPLES`, `random.c:20-22`) and rekeys the generator.
pub fn init() -> RandomService<Aes256, Sha256> {
    RandomService::new(
        RandomFace::new(Aes256::new(&[0u8; crate::core::KEY_SIZE])),
        PoolSet::new(),
        ::core::array::from_fn(|_| Sha256::new()),
    )
}
