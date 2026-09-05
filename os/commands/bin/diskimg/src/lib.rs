#![cfg_attr(not(test), no_std)]

//! Disk image and media core for Minix-RS commands.
//!
//! Covers `notes/rewrite/fork-syscall-rewrite/18-stage-commands/16-image-media.md`:
//! block copying (`minix3/bin/dd/`: operands in `args.c` lines 105 to 121,
//! conversions in `conv.c`), optical media (`writeisofs`, `isoread` with
//! the `CD001` identifier at `isoread.c:41`, `vol`, `eject`, `cdprobe`),
//! memory disks (`ramdisk`, `loadramdisk`, `vnconfig`), and FAT reading
//! (`dosread`).
//!
//! # Design
//!
//! Copying bytes needs no operating system when the source and sink are
//! memory: this crate owns the operand language and the copy plan as pure
//! logic over a [`device::BlockDevice`] trait (memory and empty backends
//! ship; the driver backend lands with block device access):
//!
//! - [`dd`]: operand parsing (`if`, `of`, `bs`, `ibs`, `obs`, `cbs`,
//!   `count`, `skip`, `seek`, `conv`) plus the copy plan (block counts,
//!   partial tails, conversion flags).
//! - [`iso`]: optical volume descriptor recognition (type byte plus the
//!   `CD001` identifier, volume label, block size, both endian reads).
//! - [`device`]: the [`device::BlockDevice`] trait with memory and empty
//!   implementations backing copy tests without any hardware.
//!
//! Media control (eject, probe) and FAT parsing stay with later stages.
//! Everything uses fixed size buffers: no heap, `no_std` throughout.

pub mod dd;
pub mod device;
pub mod iso;

/// Errors produced by this crate, mapped to classic Unix error numbers.
///
/// 22 marks malformed input (`EINVAL`): bad operands, bad descriptors,
/// short devices. Running past the sink reports distinctly so callers can
/// grow it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageError {
    /// Malformed input.
    InvalidArgument,
    /// A fixed buffer proved too small.
    TooLong,
}

impl ImageError {
    /// The classic Unix error number for this failure.
    pub fn as_errno(self) -> i32 {
        match self {
            ImageError::InvalidArgument => 22,
            ImageError::TooLong => 12,
        }
    }
}
