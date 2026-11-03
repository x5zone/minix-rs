//! Block driver framework: request protocol, dispatch, and server skeleton.
//!
//! C correspondence: `minix3/minix/lib/libblockdriver/` — `driver.c`
//! (462 lines, single-threaded loop), `driver_mt.c` (581 lines,
//! multi-threaded loop), `driver_st.c` (94 lines, queue-based loop),
//! `drvlib.c` (234 lines, partition parsing), `mq.c` (108 lines, per-device
//! message queues), `trace.c` (284 lines, block tracing), `liveupdate.c`
//! (94 lines, live-update hooks) — plus `minix3/minix/include/minix/
//! blockdriver.h` (callback table) and the block request constants in
//! `minix3/minix/include/minix/com.h:963-987`.
//!
//! The single-threaded and multi-threaded loops share one request family
//! and one callback table; they differ only in how messages wait. This
//! crate renders the shared half in Rust, organized to match document
//! `02-blockdriver-framework.md`:
//!
//! - [`protocol`] — request numbers, sector constants, device extents,
//!   partition styles, queue bounds (document sections 1 and 2).
//! - [`driver`] — the driver trait, routing rules, and the server state
//!   machine (document sections 3 and 4).
//! - [`pump`] — the shared receive-classify-dispatch pump every block
//!   binary runs (C `blockdriver_task`/`blockdriver_process`), including
//!   the restart gate and the character-open refusal.
//! - [`wire`] — the block message layouts: request and reply field
//!   offsets, pinned by C-layout mirrors in its tests.
//!
//! All drivers built on this framework are single-threaded event loops in
//! this rendering: one message at a time, no shared mutable state across
//! threads. The multi-threaded C loop is intentionally not replicated (see
//! `[ARCH A-5]` in the design snapshot); its queue semantics survive as the
//! bounded [`protocol::PendingQueue`] type.

#![no_std]
#![warn(clippy::let_underscore_must_use)]

extern crate alloc;

pub mod driver;
pub mod partition;
pub mod protocol;
pub mod pump;
pub mod wire;

pub use crate::driver::{BlockDriver, decode_block_vector};
pub use crate::protocol::BlockIovec;
