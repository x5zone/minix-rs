//! Character driver framework: request protocol, dispatch, and server skeleton.
//!
//! C correspondence: `minix3/minix/lib/libchardriver/chardriver.c` (600 lines)
//! plus `minix3/minix/include/minix/chardriver.h` (callback table) and the
//! character request constants in `minix3/minix/include/minix/com.h:919-956`.
//!
//! Every Minix3 character driver runs the same skeleton: announce readiness
//! through the data store service, receive one message at a time from the
//! virtual file system service, translate it into a callback described by
//! [`driver::CharDriver`], and send the reply. This crate renders that
//! skeleton in Rust, organized so each module matches one part of document
//! `01-chardriver-framework.md` in
//! `notes/rewrite/fork-syscall-rewrite/16-stage-drivers/`:
//!
//! - [`protocol`] — request numbers, reply shapes, minor-device tracking
//!   (document sections 1 and 2).
//! - [`driver`] — the driver trait, routing rules, and the server state
//!   machine (document sections 3 and 4).
//!
//! All drivers built on this framework are single-threaded event loops: one
//! message is handled at a time and no mutable state is shared across
//! threads. Types in this crate therefore make no thread-safety promises
//! beyond what the event loop guarantees.

#![no_std]

extern crate alloc;

pub mod driver;
pub mod protocol;
