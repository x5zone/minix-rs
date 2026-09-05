//! Deterministic game randomness.
//!
//! Ground truth: games draw lots constantly (food placement, piece choice,
//! room layout). The platform random source stays with the execution layer;
//! this module owns a testable interface plus two deterministic sources so
//! game logic tests never flake.

use crate::TermGameError;

/// Random source behind game lots.
pub trait GameRandom {
    /// Next value below `bound` (uniform in spirit, exactness unneeded).
    fn below(&mut self, bound: u32) -> Result<u32, TermGameError>;
}

/// Fixed cycle source (replays the given values modulo `bound`).
pub struct FixedCycle<'a> {
    values: &'a [u32],
    index: usize,
}

impl<'a> FixedCycle<'a> {
    /// Build a source replaying `values` forever.
    pub fn new(values: &'a [u32]) -> Result<Self, TermGameError> {
        if values.is_empty() {
            return Err(TermGameError::InvalidArgument);
        }
        Ok(FixedCycle { values, index: 0 })
    }
}

impl GameRandom for FixedCycle<'_> {
    fn below(&mut self, bound: u32) -> Result<u32, TermGameError> {
        if bound == 0 {
            return Err(TermGameError::InvalidArgument);
        }
        let value = self.values[self.index % self.values.len()] % bound;
        self.index = self.index.wrapping_add(1);
        Ok(value)
    }
}

/// Zero source (always draws zero; the dullest luck possible).
pub struct ZeroSource;

impl GameRandom for ZeroSource {
    fn below(&mut self, bound: u32) -> Result<u32, TermGameError> {
        if bound == 0 {
            return Err(TermGameError::InvalidArgument);
        }
        Ok(0)
    }
}

/// Pick a random empty cell index: draw until an empty cell turns up (at most
/// `tries` draws), reporting full when none appears.
pub fn pick_empty_cell<R: GameRandom>(
    rng: &mut R,
    occupied: &[bool],
    tries: u32,
) -> Result<usize, TermGameError> {
    if occupied.is_empty() || tries == 0 {
        return Err(TermGameError::InvalidArgument);
    }
    for _ in 0..tries {
        let index = rng.below(occupied.len() as u32)? as usize;
        if !occupied[index] {
            return Ok(index);
        }
    }
    Err(TermGameError::NoSpace)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cycle_replays() {
        let values = [3, 7, 11];
        let mut rng = FixedCycle::new(&values).unwrap();
        assert_eq!(rng.below(100).unwrap(), 3);
        assert_eq!(rng.below(100).unwrap(), 7);
        assert_eq!(rng.below(100).unwrap(), 11);
        assert_eq!(rng.below(100).unwrap(), 3);
    }

    #[test]
    fn test_zero_draws_zero() {
        let mut rng = ZeroSource;
        assert_eq!(rng.below(7).unwrap(), 0);
        assert_eq!(rng.below(0).map(|_| ()), Err(TermGameError::InvalidArgument));
    }

    #[test]
    fn test_empty_cycle_rejected() {
        assert_eq!(FixedCycle::new(&[]).map(|_| ()), Err(TermGameError::InvalidArgument));
    }

    #[test]
    fn test_empty_cell_picked() {
        let values = [0, 1, 2];
        let mut rng = FixedCycle::new(&values).unwrap();
        let occupied = [true, true, false];
        assert_eq!(pick_empty_cell(&mut rng, &occupied, 10).unwrap(), 2);
    }

    #[test]
    fn test_full_reports_no_space() {
        let mut rng = ZeroSource;
        let occupied = [true, true];
        assert_eq!(
            pick_empty_cell(&mut rng, &occupied, 5),
            Err(TermGameError::NoSpace)
        );
    }
}
