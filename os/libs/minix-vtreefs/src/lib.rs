//! Virtual-tree filesystem framework: an in-memory named tree with hook
//! dispatch, shared by every virtual filesystem (process information,
//! device management, and similar).
//!
//! C correspondence: `minix3/minix/lib/libvtreefs/` (ten sources plus four
//! headers, about three thousand five hundred lines): `vtreefs.c`
//! (startup and message loop), `inode.c` (tree storage and reference
//! counting), `table.c` (driver callback table), `path.c` (name
//! resolution), `link.c` (symbolic links and device nodes), `mount.c`
//! (mounting), `stadir.c` (status), `file.c` (file and directory input and
//! output), `extra.c` (per-node extra storage), `sdbm.c` (name hashing).
//!
//! Like every file server here, consumers run a single-threaded event
//! loop: one message at a time, no shared mutable state across threads.
//! This crate owns no threads and no globals; a `Tree` value owns one
//! filesystem instance, so tests can build several side by side.

#![no_std]

extern crate alloc;

mod tree;

pub use tree::{
    DirEntry, FileStat, FsHooks, HookError, NodeId, NodeStat, Tree, TreeError, TreeStat,
};

/// Maximum name length accepted on lookup (`NAME_MAX`, sixty bytes on the
/// target; the short-name threshold below mirrors `PNAME_MAX`).
pub const NAME_MAX: usize = 60;
/// Names at or below this length use inline storage in C (`PNAME_MAX`,
/// twenty-four); longer names use heap storage. Rust stores every name the
/// same way, so this constant only documents the threshold.
pub const SHORT_NAME_MAX: usize = 24;
/// No index marker (`NO_INDEX`, minus one).
pub const NO_INDEX: i32 = -1;
/// Minimum staging buffer for directory listings (`GETDENTS_BUFSIZ`, four
/// thousand ninety-six bytes).
pub const LISTING_BUFFER_MINIMUM: usize = 4096;
/// World-readable regular file (`REG_ALL_MODE` in the process server).
pub const MODE_FILE_WORLD_READ: u16 = 0o100444;
/// World-accessible directory (`DIR_ALL_MODE` in the process server).
pub const MODE_DIR_WORLD_ACCESS: u16 = 0o040555;
