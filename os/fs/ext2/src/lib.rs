//! Second extended filesystem server: allocation, layout, and mapping.
//!
//! C correspondence: `minix3/minix/fs/ext2/` (seventeen sources). This
//! server speaks the same driver protocol as the Minix server and reuses
//! its request semantics; only the disk format and the placement
//! policies differ, so each module documents its difference against the
//! Minix reference instead of repeating shared semantics.
//!
//! Like every file server here, ext2 is a single-threaded event loop:
//! one message at a time, no shared mutable state across threads.
//!
//! Module map: [`superblock`] validates the superblock and derives
//! geometry plus feature gates, [`placement`] places inodes and blocks
//! across groups, [`dir`] walks variable-length entries, [`inode`]
//! transfers the 128-byte record, [`mapping`] decomposes addresses
//! across three indirection levels.

#![no_std]

// SD-19 机械化门（静默吞错家族）：本 crate 内新增无理由的 `let _ =`
// 会当场多报一条 `clippy::let_underscore_must_use` 警告。不做 crate 级
// `#![allow]` blanket，保门有效；已定性的历史站点逐站在语句级 `#[allow]`
// 上方留 `//[SD19-gate] <理由>`。参考 `os/libs/minix-sys/src/lib.rs` 同
// 族头部注释。
#![warn(clippy::let_underscore_must_use)]

extern crate alloc;

pub mod placement;
pub mod dir;
pub mod inode;
pub mod mapping;
pub mod superblock;
pub mod server;

use minix_types::{EFBIG, EINVAL, Errno};

/// Why an inode transfer failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InodeError {
    /// Bad record (short buffer, corrupt sizes).
    Invalid,
}

impl InodeError {
    /// The wire error code.
    pub const fn to_errno(self) -> Errno {
        match self {
            Self::Invalid => Errno::from_i32(EINVAL),
        }
    }
}

/// Why address decomposition failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MappingError {
    /// Bad geometry (no addresses per block).
    Invalid,
    /// Past the triple-indirect cube (`EFBIG`).
    TooBig,
}

impl MappingError {
    /// The wire error code.
    pub const fn to_errno(self) -> Errno {
        match self {
            Self::Invalid => Errno::from_i32(EINVAL),
            Self::TooBig => Errno::from_i32(EFBIG),
        }
    }
}

/// Service initialization entry (kept for the server binary).
pub fn init() {}
