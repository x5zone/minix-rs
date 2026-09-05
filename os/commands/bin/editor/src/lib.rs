#![cfg_attr(not(test), no_std)]

//! Line editor core for Minix-RS commands.
//!
//! Covers `notes/rewrite/fork-syscall-rewrite/18-stage-commands/09-editors.md`:
//! the line editor (`minix3/bin/ed/`: addresses in `main.c` at
//! `extract_addr_range` line 285 and `next_addr` line 314, commands in
//! `exec_command` line 465 with cases from line 481, line operations at
//! lines 1051 to 1242, marks at lines 1271 to 1297) and the screen editor
//! (`minix3/minix/usr.bin/mined/mined1.c` 1774 lines, `mined2.c` 1666
//! lines).
//!
//! # Design
//!
//! Both editors manipulate text, and both need the same three answers:
//! which lines an address names, how text is stored, and what a command
//! letter asks for. This crate owns those answers as pure logic over
//! borrowed text:
//!
//! - [`store`]: the [`store::TextStore`] trait with two backends — a gap
//!   buffer (cheap edits at the cursor, the classic editor structure) and
//!   a line table (cheap random access by line number, the natural fit for
//!   address arithmetic). Same interface, different performance shapes; the
//!   caller picks per workload. This mirrors how Redox keeps alternate data
//!   structures behind one behaviour interface.
//! - [`addr`]: `ed` address parsing and evaluation (current line, last
//!   line, numbers, marks, offsets, ranges with `,` and `;`).
//! - [`cmd`]: `ed` command letter parsing (append, change, delete, insert,
//!   print, substitute, write, quit, ...).
//!
//! Screen handling, file input/output, and regular expression matching stay
//! outside: the screen belongs to the terminal stage, files to the file
//! system layer, and patterns reuse the stage's search crate. Everything
//! here borrows from the input and uses fixed size buffers: no heap,
//! `no_std` throughout.

pub mod addr;
pub mod cmd;
pub mod store;

/// Errors produced by this crate, mapped to classic Unix error numbers.
///
/// 22 marks malformed input (`EINVAL`): bad addresses, unknown commands,
/// overfull buffers. Out of range lines are also 22 (the C editor prints
/// `?` for all of these; the exit status face stays uniform).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorError {
    /// Malformed input or out of range line.
    InvalidArgument,
    /// A fixed buffer proved too small.
    TooLong,
}

impl EditorError {
    /// The classic Unix error number for this failure.
    pub fn as_errno(self) -> i32 {
        match self {
            EditorError::InvalidArgument => 22,
            EditorError::TooLong => 12,
        }
    }
}
