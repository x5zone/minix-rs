#![cfg_attr(not(test), no_std)]

//! Terminal control core for Minix-RS commands.
//!
//! Covers `notes/rewrite/fork-syscall-rewrite/18-stage-commands/13-terminal-termios.md`:
//! the control command (`minix3/bin/stty/` with control characters in
//! `cchar.c`, flag tables in `modes.c`, speed handling through
//! `cfsetospeed` in `stty.c:137` and `key.c:258`, reporting in
//! `print.c:71-73`), the capability database
//! (`minix3/etc/termcap*`, queried by `term`/`tget`), and the font/key/screen
//! tools (`loadfont`, `loadkeys`, `screendump` with `minix3/etc/fonts`).
//!
//! # Design
//!
//! The kernel owns the live terminal state (the termios structure behind
//! `tcsetattr`); user space only names what it wants. This crate owns the
//! naming half as pure logic:
//!
//! - [`baud`]: speed number/baud tables in both directions.
//! - [`cchar`]: control character names (`intr`, `erase`, ...) plus caret
//!   notation (`^C`) and `undef`, mirroring the `cchar.c` table.
//! - [`stty`]: `stty` argument parsing into [`stty::SttyOp`] values
//!   (speeds, control characters, `name`/`-name` flags).
//! - [`caps`]: termcap entry parsing plus the [`caps::TermcapSource`]
//!   trait with an empty and a slice implementation (the same shape the
//!   stage's database crates use: corrupt lines are skipped, first match
//!   wins).
//!
//! Applying settings to a real terminal stays with the execution layer
//! behind the runtime termios binding. Everything here borrows from the
//! input and uses fixed size buffers: no heap, `no_std` throughout.

pub mod baud;
pub mod caps;
pub mod cchar;
pub mod stty;

/// Errors produced by this crate, mapped to classic Unix error numbers.
///
/// 22 marks malformed input (`EINVAL`): unknown speeds, names, or flags.
/// All bounds are explicit fixed capacities; overflow is an error, never a
/// silent truncation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TermError {
    /// Malformed input.
    InvalidArgument,
}

impl TermError {
    /// The classic Unix error number for this failure.
    pub fn as_errno(self) -> i32 {
        match self {
            TermError::InvalidArgument => 22,
        }
    }
}
