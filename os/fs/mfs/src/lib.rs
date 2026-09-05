//! Minix file server: startup, dispatch table, and cache wrappers.
//!
//! C correspondence: `minix3/minix/fs/mfs/main.c`, `table.c`, `cache.c`.
//! This crate owns the server skeleton; the superblock, inode, path, and
//! data-path stages (documents 08-17) fill in the handlers row by row (see
//! [`table`] for the per-row owners).
//!
//! Like every file server here, MFS is a single-threaded event loop: one
//! message at a time, no shared mutable state across threads.

#![no_std]

extern crate alloc;

pub mod dir;
pub mod inode;
pub mod link;
pub mod meta;
pub mod mfs_cache;
pub mod mount;
pub mod open;
pub mod read;
pub mod startup;
pub mod superblock;
pub mod table;
pub mod write;
