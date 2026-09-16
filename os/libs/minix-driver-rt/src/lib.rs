//! Driver service runtime: the common shell behind every driver binary.
//!
//! Every Minix3 driver runs the same skeleton — SEF startup, announce
//! through the data store, then a receive-classify-dispatch loop fed by
//! the virtual file system service and device notifications
//! (`chardriver_task`, `chardriver.c:455-573`; the same shape in
//! `blockdriver_task` and `netdriver_task`). This crate renders that
//! skeleton once, so a driver binary is "construct the device, run the
//! runtime" instead of fifty-seven hand-copied loop shells.
//!
//! The seam is transport injection: policy code is testable with a
//! canned transport, and the production kernel transport is one
//! implementation among several. The SEF lifecycle switch point lives
//! inside the transport implementation — when a driver gains SEF
//! handling, its transport's `receive` swaps to
//! `minix_sef::sef_receive_status` (RS ping absorption, the IS/MIB
//! precedent) and nothing in the loop changes.
//!
//! `[ARCH: 驱动服务运行时统一]` — the runtime is stage infrastructure:
//! `16-stage-drivers/todo.md` A1 records the decision; document
//! `00-drivers-overview.md` carries the architecture narrative once
//! expanded.
//!
//! Single-threaded event loop: one message at a time, no shared mutable
//! state across threads (`!Send`/`RefCell` discipline applies to drivers
//! built on it).

#![no_std]

extern crate alloc;

pub mod kernel;
pub mod runtime;
pub mod transport;
