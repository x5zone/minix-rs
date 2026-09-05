#![cfg_attr(not(test), no_std)]

//! Regular expressions and text search core for Minix-RS commands.
//!
//! Covers `notes/rewrite/fork-syscall-rewrite/18-stage-commands/08-grep-sed.md`:
//! the pattern face shared by `grep` (`minix3/minix/usr.bin/grep/`, built on
//! the POSIX `regcomp`/`regexec` interface) and `sed`
//! (`minix3/usr.bin/sed/compile.c` for parsing, `process.c` for running).
//!
//! # Design
//!
//! The C tools delegate matching to the system regex library. A from-scratch
//! engine is a deliberate, documented deviation: the Minix-RS user space
//! cannot assume a POSIX regex library exists yet, so the command layer
//! needs its own small engine with identical observable behaviour on the
//! common pattern subset. The engine is a bounded backtracker (step budget
//! plus depth limit), the same structural choice early Unix search tools
//! made before Thompson virtual machines became standard: simple,
//! predictable, and fast enough for line oriented command input.
//!
//! The engine works on bytes and treats the input as ASCII text, matching
//! how the C tools behave under the default C locale. Multi byte characters
//! pass through untouched inside literal runs; character classes and the dot
//! match single bytes. Full locale aware matching is deferred (see the
//! architecture note in the stage document).
//!
//! All structures use fixed size arrays and borrow the input; the crate
//! compiles under `no_std`.
//!
//! # Modules
//!
//! - [`pattern`]: pattern compilation (basic vs extended spelling).
//! - [`matcher`]: the [`matcher::Matcher`] trait with one implementation per
//!   matching strategy.
//! - [`grep`]: search option model and exit status computation.
//! - [`sed`]: substitution command parsing and application.

pub mod grep;
pub mod matcher;
pub mod pattern;
pub mod sed;

/// Errors produced by this crate, mapped to classic Unix error numbers.
///
/// A malformed pattern is `EINVAL` (22), the same failure a `regcomp` call
/// reports. Missing input files are the execution layer's concern and are
/// not modelled here.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RegexError {
    /// The pattern text is malformed.
    InvalidPattern,
    /// A compiled structure overflowed its fixed capacity.
    TooComplex,
}

impl RegexError {
    /// The classic Unix error number for this failure.
    pub fn as_errno(self) -> i32 {
        match self {
            RegexError::InvalidPattern => 22,
            RegexError::TooComplex => 22,
        }
    }
}
