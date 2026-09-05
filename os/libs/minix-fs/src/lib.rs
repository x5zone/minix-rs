//! File server framework: request protocol, dispatch, adapters, caller-data
//! helpers, path resolution, block cache, and a memory file server.
//!
//! C correspondence: `minix3/minix/lib/libfsdriver/` (framework) and
//! `minix3/minix/lib/libminixfs/` (block cache plus block input and output).
//!
//! Every Minix3 file server runs the same skeleton: receive a message from
//! the virtual file system service, translate it into a callback, send the
//! reply. The C code shares that skeleton through two libraries; this crate
//! is their Rust rendering, organized so each module matches one design
//! document in `notes/rewrite/fork-syscall-rewrite/15-stage-fs/`:
//!
//! - [`protocol`] — request numbers, transaction identifiers, flags
//!   (document 01).
//! - [`driver`] — the driver trait, mount state, dispatch rules
//!   (document 01).
//! - [`call`] — the thirty-one request adapters (document 02).
//! - [`data`] — caller data channel: copy-in, copy-out, zero, names
//!   (document 03).
//! - [`dentry`] — directory entry listing encoder (document 03).
//! - [`lookup`] — path resolution walk (document 03).
//! - [`cache`] — hashed least-recently-used block cache (document 04).
//! - [`bio`] — raw block transfer, prefetch, driver binding, ramdisk
//!   (document 05).
//! - [`memfs`] — in-memory file server proving the trait (documents 01-03).
//!
//! All servers built on this framework are single-threaded event loops: one
//! message at a time, no shared mutable state across threads. Types in this
//! crate therefore make no thread-safety promises beyond what the event loop
//! guarantees.

#![no_std]

extern crate alloc;

pub mod bio;
pub mod cache;
pub mod call;
pub mod data;
pub mod dentry;
pub mod driver;
pub mod lookup;
pub mod memfs;
pub mod protocol;
