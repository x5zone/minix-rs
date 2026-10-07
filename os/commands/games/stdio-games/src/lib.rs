#![cfg_attr(not(test), no_std)]

//! Pure standard input and output games core for Minix-RS commands.
//!
//! Covers `rewrite-notes/18-stage-commands/22-stdio-games.md`:
//! number factoring (`minix3/games/factor/factor.c` with the factor printing
//! function near line 184, the large number path near line 110, and the usage
//! line `factor [value ...]` near line 268), prime generation
//! (`minix3/games/primes/primes.c`, pattern tables in `pattern.c`, prime
//! tables in `pr_tbl.c`), rotation cipher (`minix3/games/caesar/caesar.c`
//! with the rotation table builder near line 82, rotation parsing near line
//! 125, and overflow guarding with `rot %= LETTERS` near line 86), morse code
//! (`minix3/games/morse/morse.c` with the code table near line 96, the usage
//! line `morse [-ds] [string ...]` near line 140), pig latin
//! (`minix3/games/pig/pig.c` with the vowel rule near line 103 and the usage
//! line `pig` near line 133), arithmetic quiz
//! (`minix3/games/arithmetic/arithmetic.c` with the default range 10 near
//! line 99 and the right and wrong counters near line 100), and big banners
//! (`minix3/games/banner/banner.c` with the full width 132 near line 58 and
//! the usage line `banner [-w width] [message]` near line 1057). Number
//! words (`minix3/games/number/number.c`), binary coded display
//! (`minix3/games/bcd/bcd.c`), and paper tape punch (`minix3/games/ppt/ppt.c`)
//! are part of this document's contract but not yet written here — their
//! deciding halves land with the execution batch that wires their binaries.
//!
//! # Design
//!
//! These games only read and write; they never touch the screen directly.
//! What is pure here lives in this crate, what reads the keyboard and writes
//! the display stays with the thin `src/bin/` programs of this same crate
//! (the former per-game placeholder binary crates were removed on
//! 2026-09-17; see the stage todo §6.1 step 3):
//!
//! - [`factor`]: decimal parsing, trial division with a small prime wheel,
//!   and `number: factor ...` line rendering.
//! - [`primes`]: prime sieving over an interval.
//! - [`caesar`]: rotation table building, rotation parsing, line rotation.
//! - [`morse`]: code table lookup over the [`morse::MorseTable`] trait (a
//!   static table plus an empty table so tests run without data files).
//! - [`words`]: pig latin word conversion and small number to English words.
//! - [`banner`]: display width scaling over the [`banner::GlyphSource`]
//!   trait (a block renderer plus an empty source).
//! - [`quiz`]: arithmetic quiz scoring (right and wrong counters, range
//!   checks, question generation over dice values).
//!
//! Everything borrows from the input and uses fixed size buffers: no heap,
//! `no_std` throughout. Keyboard reads and display writes stay with the
//! execution layer.

pub mod banner;
pub mod bcd;
pub mod caesar;
pub mod factor;
pub mod morse;
pub mod ppt;
pub mod primes;
pub mod quiz;
pub mod words;

/// Errors produced by this crate, mapped to classic Unix error numbers.
///
/// 22 marks malformed input (`EINVAL`): bad numbers, bad rotations, bad
/// widths. 2 marks a missing entry (`ENOENT`): a code, glyph, or word with no
/// record behind it. 34 marks a range failure (`ERANGE`, the same number
/// rotation parsing reports for an overflowing value).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GameError {
    /// Malformed input.
    InvalidArgument,
    /// No such entry.
    NotFound,
    /// Value out of range.
    OutOfRange,
}

impl GameError {
    /// The classic Unix error number for this failure.
    pub fn as_errno(self) -> i32 {
        match self {
            GameError::InvalidArgument => 22,
            GameError::NotFound => 2,
            GameError::OutOfRange => 34,
        }
    }
}
