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
// [SD-19] 静默吞错家族编译门：无理由的新 `let _ =`（丢弃 `#[must_use]` 返回值）会在
// 每次 clippy 当场多报一条；有意丢弃处已逐站 `// [SD19-gate] <理由>` + 语句级 `#[allow]`。
#![warn(clippy::let_underscore_must_use)]

extern crate alloc;

pub mod dir;
pub mod dir_io;
pub mod fsck;
pub mod inode;
pub mod link;
pub mod maint;
pub mod meta;
pub mod mfs_cache;
pub mod mkfs;
pub mod mount;
pub mod open;
pub mod read;
pub mod second_level;
pub mod server;
pub mod startup;
pub mod superblock;
pub mod table;
pub mod write;
