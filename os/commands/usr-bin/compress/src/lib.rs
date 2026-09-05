#![cfg_attr(not(test), no_std)]

//! Compression and archiving core for Minix-RS commands.
//!
//! Covers `notes/rewrite/fork-syscall-rewrite/18-stage-commands/11-compress-archive.md`:
//! the transfer codecs (`minix3/usr.bin/uuencode/uuencode.c` 202 lines with
//! `encode` and `base64_encode`), the Lempel-Ziv family
//! (`minix3/minix/commands/compress/compress.c` 1618 lines: "Modified
//! Lempel-Ziv encoding" with `-d`/`-f`/`-v`/`-c`/`-b` documented in its
//! header comment), the shell archives (`minix3/usr.bin/shar/shar.sh`),
//! and the integrity checks the archive formats rely on.
//!
//! # Design
//!
//! Full `gzip`/`bzip2`/`unzip` file formats (headers, trailers, multi
//! member streams) stay with later stages; what lands here are the
//! algorithmic cores, each pure over caller buffers:
//!
//! - [`uu`]: uuencode/uudecode byte groups (the oldest binary to text
//!   codec, still the exact wire format).
//! - [`lzw`]: Lempel-Ziv-Welch compression with explicit clear codes,
//!   variable width codes, and the same code assignment the C tool uses
//!   (first free code 258, clear 256, end 257) over an owncontainer framing
//!   documented below.
//! - [`shar`]: shell archive header parsing (`# : shar`-style preamble and
//!   `begin`/`end` file stanzas emitted by the shell script).
//! - [`checksum`]: the [`checksum::Checksum`] trait with the two archive
//!   integrity functions (cyclic redundancy check and Adler's checksum).
//!
//! Everything uses fixed size buffers: no heap, `no_std` throughout.

pub mod checksum;
pub mod lzw;
pub mod shar;
pub mod uu;

/// Errors produced by this crate, mapped to classic Unix error numbers.
///
/// 22 marks malformed input (`EINVAL`): bad groups, bad codes, bad
/// headers. A fixed buffer proving too small is reported distinctly so the
/// caller can grow it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompressError {
    /// Malformed input.
    InvalidArgument,
    /// A fixed buffer proved too small for the result.
    TooLong,
}

impl CompressError {
    /// The classic Unix error number for this failure.
    pub fn as_errno(self) -> i32 {
        match self {
            CompressError::InvalidArgument => 22,
            CompressError::TooLong => 12,
        }
    }
}
