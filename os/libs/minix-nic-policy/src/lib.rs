//! Shared NIC ring policy: modulo cursors and wraparound arithmetic.
//!
//! C correspondence: the ring walks that several cards already write
//! the same way — e1000's descriptor tail (`(tail + 1) %
//! E1000_*DESC_NR`, the receive and transmit rings of `e1000.c`) and
//! rtl8139's transmit slot picker (`(slot + 1) % RL_N_TX`, four slots,
//! `rtl8139.h:19`) plus the continuous 64K receive buffer whose read
//! cursor wraps by packet length (`rtl8139.c:619-688`).
//!
//! Not every net-card ring walk belongs here. dp8390's receive cursor
//! (`dp8390.c:598-720`) walks a u8 page space with a sentinel boundary
//! and wraps on an equality test against the stop page — a different
//! shape, so that cursor stays with its card. lance has no cursor
//! arithmetic yet (its `ring.rs` holds ring sizes and chip identity).
//!
//! What is shared is the arithmetic, not the geometry: each card owns
//! its constants (descriptor counts, slot counts, buffer sizes) and
//! consumes [`ModuloCursor`] and [`wrapped_offset`] for the walk.

#![no_std]

/// A cursor that advances inside `0..modulus` and wraps at the end.
///
/// The modulus is fixed at construction: a descriptor ring's count, a
/// transmit slot count, or a receive buffer size in bytes. The cursor
/// models the "index modulo N" half of a card's ring walk; the card
/// decides when to advance and what the index means.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModuloCursor {
    index: usize,
    modulus: usize,
}

impl ModuloCursor {
    /// A cursor at index zero over the given modulus.
    ///
    /// `modulus` must be non-zero: a ring of nothing cannot be walked
    /// (the C cards would divide by zero here).
    pub const fn new(modulus: usize) -> Self {
        assert!(modulus > 0, "ModuloCursor needs a non-zero modulus");
        ModuloCursor { index: 0, modulus }
    }

    /// Current index (`0..modulus`).
    pub const fn index(&self) -> usize {
        self.index
    }

    /// The modulus this cursor wraps at.
    pub const fn modulus(&self) -> usize {
        self.modulus
    }

    /// Advance one step, wrapping at the end; returns the new index.
    pub fn advance(&mut self) -> usize {
        self.index = (self.index + 1) % self.modulus;
        self.index
    }

    /// Advance by `count` steps, wrapping at the end; returns the new
    /// index.
    pub fn advance_by(&mut self, count: usize) -> usize {
        self.index = (self.index + count) % self.modulus;
        self.index
    }
}

/// `base + by` wrapped inside `0..modulus`.
///
/// The stateless half of the cursor, for callers that track an index of
/// their own (rtl8139's receive cursor advances by a packet's byte
/// count, not by one).
pub const fn wrapped_offset(base: usize, by: usize, modulus: usize) -> usize {
    (base + by) % modulus
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_advance_wraps_at_the_modulus() {
        let mut cursor = ModuloCursor::new(4);
        for expected in [1, 2, 3, 0, 1] {
            assert_eq!(cursor.advance(), expected);
        }
    }

    #[test]
    fn test_advance_by_wraps_like_the_repeated_single_step() {
        let mut stepped = ModuloCursor::new(64 * 1024);
        let mut jumped = ModuloCursor::new(64 * 1024);
        for _ in 0..100 {
            stepped.advance();
        }
        assert_eq!(jumped.advance_by(100), stepped.index());
        assert_eq!(stepped.index(), 100);
        assert_eq!(jumped.advance_by(65436), 0);
        assert_eq!(jumped.advance_by(65535), 65535);
    }

    #[test]
    fn test_wrapped_offset_matches_the_cursor_walk() {
        let mut cursor = ModuloCursor::new(256);
        let mut expected = 0usize;
        for step in [1usize, 255, 2] {
            expected = wrapped_offset(expected, step, 256);
            assert_eq!(cursor.advance_by(step), expected);
        }
        assert_eq!(wrapped_offset(65535, 1, 65536), 0);
        assert_eq!(wrapped_offset(2, 3, 4), 1);
    }

    #[test]
    #[should_panic(expected = "ModuloCursor needs a non-zero modulus")]
    fn test_zero_modulus_is_refused() {
        let _ = ModuloCursor::new(0);
    }
}
