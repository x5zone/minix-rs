#![cfg_attr(not(test), no_std)]

//! Text filtering and data processing core for Minix-RS commands.
//!
//! Covers `notes/rewrite/fork-syscall-rewrite/18-stage-commands/07-text-filter.md`:
//! the line oriented tools in `minix3/usr.bin/` (`head`, `tail`, `sort`,
//! `uniq`, `wc`, `cut`, `tr`, `expand`, `fold`, `rev`, `seq`, `tee`, ...).
//! Every tool here reads lines and writes lines; the only thing that
//! differs is the per line decision. This crate owns those decisions as
//! pure functions over borrowed text:
//!
//! - [`window`]: head/tail line windows behind the [`window::LineWindow`]
//!   trait.
//! - [`count`]: the `wc` counters (lines, words, bytes, longest line).
//! - [`cut`]: field list parsing and column selection.
//! - [`tr`]: character set translation, deletion, and squeezing over the
//!   [`tr::CharClass`] trait.
//! - [`uniq`]: adjacent duplicate handling (`-c` counts, `-d`/`-u`
//!   selection).
//!
//! Sorting (`sort`), comparison (`cmp`, `diff`, `patch`), pagination, and
//! number formatting stay with later stages: `sort` needs ordering plus
//! buffering policy, the comparators need file pairing, and the formatters
//! need output width policy. The line algorithms here are the shared
//! foundation they will all reuse.
//!
//! Everything borrows from the input and uses fixed size buffers: no heap,
//! `no_std` throughout.

pub mod count;
pub mod cut;
pub mod tr;
pub mod uniq;
pub mod window;

/// Errors produced by this crate, mapped to classic Unix error numbers.
///
/// 22 marks malformed input (`EINVAL`): bad field lists, bad counts, bad
/// sets. All bounds are explicit fixed capacities; overflow is an error,
// never a silent truncation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextError {
    /// Malformed input.
    InvalidArgument,
    /// A fixed buffer proved too small for the result.
    TooLong,
}

impl TextError {
    /// The classic Unix error number for this failure.
    pub fn as_errno(self) -> i32 {
        match self {
            TextError::InvalidArgument => 22,
            TextError::TooLong => 12,
        }
    }
}
