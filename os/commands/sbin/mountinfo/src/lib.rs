#![cfg_attr(not(test), no_std)]

//! Mount and filesystem check core for Minix-RS commands.
//!
//! Covers `notes/rewrite/fork-syscall-rewrite/18-stage-commands/14-mount-fsck.md`:
//! the mount command face (`minix3/minix/commands/mount/mount.c`: type and
//! option flags at lines 41 to 60, usage at lines 17 and 168; unmount in
//! `minix3/minix/commands/umount/umount.c`), the filesystem table (six
//! field rows consumed by the checker), and the check ordering behind
//! `fsck` (`minix3/sbin/fsck/`: pass number zero means skip at
//! `fsck.c:254`, preen mode in `preen.c`).
//!
//! # Design
//!
//! Mounting itself is a file system service call; deciding what to mount,
//! with which options, and in which check order is pure text processing:
//!
//! - [`fstab`]: six field table row parsing (device, mount point, type,
//!   options, dump frequency, check pass number).
//! - [`options`]: mount option list parsing (`ro`, `rw`, `noexec`,
//!   `nosuid`, `sync`, ...) into a flag set.
//! - [`order`]: check scheduling from pass numbers (pass 1 first, then
//!   ascending passes, pass 0 skipped) plus the [`order::MountTable`]
//!   trait with an empty and a slice implementation.
//!
//! The live mount call and the checker passes stay with the execution
//! layer. Everything here borrows from the input and uses fixed size
//! buffers: no heap, `no_std` throughout.

pub mod fstab;
pub mod options;
pub mod order;

/// Errors produced by this crate, mapped to classic Unix error numbers.
///
/// 22 marks malformed input (`EINVAL`): bad rows, unknown options, bad
/// pass numbers. A mount point with no table row behind it is `ENOENT`
/// (2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MountError {
    /// Malformed input.
    InvalidArgument,
    /// No table row for the mount point.
    NotFound,
}

impl MountError {
    /// The classic Unix error number for this failure.
    pub fn as_errno(self) -> i32 {
        match self {
            MountError::InvalidArgument => 22,
            MountError::NotFound => 2,
        }
    }
}
