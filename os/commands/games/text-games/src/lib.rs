#![cfg_attr(not(test), no_std)]

//! Text games core for Minix-RS commands.
//!
//! Covers `notes/rewrite/fork-syscall-rewrite/18-stage-commands/24-text-games.md`:
//! cave adventure (`minix3/games/adventure/hdr.h` with the vocabulary limit
//! `HTSIZE 512` near line 78, object descriptions near line 101, initial
//! object placement near line 116; movement in `vocab.c` near line 72),
//! board dealing (`minix3/games/monop/`), fortune cookies
//! (`minix3/games/fortune/fortune/fortune.c` with seek table reads near lines
//! 267 and 980, data files in `datfiles/`), card fishing
//! (`minix3/games/fish/fish.c` with thirteen `RANKS` near line 61, the full
//! deck near line 64, and book counting near line 83), acronym lookup
//! (`minix3/games/wtf/wtf` with the `-o` and `-f` flags and the skipped `is`
//! word), random filtering (`minix3/games/random/random.c` with time plus
//! process seeding near line 107 and the one over denominator selection near
//! line 126), and the war games script (`minix3/games/wargames/wargames.sh`).
//!
//! Architecture decision: large prose data files ship as program resources,
//! not as code tables (see the design document). Tests use small inline
//! tables with the same shapes.
//!
//! # Design
//!
//! These games tell stories; the stories live in data. What is pure here
//! lives in this crate, what reads data files and prompts players stays with
//! the per game binaries (`os/commands/games/`, currently execution stubs):
//!
//! - [`adventure`]: vocabulary lookup over the [`adventure::VocabTable`]
//!   trait (a slice table plus an empty table), object movement between
//!   rooms, and room navigation over exits.
//! - [`monop`]: board squares, money arithmetic with bankruptcy checks, and
//!   dice movement with board wrap.
//! - [`fortune`]: quip lookup over the [`fortune::QuipDb`] trait (a slice
//!   database plus an empty database) and uniform quip picking.
//! - [`fish`]: rank hands, book counting, and ask decisions.
//! - [`acronym`]: acronym file parsing (`term: expansion` lines) and the
//!   skipped `is` word.
//! - [`lottery`]: one over denominator selection (the random filter rule).
//!
//! Everything borrows from the input and uses fixed size buffers: no heap,
//! `no_std` throughout. File reads and player prompts stay with the execution
//! layer.

pub mod acronym;
pub mod adventure;
pub mod fish;
pub mod fortune;
pub mod lottery;
pub mod monop;

/// Errors produced by this crate, mapped to classic Unix error numbers.
///
/// 22 marks malformed input (`EINVAL`): bad words, bad dice, bad denominators.
/// 2 marks a missing entry (`ENOENT`): a word, quip, or acronym with no record
/// behind it. 34 marks a range failure (`ERANGE`, the same number dice and
/// money arithmetic report for overflowing values).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextGameError {
    /// Malformed input.
    InvalidArgument,
    /// No such entry.
    NotFound,
    /// Value out of range.
    OutOfRange,
}

impl TextGameError {
    /// The classic Unix error number for this failure.
    pub fn as_errno(self) -> i32 {
        match self {
            TextGameError::InvalidArgument => 22,
            TextGameError::NotFound => 2,
            TextGameError::OutOfRange => 34,
        }
    }
}
