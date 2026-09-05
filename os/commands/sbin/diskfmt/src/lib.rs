#![cfg_attr(not(test), no_std)]

//! Partition and format core for Minix-RS commands.
//!
//! Covers `notes/rewrite/fork-syscall-rewrite/18-stage-commands/15-partition-format.md`:
//! the partition table layout (`minix3/sys/sys/bootblock.h`: table at
//! offset 446, magic `0xAA55` at 510, four 16 byte entries, active flag
//! `0x80`, Minix types `0x80`/`0x81`), read and rewritten by
//! `minix3/minix/commands/part/part.c` (512 byte boot block at line 300,
//! table copy at lines 384 to 482, base/size helpers at lines 550 to 551
//! and 691 to 702) and shown by `minix3/minix/commands/fdisk/fdisk.c`.
//!
//! # Design
//!
//! Partitioning is byte layout plus arithmetic, both pure:
//!
//! - [`mbr`]: master boot record parsing (magic check, four entries, boot
//!   flag, type names, little endian start/size) plus the
//!   [`mbr::PartitionTable`] trait with an empty and a memory
//!   implementation.
//! - [`size`]: human size parsing (`10M`, `512K`, `1G`) behind the
//!   `newfs`/`mkfs` size options, with overflow errors instead of
//!   wraparound.
//!
//! Writing tables and formatting volumes stay with the execution layer
//! behind block device access. Everything here borrows from the input and
//! uses fixed size buffers: no heap, `no_std` throughout.

pub mod mbr;
pub mod size;

/// Errors produced by this crate, mapped to classic Unix error numbers.
///
/// 22 marks malformed input (`EINVAL`): bad magic, bad sizes, bad table
/// shapes. A partition slot holding no partition is reported distinctly so
/// callers can tell "empty" from "corrupt".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiskError {
    /// Malformed input.
    InvalidArgument,
    /// An empty partition slot or missing table.
    NotFound,
}

impl DiskError {
    /// The classic Unix error number for this failure.
    pub fn as_errno(self) -> i32 {
        match self {
            DiskError::InvalidArgument => 22,
            DiskError::NotFound => 2,
        }
    }
}
