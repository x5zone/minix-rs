#![cfg_attr(not(test), no_std)]

//! Documentation and manual page core for Minix-RS commands.
//!
//! Covers `notes/rewrite/fork-syscall-rewrite/18-stage-commands/10-doc-man-tools.md`:
//! the manual system (`minix3/usr.bin/man/man.c` 1088 lines with
//! configuration in `manconf.c` 272 lines, the live `minix3/etc/man.conf`,
//! the `whatis` database built by `minix3/libexec/makewhatis/makewhatis.c`
//! 1174 lines) and the calendar (`minix3/usr.bin/cal/cal.c` 924 lines with
//! its Julian/Gregorian reformation arithmetic).
//!
//! # Design
//!
//! The manual system is three separable decisions: where to look (section
//! search order and build rules from the configuration file), what matches
//! (the `name(section) - description` database), and how dates grid out
//! (calendar arithmetic). Each is pure text or number processing:
//!
//! - [`manconf`]: configuration file directives (`_whatdb`, `_subdir`,
//!   `_suffix`, `_build`, ...).
//! - [`whatis`]: database line parsing plus the [`whatis::ManDb`] trait
//!   with an empty and a slice implementation.
//! - [`cal`]: month grid computation over a Julian Day Number core (valid
//!   for both Julian and Gregorian reckonings; the reformation gap is the
//!   caller's date range choice, matching how `cal.c` parameterises the
//!   missing days).
//!
//! Typesetting (`cawf`, `troff` friends), spell checking, message
//! catalogs, and internationalisation stay with later stages. Everything
//! here borrows from the input and uses fixed size buffers: no heap,
//! `no_std` throughout.

pub mod cal;
pub mod manconf;
pub mod whatis;

/// Errors produced by this crate, mapped to classic Unix error numbers.
///
/// 22 marks malformed input (`EINVAL`): bad directives, bad database
/// lines, impossible dates. A name with no manual entry is `ENOENT` (2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DocError {
    /// Malformed input.
    InvalidArgument,
    /// No manual entry (or no database row) for the name.
    NotFound,
}

impl DocError {
    /// The classic Unix error number for this failure.
    pub fn as_errno(self) -> i32 {
        match self {
            DocError::InvalidArgument => 22,
            DocError::NotFound => 2,
        }
    }
}
