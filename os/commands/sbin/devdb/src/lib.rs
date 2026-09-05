#![cfg_attr(not(test), no_std)]

//! Device nodes and system databases core for Minix-RS commands.
//!
//! Covers `notes/rewrite/fork-syscall-rewrite/18-stage-commands/04-device-database.md`:
//! static device node creation (`MAKEDEV.sh` in
//! `minix3/minix/commands/MAKEDEV/`, `mknod` in `minix3/sbin/mknod/`),
//! the device database builder (`dev_mkdb` in `minix3/usr.sbin/dev_mkdb/`),
//! the directory hierarchy specification (`mtree`), and the tiny query
//! command (`getent` in `minix3/usr.bin/getent/getent.c`) over the system
//! database files (`minix3/etc/group`, `hosts`, `services`, `protocols`,
//! `shells`, ...).
//!
//! # Design
//!
//! Same split as the sibling crates: pure parsing and lookup logic here,
//! effects (creating nodes, building database files) with the caller. The
//! parsers borrow from the input text, keep no heap, and compile under
//! `no_std`, following the Redox habit of testable plain libraries under
//! thin programs.
//!
//! # Modules
//!
//! - [`group`]: group file line format.
//! - [`services`]: services file line format.
//! - [`mtree`]: directory hierarchy specification lines.
//! - [`lookup`]: the [`lookup::LookupTable`] trait with one implementation
//!   per database.

pub mod group;
pub mod lookup;
pub mod mtree;
pub mod services;

/// Errors produced by this crate, mapped to classic Unix error numbers.
///
/// 22 marks malformed input (`EINVAL`), 2 marks an unknown key (`ENOENT`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DevDbError {
    /// Malformed input: bad line format, bad number, empty key.
    InvalidArgument,
    /// A database key that does not exist.
    NotFound,
}

impl DevDbError {
    /// The classic Unix error number for this failure.
    pub fn as_errno(self) -> i32 {
        match self {
            DevDbError::InvalidArgument => 22,
            DevDbError::NotFound => 2,
        }
    }
}
