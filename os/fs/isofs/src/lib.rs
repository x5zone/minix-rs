//! Read-only ISO9660 filesystem: volume discovery, record decoding,
//! and Rock Ridge personality.
//!
//! C correspondence: `minix3/minix/fs/isofs/` (twelve sources). Reads
//! clamp to the file size and walk extents block by block; listings
//! walk decoded records; links report stored targets; only mounting,
//! lookup, reads, listings, links, and status exist (no creation,
//! writing, or linking: the disc is read-only, and the callback table
//! leaves those rows empty).
//!
//! Like every file server here, isofs is a single-threaded event loop:
//! one message at a time, no shared mutable state across threads.
//!
//! Module map: [`volume`] scans descriptors and checks primaries,
//! [`record`] decodes records and walks extents, [`rockridge`] parses
//! the system-use tail.

#![no_std]

extern crate alloc;

pub mod record;
pub mod rockridge;
pub mod volume;

use minix_types::{EINVAL, Errno};

/// Whether Rock Ridge applies (`norock` option, `main.c:10`: set means
/// skip the system-use tail and show raw interchange names).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Options {
    /// Skip Rock Ridge parsing.
    pub no_rock_ridge: bool,
}

impl Options {
    /// Default options (`main.c:21`: Rock Ridge on).
    pub const fn default_options() -> Self {
        Self { no_rock_ridge: false }
    }
}

/// Why an ISO9660 call failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IsoError {
    /// Bad record or option (short buffer, corrupt length).
    Invalid,
}

impl IsoError {
    /// The wire error code.
    pub const fn to_errno(self) -> Errno {
        match self {
            Self::Invalid => Errno::from_i32(EINVAL),
        }
    }
}

/// Why Rock Ridge parsing failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RockError {
    /// Bad entry (short header, overrun).
    Invalid,
}

impl RockError {
    /// The wire error code.
    pub const fn to_errno(self) -> Errno {
        match self {
            Self::Invalid => Errno::from_i32(EINVAL),
        }
    }
}

/// Clamp a read to the file remainder (`fs_read`, `read.c`): at or past
/// the end reports zero (end of file, not an error); anything longer
/// than the remainder shrinks to it.
pub const fn clamp_read(position: u64, size: u64, request: u64) -> u64 {
    if position >= size {
        return 0;
    }
    let rest = size - position;
    if request > rest {
        rest
    } else {
        request
    }
}

/// Service initialization entry (kept for the server binary).
pub fn init() {}
