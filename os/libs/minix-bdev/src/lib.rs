//! Block-device client library: synchronous and asynchronous callers.
//!
//! C correspondence: `minix3/minix/lib/libbdev/` — `bdev.c` (642 lines,
//! synchronous and asynchronous transfer entry points), `call.c` (118
//! lines, call-slot management), `driver.c` (122 lines, driver endpoint
//! table), `ipc.c` (346 lines, send paths plus asynchronous reply
//! demultiplexing), `minor.c` (136 lines, open reference counts) — plus
//! `minix3/minix/include/minix/bdev.h` (public API) and the sizing
//! constants in `minix3/minix/lib/libbdev/const.h`.
//!
//! The library is the mirror of the block framework: where the framework
//! serves `BDEV_*` requests, this library sends them — from the virtual
//! file system service and file servers toward block drivers. It owns the
//! driver endpoint table, the per-minor open counts, and the asynchronous
//! call slots with their retry budgets. This crate renders that ownership
//! in Rust, organized to match document `04-bdev-client.md`:
//!
//! - [`transport`] — the message-transport abstraction with test doubles
//!   (document section 3).
//! - [`client`] — driver table, open counts, synchronous requests, and the
//!   asynchronous call machine (document sections 2 and 4).
//!
//! Callers are single-threaded event loops: one reply is demultiplexed at
//! a time, so the call table needs no locking.

#![no_std]

extern crate alloc;

pub mod client;
pub mod transport;
