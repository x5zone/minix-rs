//! Memory driver: RAM disks, absolute and kernel memory, null and zero.
//!
//! C correspondence: `minix3/minix/drivers/storage/memory/memory.c` (599
//! lines). This crate owns the numbers (minor decoding, geometry table,
//! open counts, transfer plans, page-window policy); the service binary
//! owns the transport (announce, message loop, grant copies, physical
//! mapping). See document `05-memory-driver.md` in
//! `notes/rewrite/fork-syscall-rewrite/16-stage-drivers/`.
//!
//! The driver serves both faces with one receive loop: block requests go
//! to the block framework, everything else to the character framework
//! (`memory.c:101-104`). That dispatch lives in the service binary; the
//! face-membership predicate ([`device::MemoryMinor::is_character_only`])
//! is the shared rule both sides apply.
//!
//! Single-threaded event loop: one message at a time, no shared mutable
//! state across threads.

#![no_std]

extern crate alloc;

pub mod device;
pub mod transfer;

/// Service initialization entry (wires the tables; transport stays out).
pub fn init() {}
