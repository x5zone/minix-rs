#![cfg_attr(not(test), no_std)]

//! Terminal control games core for Minix-RS commands.
//!
//! Covers `rewrite-notes/18-stage-commands/23-terminal-games.md`:
//! falling blocks (`minix3/games/tetris/shapes.c` with the neighbor offsets
//! near lines 46 to 53, the fit test near line 82, placement near line 97,
//! and the board size `B_COLS 12` by `B_ROWS 23` in `tetris.h` near lines 54
//! to 56; the board array and row clearing in `tetris.c` near lines 62 and
//! 109 to 118), crawlers (`minix3/games/worm/worm.c` with screen setup near
//! line 120 and cell drawing near lines 203 to 204, `minix3/games/rain/rain.c`
//! with the delay option near line 87 and drop drawing near lines 117 to 118,
//! `minix3/games/colorbars/colorbars.c`), the snake game
//! (`minix3/games/snake/`), and the dungeon game (`minix3/games/rogue/rogue.h`
//! with wall and door flags near lines 54 to 56 and the room limit `MAXROOMS 9`
//! near line 293, room layout in `room.c`).
//!
//! Architecture decision: the screen speaks through escape sequences produced
//! here; the terminal capability database is not ported (see the design
//! document). Six game makefiles link the terminal library in the original
//! tree; here no terminal library is needed because output is plain text.
//!
//! # Design
//!
//! These games move on a grid; the grid never touches hardware. What is pure
//! here lives in this crate, what reads keys and paints the display stays with
//! the per game binaries (`os/commands/games/`, currently execution stubs):
//!
//! - [`screen`]: cell buffers over the [`screen::Screen`] trait (a memory
//!   screen plus a null screen), and escape sequence builders (clear screen,
//!   move cursor home).
//! - [`tetris`]: piece offsets, fit testing, placement, row clearing, and
//!   scoring.
//! - [`snake`]: heading steps, growth, wall and self collision.
//! - [`crawler`]: segment list movement shared by crawlers and rain drops.
//! - [`dungeon`]: room overlap checks and corridor carving for dungeon maps.
//! - [`random`]: deterministic game randomness over the [`random::GameRandom`]
//!   trait (a fixed cycle plus a zero source so tests stay deterministic).
//!
//! Everything borrows from the input and uses fixed size buffers: no heap,
//! `no_std` throughout. Key reads and display writes stay with the execution
//! layer.

pub mod crawler;
pub mod dungeon;
pub mod random;
pub mod screen;
pub mod snake;
pub mod tetris;

/// Errors produced by this crate, mapped to classic Unix error numbers.
///
/// 22 marks malformed input (`EINVAL`): bad sizes, bad positions, bad delays.
/// 2 marks a missing entry (`ENOENT`): a room or cell with no record behind
/// it. 28 marks a full board (`ENOSPC`, the same number placement reports
/// when no empty cell remains).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TermGameError {
    /// Malformed input.
    InvalidArgument,
    /// No such entry.
    NotFound,
    /// Board full.
    NoSpace,
}

impl TermGameError {
    /// The classic Unix error number for this failure.
    pub fn as_errno(self) -> i32 {
        match self {
            TermGameError::InvalidArgument => 22,
            TermGameError::NotFound => 2,
            TermGameError::NoSpace => 28,
        }
    }
}
