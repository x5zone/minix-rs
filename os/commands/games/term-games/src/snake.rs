//! Snake movement and collision.
//!
//! Ground truth: `minix3/games/snake/` (play plus high score table). The snake
//! advances one cell per tick, grows when eating, and dies on walls or on
//! itself. The high score file stays with the execution layer; this module
//! owns headings, steps, and collisions.

use crate::TermGameError;

/// Playfield size in cells.
pub const FIELD_WIDTH: i32 = 40;
/// Playfield size in cells.
pub const FIELD_HEIGHT: i32 = 20;

/// Longest snake accepted (bounds the segment buffer).
pub const MAX_SEGMENTS: usize = 256;

/// Heading directions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Heading {
    /// Facing up (decreasing row).
    Up,
    /// Facing down (increasing row).
    Down,
    /// Facing left (decreasing column).
    Left,
    /// Facing right (increasing column).
    Right,
}

/// True when `next` reverses `current` (reversals are illegal moves).
pub fn is_reversal(current: Heading, next: Heading) -> bool {
    matches!(
        (current, next),
        (Heading::Up, Heading::Down)
            | (Heading::Down, Heading::Up)
            | (Heading::Left, Heading::Right)
            | (Heading::Right, Heading::Left)
    )
}

/// Step one cell from (`x`, `y`) along `heading`.
pub fn step_from(x: i32, y: i32, heading: Heading) -> (i32, i32) {
    match heading {
        Heading::Up => (x, y - 1),
        Heading::Down => (x, y + 1),
        Heading::Left => (x - 1, y),
        Heading::Right => (x + 1, y),
    }
}

/// Snake body: head first, tail last.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Snake {
    segments: [(i32, i32); MAX_SEGMENTS],
    len: usize,
    heading: Heading,
}

impl Snake {
    /// A three cell snake facing right with its head at (`x`, `y`).
    pub fn new(x: i32, y: i32) -> Result<Self, TermGameError> {
        if !(2..FIELD_WIDTH).contains(&x) || !(0..FIELD_HEIGHT).contains(&y) {
            return Err(TermGameError::InvalidArgument);
        }
        let mut segments = [(0i32, 0i32); MAX_SEGMENTS];
        segments[0] = (x, y);
        segments[1] = (x - 1, y);
        segments[2] = (x - 2, y);
        Ok(Snake {
            segments,
            len: 3,
            heading: Heading::Right,
        })
    }

    /// Head position.
    pub fn head(&self) -> (i32, i32) {
        self.segments[0]
    }

    /// Body length in cells.
    pub fn len(&self) -> usize {
        self.len
    }

    /// True when no segment is stored (never for a live snake).
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Current heading.
    pub fn heading(&self) -> Heading {
        self.heading
    }

    /// Turn to `next` (reversals are rejected).
    pub fn turn(&mut self, next: Heading) -> Result<(), TermGameError> {
        if is_reversal(self.heading, next) {
            return Err(TermGameError::InvalidArgument);
        }
        self.heading = next;
        Ok(())
    }

    /// Advance one tick: `grow` keeps the tail (after eating).
    ///
    /// Fails on wall hits, self hits, and a full segment buffer.
    pub fn advance(&mut self, grow: bool) -> Result<(), TermGameError> {
        let (hx, hy) = self.head();
        let next = step_from(hx, hy, self.heading);
        if next.0 < 0 || next.1 < 0 || next.0 >= FIELD_WIDTH || next.1 >= FIELD_HEIGHT {
            return Err(TermGameError::InvalidArgument);
        }
        // The tail cell frees unless growing; exclude it from self hits.
        let body = if grow { self.len } else { self.len.saturating_sub(1) };
        if self.segments[..body].contains(&next) {
            return Err(TermGameError::InvalidArgument);
        }
        if grow {
            if self.len >= MAX_SEGMENTS {
                return Err(TermGameError::NoSpace);
            }
            self.segments.copy_within(0..self.len, 1);
            self.len += 1;
        } else {
            self.segments.copy_within(0..self.len - 1, 1);
        }
        self.segments[0] = next;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_reversals_illegal() {
        assert!(is_reversal(Heading::Up, Heading::Down));
        assert!(!is_reversal(Heading::Up, Heading::Left));
    }

    #[test]
    fn test_new_snake_shape() {
        let snake = Snake::new(5, 5).unwrap();
        assert_eq!(snake.head(), (5, 5));
        assert_eq!(snake.len(), 3);
        assert_eq!(Snake::new(1, 5).map(|_| ()), Err(TermGameError::InvalidArgument));
    }

    #[test]
    fn test_turn_rejects_reversal() {
        let mut snake = Snake::new(5, 5).unwrap();
        assert_eq!(snake.turn(Heading::Left), Err(TermGameError::InvalidArgument));
        snake.turn(Heading::Up).unwrap();
        assert_eq!(snake.heading(), Heading::Up);
    }

    #[test]
    fn test_advance_moves_head() {
        let mut snake = Snake::new(5, 5).unwrap();
        snake.advance(false).unwrap();
        assert_eq!(snake.head(), (6, 5));
        assert_eq!(snake.len(), 3);
    }

    #[test]
    fn test_grow_keeps_tail() {
        let mut snake = Snake::new(5, 5).unwrap();
        snake.advance(true).unwrap();
        assert_eq!(snake.len(), 4);
    }

    #[test]
    fn test_wall_kills() {
        let mut snake = Snake::new(FIELD_WIDTH - 1, 5).unwrap();
        assert_eq!(snake.advance(false), Err(TermGameError::InvalidArgument));
    }
}
