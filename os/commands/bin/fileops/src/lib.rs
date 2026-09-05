#![cfg_attr(not(test), no_std)]

//! File operation command core for Minix-RS commands.
//!
//! Covers `notes/rewrite/fork-syscall-rewrite/18-stage-commands/06-file-ops.md`:
//! the thirty file manipulation commands (`minix3/bin/` fifteen,
//! `minix3/usr.bin/` fifteen, `usr.sbin` three, `minix/commands` one).
//! The commands themselves split into two halves: deciding (which bits,
//! which files, which truth value) and doing (system calls). This crate
//! owns the deciding half, which is pure text and number processing:
//!
//! - [`mode`]: permission bit arithmetic behind `chmod` (octal and symbolic
//!   modes, mirroring the `setmode`/`getmode` pair `chmod.c` programs
//!   against at lines 166 and 216).
//! - [`testexpr`]: the `test`/`[` expression language (operator precedence
//!   `or` → `and` → `not` → primary, mirroring `oexpr`/`aexpr`/`nexpr`/
//!   `primary` in `minix3/bin/test/test.c` lines 160 to 163) over the
//!   [`testexpr::FileTester`] trait.
//! - [`path`]: `basename`/`dirname` splitting (pure string surgery).
//!
//! File status queries go through [`testexpr::FileTester`] with one
//! implementation per deployment stage, so unit tests never touch a real
//! file system. The split follows the stage's standing rule (pure decision
//! down, system interaction up) and Redox's habit of testable plain
//! libraries under thin programs.
//!
//! Everything borrows from the input and uses fixed size buffers: no heap,
//! `no_std` throughout.

pub mod mode;
pub mod path;
pub mod testexpr;

/// Errors produced by this crate, mapped to classic Unix error numbers.
///
/// 22 marks malformed input (`EINVAL`): bad mode text, bad expression, bad
/// path. A missing file during evaluation is `ENOENT` (2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileOpError {
    /// Malformed input.
    InvalidArgument,
    /// A file the expression names does not exist.
    NotFound,
}

impl FileOpError {
    /// The classic Unix error number for this failure.
    pub fn as_errno(self) -> i32 {
        match self {
            FileOpError::InvalidArgument => 22,
            FileOpError::NotFound => 2,
        }
    }
}
