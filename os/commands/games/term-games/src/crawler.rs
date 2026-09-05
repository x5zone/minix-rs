//! Crawler segments and rain drops.
//!
//! Ground truth: `minix3/games/worm/worm.c` (a worm is a segment list whose
//! head advances and whose tail follows), `minix3/games/worms/` (several
//! worms at once), and `minix3/games/rain/rain.c` (drops fall one row per
//! frame with a configurable delay). This module owns the shared motion:
//! heads advance, middles follow, drops fall.

use crate::TermGameError;

/// Longest crawler accepted.
pub const MAX_CRAWLER: usize = 128;

/// One crawler: head first, tail last.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Crawler {
    segments: [(i32, i32); MAX_CRAWLER],
    len: usize,
}

impl Crawler {
    /// A crawler of `len` cells starting at (`x`, `y`) stretching left.
    pub fn new(x: i32, y: i32, len: usize) -> Result<Self, TermGameError> {
        if len == 0 || len > MAX_CRAWLER {
            return Err(TermGameError::InvalidArgument);
        }
        let mut segments = [(0i32, 0i32); MAX_CRAWLER];
        for (index, slot) in segments.iter_mut().enumerate().take(len) {
            *slot = (x - index as i32, y);
        }
        Ok(Crawler { segments, len })
    }

    /// Head position.
    pub fn head(&self) -> (i32, i32) {
        self.segments[0]
    }

    /// Body length in cells.
    pub fn len(&self) -> usize {
        self.len
    }

    /// True when empty (never for a live crawler).
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Slither the head to `next`, every segment taking its predecessor cell.
    pub fn slither(&mut self, next: (i32, i32)) {
        if self.len > 1 {
            self.segments.copy_within(0..self.len - 1, 1);
        }
        self.segments[0] = next;
    }
}

/// One rain drop (column plus current row).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Drop {
    /// Column the drop falls in.
    pub column: i32,
    /// Current row.
    pub row: i32,
}

/// Fall one frame: the row increases by one, wrapping past `bottom` to zero.
pub fn fall_drop(drop: Drop, bottom: i32) -> Result<Drop, TermGameError> {
    if bottom <= 0 || drop.column < 0 || drop.row < 0 || drop.row > bottom {
        return Err(TermGameError::InvalidArgument);
    }
    let row = if drop.row >= bottom { 0 } else { drop.row + 1 };
    Ok(Drop {
        column: drop.column,
        row,
    })
}

/// Parse a rain delay in milliseconds (1 through 999, like the `-d` flag).
pub fn parse_delay(word: &str) -> Result<u32, TermGameError> {
    if word.is_empty() {
        return Err(TermGameError::InvalidArgument);
    }
    let mut value: u32 = 0;
    for byte in word.bytes() {
        if !byte.is_ascii_digit() {
            return Err(TermGameError::InvalidArgument);
        }
        value = value
            .checked_mul(10)
            .and_then(|scaled| scaled.checked_add((byte - b'0') as u32))
            .ok_or(TermGameError::InvalidArgument)?;
    }
    if !(1..=999).contains(&value) {
        return Err(TermGameError::InvalidArgument);
    }
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_crawler_slithers() {
        let mut worm = Crawler::new(5, 5, 3).unwrap();
        worm.slither((6, 5));
        assert_eq!(worm.head(), (6, 5));
        assert_eq!(worm.len(), 3);
    }

    #[test]
    fn test_bad_lengths_rejected() {
        assert_eq!(Crawler::new(5, 5, 0).map(|_| ()), Err(TermGameError::InvalidArgument));
        assert_eq!(
            Crawler::new(5, 5, MAX_CRAWLER + 1).map(|_| ()),
            Err(TermGameError::InvalidArgument)
        );
    }

    #[test]
    fn test_drops_fall_and_wrap() {
        let drop = Drop { column: 3, row: 4 };
        assert_eq!(fall_drop(drop, 10).unwrap(), Drop { column: 3, row: 5 });
        let edge = Drop { column: 3, row: 10 };
        assert_eq!(fall_drop(edge, 10).unwrap().row, 0);
        assert_eq!(
            fall_drop(drop, 0).map(|_| ()),
            Err(TermGameError::InvalidArgument)
        );
    }

    #[test]
    fn test_delays_parse() {
        assert_eq!(parse_delay("50"), Ok(50));
        assert_eq!(parse_delay("0"), Err(TermGameError::InvalidArgument));
        assert_eq!(parse_delay("1000"), Err(TermGameError::InvalidArgument));
        assert_eq!(parse_delay("fast"), Err(TermGameError::InvalidArgument));
    }
}
